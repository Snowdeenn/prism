//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
//! Versioned resources, explicit access validation, culling and stable scheduling.
//! Versions share one allocation. Anti-dependencies prevent overwriting an older
//! version before all its readers finish. There is no cross-resource aliasing.
use crate::{RenderError, RenderResult, errors::gpu_checked};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_GRAPH: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Graph-local logical version. All versions of a resource share one allocation.
pub struct ResourceVersion {
    graph: u64,
    pub resource: usize,
    pub version: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Pass identity belonging to one graph; registration index is not a global identifier.
pub struct PassId {
    graph: u64,
    pub index: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Owned texture allocation/import requirements.
pub struct TextureDesc {
    pub size: wgpu::Extent3d,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub dimension: wgpu::TextureDimension,
}
impl TextureDesc {
    /// Describe a one-layer, one-mip, single-sample 2D renderable and sampled color texture.
    pub fn color(width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        Self {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            sample_count: 1,
            mip_level_count: 1,
            dimension: wgpu::TextureDimension::D2,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Texture or buffer requirements for one logical graph resource.
pub enum ResourceDesc {
    Texture(TextureDesc),
    Buffer {
        size: u64,
        usage: wgpu::BufferUsages,
    },
}
#[derive(Debug, Clone)]
struct Resource {
    name: String,
    desc: ResourceDesc,
    imported: bool,
    initialized: bool,
    versions: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Whole-resource access mode. Only color/depth loads imply reading a predecessor.
/// See the [access table](crate::guide#render-graph-semantics).
pub enum AccessKind {
    Sampled,
    StorageRead,
    StorageWrite,
    ColorClear,
    ColorLoad,
    DepthClear,
    DepthLoad,
    DepthRead,
    Uniform,
    Vertex,
    Index,
    Indirect,
    CopyRead,
    CopyWrite,
}
impl AccessKind {
    fn writes(self) -> bool {
        matches!(
            self,
            Self::StorageWrite
                | Self::ColorClear
                | Self::ColorLoad
                | Self::DepthClear
                | Self::DepthLoad
                | Self::CopyWrite
        )
    }
    fn loads(self) -> bool {
        matches!(self, Self::ColorLoad | Self::DepthLoad)
    }
}
#[derive(Debug, Clone, Copy)]
/// One exact resource version and its declared usage in a pass.
pub struct Access {
    pub resource: ResourceVersion,
    pub kind: AccessKind,
}
#[derive(Debug, Clone, Default)]
/// Node name, accesses, explicit dependencies and observability.
/// One pass may declare only one access per logical resource.
pub struct PassDescriptor {
    pub name: String,
    pub accesses: Vec<Access>,
    pub after: Vec<PassId>,
    pub side_effect: bool,
}
impl PassDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
    pub fn access(mut self, resource: ResourceVersion, kind: AccessKind) -> Self {
        self.accesses.push(Access { resource, kind });
        self
    }
    pub fn after(mut self, pass: PassId) -> Self {
        self.after.push(pass);
        self
    }
    pub fn side_effect(mut self) -> Self {
        self.side_effect = true;
        self
    }
}
#[derive(Debug)]
/// Versioned graph description. Compilation validates before culling and produces
/// a stable dependency order; see [graph semantics](crate::guide#render-graph-semantics).
pub struct RenderGraph {
    id: u64,
    resources: Vec<Resource>,
    passes: Vec<PassDescriptor>,
    outputs: Vec<ResourceVersion>,
}
impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}
impl RenderGraph {
    pub fn new() -> Self {
        Self {
            id: NEXT_GRAPH.fetch_add(1, Ordering::Relaxed),
            resources: Vec::new(),
            passes: Vec::new(),
            outputs: Vec::new(),
        }
    }
    fn resource(
        &mut self,
        name: impl Into<String>,
        desc: ResourceDesc,
        imported: bool,
        initialized: bool,
    ) -> ResourceVersion {
        let handle = ResourceVersion {
            graph: self.id,
            resource: self.resources.len(),
            version: 0,
        };
        self.resources.push(Resource {
            name: name.into(),
            desc,
            imported,
            initialized,
            versions: 1,
        });
        handle
    }
    pub fn create_texture(
        &mut self,
        name: impl Into<String>,
        desc: TextureDesc,
    ) -> ResourceVersion {
        self.resource(name, ResourceDesc::Texture(desc), false, false)
    }
    pub fn import_texture(
        &mut self,
        name: impl Into<String>,
        desc: TextureDesc,
        initialized: bool,
    ) -> ResourceVersion {
        self.resource(name, ResourceDesc::Texture(desc), true, initialized)
    }
    pub fn create_buffer(
        &mut self,
        name: impl Into<String>,
        size: u64,
        usage: wgpu::BufferUsages,
    ) -> ResourceVersion {
        self.resource(name, ResourceDesc::Buffer { size, usage }, false, false)
    }
    pub fn import_buffer(
        &mut self,
        name: impl Into<String>,
        size: u64,
        usage: wgpu::BufferUsages,
        initialized: bool,
    ) -> ResourceVersion {
        self.resource(
            name,
            ResourceDesc::Buffer { size, usage },
            true,
            initialized,
        )
    }
    fn check(&self, handle: ResourceVersion) -> RenderResult<&Resource> {
        self.resources
            .get(handle.resource)
            .filter(|r| handle.graph == self.id && handle.version < r.versions)
            .ok_or_else(|| {
                RenderError::new("graph", "invalid resource/version or foreign graph handle")
            })
    }
    /// Create a successor of the latest logical version without allocating or declaring a writer.
    pub fn next_version(&mut self, previous: ResourceVersion) -> RenderResult<ResourceVersion> {
        let r = self.check(previous)?;
        if previous.version + 1 != r.versions {
            return Err(RenderError::new(
                &r.name,
                "writes must extend the latest resource version",
            ));
        }
        self.resources[previous.resource].versions += 1;
        Ok(ResourceVersion {
            version: previous.version + 1,
            ..previous
        })
    }
    pub fn add_pass(&mut self, pass: PassDescriptor) -> PassId {
        let id = PassId {
            graph: self.id,
            index: self.passes.len(),
        };
        self.passes.push(pass);
        id
    }
    /// Add an explicit scheduling and liveness dependency; compilation detects cycles.
    pub fn depends_on(&mut self, pass: PassId, dependency: PassId) -> RenderResult<()> {
        if pass.graph != self.id
            || dependency.graph != self.id
            || pass.index >= self.passes.len()
            || dependency.index >= self.passes.len()
        {
            return Err(RenderError::new("graph", "invalid pass handle"));
        }
        self.passes[pass.index].after.push(dependency);
        Ok(())
    }
    /// Mark a version observable for culling. Does not perform readback or transfer ownership.
    pub fn export(&mut self, resource: ResourceVersion) {
        self.outputs.push(resource);
    }
    pub fn descriptor(&self, version: ResourceVersion) -> RenderResult<&ResourceDesc> {
        Ok(&self.check(version)?.desc)
    }
    /// Validate all nodes, detect hazards/cycles, cull unobservable work and schedule live passes.
    pub fn compile(&self) -> RenderResult<CompiledGraph> {
        let n = self.passes.len();
        let mut dependencies = vec![BTreeSet::new(); n];
        let mut writers = HashMap::new();
        let mut readers: HashMap<ResourceVersion, BTreeSet<usize>> = HashMap::new();
        for (i, pass) in self.passes.iter().enumerate() {
            let mut touched = HashSet::new();
            for access in &pass.accesses {
                let resource = self.check(access.resource)?;
                validate_access(&pass.name, &resource.desc, access.kind)?;
                if !touched.insert(access.resource.resource) {
                    return Err(RenderError::new(
                        &pass.name,
                        format!(
                            "conflicting accesses to {} in one pass; use a separate output texture",
                            resource.name
                        ),
                    ));
                }
                if access.kind.writes() {
                    if writers.insert(access.resource, i).is_some() {
                        return Err(RenderError::new(
                            &pass.name,
                            "multiple writers to the same version",
                        ));
                    }
                    if access.kind.loads() {
                        if access.resource.version == 0 {
                            return Err(RenderError::new(
                                &pass.name,
                                "load requires a previous version",
                            ));
                        }
                        readers
                            .entry(ResourceVersion {
                                version: access.resource.version - 1,
                                ..access.resource
                            })
                            .or_default()
                            .insert(i);
                    }
                } else {
                    readers.entry(access.resource).or_default().insert(i);
                }
            }
            for dependency in &pass.after {
                if dependency.graph != self.id || dependency.index >= n {
                    return Err(RenderError::new(&pass.name, "invalid pass dependency"));
                }
                dependencies[i].insert(dependency.index);
            }
        }
        let producer = |version: ResourceVersion| -> RenderResult<Option<usize>> {
            let r = self.check(version)?;
            match writers.get(&version) {
                Some(i) => Ok(Some(*i)),
                None if version.version == 0 && r.imported && r.initialized => Ok(None),
                None => Err(RenderError::new(
                    &r.name,
                    format!("read of uninitialized version {}", version.version),
                )),
            }
        };
        for (version, uses) in &readers {
            if let Some(writer) = producer(*version)? {
                for reader in uses {
                    dependencies[*reader].insert(writer);
                }
            }
        }
        // Only data and explicitly requested dependencies keep passes alive.
        // Anti-dependencies and write ordering constrain scheduling, but must not
        // resurrect an unobservable reader or an overwritten clear.
        let observable_dependencies = dependencies.clone();
        for (version, writer) in &writers {
            if version.version > 0 {
                let previous = ResourceVersion {
                    version: version.version - 1,
                    ..*version
                };
                // In-place resources: prior writes and readers must precede the overwrite.
                if let Some(previous_writer) = writers.get(&previous) {
                    dependencies[*writer].insert(*previous_writer);
                }
                if let Some(uses) = readers.get(&previous) {
                    for reader in uses {
                        if reader != writer {
                            dependencies[*writer].insert(*reader);
                        }
                    }
                }
            }
        }
        let mut ready: BTreeSet<_> = (0..n).filter(|i| dependencies[*i].is_empty()).collect();
        let mut remaining: Vec<_> = dependencies.iter().map(BTreeSet::len).collect();
        let mut dependents = vec![Vec::new(); n];
        for (i, deps) in dependencies.iter().enumerate() {
            for dep in deps {
                dependents[*dep].push(i);
            }
        }
        let mut order = Vec::new();
        while let Some(i) = ready.pop_first() {
            order.push(i);
            for next in &dependents[i] {
                remaining[*next] -= 1;
                if remaining[*next] == 0 {
                    ready.insert(*next);
                }
            }
        }
        if order.len() != n {
            return Err(RenderError::new("graph", "dependency cycle"));
        }
        let mut active = HashSet::new();
        let mut stack: Vec<_> = self
            .passes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.side_effect)
            .map(|(i, _)| i)
            .collect();
        for output in &self.outputs {
            if let Some(i) = producer(*output)? {
                stack.push(i);
            }
        }
        while let Some(i) = stack.pop() {
            if active.insert(i) {
                stack.extend(observable_dependencies[i].iter().copied());
            }
        }
        order.retain(|i| active.contains(i));
        let mut lifetimes = vec![None; self.resources.len()];
        for (step, i) in order.iter().enumerate() {
            for access in &self.passes[*i].accesses {
                let range = lifetimes[access.resource.resource].get_or_insert(step..=step);
                *range = *range.start()..=step;
            }
        }
        Ok(CompiledGraph {
            id: self.id,
            order: order
                .into_iter()
                .map(|index| PassId {
                    graph: self.id,
                    index,
                })
                .collect(),
            resources: self.resources.clone(),
            passes: self.passes.clone(),
            lifetimes,
        })
    }
}
fn validate_access(label: &str, desc: &ResourceDesc, kind: AccessKind) -> RenderResult<()> {
    use AccessKind::*;
    let valid = match desc {
        ResourceDesc::Texture(t) => {
            let usage = match kind {
                Sampled => wgpu::TextureUsages::TEXTURE_BINDING,
                StorageRead | StorageWrite => wgpu::TextureUsages::STORAGE_BINDING,
                ColorClear | ColorLoad | DepthClear | DepthLoad | DepthRead => {
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                }
                CopyRead => wgpu::TextureUsages::COPY_SRC,
                CopyWrite => wgpu::TextureUsages::COPY_DST,
                _ => return Err(RenderError::new(label, "buffer access to a texture")),
            };
            let depth = t.format.is_depth_stencil_format();
            t.size.width > 0
                && t.size.height > 0
                && t.size.depth_or_array_layers > 0
                && t.usage.contains(usage)
                && (!matches!(kind, ColorClear | ColorLoad) || !depth)
                && (!matches!(kind, DepthClear | DepthLoad | DepthRead) || depth)
                && (!matches!(kind, StorageRead | StorageWrite) || t.sample_count == 1)
        }
        ResourceDesc::Buffer { size, usage } => {
            let required = match kind {
                StorageRead | StorageWrite => wgpu::BufferUsages::STORAGE,
                Uniform => wgpu::BufferUsages::UNIFORM,
                Vertex => wgpu::BufferUsages::VERTEX,
                Index => wgpu::BufferUsages::INDEX,
                Indirect => wgpu::BufferUsages::INDIRECT,
                CopyRead => wgpu::BufferUsages::COPY_SRC,
                CopyWrite => wgpu::BufferUsages::COPY_DST,
                _ => return Err(RenderError::new(label, "texture access to a buffer")),
            };
            *size > 0 && usage.contains(required)
        }
    };
    if valid {
        Ok(())
    } else {
        Err(RenderError::new(
            label,
            format!("incompatible resource format, size or usage for {kind:?}"),
        ))
    }
}
#[derive(Debug)]
/// Validated live schedule and resource lifetimes, ready for GPU allocation/imports.
pub struct CompiledGraph {
    id: u64,
    pub order: Vec<PassId>,
    resources: Vec<Resource>,
    passes: Vec<PassDescriptor>,
    pub lifetimes: Vec<Option<std::ops::RangeInclusive<usize>>>,
}
#[derive(Clone)]
/// Physical GPU allocation; texture imports must pair the texture with its own view.
pub enum GraphResource {
    Texture {
        texture: wgpu::Texture,
        view: wgpu::TextureView,
    },
    Buffer(wgpu::Buffer),
}
#[derive(Default)]
/// External allocations keyed by imported version zero.
pub struct GraphImports {
    entries: HashMap<ResourceVersion, GraphResource>,
}
impl GraphImports {
    pub fn insert(&mut self, version: ResourceVersion, resource: GraphResource) {
        self.entries.insert(version, resource);
    }
}
/// Allocated active resources for one compiled graph.
pub struct GraphResources {
    id: u64,
    entries: Vec<Option<GraphResource>>,
    versions: Vec<u32>,
}
impl GraphResources {
    pub fn get(&self, version: ResourceVersion) -> RenderResult<&GraphResource> {
        if version.graph != self.id
            || self
                .versions
                .get(version.resource)
                .is_none_or(|n| version.version >= *n)
        {
            return Err(RenderError::new("graph", "foreign resource version"));
        }
        self.entries
            .get(version.resource)
            .and_then(Option::as_ref)
            .ok_or_else(|| RenderError::new("graph", "resource is unavailable or culled"))
    }
    pub fn view(&self, version: ResourceVersion) -> RenderResult<&wgpu::TextureView> {
        match self.get(version)? {
            GraphResource::Texture { view, .. } => Ok(view),
            _ => Err(RenderError::new("graph", "expected texture")),
        }
    }
    pub fn buffer(&self, version: ResourceVersion) -> RenderResult<&wgpu::Buffer> {
        match self.get(version)? {
            GraphResource::Buffer(buffer) => Ok(buffer),
            _ => Err(RenderError::new("graph", "expected buffer")),
        }
    }
}
/// Restricted resource lookup for a node: an undeclared dependency is an error.
/// Lookup restricted to exact versions declared by a pass.
/// Custom code remains responsible for not accessing undeclared raw resources.
pub struct PassResources<'a> {
    all: &'a GraphResources,
    descriptor: &'a PassDescriptor,
}
impl PassResources<'_> {
    pub fn get(&self, version: ResourceVersion) -> RenderResult<&GraphResource> {
        if !self
            .descriptor
            .accesses
            .iter()
            .any(|a| a.resource == version)
        {
            return Err(RenderError::new(
                &self.descriptor.name,
                "undeclared resource access",
            ));
        }
        self.all.get(version)
    }
    pub fn view(&self, version: ResourceVersion) -> RenderResult<&wgpu::TextureView> {
        match self.get(version)? {
            GraphResource::Texture { view, .. } => Ok(view),
            _ => Err(RenderError::new(&self.descriptor.name, "expected texture")),
        }
    }
    pub fn buffer(&self, version: ResourceVersion) -> RenderResult<&wgpu::Buffer> {
        match self.get(version)? {
            GraphResource::Buffer(b) => Ok(b),
            _ => Err(RenderError::new(&self.descriptor.name, "expected buffer")),
        }
    }
}
impl CompiledGraph {
    pub fn pass_resources<'a>(
        &'a self,
        id: PassId,
        resources: &'a GraphResources,
    ) -> RenderResult<PassResources<'a>> {
        if id.graph != self.id || id.index >= self.passes.len() || resources.id != self.id {
            return Err(RenderError::new("graph", "foreign pass or resources"));
        }
        Ok(PassResources {
            all: resources,
            descriptor: &self.passes[id.index],
        })
    }
    /// Resolve active imports and allocate active transients without pooling or aliasing.
    pub fn prepare(
        &self,
        device: &wgpu::Device,
        mut imports: GraphImports,
    ) -> RenderResult<GraphResources> {
        let mut entries = Vec::with_capacity(self.resources.len());
        for (i, r) in self.resources.iter().enumerate() {
            if self.lifetimes[i].is_none() {
                entries.push(None);
                continue;
            }
            let resource = if r.imported {
                let key = ResourceVersion {
                    graph: self.id,
                    resource: i,
                    version: 0,
                };
                let imported = imports.entries.remove(&key).ok_or_else(|| {
                    RenderError::new(&r.name, "missing imported resource (use version zero)")
                })?;
                let valid = match (&r.desc, &imported) {
                    (ResourceDesc::Texture(d), GraphResource::Texture { texture, .. }) => {
                        texture.size() == d.size
                            && texture.format() == d.format
                            && texture.sample_count() == d.sample_count
                            && texture.mip_level_count() == d.mip_level_count
                            && texture.dimension() == d.dimension
                            && texture.usage().contains(d.usage)
                    }
                    (ResourceDesc::Buffer { size, usage }, GraphResource::Buffer(b)) => {
                        b.size() >= *size && b.usage().contains(*usage)
                    }
                    _ => false,
                };
                if !valid {
                    return Err(RenderError::new(
                        &r.name,
                        "import does not match its resource descriptor",
                    ));
                }
                imported
            } else {
                gpu_checked(device, &r.name, || match &r.desc {
                    ResourceDesc::Texture(d) => {
                        let texture = device.create_texture(&wgpu::TextureDescriptor {
                            label: Some(&r.name),
                            size: d.size,
                            mip_level_count: d.mip_level_count,
                            sample_count: d.sample_count,
                            dimension: d.dimension,
                            format: d.format,
                            usage: d.usage,
                            view_formats: &[],
                        });
                        let view = texture.create_view(&Default::default());
                        GraphResource::Texture { texture, view }
                    }
                    ResourceDesc::Buffer { size, usage } => {
                        GraphResource::Buffer(device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some(&r.name),
                            size: *size,
                            usage: *usage,
                            mapped_at_creation: false,
                        }))
                    }
                })?
            };
            if entries
                .iter()
                .flatten()
                .any(|other| match (&resource, other) {
                    (
                        GraphResource::Texture { texture: a, .. },
                        GraphResource::Texture { texture: b, .. },
                    ) => a == b,
                    (GraphResource::Buffer(a), GraphResource::Buffer(b)) => a == b,
                    _ => false,
                })
            {
                return Err(RenderError::new(
                    &r.name,
                    "one physical resource was imported under multiple logical handles",
                ));
            }
            entries.push(Some(resource));
        }
        Ok(GraphResources {
            id: self.id,
            entries,
            versions: self.resources.iter().map(|r| r.versions).collect(),
        })
    }
}

impl CompiledGraph {
    pub(crate) fn transient_texture_bytes(&self, index: usize) -> u64 {
        let r = &self.resources[index];
        if r.imported {
            return 0;
        }
        match &r.desc {
            ResourceDesc::Texture(t) => {
                let (bw, bh) = t.format.block_dimensions();
                let bytes = t.format.block_copy_size(None).unwrap_or(4) as u64;
                (0..t.mip_level_count)
                    .map(|m| {
                        let width = (t.size.width >> m).max(1).div_ceil(bw) as u64;
                        let height = (t.size.height >> m).max(1).div_ceil(bh) as u64;
                        let layers = if t.dimension == wgpu::TextureDimension::D3 {
                            (t.size.depth_or_array_layers >> m).max(1)
                        } else {
                            t.size.depth_or_array_layers
                        } as u64;
                        width * height * layers * bytes * t.sample_count as u64
                    })
                    .sum()
            }
            _ => 0,
        }
    }
}
