use std::sync::Arc;
use utils::ids::{BufferId, ShaderId, TextureId};

use crate::{
    GpuResources,
    context::GpuContext,
    draw::commands::DrawCommand,
    errors::PassError,
    geometry::{mesh::RawMesh, shape::Shape, tesselator::Tesselator},
    pass::{Pass, WorldInput},
    resource::pipeline::{BlendMode, PipelineKey, PipelineManager, VertexFormat},
};

pub struct TextureBatch {
    index_offset: u32,
    index_count: u32,
    bind_group: wgpu::BindGroup,
}

pub struct WorldPass {
    vert_shader: ShaderId,
    frag_shader: ShaderId,
    vertex_buffer: BufferId,
    index_buffer: BufferId,
    vertex_buffer_size: u64,
    index_buffer_size: u64,
    index_count: u32,
    mesh: RawMesh,
    pipeline: Arc<wgpu::RenderPipeline>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    batches: Vec<TextureBatch>,
    texture_group_layout: wgpu::BindGroupLayout,
}

impl WorldPass {
    pub fn new(
        ctx: &GpuContext,
        gpu_resources: &mut GpuResources,
        pipelines: &mut PipelineManager,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<Self, PassError> {
        let _span = tracing::info_span!("WorldPass::new").entered();

        let index_buffer_size = 1024 * 12;
        let vertex_buffer_size = 1024 * 64;

        let index_buffer = gpu_resources.create_buffer(
            ctx,
            Some("WorldPass Index Buffer"),
            index_buffer_size,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        )?;

        let vertex_buffer = gpu_resources.create_buffer(
            ctx,
            Some("WorldPass Vertex Buffer"),
            vertex_buffer_size,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        )?;

        let camera_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WorldPass Camera Uniform Buffer"),
            size: std::mem::size_of::<utils::math::Mat4>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mesh = RawMesh::with_capacity(1024, 3072);
        let index_count = mesh.indices().len() as u32;

        let pipeline_key = PipelineKey {
            vertex_shader: vert_shader,
            fragment_shader: frag_shader,
            blend_mode: BlendMode::Alpha,
            vertex_format: VertexFormat::Pos2UvColor,
            bind_groups: &crate::TEXTURE_BIND_GROUP,
        };

        let pipeline = pipelines.get_or_create(ctx, gpu_resources, pipeline_key.clone())?;
        let layouts = pipelines.get_layouts(&pipeline_key).ok_or_else(|| {
            tracing::error!("Impossible de récupérer les BindGroupLayouts pour la WorldPass");
            PassError::LayoutsNotFound
        })?;

        let camera_layout = layouts.first().ok_or_else(|| {
            tracing::error!("BindGroupLayout (Camera) manquant à l'index 0 pour la WorldPass");
            PassError::LayoutsNotFound
        })?;

        let camera_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WorldPass Camera BindGroup"),
            layout: camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let batches = Vec::with_capacity(4096);
        let texture_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("WorldPass Texture BindGroupLayout"),
                    entries: &[
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        tracing::info!("Passe de rendu WorldPass initialisée avec succès");

        Ok(Self {
            vert_shader,
            frag_shader,
            vertex_buffer,
            index_buffer,
            vertex_buffer_size,
            index_buffer_size,
            index_count,
            mesh,
            pipeline,
            camera_buffer,
            camera_bind_group,
            batches,
            texture_group_layout,
        })
    }

    pub fn set_shader(
        &mut self,
        ctx: &GpuContext,
        pipelines: &mut PipelineManager,
        gpu_resources: &GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<(), PassError> {
        let pipeline = pipelines.get_or_create(
            ctx,
            gpu_resources,
            PipelineKey {
                vertex_shader: vert_shader,
                fragment_shader: frag_shader,
                blend_mode: BlendMode::Alpha,
                vertex_format: VertexFormat::Pos2UvColor,
                bind_groups: &crate::TEXTURE_BIND_GROUP,
            },
        )?;

        self.vert_shader = vert_shader;
        self.frag_shader = frag_shader;
        self.pipeline = pipeline;

        tracing::info!(vs = %vert_shader, fs = %frag_shader, "Shaders de la WorldPass mis à jour");
        Ok(())
    }

    /// Crée un BindGroup pour une texture donnée.
    /// Si la texture est introuvable, utilise la texture blanche par défaut pour éviter un crash.
    fn create_texture_bind_group(
        &self,
        ctx: &GpuContext,
        texture_id: TextureId,
        gpu_resources: &GpuResources,
    ) -> wgpu::BindGroup {
        let gpu_texture = gpu_resources
            .get_texture(texture_id)
            .or_else(|| {
                tracing::warn!(
                    id = %texture_id,
                    "Texture introuvable dans WorldPass, utilisation de la texture blanche de repli"
                );
                gpu_resources.get_texture(gpu_resources.white_texture())
            })
            .expect(
                "La texture blanche par défaut doit toujours être présente dans TextureManager",
            );

        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WorldPass Texture Batch BindGroup"),
            layout: &self.texture_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gpu_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&gpu_texture.sampler),
                },
            ],
        })
    }
}

impl Pass for WorldPass {
    type Input<'a> = WorldInput<'a>;

    fn prepare<'a>(
        &mut self,
        ctx: &GpuContext,
        gpu_resources: &mut GpuResources,
        input: &mut Self::Input<'a>,
    ) {
        let _span = tracing::trace_span!("WorldPass::prepare").entered();
        self.mesh.clear();
        self.batches.clear();

        ctx.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[input.camera]),
        );

        let white_id = gpu_resources.white_texture();

        let get_cmd_texture_id = |cmd: &DrawCommand| -> Option<TextureId> {
            match cmd {
                DrawCommand::Texture { id, .. } => Some(*id),
                DrawCommand::Text { .. } => None,
                _ => Some(white_id),
            }
        };

        let get_sort_key = |cmd: &DrawCommand| -> (u8, usize) {
            match cmd {
                DrawCommand::Texture { id, layer, .. } => (*layer, id.index),
                DrawCommand::Shape { layer, .. } => (*layer, white_id.index),
                DrawCommand::Mesh { layer, .. } => (*layer, white_id.index),
                DrawCommand::Text { layer, .. } => (*layer, usize::MAX),
                DrawCommand::Material {
                    layer, texture_id, ..
                } => (
                    *layer,
                    texture_id.map(|t| t.index).unwrap_or(white_id.index),
                ),
                DrawCommand::NinePatch { id, layer, .. } => (*layer, id.index),
            }
        };

        input
            .commands
            .commands_mut()
            .sort_by_key(|cmd| get_sort_key(cmd));

        let mut current_texture_id: Option<TextureId> = None;
        let mut batch_index_start = 0u32;

        for cmd in input.commands.commands() {
            let tex_id = match get_cmd_texture_id(cmd) {
                Some(id) => id,
                None => continue,
            };
            if current_texture_id != Some(tex_id) {
                if let Some(prev_id) = current_texture_id {
                    let index_count = self.mesh.indices().len() as u32 - batch_index_start;
                    if index_count > 0 {
                        let bind_group =
                            self.create_texture_bind_group(ctx, prev_id, gpu_resources);
                        self.batches.push(TextureBatch {
                            index_offset: batch_index_start,
                            index_count,
                            bind_group,
                        });
                    }
                }
                current_texture_id = Some(tex_id);
                batch_index_start = self.mesh.indices().len() as u32;
            }
            match cmd {
                DrawCommand::Shape { shape, .. } => Tesselator::tesselate(shape, &mut self.mesh),
                DrawCommand::Texture {
                    pos,
                    size,
                    rotation,
                    uv,
                    tint,
                    ..
                } => {
                    Tesselator::tesselate(
                        &Shape::Quad {
                            pos: *pos,
                            size: *size,
                            rotation: *rotation,
                            color: *tint,
                            uv: *uv,
                        },
                        &mut self.mesh,
                    );
                }
                DrawCommand::Mesh { mesh, .. } => {
                    self.mesh.append(mesh);
                }
                _ => (),
            }
        }

        if let Some(last_id) = current_texture_id {
            let index_count = self.mesh.indices().len() as u32 - batch_index_start;
            if index_count > 0 {
                let bind_group = self.create_texture_bind_group(ctx, last_id, gpu_resources);
                self.batches.push(TextureBatch {
                    index_offset: batch_index_start,
                    index_count,
                    bind_group,
                });
            }
        }
        let required_vertex_bytes = self.mesh.vertices().len() as u64
            * std::mem::size_of::<crate::geometry::mesh::Vertex>() as u64;

        if required_vertex_bytes > self.vertex_buffer_size {
            self.vertex_buffer_size = (self.vertex_buffer_size * 2).max(required_vertex_bytes);
            tracing::info!(
                new_size = self.vertex_buffer_size,
                "Agrandissement du Vertex Buffer dans WorldPass"
            );

            let former_buffer = self.vertex_buffer;
            if let Ok(new_buf) = gpu_resources.create_buffer(
                ctx,
                Some("WorldPass Vertex Buffer (Resized)"),
                self.vertex_buffer_size,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            ) {
                self.vertex_buffer = new_buf;
                let _ = gpu_resources.remove_buffer(former_buffer);
            }
        }
        let required_index_bytes =
            self.mesh.indices().len() as u64 * std::mem::size_of::<u32>() as u64;

        if required_index_bytes > self.index_buffer_size {
            self.index_buffer_size = (self.index_buffer_size * 2).max(required_index_bytes);
            tracing::info!(
                new_size = self.index_buffer_size,
                "Agrandissement de l'Index Buffer dans WorldPass"
            );

            let former_buffer = self.index_buffer;
            if let Ok(new_buf) = gpu_resources.create_buffer(
                ctx,
                Some("WorldPass Index Buffer (Resized)"),
                self.index_buffer_size,
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            ) {
                self.index_buffer = new_buf;
                let _ = gpu_resources.remove_buffer(former_buffer);
            }
        }

        self.index_count = self.mesh.indices().len() as u32;

        if let Err(err) =
            gpu_resources.write_buffer(ctx, self.vertex_buffer, self.mesh.vertices_bytes())
        {
            tracing::error!("Échec d'écriture dans le Vertex Buffer de WorldPass : {err}");
        }

        if let Err(err) =
            gpu_resources.write_buffer(ctx, self.index_buffer, self.mesh.indices_bytes())
        {
            tracing::error!("Échec d'écriture dans l'Index Buffer de WorldPass : {err}");
        }
    }

    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        gpu_resources: &GpuResources,
    ) {
        if self.index_count == 0 {
            return;
        }

        let index_buffer = match gpu_resources.get_buffer(self.index_buffer) {
            Some(b) => b,
            None => {
                tracing::error!(
                    "Index Buffer introuvable dans WorldPass (ID : %{})",
                    self.index_buffer
                );
                return;
            }
        };

        let vertex_buffer = match gpu_resources.get_buffer(self.vertex_buffer) {
            Some(b) => b,
            None => {
                tracing::error!(
                    "Vertex Buffer introuvable dans WorldPass (ID : %{})",
                    self.vertex_buffer
                );
                return;
            }
        };

        let mut world_render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("World Render Pass"),
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

        world_render_pass.set_pipeline(&self.pipeline);
        world_render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        world_render_pass
            .set_index_buffer(index_buffer.buffer.slice(..), wgpu::IndexFormat::Uint32);
        world_render_pass.set_vertex_buffer(0, vertex_buffer.buffer.slice(..));

        for batch in &self.batches {
            world_render_pass.set_bind_group(1, &batch.bind_group, &[]);
            world_render_pass.draw_indexed(
                batch.index_offset..batch.index_offset + batch.index_count,
                0,
                0..1,
            );
        }
    }
}
