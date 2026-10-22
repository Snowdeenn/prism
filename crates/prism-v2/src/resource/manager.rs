//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
//! Device-owned V2 resources. Removing a handle does not destroy in-flight wgpu
//! objects: encoded commands retain them until their GPU submission completes.
use crate::{
    RenderError, RenderResult,
    arena::Arena,
    errors::gpu_checked,
    ids::*,
    material::{BindingSet, Material, MaterialTemplate},
    pipeline::ShaderRevisionKey,
};
use std::ops::Range;

/// Validated shader module with a revision incremented on successful replacement.
pub struct Shader {
    pub(crate) module: wgpu::ShaderModule,
    revision: u64,
    pub label: String,
}
impl Shader {
    pub fn module(&self) -> &wgpu::ShaderModule {
        &self.module
    }
}
#[derive(Clone)]
/// Managed GPU buffer. Do not manually destroy it while commands may still use it.
pub struct Buffer {
    pub(crate) raw: wgpu::Buffer,
}
impl Buffer {
    pub fn raw(&self) -> &wgpu::Buffer {
        &self.raw
    }
}
#[derive(Clone)]
/// Managed GPU texture. Removing its handle invalidates manager lookups of child views.
pub struct Texture {
    pub(crate) raw: wgpu::Texture,
}
impl Texture {
    pub fn raw(&self) -> &wgpu::Texture {
        &self.raw
    }
}
#[derive(Clone)]
/// Managed view retaining the generational identity of its parent texture.
pub struct TextureView {
    pub(crate) raw: wgpu::TextureView,
    pub texture: TextureId,
    pub dimension: wgpu::TextureViewDimension,
    pub format: wgpu::TextureFormat,
}
#[derive(Clone)]
/// Managed sampler with its inferred comparison/filtering binding classification.
pub struct Sampler {
    pub(crate) raw: wgpu::Sampler,
    pub kind: wgpu::SamplerBindingType,
}
#[derive(Debug, Clone)]
/// Geometry stream reference with explicit slot and half-open byte range.
pub struct VertexBuffer {
    pub slot: u32,
    pub buffer: BufferId,
    pub range: Range<u64>,
}
#[derive(Debug, Clone)]
/// Index stream reference with byte range and element format.
pub struct IndexBuffer {
    pub buffer: BufferId,
    pub range: Range<u64>,
    pub format: wgpu::IndexFormat,
}
#[derive(Debug, Clone)]
/// Reusable draw ranges referencing separately owned vertex/index buffers.
/// Deleting geometry does not delete those buffers.
pub struct Geometry {
    pub vertices: Vec<VertexBuffer>,
    pub indices: Option<IndexBuffer>,
    /// Vertex range, or index range when an index buffer is present.
    pub elements: Range<u32>,
    pub base_vertex: i32,
}
impl Geometry {
    /// Select three procedural vertices without any buffers, indices or base-vertex offset.
    pub fn fullscreen() -> Self {
        Self {
            vertices: Vec::new(),
            indices: None,
            elements: 0..3,
            base_vertex: 0,
        }
    }
}

static NEXT_MANAGER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Device-owned V2 asset store. Handles are generational and local to this manager.
/// See [ownership and removal contracts](crate::guide#resource-management-and-lifetime).
pub struct ResourceManager {
    pub(crate) identity: u64,
    device: wgpu::Device,
    shaders: Arena<Shader, ShaderTag>,
    buffers: Arena<Buffer, BufferTag>,
    textures: Arena<Texture, TextureTag>,
    views: Arena<TextureView, TextureViewTag>,
    samplers: Arena<Sampler, SamplerTag>,
    geometries: Arena<Geometry, GeometryTag>,
    templates: Arena<MaterialTemplate, MaterialTemplateTag>,
    materials: Arena<Material, MaterialInstanceTag>,
    bindings: Arena<BindingSet, BindingSetTag>,
}
impl ResourceManager {
    /// Create a V2 resource store using the GPU owned by the context.
    /// No device access is needed by the caller; handles remain local to this store.
    pub fn from_context(context: &crate::GpuContext) -> Self {
        Self::new(&context.device)
    }

    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            identity: NEXT_MANAGER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            device: device.clone(),
            shaders: Arena::new(),
            buffers: Arena::new(),
            textures: Arena::new(),
            views: Arena::new(),
            samplers: Arena::new(),
            geometries: Arena::new(),
            templates: Arena::new(),
            materials: Arena::new(),
            bindings: Arena::new(),
        }
    }
    /// Access the owning device, including the features and limits actually enabled.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    /// Validate WGSL and register a module at revision zero.
    pub fn create_shader(&mut self, label: &str, source: &str) -> RenderResult<ShaderId> {
        let module = gpu_checked(&self.device, label, || {
            self.device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(label),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                })
        })?;
        Ok(self.shaders.insert(Shader {
            module,
            revision: 0,
            label: label.into(),
        }))
    }
    /// Validate then replace WGSL and increment revision. Failure preserves the old module.
    pub fn reload_shader(&mut self, id: ShaderId, source: &str) -> RenderResult<()> {
        let shader = self.shader(id)?;
        let module = gpu_checked(&self.device, &shader.label, || {
            self.device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&shader.label),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                })
        })?;
        let shader = self
            .shaders
            .get_mut(id)
            .ok_or_else(|| missing("shader", id))?;
        shader.module = module;
        shader.revision += 1;
        Ok(())
    }
    pub fn shader(&self, id: ShaderId) -> RenderResult<&Shader> {
        self.shaders.get(id).ok_or_else(|| missing("shader", id))
    }
    pub fn shader_revision(&self, id: ShaderId) -> RenderResult<ShaderRevisionKey> {
        Ok(ShaderRevisionKey {
            shader: id,
            revision: self.shader(id)?.revision,
        })
    }
    pub fn remove_shader(&mut self, id: ShaderId) -> bool {
        self.shaders.remove(id).is_some()
    }
    /// Allocate a nonempty unmapped buffer. Supply data using frame uploads.
    pub fn create_buffer(
        &mut self,
        label: &str,
        size: u64,
        usage: wgpu::BufferUsages,
    ) -> RenderResult<BufferId> {
        if size == 0 {
            return Err(RenderError::new(label, "empty buffer"));
        }
        let raw = gpu_checked(&self.device, label, || {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        })?;
        Ok(self.buffers.insert(Buffer { raw }))
    }
    pub fn buffer(&self, id: BufferId) -> RenderResult<&Buffer> {
        self.buffers.get(id).ok_or_else(|| missing("buffer", id))
    }
    pub fn remove_buffer(&mut self, id: BufferId) -> bool {
        self.buffers.remove(id).is_some()
    }
    /// Allocate a texture from an owned descriptor, with no alternate view formats.
    pub fn create_texture(
        &mut self,
        label: &str,
        desc: &crate::graph::TextureDesc,
    ) -> RenderResult<TextureId> {
        let raw = gpu_checked(&self.device, label, || {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: desc.size,
                mip_level_count: desc.mip_level_count,
                sample_count: desc.sample_count,
                dimension: desc.dimension,
                format: desc.format,
                usage: desc.usage,
                view_formats: &[],
            })
        })?;
        Ok(self.textures.insert(Texture { raw }))
    }
    pub fn texture(&self, id: TextureId) -> RenderResult<&Texture> {
        self.textures.get(id).ok_or_else(|| missing("texture", id))
    }
    pub fn remove_texture(&mut self, id: TextureId) -> bool {
        self.textures.remove(id).is_some()
    }
    /// Create a view of a live managed texture. Parent removal invalidates later view lookup.
    pub fn create_view(
        &mut self,
        texture: TextureId,
        desc: &wgpu::TextureViewDescriptor<'_>,
    ) -> RenderResult<TextureViewHandle> {
        let tex = self.texture(texture)?;
        let dimension = desc.dimension.unwrap_or(match tex.raw.dimension() {
            wgpu::TextureDimension::D1 => wgpu::TextureViewDimension::D1,
            wgpu::TextureDimension::D2 if tex.raw.depth_or_array_layers() > 1 => {
                wgpu::TextureViewDimension::D2Array
            }
            wgpu::TextureDimension::D2 => wgpu::TextureViewDimension::D2,
            wgpu::TextureDimension::D3 => wgpu::TextureViewDimension::D3,
        });
        let format = desc.format.unwrap_or(tex.raw.format());
        let raw = gpu_checked(&self.device, "texture view", || tex.raw.create_view(desc))?;
        Ok(self.views.insert(TextureView {
            raw,
            texture,
            dimension,
            format,
        }))
    }
    pub fn view(&self, id: TextureViewHandle) -> RenderResult<&TextureView> {
        let view = self
            .views
            .get(id)
            .ok_or_else(|| missing("texture view", id))?;
        self.texture(view.texture)?;
        Ok(view)
    }
    pub fn remove_view(&mut self, id: TextureViewHandle) -> bool {
        self.views.remove(id).is_some()
    }
    /// Create and classify a sampler according to filtering and comparison settings.
    pub fn create_sampler(
        &mut self,
        desc: &wgpu::SamplerDescriptor<'_>,
    ) -> RenderResult<SamplerHandle> {
        let raw = gpu_checked(&self.device, "sampler", || self.device.create_sampler(desc))?;
        let kind = if desc.compare.is_some() {
            wgpu::SamplerBindingType::Comparison
        } else if desc.mag_filter == wgpu::FilterMode::Linear
            || desc.min_filter == wgpu::FilterMode::Linear
            || desc.mipmap_filter == wgpu::MipmapFilterMode::Linear
            || desc.anisotropy_clamp > 1
        {
            wgpu::SamplerBindingType::Filtering
        } else {
            wgpu::SamplerBindingType::NonFiltering
        };
        Ok(self.samplers.insert(Sampler { raw, kind }))
    }
    pub fn sampler(&self, id: SamplerHandle) -> RenderResult<&Sampler> {
        self.samplers.get(id).ok_or_else(|| missing("sampler", id))
    }
    pub fn remove_sampler(&mut self, id: SamplerHandle) -> bool {
        self.samplers.remove(id).is_some()
    }
    /// Register a geometry description; buffer validation is deferred until preparation.
    pub fn create_geometry(&mut self, geometry: Geometry) -> GeometryHandle {
        self.geometries.insert(geometry)
    }
    pub fn geometry(&self, id: GeometryHandle) -> RenderResult<&Geometry> {
        self.geometries
            .get(id)
            .ok_or_else(|| missing("geometry", id))
    }
    pub fn remove_geometry(&mut self, id: GeometryHandle) -> bool {
        self.geometries.remove(id).is_some()
    }
    /// Validate schema and shader handles and register an immutable template.
    /// Pipeline compilation remains deferred until first use.
    pub fn create_template(
        &mut self,
        mut template: MaterialTemplate,
    ) -> RenderResult<MaterialTemplateHandle> {
        template.schema = template.schema.validate(
            &template.label,
            &self.device.limits(),
            self.device.features(),
        )?;
        self.shader(template.vertex.shader)?;
        if let Some(fragment) = &template.fragment {
            self.shader(fragment.shader)?;
        }
        Ok(self.templates.insert(template))
    }
    pub fn template(&self, id: MaterialTemplateHandle) -> RenderResult<&MaterialTemplate> {
        self.templates
            .get(id)
            .ok_or_else(|| missing("material template", id))
    }
    pub fn remove_template(&mut self, id: MaterialTemplateHandle) -> bool {
        self.templates.remove(id).is_some()
    }
    /// Register a material after checking its template. Bindings are validated on use.
    pub fn create_material(&mut self, material: Material) -> RenderResult<MaterialHandle> {
        self.template(material.template)?;
        Ok(self.materials.insert(material))
    }
    pub fn material(&self, id: MaterialHandle) -> RenderResult<&Material> {
        self.materials
            .get(id)
            .ok_or_else(|| missing("material", id))
    }
    pub fn material_mut(&mut self, id: MaterialHandle) -> RenderResult<&mut Material> {
        self.materials
            .get_mut(id)
            .ok_or_else(|| missing("material", id))
    }
    pub fn remove_material(&mut self, id: MaterialHandle) -> bool {
        self.materials.remove(id).is_some()
    }
    /// Register one group of bindings. Resource and schema validation is deferred.
    pub fn create_bindings(&mut self, bindings: BindingSet) -> BindingSetHandle {
        self.bindings.insert(bindings)
    }
    pub fn bindings(&self, id: BindingSetHandle) -> RenderResult<&BindingSet> {
        self.bindings
            .get(id)
            .ok_or_else(|| missing("binding set", id))
    }
    pub fn bindings_mut(&mut self, id: BindingSetHandle) -> RenderResult<&mut BindingSet> {
        self.bindings
            .get_mut(id)
            .ok_or_else(|| missing("binding set", id))
    }
    pub fn remove_bindings(&mut self, id: BindingSetHandle) -> bool {
        self.bindings.remove(id).is_some()
    }
}
fn missing<Tag>(kind: &str, id: crate::arena::Id<Tag>) -> RenderError {
    RenderError::new(kind, format!("missing or stale handle {id}"))
}
