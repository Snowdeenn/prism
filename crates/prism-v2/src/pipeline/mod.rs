//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
//! Complete structural pipeline keys. Per-instance parameters never enter these keys.
use crate::{
    RenderError, RenderResult,
    bindings::{BindingSchema, CacheStats, LayoutCache},
    errors::gpu_checked,
    ids::ShaderId,
};
use std::{collections::HashMap, num::NonZeroU32, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Shader identity plus successful reload revision, used for cache invalidation.
pub struct ShaderRevisionKey {
    pub shader: ShaderId,
    pub revision: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Exact compiler inputs for one stage. Constants store `f64::to_bits()` values.
pub struct ShaderStageKey {
    pub shader: ShaderRevisionKey,
    pub entry_point: String,
    /// IEEE bits preserve exact compiler inputs, including signed zero.
    pub constants: Vec<(String, u64)>,
    pub zero_initialize_workgroup_memory: bool,
}
impl ShaderStageKey {
    pub fn new(shader: ShaderRevisionKey, entry_point: impl Into<String>) -> Self {
        Self {
            shader,
            entry_point: entry_point.into(),
            constants: Vec::new(),
            zero_initialize_workgroup_memory: true,
        }
    }
    fn normalize(&mut self) -> RenderResult<()> {
        self.constants.sort_by(|a, b| a.0.cmp(&b.0));
        if self.constants.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(RenderError::new(
                &self.entry_point,
                "duplicate specialization constant",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Vertex or instance stream stride, step mode and attributes for one buffer slot.
pub struct VertexLayout {
    pub stride: u64,
    pub step_mode: wgpu::VertexStepMode,
    pub attributes: Vec<wgpu::VertexAttribute>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Complete identity of the supported raster pipeline descriptor, excluding asset
/// bindings and parameter bytes. See [cache contracts](crate::guide#pipeline-caches-and-shader-reload).
pub struct PipelineKey {
    pub vertex: ShaderStageKey,
    pub fragment: Option<ShaderStageKey>,
    pub layout: BindingSchema,
    pub vertex_layouts: Vec<Option<VertexLayout>>,
    pub primitive: wgpu::PrimitiveState,
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    pub targets: Vec<Option<wgpu::ColorTargetState>>,
    pub multisample: wgpu::MultisampleState,
    pub multiview_mask: Option<NonZeroU32>,
}
impl PipelineKey {
    pub fn normalized(&self) -> RenderResult<Self> {
        let mut key = self.clone();
        key.layout = key.layout.normalized()?;
        key.vertex.normalize()?;
        if let Some(fragment) = &mut key.fragment {
            fragment.normalize()?;
        }
        for layout in key.vertex_layouts.iter_mut().flatten() {
            layout.attributes.sort_by_key(|a| a.shader_location);
        }
        Ok(key)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Compute stage compiler inputs and structural binding schema.
pub struct ComputePipelineKey {
    pub stage: ShaderStageKey,
    pub layout: BindingSchema,
}

/// Device-local raster/compute cache sharing a layout cache. Callers of low-level
/// creation methods must supply modules matching the keyed IDs and revisions.
pub struct PipelineCache {
    device: wgpu::Device,
    pub layouts: LayoutCache,
    render: HashMap<PipelineKey, Arc<wgpu::RenderPipeline>>,
    compute: HashMap<ComputePipelineKey, Arc<wgpu::ComputePipeline>>,
    pub render_stats: CacheStats,
    pub compute_stats: CacheStats,
}
impl PipelineCache {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            layouts: LayoutCache::new(device),
            render: HashMap::new(),
            compute: HashMap::new(),
            render_stats: CacheStats::default(),
            compute_stats: CacheStats::default(),
        }
    }
    /// Resolve a normalized raster key, validating creation on a cache miss.
    /// Fragment module presence must match the key; modules must belong to this device.
    pub fn render(
        &mut self,
        label: &str,
        key: &PipelineKey,
        vertex: &wgpu::ShaderModule,
        fragment: Option<&wgpu::ShaderModule>,
    ) -> RenderResult<Arc<wgpu::RenderPipeline>> {
        let key = key.normalized()?;
        if key.fragment.is_some() != fragment.is_some() {
            return Err(RenderError::new(
                label,
                "fragment module does not match key",
            ));
        }
        if let Some(pipeline) = self.render.get(&key) {
            self.render_stats.hits += 1;
            return Ok(pipeline.clone());
        }
        let layout = self.layouts.resolve(label, &key.layout)?;
        let buffers: Vec<_> = key
            .vertex_layouts
            .iter()
            .map(|v| {
                v.as_ref().map(|v| wgpu::VertexBufferLayout {
                    array_stride: v.stride,
                    step_mode: v.step_mode,
                    attributes: &v.attributes,
                })
            })
            .collect();
        let vertex_constants: Vec<_> = key
            .vertex
            .constants
            .iter()
            .map(|(n, v)| (n.as_str(), f64::from_bits(*v)))
            .collect();
        let fragment_constants: Vec<_> = key
            .fragment
            .iter()
            .flat_map(|s| &s.constants)
            .map(|(n, v)| (n.as_str(), f64::from_bits(*v)))
            .collect();
        let fragment_state =
            key.fragment
                .as_ref()
                .zip(fragment)
                .map(|(s, module)| wgpu::FragmentState {
                    module,
                    entry_point: Some(&s.entry_point),
                    targets: &key.targets,
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &fragment_constants,
                        zero_initialize_workgroup_memory: s.zero_initialize_workgroup_memory,
                    },
                });
        let pipeline = Arc::new(gpu_checked(&self.device, label, || {
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&layout.pipeline),
                    vertex: wgpu::VertexState {
                        module: vertex,
                        entry_point: Some(&key.vertex.entry_point),
                        buffers: &buffers,
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants: &vertex_constants,
                            zero_initialize_workgroup_memory: key
                                .vertex
                                .zero_initialize_workgroup_memory,
                        },
                    },
                    fragment: fragment_state,
                    primitive: key.primitive,
                    depth_stencil: key.depth_stencil.clone(),
                    multisample: key.multisample,
                    multiview_mask: key.multiview_mask,
                    cache: None,
                })
        })?);
        self.render.insert(key, pipeline.clone());
        self.render_stats.misses += 1;
        Ok(pipeline)
    }
    /// Resolve a compute pipeline; the module must correspond to the keyed shader revision.
    pub fn compute(
        &mut self,
        label: &str,
        key: &ComputePipelineKey,
        module: &wgpu::ShaderModule,
    ) -> RenderResult<Arc<wgpu::ComputePipeline>> {
        let mut key = key.clone();
        key.layout = key.layout.normalized()?;
        key.stage.normalize()?;
        if let Some(pipeline) = self.compute.get(&key) {
            self.compute_stats.hits += 1;
            return Ok(pipeline.clone());
        }
        let layout = self.layouts.resolve(label, &key.layout)?;
        let constants: Vec<_> = key
            .stage
            .constants
            .iter()
            .map(|(n, v)| (n.as_str(), f64::from_bits(*v)))
            .collect();
        let pipeline = Arc::new(gpu_checked(&self.device, label, || {
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(label),
                    layout: Some(&layout.pipeline),
                    module,
                    entry_point: Some(&key.stage.entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        zero_initialize_workgroup_memory: key
                            .stage
                            .zero_initialize_workgroup_memory,
                    },
                    cache: None,
                })
        })?);
        self.compute.insert(key, pipeline.clone());
        self.compute_stats.misses += 1;
        Ok(pipeline)
    }
    /// Evict raster and compute entries using this shader, retaining layout caches.
    pub fn invalidate_shader(&mut self, id: ShaderId) {
        self.render.retain(|k, _| {
            k.vertex.shader.shader != id
                && k.fragment.as_ref().is_none_or(|s| s.shader.shader != id)
        });
        self.compute.retain(|k, _| k.stage.shader.shader != id);
    }
    /// Clear pipelines and layouts without resetting cumulative hit/miss counters.
    pub fn clear(&mut self) {
        self.render.clear();
        self.compute.clear();
        self.layouts.clear();
    }
    /// Return the number of currently cached raster pipelines.
    pub fn render_count(&self) -> usize {
        self.render.len()
    }
    /// Discard superseded revisions even when a reload occurred outside the renderer.
    /// Retain only pipelines whose shader revisions satisfy the supplied predicate.
    pub fn retain_shaders(&mut self, mut current: impl FnMut(ShaderRevisionKey) -> bool) {
        self.render.retain(|k, _| {
            current(k.vertex.shader) && k.fragment.as_ref().is_none_or(|s| current(s.shader))
        });
        self.compute.retain(|k, _| current(k.stage.shader));
    }
}
