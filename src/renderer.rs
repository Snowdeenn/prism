use std::sync::Arc;

use utils::ids::ShaderId;

use crate::{FrameManager, pass::Pass};

pub struct Renderer {
    pipelines: crate::PipelineManager,
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
        let (intermediate_a, intermediate_view_a) = Self::create_intermediate(&ctx);
        let (intermediate_b, intermediate_view_b) = Self::create_intermediate(&ctx);

        let world = crate::WorldPass::new(
            ctx,
            gpu_resources,
            &mut pipelines,
            text_vert_id,
            text_frag_id,
        )?;
        let vfx = crate::VfxPass::new(
            &ctx,
            gpu_resources,
            &mut pipelines,
            vert_shader,
            frag_shader,
        )?;
        let hud = crate::HudPass::new(
            &ctx,
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
        let surface_texture = match ctx.current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                tracing::warn!("Surface GPU suboptimale, reconfiguration au prochain cycle");
                ctx.reconfigure();
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

        let (_source, target) = match self.current_source {
            DoubleBufferIndex::Primary => (&self.intermediate_view_a, &self.intermediate_view_b),
            DoubleBufferIndex::Secondary => (&self.intermediate_view_b, &self.intermediate_view_a),
        };
        let screen_w = ctx.size.width;
        let screen_h = ctx.size.height;
        let cam_matrix = build_camera_matrix(
            frame.camera_pos,
            frame.cam_shake_offset,
            screen_w as f32,
            screen_h as f32,
        );

        self.world.prepare(
            &ctx,
            gpu_resources,
            &mut crate::WorldInput {
                commands: &mut frame.world,
                camera: cam_matrix,
            },
        );
        self.vfx.prepare(
            &ctx,
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
            &ctx,
            gpu_resources,
            &mut crate::HudInput {
                commands: &mut frame.hud,
                camera: hud_camera,
            },
        );

        self.world.execute(&mut encoder, target, gpu_resources);
        self.vfx.execute(&mut encoder, target, gpu_resources);

        let last = self.post_process_passes.len().saturating_sub(1);
        for i in 0..self.post_process_passes.len() {
            self.current_source = self.current_source.swap();
            let (src, tgt) = match self.current_source {
                DoubleBufferIndex::Primary => {
                    (&self.intermediate_view_a, &self.intermediate_view_b)
                }
                DoubleBufferIndex::Secondary => {
                    (&self.intermediate_view_b, &self.intermediate_view_a)
                }
            };
            let render_target = if i == last { &surface_view } else { tgt };
            self.post_process_passes[i].prepare(
                &ctx,
                gpu_resources,
                &mut crate::PostProcessInput {
                    source: src,
                    target: tgt,
                },
            );
            self.post_process_passes[i].execute(&mut encoder, render_target, gpu_resources);
        }
        self.hud.execute(&mut encoder, &surface_view, gpu_resources);

        Some(crate::FrameContext {
            encoder,
            surface_view,
            surface_texture,
            size: (screen_w, screen_h),
        })
    }

    fn create_intermediate(ctx: &crate::GpuContext) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Intermediate Target"),
            size: wgpu::Extent3d {
                width: ctx.size.width,
                height: ctx.size.height,
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
        let pass = &self.post_process_passes[id];
        pass.write_scratch_buffer(ctx, data);
    }

    pub fn enable_post_process_pass(&mut self, id: crate::PostProcessPassId) {
        self.post_process_passes[*id].enabled = true;
    }

    pub fn disable_post_process_pass(&mut self, id: crate::PostProcessPassId) {
        self.post_process_passes[*id].enabled = false;
    }

    pub fn create_pipeline(
        &mut self,
        ctx: &crate::GpuContext,
        gpu_resources: &crate::GpuResources,
        key: crate::PipelineKey,
    ) -> Result<Arc<wgpu::RenderPipeline>, crate::PassError> {
        Ok(self.pipelines.get_or_create(ctx, gpu_resources, key)?)
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
