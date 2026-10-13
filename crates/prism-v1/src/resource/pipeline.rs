use prism_v2::bindings::{BindingEntryDesc, BindingKind, BindingSchema, GroupSchema};
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

/// Compatibility facade backed by the V2 structural caches.
pub struct PipelineManager {
    cache: Option<prism_v2::pipeline::PipelineCache>,
    device: Option<wgpu::Device>,
    layouts: HashMap<PipelineKey, Vec<wgpu::BindGroupLayout>>,
    surface_format: wgpu::TextureFormat,
}
impl PipelineManager {
    pub fn new(surface_format: wgpu::TextureFormat) -> Self {
        Self {
            cache: None,
            device: None,
            layouts: HashMap::new(),
            surface_format,
        }
    }
    pub fn get_or_create(
        &mut self,
        ctx: &GpuContext,
        resources: &GpuResources,
        key: PipelineKey,
    ) -> Result<Arc<wgpu::RenderPipeline>, PipelineError> {
        if self.device.as_ref() != Some(&ctx.device) {
            self.cache = Some(prism_v2::pipeline::PipelineCache::new(&ctx.device));
            self.device = Some(ctx.device.clone());
            self.layouts.clear();
        }
        let vertex =
            resources
                .get_shader(key.vertex_shader)
                .ok_or(PipelineError::ShaderNotFound {
                    id: key.vertex_shader,
                })?;
        let fragment =
            resources
                .get_shader(key.fragment_shader)
                .ok_or(PipelineError::ShaderNotFound {
                    id: key.fragment_shader,
                })?;
        let schema = Self::schema(&key);
        let vertex_layout = Self::vertex_layout(key.vertex_format);
        let full_key = prism_v2::pipeline::PipelineKey {
            vertex: prism_v2::pipeline::ShaderStageKey::new(
                prism_v2::pipeline::ShaderRevisionKey {
                    shader: key.vertex_shader,
                    revision: vertex.revision,
                },
                "vs_main",
            ),
            fragment: Some(prism_v2::pipeline::ShaderStageKey::new(
                prism_v2::pipeline::ShaderRevisionKey {
                    shader: key.fragment_shader,
                    revision: fragment.revision,
                },
                "fs_main",
            )),
            layout: schema.clone(),
            vertex_layouts: vec![Some(prism_v2::pipeline::VertexLayout {
                stride: vertex_layout.array_stride,
                step_mode: vertex_layout.step_mode,
                attributes: vertex_layout.attributes.to_vec(),
            })],
            primitive: Default::default(),
            depth_stencil: None,
            targets: vec![Some(wgpu::ColorTargetState {
                format: self.surface_format,
                blend: Self::blend_mode(key.blend_mode),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            multisample: Default::default(),
            multiview_mask: None,
        };
        let cache = self.cache.as_mut().expect("cache initialized above");
        cache.retain_shaders(|key| {
            resources
                .get_shader(key.shader)
                .is_some_and(|s| s.revision == key.revision)
        });
        let pipeline = cache
            .render(
                "legacy material",
                &full_key,
                &vertex.module,
                Some(&fragment.module),
            )
            .map_err(|e| PipelineError::Validation(e.to_string()))?;
        let layout = cache
            .layouts
            .resolve("legacy material", &schema)
            .map_err(|e| PipelineError::Validation(e.to_string()))?;
        self.layouts.insert(
            key,
            layout
                .groups
                .into_iter()
                .flatten()
                .map(|g| (*g).clone())
                .collect(),
        );
        Ok(pipeline)
    }
    pub fn invalidate_shader(&mut self, id: ShaderId) {
        if let Some(cache) = &mut self.cache {
            cache.invalidate_shader(id);
        }
    }
    pub fn invalidate_all(&mut self) {
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }
        self.layouts.clear();
    }
    pub fn get_layouts(&self, key: &PipelineKey) -> Option<&[wgpu::BindGroupLayout]> {
        self.layouts.get(key).map(Vec::as_slice)
    }
    fn schema(key: &PipelineKey) -> BindingSchema {
        BindingSchema {
            groups: key
                .bind_groups
                .iter()
                .enumerate()
                .map(|(index, entries)| GroupSchema {
                    index: index as u32,
                    entries: entries
                        .iter()
                        .map(|e| BindingEntryDesc {
                            binding: e.binding,
                            visibility: e.visibility,
                            count: None,
                            kind: match e.ty {
                                BindingTypeKey::UniformBuffer => BindingKind::Uniform {
                                    min_size: 0,
                                    dynamic_offset: false,
                                },
                                BindingTypeKey::DynamicUniformBuffer => BindingKind::Uniform {
                                    min_size: 0,
                                    dynamic_offset: true,
                                },
                                BindingTypeKey::Texture2D => BindingKind::Texture {
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: true,
                                    },
                                    dimension: wgpu::TextureViewDimension::D2,
                                    multisampled: false,
                                },
                                BindingTypeKey::Sampler => BindingKind::Sampler {
                                    kind: wgpu::SamplerBindingType::Filtering,
                                },
                            },
                        })
                        .collect(),
                })
                .collect(),
        }
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
}
