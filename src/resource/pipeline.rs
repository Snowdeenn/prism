use std::collections::HashMap;
use std::sync::Arc;
use utils::ids::ShaderId;
use wgpu::VertexBufferLayout;

use crate::GpuResources;
use crate::context::GpuContext;
use crate::errors::PipelineError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindGroupLayoutEntryKey {
    pub binding: u32,
    pub visibility: wgpu::ShaderStages,
    pub ty: BindingTypeKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BindingTypeKey {
    UniformBuffer,
    DynamicUniformBuffer,
    Texture2D,
    Sampler,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub vertex_shader: ShaderId,
    pub fragment_shader: ShaderId,
    pub blend_mode: BlendMode,
    pub vertex_format: VertexFormat,
    pub bind_groups: &'static [&'static [BindGroupLayoutEntryKey]],
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendMode {
    Alpha,    // rendu normal avec transparence
    Additive, // glow, VFX lumineux
    Multiply, // ombres, effets sombres
    Opaque,   // pas de transparence — le plus rapide
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VertexFormat {
    Pos2Color,   // [Vec2 pos, Vec4 color] — géométrie simple
    Pos2UvColor, // [Vec2 pos, Vec2 uv, Vec4 color] — sprites
}

pub struct PipelineManager {
    pipelines: HashMap<PipelineKey, Arc<wgpu::RenderPipeline>>,
    layouts: HashMap<PipelineKey, Vec<wgpu::BindGroupLayout>>,
    surface_format: wgpu::TextureFormat,
}

impl PipelineManager {
    pub fn new(surface_format: wgpu::TextureFormat) -> Self {
        Self {
            pipelines: HashMap::new(),
            layouts: HashMap::new(),
            surface_format,
        }
    }

    pub fn get_or_create(
        &mut self,
        ctx: &GpuContext,
        gpu_resources: &GpuResources,
        key: PipelineKey,
    ) -> Result<Arc<wgpu::RenderPipeline>, PipelineError> {
        if let Some(pipeline) = self.pipelines.get(&key) {
            return Ok(pipeline.clone());
        }

        let _span = tracing::info_span!(
            "PipelineManager::create_pipeline",
            vs = %key.vertex_shader,
            fs = %key.fragment_shader
        )
        .entered();

        let layouts = Self::create_bind_group_layouts(ctx, &key);
        let pipeline = self.create_pipeline(ctx, gpu_resources, &key, &layouts)?;
        let pipeline_arc = Arc::new(pipeline);

        self.layouts.insert(key.clone(), layouts);
        self.pipelines.insert(key, pipeline_arc.clone());

        tracing::info!("Nouvelle RenderPipeline WGPU compilé et mis en cache");
        Ok(pipeline_arc)
    }

    pub fn invalidate_shader(&mut self, shader_id: ShaderId) {
        tracing::debug!(shader_id = %shader_id, "Invalidation des pipelines associés au shader");

        self.pipelines
            .retain(|key, _| key.vertex_shader != shader_id && key.fragment_shader != shader_id);
        self.layouts
            .retain(|key, _| key.vertex_shader != shader_id && key.fragment_shader != shader_id);
    }

    pub fn invalidate_all(&mut self) {
        tracing::debug!("Vidage complet du cache des pipelines");
        self.pipelines.clear();
        self.layouts.clear();
    }

    pub fn get_layouts(&self, key: &PipelineKey) -> Option<&[wgpu::BindGroupLayout]> {
        self.layouts.get(key).map(|v| v.as_slice())
    }

    fn create_pipeline(
        &self,
        ctx: &GpuContext,
        gpu_resources: &GpuResources,
        key: &PipelineKey,
        bind_group_layouts: &[wgpu::BindGroupLayout],
    ) -> Result<wgpu::RenderPipeline, PipelineError> {
        let blend_mode = Self::blend_mode(key.blend_mode);
        let vertex_format = Self::vertex_layout(key.vertex_format);

        let vertex_shader = gpu_resources.get_shader(key.vertex_shader).ok_or_else(|| {
            tracing::error!(id = %key.vertex_shader, "Vertex shader introuvable lors de la création du pipeline");
            PipelineError::ShaderNotFound { id: key.vertex_shader }
        })?;

        let frag_shader = gpu_resources.get_shader(key.fragment_shader).ok_or_else(|| {
            tracing::error!(id = %key.fragment_shader, "Fragment shader introuvable lors de la création du pipeline");
            PipelineError::ShaderNotFound { id: key.fragment_shader }
        })?;

        let bind_group_layout_refs: Vec<Option<&wgpu::BindGroupLayout>> =
            bind_group_layouts.iter().map(Some).collect();

        let layout_label = format!(
            "PipelineLayout (Vertex Shader:{}, Fragement Shader:{})",
            key.vertex_shader, key.fragment_shader
        );
        let pipeline_label = format!(
            "RenderPipeline (Vertex Shader:{}, Fragement Shader:{}, Blend:{:?}, Format:{:?})",
            key.vertex_shader, key.fragment_shader, key.blend_mode, key.vertex_format
        );

        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&layout_label),
                bind_group_layouts: &bind_group_layout_refs,
                immediate_size: 0,
            });

        Ok(ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&pipeline_label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vertex_shader.module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_format)],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &frag_shader.module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.surface_format,
                        blend: blend_mode,
                        write_mask: wgpu::ColorWrites::all(),
                    })],
                }),
                multiview_mask: None,
                cache: None,
            }))
    }

    fn blend_mode(mode: BlendMode) -> Option<wgpu::BlendState> {
        match mode {
            BlendMode::Additive => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            BlendMode::Alpha => Some(wgpu::BlendState::ALPHA_BLENDING),
            BlendMode::Multiply => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Dst,
                    dst_factor: wgpu::BlendFactor::Zero,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::DstAlpha,
                    dst_factor: wgpu::BlendFactor::Zero,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            BlendMode::Opaque => None,
        }
    }

    fn vertex_layout(format: VertexFormat) -> wgpu::VertexBufferLayout<'static> {
        match format {
            VertexFormat::Pos2Color => VertexBufferLayout {
                array_stride: (size_of::<[f32; 2]>() + size_of::<[f32; 4]>()) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: size_of::<[f32; 2]>() as u64,
                        shader_location: 1,
                    },
                ],
            },
            VertexFormat::Pos2UvColor => VertexBufferLayout {
                array_stride: (size_of::<[f32; 2]>()
                    + size_of::<[f32; 2]>()
                    + size_of::<[f32; 4]>()) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: (size_of::<[f32; 2]>()) as u64,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: (size_of::<[f32; 2]>() + size_of::<[f32; 2]>()) as u64,
                        shader_location: 2,
                    },
                ],
            },
        }
    }

    fn create_bind_group_layouts(
        ctx: &GpuContext,
        key: &PipelineKey,
    ) -> Vec<wgpu::BindGroupLayout> {
        key.bind_groups
            .iter()
            .enumerate()
            .map(|(index, entries)| {
                let wgpu_entries: Vec<wgpu::BindGroupLayoutEntry> = entries
                    .iter()
                    .map(|e| wgpu::BindGroupLayoutEntry {
                        binding: e.binding,
                        visibility: e.visibility,
                        ty: match e.ty {
                            BindingTypeKey::UniformBuffer => wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            BindingTypeKey::Texture2D => wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            BindingTypeKey::Sampler => {
                                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)
                            }
                            BindingTypeKey::DynamicUniformBuffer => wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: true,
                                min_binding_size: None,
                            },
                        },
                        count: None,
                    })
                    .collect();

                let group_label = format!(
                    "BindGroupLayout #{} (Vertex Shader:{}, Fragement Shader:{})",
                    index, key.vertex_shader, key.fragment_shader
                );

                ctx.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some(&group_label),
                        entries: &wgpu_entries,
                    })
            })
            .collect()
    }
}
