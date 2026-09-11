use std::{ops::Deref, sync::Arc};
use utils::ids::ShaderId;

use crate::{
    GpuResources,
    context::GpuContext,
    errors::PassError,
    pass::{Pass, PostProcessInput},
};

#[repr(C)]
#[derive(bytemuck::Zeroable, bytemuck::Pod, Clone, Copy)]
pub struct PostProcessPassId(pub usize);

impl Deref for PostProcessPassId {
    type Target = usize;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct PostProcessPass {
    vert_shader: ShaderId,
    frag_shader: ShaderId,
    pipeline: Arc<wgpu::RenderPipeline>,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    surface_format: wgpu::TextureFormat,

    // Buffer scratch pour les uniforms custom
    scratch_buffer: wgpu::Buffer,
    scratch_buffer_size: u64,
    scratch_bind_group_layout: wgpu::BindGroupLayout,
    scratch_bind_group: Option<wgpu::BindGroup>,

    pub enabled: bool,
}

impl PostProcessPass {
    pub fn new(
        ctx: &GpuContext,
        gpu_resources: &GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
        surface_format: wgpu::TextureFormat,
    ) -> Result<Self, PassError> {
        let _span = tracing::info_span!("PostProcessPass::new").entered();

        let bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("PostProcess BindGroupLayout"),
                    entries: &[
                        // Texture source à traiter
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });
        // Scratch buffer pour les uniforms custom des matériaux (4KB)
        let scratch_buffer_size = 4096u64;
        let scratch_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Post-Process Pass Scratch Uniform Buffer"),
            size: scratch_buffer_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Layout pour les uniforms custom
        let scratch_bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Post-Process Pass Scratch BindGroup Layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
        let scratch_bind_group = Some(ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Post-Process scratch buffer bind group"),
            layout: &scratch_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &scratch_buffer,
                    offset: 0,
                    size: None, // TODO: Faire en sort d'exposer N byte en fonction du type de l'uniform
                }),
            }],
        }));

        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("PostProcess Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: f32::MAX,
            anisotropy_clamp: 1,
            compare: None,
            border_color: None,
        });

        let pipeline = Self::create_pipeline(
            ctx,
            gpu_resources,
            vert_shader,
            frag_shader,
            surface_format,
            &bind_group_layout,
            &scratch_bind_group_layout,
        )?;

        tracing::info!("Passe de rendu PostProcessPass initialisée avec succès");

        Ok(Self {
            vert_shader,
            frag_shader,
            pipeline: Arc::new(pipeline),
            bind_group_layout,
            bind_group: None,
            sampler,
            surface_format,
            scratch_bind_group,
            scratch_bind_group_layout,
            scratch_buffer,
            scratch_buffer_size,
            enabled: true,
        })
    }

    pub fn set_shader(
        &mut self,
        ctx: &GpuContext,
        gpu_resources: &GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<(), PassError> {
        let pipeline = Self::create_pipeline(
            ctx,
            gpu_resources,
            vert_shader,
            frag_shader,
            self.surface_format,
            &self.bind_group_layout,
            &self.scratch_bind_group_layout,
        )?;

        self.vert_shader = vert_shader;
        self.frag_shader = frag_shader;
        self.pipeline = Arc::new(pipeline);

        tracing::debug!(vs = %vert_shader, fs = %frag_shader, "Shaders de la PostProcessPass mis à jour");
        Ok(())
    }

    fn create_pipeline(
        ctx: &GpuContext,
        gpu_resources: &GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
        surface_format: wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
        scratch_goup_layout: &wgpu::BindGroupLayout,
    ) -> Result<wgpu::RenderPipeline, PassError> {
        let vs_module = gpu_resources.get_shader(vert_shader).ok_or_else(|| {
            tracing::error!(id = %vert_shader, "Vertex shader introuvable pour la PostProcessPass");
            crate::errors::PipelineError::ShaderNotFound { id: vert_shader }
        })?;

        let fs_module = gpu_resources.get_shader(frag_shader).ok_or_else(|| {
            tracing::error!(id = %frag_shader, "Fragment shader introuvable pour la PostProcessPass");
            crate::errors::PipelineError::ShaderNotFound { id: frag_shader }
        })?;

        let pipeline_layout_label =
            format!("PostProcess PipelineLayout (VS:{vert_shader}, FS:{frag_shader})");
        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&pipeline_layout_label),
                bind_group_layouts: &[Some(bind_group_layout), Some(scratch_goup_layout)],
                immediate_size: 0,
            });

        let pipeline_label = format!("PostProcess Pipeline (VS:{vert_shader}, FS:{frag_shader})");

        Ok(ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&pipeline_label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vs_module.module,
                    entry_point: Some("vs_main"),
                    buffers: &[], // Pas de vertex buffer : géométrie générée à la volée
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fs_module.module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            }))
    }

    // Helper pour écrire dans le scratch buffer sans l'exposer
    pub fn write_scratch_buffer<Data: bytemuck::Pod + bytemuck::Zeroable>(
        &self,
        ctx: &GpuContext,
        data: Data,
    ) {
        ctx.queue
            .write_buffer(&self.scratch_buffer, 0, bytemuck::bytes_of(&data));
    }
}

impl Pass for PostProcessPass {
    type Input<'a> = PostProcessInput<'a>;

    fn prepare<'a>(
        &mut self,
        ctx: &GpuContext,
        _gpu_resources: &mut GpuResources,
        input: &mut Self::Input<'a>,
    ) {
        let _span = tracing::trace_span!("PostProcessPass::prepare").entered();

        self.bind_group = Some(ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PostProcess BindGroup"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input.source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
    }

    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        _gpu_resources: &GpuResources,
    ) {
        let _span = tracing::trace_span!("PostProcessPass::execute").entered();
        if !self.enabled {
            // Pass non active on ignore l'éxecution
            return;
        }
        let Some(bind_group) = &self.bind_group else {
            tracing::warn!("Exécution de PostProcessPass ignorée : BindGroup non disponible");
            return;
        };
        let Some(scratch_bind_group) = &self.scratch_bind_group else {
            tracing::warn!(
                "Exécution de PostProcessPass ignorée : Scratch BindGroup non disponible"
            );
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("PostProcess Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_bind_group(1, scratch_bind_group, &[]);
        pass.draw(0..3, 0..1); // Triplet de sommets généré procéduralement dans le VS
    }
}
