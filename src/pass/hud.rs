use std::sync::Arc;

use crate::{
    GpuResources, PassError, TextureManager,
    context::GpuContext,
    draw::{commands::DrawCommand, text::TextRenderer},
    geometry::{mesh::RawMesh, shape::Shape, tesselator::Tesselator},
    pass::{HudInput, Pass},
    resource::pipeline::{BlendMode, PipelineKey, PipelineManager, VertexFormat},
};
use utils::ids::{BufferId, MaterialId, ShaderId, TextureId};

enum HudBatch {
    Standard {
        index_offset: u32,
        index_count: u32,
        texture_bind_group: wgpu::BindGroup,
    },
    Material {
        index_offset: u32,
        index_count: u32,
        material_id: MaterialId,
        texture_bind_group: wgpu::BindGroup,
        uniform_offset: u32, // offset dans le scratch buffer, passé à set_bind_group
    },
}

pub struct HudPass {
    vert_shader: ShaderId,
    frag_shader: ShaderId,

    vertex_buffer: BufferId,
    index_buffer: BufferId,
    vertex_buffer_size: u64,
    index_buffer_size: u64,

    // Buffer scratch pour les uniforms custom des matériaux
    scratch_buffer: wgpu::Buffer,
    scratch_buffer_size: u64,
    scratch_bind_group_layout: wgpu::BindGroupLayout,

    mesh: RawMesh,
    default_pipeline: Arc<wgpu::RenderPipeline>,
    texture_bind_group_layout: wgpu::BindGroupLayout,

    // Bind group caméra — HudPass utilise une matrice orthographique écran
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    // Un seul bind group pour tout le scratch buffer — l'offset est dynamique
    scratch_bind_group: wgpu::BindGroup,
    batches: Vec<HudBatch>,
    text_renderer: TextRenderer,
    has_text: bool,
}

impl HudPass {
    pub fn new(
        ctx: &GpuContext,
        gpu_resources: &mut GpuResources,
        pipelines: &mut PipelineManager,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
        surface_format: wgpu::TextureFormat,
    ) -> Result<Self, PassError> {
        let _span = tracing::info_span!("HudPass::new").entered();

        let index_buffer_size = 1024 * 12;
        let vertex_buffer_size = 1024 * 64;

        let index_buffer = gpu_resources
            .create_buffer(
                ctx,
                Some("Index Buffer Hud Pass"),
                index_buffer_size,
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            )
            .map_err(|e| {
                tracing::error!("Échec de création de l'index buffer pour HudPass : {e:?}");
                e
            })?;

        let vertex_buffer = gpu_resources
            .create_buffer(
                ctx,
                Some("Vertex Buffer Hud Pass"),
                vertex_buffer_size,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            )
            .map_err(|e| {
                tracing::error!("Échec de création du vertex buffer pour HudPass : {e:?}");
                e
            })?;

        // Scratch buffer pour les uniforms custom des matériaux (4KB)
        let scratch_buffer_size = 4096u64;
        let scratch_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HudPass Scratch Uniform Buffer"),
            size: scratch_buffer_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Layout pour les uniforms custom group(2)
        let scratch_bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("HudPass Scratch BindGroup Layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true, // offset dynamique par matériau
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        // Layout texture group(1)
        let texture_bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("HudPass Texture BindGroup Layout"),
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

        // Camera buffer group(0)
        let camera_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HudPass Camera Uniform"),
            size: std::mem::size_of::<utils::math::Mat4>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mesh = RawMesh::with_capacity(1024, 3072);

        let pipeline_key = PipelineKey {
            vertex_shader: vert_shader,
            fragment_shader: frag_shader,
            blend_mode: BlendMode::Alpha,
            vertex_format: VertexFormat::Pos2UvColor,
            bind_groups: &crate::TEXTURE_BIND_GROUP,
        };

        let default_pipeline = pipelines
            .get_or_create(ctx, gpu_resources, pipeline_key.clone())
            .map_err(|e| {
                tracing::error!(vs = %vert_shader, fs = %frag_shader, "Échec de création de la pipeline HudPass : {e:?}");
                e
            })?;

        let layouts = pipelines.get_layouts(&pipeline_key).ok_or_else(|| {
            tracing::error!("Impossible d'obtenir les BindGroupLayouts pour la HudPass");
            PassError::LayoutsNotFound
        })?;

        let camera_layout = layouts.first().ok_or_else(|| {
            tracing::error!("BindGroupLayout (Camera) manquant à l'index 0 pour la HudPass");
            PassError::LayoutsNotFound
        })?;

        let camera_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HudPass Camera BindGroup"),
            layout: camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Un seul bind group pour tout le scratch buffer
        let scratch_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HudPass Scratch BindGroup"),
            layout: &scratch_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &scratch_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(256), // taille minimale — l'offset dynamique gère le reste
                }),
            }],
        });

        let text_renderer = TextRenderer::new(ctx, surface_format);

        tracing::info!("Passe de rendu HudPass initialisée avec succès");
        Ok(Self {
            vert_shader,
            frag_shader,
            vertex_buffer,
            index_buffer,
            vertex_buffer_size,
            index_buffer_size,
            scratch_buffer,
            scratch_buffer_size,
            scratch_bind_group_layout,
            texture_bind_group_layout,
            mesh,
            default_pipeline,
            camera_buffer,
            camera_bind_group,
            scratch_bind_group,
            batches: Vec::new(),
            text_renderer,
            has_text: false,
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
        let pipeline = pipelines
            .get_or_create(
                ctx,
                gpu_resources,
                PipelineKey {
                    vertex_shader: vert_shader,
                    fragment_shader: frag_shader,
                    blend_mode: BlendMode::Alpha,
                    vertex_format: VertexFormat::Pos2UvColor,
                    bind_groups: &crate::TEXTURE_BIND_GROUP,
                },
            )
            .map_err(|err| {
                tracing::error!(vs = %vert_shader, fs = %frag_shader, "Échec du changement de shader pour HudPass : {err:?}");
                err
            })?;

        self.default_pipeline = pipeline;
        self.vert_shader = vert_shader;
        self.frag_shader = frag_shader;
        Ok(())
    }

    fn create_texture_bind_group(
        &self,
        ctx: &GpuContext,
        texture_id: TextureId,
        textures: &TextureManager,
    ) -> Option<wgpu::BindGroup> {
        let gpu_tex = match textures.get(texture_id) {
            Some(tex) => tex,
            None => {
                tracing::error!(id = %texture_id, "[HudPass] TextureId introuvable pour la création du BindGroup");
                return None;
            }
        };

        Some(ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HudPass Texture BindGroup"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gpu_tex.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&gpu_tex.sampler),
                },
            ],
        }))
    }

    fn create_uniform_bind_group(&self, ctx: &GpuContext, size: u64) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HudPass Uniform BindGroup"),
            layout: &self.scratch_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &self.scratch_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size),
                }),
            }],
        })
    }
}

fn align256(size: usize) -> usize {
    (size + 255) & !255
}

impl Pass for HudPass {
    type Input<'a> = HudInput<'a>;

    fn prepare<'a>(
        &mut self,
        ctx: &GpuContext,
        gpu_resources: &mut GpuResources,
        input: &mut Self::Input<'a>,
    ) {
        self.mesh.clear();
        self.batches.clear();

        // Upload caméra
        ctx.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[input.camera]),
        );

        let white_id = gpu_resources.white_texture();
        self.has_text = input
            .commands
            .commands()
            .iter()
            .any(|cmd| matches!(cmd, DrawCommand::Text { .. }));

        input.commands.sort_commands();

        let mut current_texture_id: Option<TextureId> = None;
        let mut current_material_id: Option<MaterialId> = None;
        let mut current_uniform_offset: u32 = 0; // offset du batch Material courant
        let mut batch_index_start: u32 = 0;
        let mut scratch_offset: usize = 0;
        let mut scratch_data: Vec<u8> = Vec::new();

        // Helper pour créer un texture bind group de manière sécurisée
        let make_tex_bg = |ctx: &GpuContext,
                           layout: &wgpu::BindGroupLayout,
                           gpu_resources: &GpuResources,
                           tex_id: TextureId|
         -> Option<wgpu::BindGroup> {
            let gpu_tex = match gpu_resources.get_texture(tex_id) {
                Some(t) => t,
                None => {
                    tracing::error!(id = %tex_id, "[HudPass] Texture introuvable pendant la création du batch");
                    return None;
                }
            };

            Some(ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("HudPass Texture BindGroup"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&gpu_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&gpu_tex.sampler),
                    },
                ],
            }))
        };

        for cmd in input.commands.commands() {
            // Résoudre texture_id et material_id effectifs
            let (tex_id, mat_id, uniform_data) = match cmd {
                DrawCommand::Shape { .. } | DrawCommand::Mesh { .. } => (white_id, None, None),
                DrawCommand::Texture { id, .. } => (*id, None, None),
                DrawCommand::Material {
                    texture_id,
                    material_id,
                    uniform_data,
                    ..
                } => (
                    texture_id.unwrap_or(white_id),
                    Some(*material_id),
                    Some(uniform_data.as_slice()),
                ),
                DrawCommand::NinePatch { id, .. } => (*id, None, None),
                DrawCommand::Text { .. } => continue,
            };

            let batch_changed = current_texture_id != Some(tex_id) || current_material_id != mat_id;

            if batch_changed {
                // Clore le batch précédent
                if let Some(prev_tex) = current_texture_id {
                    let index_count = self.mesh.indices().len() as u32 - batch_index_start;
                    if index_count > 0 {
                        if let Some(texture_bind_group) = make_tex_bg(
                            ctx,
                            &self.texture_bind_group_layout,
                            gpu_resources,
                            prev_tex,
                        ) {
                            match current_material_id {
                                None => self.batches.push(HudBatch::Standard {
                                    index_offset: batch_index_start,
                                    index_count,
                                    texture_bind_group,
                                }),
                                Some(material_id) => self.batches.push(HudBatch::Material {
                                    index_offset: batch_index_start,
                                    index_count,
                                    material_id,
                                    texture_bind_group,
                                    uniform_offset: current_uniform_offset,
                                }),
                            }
                        }
                    }
                }

                // Ouvrir le nouveau batch
                current_texture_id = Some(tex_id);
                current_material_id = mat_id;
                batch_index_start = self.mesh.indices().len() as u32;

                // Si c'est un Material avec des uniforms, écrire dans scratch et noter l'offset
                if let Some(data) = uniform_data {
                    current_uniform_offset = scratch_offset as u32;
                    scratch_data.extend_from_slice(data);
                    let aligned = align256(data.len());
                    scratch_data.resize(scratch_offset + aligned, 0);
                    scratch_offset += aligned;
                }
            }

            match cmd {
                DrawCommand::Shape { shape, .. } => {
                    Tesselator::tesselate(shape, &mut self.mesh);
                }
                DrawCommand::Texture {
                    pos,
                    size,
                    rotation,
                    uv,
                    tint,
                    ..
                }
                | DrawCommand::Material {
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
                DrawCommand::NinePatch {
                    pos,
                    size,
                    texture_size,
                    margins,
                    tint,
                    ..
                } => {
                    Tesselator::tesselate(
                        &Shape::NinePatch {
                            pos: *pos,
                            size: *size,
                            texture_size: *texture_size,
                            margins: *margins,
                            color: *tint,
                        },
                        &mut self.mesh,
                    );
                }
                DrawCommand::Text { .. } => (),
            }
        }

        // Clore le dernier batch
        if let Some(prev_tex) = current_texture_id {
            let index_count = self.mesh.indices().len() as u32 - batch_index_start;
            if index_count > 0 {
                if let Some(gpu_tex) = gpu_resources.get_texture(prev_tex) {
                    let texture_bind_group =
                        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("HudPass Texture BindGroup"),
                            layout: &self.texture_bind_group_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(&gpu_tex.view),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(&gpu_tex.sampler),
                                },
                            ],
                        });

                    match current_material_id {
                        None => self.batches.push(HudBatch::Standard {
                            index_offset: batch_index_start,
                            index_count,
                            texture_bind_group,
                        }),
                        Some(material_id) => self.batches.push(HudBatch::Material {
                            index_offset: batch_index_start,
                            index_count,
                            material_id,
                            texture_bind_group,
                            uniform_offset: current_uniform_offset,
                        }),
                    }
                } else {
                    tracing::error!(id = %prev_tex, "[HudPass] Texture introuvable lors de la clôture du dernier batch");
                }
            }
        }

        // Upload scratch buffer en une seule fois
        if !scratch_data.is_empty() {
            ctx.queue
                .write_buffer(&self.scratch_buffer, 0, &scratch_data);
        }

        // Text renderer
        if self.has_text {
            if let Err(err) = self.text_renderer.prepare(ctx, input.commands.commands()) {
                tracing::error!("Échec de la préparation du TextRenderer dans HudPass : {err:?}");
            }
        }

        // Resize vertex buffer
        let required_vertex_bytes = self.mesh.vertices().len() as u64
            * std::mem::size_of::<crate::geometry::mesh::Vertex>() as u64;

        if required_vertex_bytes > self.vertex_buffer_size {
            self.vertex_buffer_size = (self.vertex_buffer_size * 2).max(required_vertex_bytes);
            tracing::info!(
                new_size = self.vertex_buffer_size,
                "Agrandissement du Vertex Buffer dans HudPass"
            );
            let former = self.vertex_buffer;
            match gpu_resources.create_buffer(
                ctx,
                Some("Vertex Buffer Hud Pass resized"),
                self.vertex_buffer_size,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            ) {
                Ok(new_buf) => {
                    self.vertex_buffer = new_buf;
                    if let Err(e) = gpu_resources.remove_buffer(former) {
                        tracing::error!("Echec lors de la suppression du buffer : {e}");
                    }
                }
                Err(err) => {
                    tracing::error!(
                        "Échec du redimensionnement du Vertex Buffer dans HudPass : {err:?}"
                    );
                }
            }
        }

        // Resize index buffer
        let required_index_bytes = self.mesh.vertices().len() as u64
            * std::mem::size_of::<crate::geometry::mesh::Vertex>() as u64;

        if required_index_bytes > self.index_buffer_size {
            self.vertex_buffer_size = (self.vertex_buffer_size * 2).max(required_index_bytes);
            tracing::info!(
                new_size = self.vertex_buffer_size,
                "Agrandissement de l'index Buffer dans HudPass"
            );
            let former = self.index_buffer;
            match gpu_resources.create_buffer(
                ctx,
                Some("Index Buffer Hud Pass resized"),
                self.index_buffer_size,
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            ) {
                Ok(new_buf) => {
                    self.index_buffer = new_buf;
                    if let Err(e) = gpu_resources.remove_buffer(former) {
                        tracing::error!("Echec lors de la suppression du buffer : {e}");
                    }
                }
                Err(err) => {
                    tracing::error!(
                        "Échec du redimensionnement de l'Index Buffer dans HudPass : {err:?}"
                    );
                }
            }
        }

        if let Err(e) =
            gpu_resources.write_buffer(ctx, self.vertex_buffer, self.mesh.vertices_bytes())
        {
            tracing::error!("Impossible d'ecrire dans le buffer : {e}");
            return;
        }
        if let Err(e) =
            gpu_resources.write_buffer(ctx, self.index_buffer, self.mesh.indices_bytes())
        {
            tracing::error!("Impossible d'ecrire dans le buffer : {e}");
            return;
        }
    }

    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        gpu_resources: &GpuResources,
    ) {
        let has_geometry = !self.batches.is_empty();
        // La render pass doit avoir des vertex a dessiner sinon on skip
        if !has_geometry && !self.has_text {
            return;
        }

        let Some(index_buffer) = gpu_resources.get_buffer(self.index_buffer) else {
            tracing::error!(id = ?self.index_buffer, "[HudPass] Index buffer introuvable dans GpuBufferManager");
            return;
        };

        let Some(vertex_buffer) = gpu_resources.get_buffer(self.vertex_buffer) else {
            tracing::error!(id = ?self.vertex_buffer, "[HudPass] Vertex buffer introuvable dans GpuBufferManager");
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Hud Render Pass"),
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

        pass.set_index_buffer(index_buffer.buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_vertex_buffer(0, vertex_buffer.buffer.slice(..));
        pass.set_bind_group(0, &self.camera_bind_group, &[]);

        let mut current_pipeline_is_default = false;
        let mut current_material: Option<MaterialId> = None;

        for batch in &self.batches {
            match batch {
                HudBatch::Standard {
                    index_offset,
                    index_count,
                    texture_bind_group,
                } => {
                    if !current_pipeline_is_default {
                        pass.set_pipeline(&self.default_pipeline);
                        current_pipeline_is_default = true;
                        current_material = None;
                    }
                    pass.set_bind_group(1, texture_bind_group, &[]);
                    pass.draw_indexed(*index_offset..*index_offset + *index_count, 0, 0..1);
                }
                HudBatch::Material {
                    index_offset,
                    index_count,
                    material_id,
                    texture_bind_group,
                    uniform_offset,
                } => {
                    if current_material != Some(*material_id) {
                        if let Some(mat) = gpu_resources.get_material(*material_id) {
                            pass.set_pipeline(&mat.pipeline);
                            current_pipeline_is_default = false;
                            current_material = Some(*material_id);
                        } else {
                            tracing::warn!(id = %material_id, "[HudPass] Matériau introuvable lors de l'exécution du batch");
                        }
                    }
                    pass.set_bind_group(1, texture_bind_group, &[]);
                    // Dynamic offset : un seul bind group, offset variable par batch
                    pass.set_bind_group(2, &self.scratch_bind_group, &[*uniform_offset]);
                    pass.draw_indexed(*index_offset..*index_offset + *index_count, 0, 0..1);
                }
            }
        }

        if self.has_text {
            if let Err(err) = self.text_renderer.render(&mut pass) {
                tracing::error!("Échec du rendu du TextRenderer dans HudPass : {err:?}");
            }
        }
    }
}
