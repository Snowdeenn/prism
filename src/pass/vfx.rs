use std::sync::Arc;
use utils::ids::{BufferId, ShaderId};

use crate::{
    GpuResources,
    context::GpuContext,
    draw::commands::DrawCommand,
    errors::PassError,
    geometry::{mesh::RawMesh, shape::Shape, tesselator::Tesselator},
    pass::{Pass, VfxInput},
    resource::pipeline::{BlendMode, PipelineKey, PipelineManager, VertexFormat},
};

pub struct VfxPass {
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
}

impl VfxPass {
    pub fn new(
        ctx: &GpuContext,
        gpu_resource: &mut GpuResources,
        pipelines: &mut PipelineManager,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<Self, PassError> {
        let _span = tracing::info_span!("VfxPass::new").entered();

        let index_buffer_size = 1024 * 12;
        let vertex_buffer_size = 1024 * 64;

        let index_buffer = gpu_resource.create_buffer(
            ctx,
            Some("VfxPass Index Buffer"),
            index_buffer_size,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        )?;

        let vertex_buffer = gpu_resource.create_buffer(
            ctx,
            Some("VfxPass Vertex Buffer"),
            vertex_buffer_size,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        )?;

        let camera_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VfxPass Camera Uniform Buffer"),
            size: std::mem::size_of::<utils::math::Mat4>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mesh = RawMesh::with_capacity(1024, 3072);
        let index_count = mesh.indices().len() as u32;

        let pipeline_key = PipelineKey {
            vertex_shader: vert_shader,
            fragment_shader: frag_shader,
            blend_mode: BlendMode::Additive,
            vertex_format: VertexFormat::Pos2UvColor,
            bind_groups: &crate::CAM_BIND_GROUP,
        };

        let pipeline = pipelines.get_or_create(ctx, gpu_resource, pipeline_key.clone())?;
        let layouts = pipelines.get_layouts(&pipeline_key).ok_or_else(|| {
            tracing::error!("Impossible de récupérer les BindGroupLayouts pour la VfxPass");
            PassError::LayoutsNotFound
        })?;

        let camera_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("VfxPass Camera BindGroup"),
            layout: &layouts[0],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        tracing::info!("Passe de rendu VfxPass initialisée avec succès");

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
                blend_mode: BlendMode::Additive,
                vertex_format: VertexFormat::Pos2UvColor,
                bind_groups: &crate::CAM_BIND_GROUP,
            },
        )?;

        self.vert_shader = vert_shader;
        self.frag_shader = frag_shader;
        self.pipeline = pipeline;

        tracing::debug!(vs = %vert_shader, fs = %frag_shader, "Shaders de la VfxPass mis à jour");
        Ok(())
    }
}

impl Pass for VfxPass {
    type Input<'a> = VfxInput<'a>;

    fn prepare<'a>(
        &mut self,
        ctx: &GpuContext,
        gpu_resources: &mut GpuResources,
        input: &mut Self::Input<'a>,
    ) {
        let _span = tracing::trace_span!("VfxPass::prepare").entered();

        self.mesh.clear();
        ctx.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[input.camera]),
        );

        for cmd in input.commands.commands() {
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

        // Redimensionnement dynamique du Vertex Buffer
        let required_vertex_bytes = self.mesh.vertices().len() as u64
            * std::mem::size_of::<crate::geometry::mesh::Vertex>() as u64;

        if required_vertex_bytes > self.vertex_buffer_size {
            self.vertex_buffer_size = (self.vertex_buffer_size * 2).max(required_vertex_bytes);
            tracing::info!(
                new_size = self.vertex_buffer_size,
                "Agrandissement du Vertex Buffer dans VfxPass"
            );

            let former_buffer = self.vertex_buffer;
            if let Ok(new_buf) = gpu_resources.create_buffer(
                ctx,
                Some("VfxPass Vertex Buffer (Resized)"),
                self.vertex_buffer_size,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            ) {
                self.vertex_buffer = new_buf;
                let _ = gpu_resources.remove_buffer(former_buffer);
            }
        }

        // Redimensionnement dynamique de l'Index Buffer
        let required_index_bytes =
            self.mesh.indices().len() as u64 * std::mem::size_of::<u32>() as u64;

        if required_index_bytes > self.index_buffer_size {
            self.index_buffer_size = (self.index_buffer_size * 2).max(required_index_bytes);
            tracing::info!(
                new_size = self.index_buffer_size,
                "Agrandissement de l'Index Buffer dans VfxPass"
            );

            let former_buffer = self.index_buffer;
            if let Ok(new_buf) = gpu_resources.create_buffer(
                ctx,
                Some("VfxPass Index Buffer (Resized)"),
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
            tracing::error!("Échec d'écriture dans le Vertex Buffer de VfxPass : {err}");
        }

        if let Err(err) =
            gpu_resources.write_buffer(ctx, self.index_buffer, self.mesh.indices_bytes())
        {
            tracing::error!("Échec d'écriture dans l'Index Buffer de VfxPass : {err}");
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
                    "Index Buffer introuvable dans VfxPass (ID : %{})",
                    self.index_buffer
                );
                return;
            }
        };

        let vertex_buffer = match gpu_resources.get_buffer(self.vertex_buffer) {
            Some(b) => b,
            None => {
                tracing::error!(
                    "Vertex Buffer introuvable dans VfxPass (ID : %{})",
                    self.vertex_buffer
                );
                return;
            }
        };

        let mut vfx_render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Vfx Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        vfx_render_pass.set_pipeline(&self.pipeline);
        vfx_render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        vfx_render_pass.set_index_buffer(index_buffer.buffer.slice(..), wgpu::IndexFormat::Uint32);
        vfx_render_pass.set_vertex_buffer(0, vertex_buffer.buffer.slice(..));
        vfx_render_pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}
