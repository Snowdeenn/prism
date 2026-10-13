use std::sync::Arc;

use utils::ids::ShaderId;

use crate::{FrameManager, pass::Pass};

pub struct Renderer {
    pipelines: crate::PipelineManager,
    graph_renderer: crate::GraphRenderer,
    world: crate::WorldPass,
    vfx: crate::VfxPass,
    hud: crate::HudPass,
    post_process_passes: Vec<crate::PostProcessPass>,
    intermediate_a: wgpu::Texture,
    intermediate_view_a: wgpu::TextureView,
    intermediate_b: wgpu::Texture,
    intermediate_view_b: wgpu::TextureView,
    current_source: DoubleBufferIndex,
    frame_manager: crate::FrameManager,
}

impl Renderer {
    pub fn new(
        ctx: &crate::GpuContext,
        gpu_resources: &mut crate::GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
        text_vert_id: ShaderId,
        text_frag_id: ShaderId,
    ) -> crate::Result<Self> {
        let mut pipelines = crate::PipelineManager::new(ctx.surface_format());
        let (intermediate_a, intermediate_view_a) = Self::create_intermediate(ctx);
        let (intermediate_b, intermediate_view_b) = Self::create_intermediate(ctx);

        let world = crate::WorldPass::new(
            ctx,
            gpu_resources,
            &mut pipelines,
            text_vert_id,
            text_frag_id,
        )?;
        let vfx =
            crate::VfxPass::new(ctx, gpu_resources, &mut pipelines, vert_shader, frag_shader)?;
        let hud = crate::HudPass::new(
            ctx,
            gpu_resources,
            &mut pipelines,
            vert_shader,
            frag_shader,
            ctx.surface_format(),
        )?;
        let post_process_passes = vec![];
        let frame_manager = crate::FrameManager::new();
        Ok(Self {
            pipelines,
            graph_renderer: crate::GraphRenderer::new(&ctx.device, &ctx.queue),
            world,
            vfx,
            hud,
            post_process_passes,
            intermediate_a,
            intermediate_view_a,
            intermediate_b,
            intermediate_view_b,
            current_source: DoubleBufferIndex::Primary,
            frame_manager,
        })
    }

    pub fn render(
        &mut self,
        ctx: &mut crate::GpuContext,
        gpu_resources: &mut crate::GpuResources,
        mut frame: crate::Frame,
    ) -> Option<crate::FrameContext> {
        if !ctx.is_ready() {
            return None;
        }
        if self.intermediate_a.width() != ctx.size.width
            || self.intermediate_a.height() != ctx.size.height
        {
            let (a, av) = Self::create_intermediate(ctx);
            let (b, bv) = Self::create_intermediate(ctx);
            self.intermediate_a = a;
            self.intermediate_view_a = av;
            self.intermediate_b = b;
            self.intermediate_view_b = bv;
        }
        let refreshed = (|| -> Result<(), crate::PassError> {
            self.world
                .refresh_pipeline(ctx, &mut self.pipelines, gpu_resources)?;
            self.vfx
                .refresh_pipeline(ctx, &mut self.pipelines, gpu_resources)?;
            self.hud
                .refresh_pipeline(ctx, &mut self.pipelines, gpu_resources)?;
            for pass in self.post_process_passes.iter_mut().filter(|p| p.enabled) {
                pass.refresh_pipeline(ctx, gpu_resources)?;
            }
            Ok(())
        })();
        if let Err(error) = refreshed {
            tracing::error!(%error, "Cannot prepare renderer pipelines");
            return None;
        }
        let surface_texture = match ctx.current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                tracing::warn!("Surface GPU suboptimale");
                t
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                tracing::trace!("Fenêtre occultée ou minimisée, rendu ignoré");
                return None;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                tracing::warn!("Surface GPU obsolète ou perdue, redimensionnement...");
                let size = ctx.size;
                ctx.resize(size.width, size.height);
                return None;
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                tracing::warn!("Timeout lors de la récupération de la frame (frame sautée)");
                return None;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                tracing::error!("Erreur de validation lors de la récupération de la texture");
                return None;
            }
        };

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.current_source = DoubleBufferIndex::Primary;
        let mut encoder = ctx.create_encoder("frame");

        let screen_w = ctx.size.width;
        let screen_h = ctx.size.height;
        self.prepare_legacy(ctx, gpu_resources, &mut frame);

        if let Err(error) = encode_legacy_graph(
            ctx,
            gpu_resources,
            &mut encoder,
            &surface_texture.texture,
            &surface_view,
            [&self.intermediate_a, &self.intermediate_b],
            [&self.intermediate_view_a, &self.intermediate_view_b],
            &mut self.world,
            &mut self.vfx,
            &mut self.hud,
            &mut self.post_process_passes,
        ) {
            tracing::error!(%error, "Cannot compile/encode frame graph");
            return None;
        }

        Some(crate::FrameContext {
            encoder,
            surface_view,
            surface_texture,
            size: (screen_w, screen_h),
        })
    }

    fn prepare_legacy(
        &mut self,
        ctx: &crate::GpuContext,
        gpu_resources: &mut crate::GpuResources,
        frame: &mut crate::Frame,
    ) {
        let screen_w = ctx.size.width;
        let screen_h = ctx.size.height;
        let cam_matrix = build_camera_matrix(
            frame.camera_pos,
            frame.cam_shake_offset,
            screen_w as f32,
            screen_h as f32,
        );

        self.world.prepare(
            ctx,
            gpu_resources,
            &mut crate::WorldInput {
                commands: &mut frame.world,
                camera: cam_matrix,
            },
        );
        self.vfx.prepare(
            ctx,
            gpu_resources,
            &mut crate::VfxInput {
                commands: &frame.vfx,
                camera: cam_matrix,
            },
        );
        let w = (ctx.size.width as f32).max(1.0);
        let h = (ctx.size.height as f32).max(1.0);

        // left = 0.0, right = w, bottom = h, top = 0.0, near = -1.0, far = 1.0
        let hud_camera = utils::math::Mat4::orthographic_wgpu(0.0, w, h, 0.0, -1.0, 1.0);
        self.hud.prepare(
            ctx,
            gpu_resources,
            &mut crate::HudInput {
                commands: &mut frame.hud,
                camera: hud_camera,
            },
        );
    }

    /// Encode the compatibility API into an offscreen texture using the same graph.
    pub fn render_offscreen(
        &mut self,
        ctx: &crate::GpuContext,
        resources: &mut crate::GpuResources,
        mut frame: crate::Frame,
        target: &wgpu::Texture,
    ) -> crate::RenderResult<wgpu::CommandBuffer> {
        if target.width() != ctx.size.width
            || target.height() != ctx.size.height
            || target.format() != ctx.surface_format()
        {
            return Err(crate::RenderError::new(
                "offscreen",
                "target differs from context size/format",
            ));
        }
        if self.intermediate_a.size() != target.size() {
            (self.intermediate_a, self.intermediate_view_a) = Self::create_intermediate(ctx);
            (self.intermediate_b, self.intermediate_view_b) = Self::create_intermediate(ctx);
        }
        self.world
            .refresh_pipeline(ctx, &mut self.pipelines, resources)
            .map_err(|e| crate::RenderError::new("world", e.to_string()))?;
        self.vfx
            .refresh_pipeline(ctx, &mut self.pipelines, resources)
            .map_err(|e| crate::RenderError::new("vfx", e.to_string()))?;
        self.hud
            .refresh_pipeline(ctx, &mut self.pipelines, resources)
            .map_err(|e| crate::RenderError::new("hud", e.to_string()))?;
        for pass in self.post_process_passes.iter_mut().filter(|p| p.enabled) {
            pass.refresh_pipeline(ctx, resources)
                .map_err(|e| crate::RenderError::new("post process", e.to_string()))?;
        }
        self.prepare_legacy(ctx, resources, &mut frame);
        let mut encoder = ctx.create_encoder("legacy offscreen");
        let view = target.create_view(&Default::default());
        encode_legacy_graph(
            ctx,
            resources,
            &mut encoder,
            target,
            &view,
            [&self.intermediate_a, &self.intermediate_b],
            [&self.intermediate_view_a, &self.intermediate_view_b],
            &mut self.world,
            &mut self.vfx,
            &mut self.hud,
            &mut self.post_process_passes,
        )?;
        Ok(encoder.finish())
    }

    fn create_intermediate(ctx: &crate::GpuContext) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Intermediate Target"),
            size: wgpu::Extent3d {
                width: ctx.size.width.max(1),
                height: ctx.size.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ctx.surface_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub fn resize(&mut self, ctx: &mut crate::GpuContext, width: u32, height: u32) {
        ctx.resize(width, height);
        if width == 0 || height == 0 {
            return;
        }
        let (intermediate_a, intermediate_view_a) = Self::create_intermediate(ctx);
        let (intermediate_b, intermediate_view_b) = Self::create_intermediate(ctx);
        self.intermediate_a = intermediate_a;
        self.intermediate_view_a = intermediate_view_a;
        self.intermediate_b = intermediate_b;
        self.intermediate_view_b = intermediate_view_b;
    }

    pub fn set_world_shaders(
        &mut self,
        ctx: &crate::GpuContext,
        gpu_resources: &crate::GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<(), crate::PrismError> {
        self.world.set_shader(
            ctx,
            &mut self.pipelines,
            gpu_resources,
            vert_shader,
            frag_shader,
        )?;
        Ok(())
    }

    pub fn set_vfx_shaders(
        &mut self,
        ctx: &crate::GpuContext,
        gpu_resources: &crate::GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<(), crate::PrismError> {
        self.vfx.set_shader(
            ctx,
            &mut self.pipelines,
            gpu_resources,
            vert_shader,
            frag_shader,
        )?;
        Ok(())
    }

    pub fn set_hud_shaders(
        &mut self,
        ctx: &crate::GpuContext,
        gpu_resources: &crate::GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<(), crate::PrismError> {
        self.hud.set_shader(
            ctx,
            &mut self.pipelines,
            gpu_resources,
            vert_shader,
            frag_shader,
        )?;
        Ok(())
    }

    pub fn set_post_process_shaders(
        &mut self,
        ctx: &crate::GpuContext,
        gpu_resources: &crate::GpuResources,
        index: usize,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<(), crate::PrismError> {
        if let Some(pass) = self.post_process_passes.get_mut(index) {
            pass.set_shader(ctx, gpu_resources, vert_shader, frag_shader)?;
            Ok(())
        } else {
            tracing::error!(
                index,
                total_passes = self.post_process_passes.len(),
                "Impossible de changer les shaders : index de post-process hors limites"
            );
            Ok(())
        }
    }

    pub fn add_post_process_pass<Data: bytemuck::Pod + bytemuck::Zeroable>(
        &mut self,
        ctx: &crate::GpuContext,
        gpu_resources: &crate::GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
        uniform_data: Option<Data>,
    ) -> Result<crate::PostProcessPassId, crate::PrismError> {
        let pass = crate::PostProcessPass::new(
            ctx,
            gpu_resources,
            vert_shader,
            frag_shader,
            ctx.surface_format(),
        )?;
        if let Some(uniform_data) = uniform_data {
            pass.write_scratch_buffer(ctx, uniform_data);
        }
        let id = crate::PostProcessPassId(self.post_process_passes.len());
        self.post_process_passes.push(pass);
        Ok(id)
    }

    pub fn write_post_process_uniform<Data: bytemuck::Pod + bytemuck::Zeroable>(
        &mut self,
        ctx: &crate::GpuContext,
        id: usize,
        data: Data,
    ) {
        if let Some(pass) = self.post_process_passes.get(id) {
            pass.write_scratch_buffer(ctx, data);
        }
    }

    pub fn enable_post_process_pass(&mut self, id: crate::PostProcessPassId) {
        if let Some(pass) = self.post_process_passes.get_mut(*id) {
            pass.enabled = true;
        }
    }

    pub fn disable_post_process_pass(&mut self, id: crate::PostProcessPassId) {
        if let Some(pass) = self.post_process_passes.get_mut(*id) {
            pass.enabled = false;
        }
    }

    pub fn create_pipeline(
        &mut self,
        ctx: &crate::GpuContext,
        gpu_resources: &crate::GpuResources,
        key: crate::PipelineKey,
    ) -> Result<Arc<wgpu::RenderPipeline>, crate::PassError> {
        Ok(self.pipelines.get_or_create(ctx, gpu_resources, key)?)
    }

    /// Access the extensible renderer to register effects and configure HDR/depth.
    pub fn graph_renderer(&mut self) -> &mut crate::GraphRenderer {
        &mut self.graph_renderer
    }

    /// V2 frames use client-provided view bindings and generic objects.
    pub fn render_v2(
        &mut self,
        ctx: &mut crate::GpuContext,
        resources: &mut prism_v2::resource::ResourceManager,
        frame: crate::FrameV2,
    ) -> crate::RenderResult<Option<prism_v2::render::FrameReport>> {
        self.graph_renderer.render(ctx, resources, frame)
    }

    pub fn frame_manager(&self) -> &FrameManager {
        &self.frame_manager
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleBufferIndex {
    Primary,
    Secondary,
}

impl DoubleBufferIndex {
    pub fn swap(self) -> Self {
        match self {
            Self::Primary => Self::Secondary,
            Self::Secondary => Self::Primary,
        }
    }
}

fn build_camera_matrix(
    pos: utils::math::Vec2,
    shake: utils::math::Vec2,
    screen_w: f32,
    screen_h: f32,
) -> utils::math::Mat4 {
    let proj = utils::math::Mat4::orthographic_wgpu(0.0, screen_w, screen_h, 0.0, -1.0, 1.0);
    let view = utils::math::Mat4::translation(
        -pos.x + (screen_w * 0.5) + shake.x,
        -pos.y + (screen_h * 0.5) + shake.y,
        0.0,
    );
    proj.multiply(view)
}

// Compatibility passes use the same versioned scheduler. The scheduler dispatches
// registered callbacks; pass names have no special meaning to the graph compiler.
#[allow(clippy::too_many_arguments)]
fn encode_legacy_graph(
    ctx: &crate::GpuContext,
    resources: &mut crate::GpuResources,
    encoder: &mut wgpu::CommandEncoder,
    surface: &wgpu::Texture,
    surface_view: &wgpu::TextureView,
    textures: [&wgpu::Texture; 2],
    views: [&wgpu::TextureView; 2],
    world: &mut crate::WorldPass,
    vfx: &mut crate::VfxPass,
    hud: &mut crate::HudPass,
    post: &mut [crate::PostProcessPass],
) -> crate::RenderResult<()> {
    use prism_v2::graph::*;
    type Encode<'a> = Box<
        dyn FnMut(
                &mut wgpu::CommandEncoder,
                &PassResources<'_>,
                &mut crate::GpuResources,
            ) -> crate::RenderResult<()>
            + 'a,
    >;
    let mut graph = RenderGraph::new();
    let mut imports = GraphImports::default();
    let mut import = |name: &str, texture: &wgpu::Texture, view: &wgpu::TextureView| {
        let desc = TextureDesc {
            size: texture.size(),
            format: texture.format(),
            usage: texture.usage(),
            sample_count: texture.sample_count(),
            mip_level_count: texture.mip_level_count(),
            dimension: texture.dimension(),
        };
        let id = graph.import_texture(name, desc, false);
        imports.insert(
            id,
            GraphResource::Texture {
                texture: texture.clone(),
                view: view.clone(),
            },
        );
        id
    };
    let surface_id = import("surface", surface, surface_view);
    let mut targets = [
        import("intermediate A", textures[0], views[0]),
        import("intermediate B", textures[1], views[1]),
    ];
    let active: Vec<_> = post.iter_mut().filter(|p| p.enabled).collect();
    let count = active.len();
    let mut scene = if count == 0 { surface_id } else { targets[0] };
    let mut nodes: Vec<(PassId, Encode<'_>)> = Vec::new();
    let world_target = scene;
    let id = graph.add_pass(PassDescriptor::new("world").access(scene, AccessKind::ColorClear));
    nodes.push((
        id,
        Box::new(move |encoder, graph, resources| {
            world.execute(encoder, graph.view(world_target)?, resources);
            Ok(())
        }),
    ));
    scene = graph.next_version(scene)?;
    let vfx_target = scene;
    let id =
        graph.add_pass(PassDescriptor::new("transparent").access(scene, AccessKind::ColorLoad));
    nodes.push((
        id,
        Box::new(move |encoder, graph, resources| {
            vfx.execute(encoder, graph.view(vfx_target)?, resources);
            Ok(())
        }),
    ));
    if count > 0 {
        targets[0] = scene;
    }
    for (i, pass) in active.into_iter().enumerate() {
        let source = scene;
        let target = if i + 1 == count {
            surface_id
        } else {
            let slot = (i + 1) % 2;
            if slot == 0 || i > 0 {
                targets[slot] = graph.next_version(targets[slot])?;
            }
            targets[slot]
        };
        let id = graph.add_pass(
            PassDescriptor::new(format!("post process {i}"))
                .access(source, AccessKind::Sampled)
                .access(target, AccessKind::ColorClear),
        );
        nodes.push((
            id,
            Box::new(move |encoder, graph, resources| {
                let source_view = graph.view(source)?;
                let target_view = graph.view(target)?;
                pass.prepare(
                    ctx,
                    resources,
                    &mut crate::PostProcessInput {
                        source: source_view,
                        target: target_view,
                    },
                );
                pass.execute(encoder, target_view, resources);
                Ok(())
            }),
        ));
        scene = target;
    }
    let output = graph.next_version(scene)?;
    let id = graph.add_pass(PassDescriptor::new("overlay").access(output, AccessKind::ColorLoad));
    nodes.push((
        id,
        Box::new(move |encoder, graph, resources| {
            hud.execute(encoder, graph.view(output)?, resources);
            Ok(())
        }),
    ));
    graph.export(output);
    let compiled = graph.compile()?;
    let allocations = compiled.prepare(&ctx.device, imports)?;
    for id in &compiled.order {
        let (_, node) = nodes
            .iter_mut()
            .find(|(key, _)| key == id)
            .ok_or_else(|| crate::RenderError::new("legacy graph", "missing pass"))?;
        node(
            encoder,
            &compiled.pass_resources(*id, &allocations)?,
            resources,
        )?;
    }
    Ok(())
}
