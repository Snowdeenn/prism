// examples/demo.rs
//
// Démo visuelle — validation GPU complète du renderer prism.
//
// Ce que tu dois voir à l'écran :
//   • Fond noir (WorldPass, LoadOp::Clear)
//   • Un quad rouge centré                      (WorldPass)
//   • Un hexagone vert en haut-gauche           (WorldPass)
//   • Un anneau blanc partiel en bas            (WorldPass)
//   • Deux lignes jaunes en croix               (WorldPass)
//   • Un quad orange semi-transparent           (VfxPass, BlendMode::Additive)
//   • Un rounded rect gris en haut-droit        (HudPass)
//   • Un quad slanted violet sur le HUD         (HudPass)
//   • Du texte blanc "PRISM DEMO" en haut       (HudPass, TextRenderer)
//
// Lancer avec :  cargo run -p prism --example demo

use prism::Pass;
use std::sync::Arc; // Ou use prism::passes::RenderPass; selon ton nom de trait

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

use math::Mat4;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers : conversion coordonnées écran → clip space
// ─────────────────────────────────────────────────────────────────────────────

/// Convertit des coordonnées pixel (0..width, 0..height) en clip space (-1..1).
fn px_to_clip(px: f32, py: f32, w: f32, h: f32) -> [f32; 2] {
    let cx = (px / w) * 2.0 - 1.0;
    let cy = 1.0 - (py / h) * 2.0; // Y inversé : pixel 0 = haut écran = clip +1
    [cx, cy]
}

/// Convertit une taille en pixels en taille clip space.
fn size_to_clip(sw: f32, sh: f32, w: f32, h: f32) -> [f32; 2] {
    [(sw / w) * 2.0, (sh / h) * 2.0]
}

// ─────────────────────────────────────────────────────────────────────────────
// Application
// ─────────────────────────────────────────────────────────────────────────────

struct Demo {
    // GPU
    ctx: prism::GpuContext,
    pipelines: prism::PipelineManager,
    resources: prism::GpuResources,

    // Passes
    world: prism::WorldPass,
    vfx: prism::VfxPass,
    hud: prism::HudPass,

    // Fenêtre
    window: Arc<Window>,
}

impl Demo {
    async fn new(window: Arc<Window>) -> prism::Result<Self> {
        let size = window.inner_size();

        // ── GpuContext ───────────────────────────────────────────────────────
        let mut ctx = prism::GpuContext::new(window.clone())
            .await
            .expect("Impossible de créer le GpuContext");
        ctx.resize(size.width, size.height);

        // ── Ressources ───────────────────────────────────────────────────────
        let mut resources = prism::GpuResources::new(&ctx);

        // Shader unique partagé par toutes les passes

        let vert_id = resources.load_shader_inline(&ctx, BASIC_WGSL_INLINE, "basic_inline");

        let frag_id = vert_id; // même module WGSL pour vs_main et fs_main

        let surface_format = ctx.surface_format();
        let mut pipelines = prism::PipelineManager::new(surface_format);

        // ── Passes ───────────────────────────────────────────────────────────
        let world = prism::WorldPass::new(&ctx, &mut resources, &mut pipelines, vert_id, frag_id)?;
        let vfx = prism::VfxPass::new(&ctx, &mut resources, &mut pipelines, vert_id, frag_id)?;
        let hud = prism::HudPass::new(
            &ctx,
            &mut resources,
            &mut pipelines,
            vert_id,
            frag_id,
            surface_format,
        )?;

        Ok(Self {
            ctx,
            resources,
            pipelines,
            world,
            vfx,
            hud,
            window,
        })
    }

    fn render(&mut self) {
        let size = self.window.inner_size();

        // Si la fenêtre est minimisée ou invisible, on ne dessine PAS
        if size.width == 0 || size.height == 0 {
            return;
        }
        let w = size.width as f32;
        let h = size.height as f32;

        // ── Commandes WorldPass ───────────────────────────────────────────────
        let mut world_cmds = prism::DrawCommandBuffer::new(32);

        // Quad rouge centré (200×200 px)
        world_cmds.push(prism::DrawCommand::Shape {
            shape: prism::Shape::Quad {
                pos: px_to_clip(w * 0.5 - 100.0, h * 0.5 - 100.0, w, h),
                size: size_to_clip(200.0, 200.0, w, h),
                rotation: 0.0,
                color: [0.9, 0.15, 0.15, 1.0],
                uv: None,
            },
            blend: prism::BlendMode::Alpha,
            layer: 0,
        });

        // Hexagone vert en haut-gauche (rayon 80px)
        world_cmds.push(prism::DrawCommand::Shape {
            shape: prism::Shape::Polygon {
                center: px_to_clip(120.0, 120.0, w, h),
                sides: 6,
                radius: (80.0 / w) * 2.0,
                color: [0.2, 0.85, 0.3, 1.0],
            },
            blend: prism::BlendMode::Alpha,
            layer: 0,
        });

        // Anneau blanc partiel en bas (arc de 0° à 210°)
        world_cmds.push(prism::DrawCommand::Shape {
            shape: prism::Shape::Ring {
                center: px_to_clip(w * 0.5, h - 100.0, w, h),
                inner_r: (60.0 / w) * 2.0,
                outer_r: (90.0 / w) * 2.0,
                start_angle: 0.0,
                end_angle: std::f32::consts::PI * 1.167,
                resolution: 48,
                color: [0.9, 0.9, 0.9, 0.8],
            },
            blend: prism::BlendMode::Alpha,
            layer: 0,
        });

        // Croix de deux lignes jaunes
        let cross_cx = px_to_clip(w * 0.75, h * 0.5, w, h);
        let thick = (3.0 / w) * 2.0;
        world_cmds.push(prism::DrawCommand::Shape {
            shape: prism::Shape::Line {
                start: [cross_cx[0] - 0.1, cross_cx[1]],
                end: [cross_cx[0] + 0.1, cross_cx[1]],
                thickness: thick,
                color: [1.0, 0.95, 0.1, 1.0],
            },
            blend: prism::BlendMode::Alpha,
            layer: 0,
        });
        world_cmds.push(prism::DrawCommand::Shape {
            shape: prism::Shape::Line {
                start: [cross_cx[0], cross_cx[1] - 0.15],
                end: [cross_cx[0], cross_cx[1] + 0.15],
                thickness: thick,
                color: [1.0, 0.95, 0.1, 1.0],
            },
            blend: prism::BlendMode::Alpha,
            layer: 0,
        });

        // ── Commandes VfxPass (Additive) ─────────────────────────────────────
        let mut vfx_cmds = prism::DrawCommandBuffer::new(8);

        // Quad orange semi-transparent centré-décalé (effet glow simulé)
        vfx_cmds.push(prism::DrawCommand::Shape {
            shape: prism::Shape::Quad {
                pos: px_to_clip(w * 0.5 - 60.0, h * 0.5 - 60.0, w, h),
                size: size_to_clip(120.0, 120.0, w, h),
                rotation: std::f32::consts::FRAC_PI_4,
                color: [1.0, 0.4, 0.0, 0.35],
                uv: None,
            },
            blend: prism::BlendMode::Additive,
            layer: 1,
        });

        // ── Commandes HudPass ─────────────────────────────────────────────────
        let mut hud_cmds = prism::DrawCommandBuffer::new(16);

        // Rounded rect gris semi-transparent en haut-droit
        hud_cmds.push(prism::DrawCommand::Shape {
            shape: prism::Shape::RoundedRect {
                pos: px_to_clip(w - 220.0, 20.0, w, h),
                size: size_to_clip(200.0, 80.0, w, h),
                radius: (8.0 / w) * 2.0,
                segments: 4,
                color: [0.1, 0.1, 0.1, 0.75],
            },
            blend: prism::BlendMode::Alpha,
            layer: 2,
        });

        // SlantedQuad violet
        hud_cmds.push(prism::DrawCommand::Shape {
            shape: prism::Shape::SlantedQuad {
                pos: px_to_clip(w - 215.0, 30.0, w, h),
                size: size_to_clip(150.0, 20.0, w, h),
                skew: (6.0 / w) * 2.0,
                color: [0.6, 0.1, 0.9, 0.9],
            },
            blend: prism::BlendMode::Alpha,
            layer: 2,
        });

        // ── Rendu ─────────────────────────────────────────────────────────────
        let surface_texture = match self.ctx.current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.ctx.reconfigure();
                return;
            }
            _ => return,
        };

        let mut encoder = self.ctx.create_encoder("demo_frame");

        let mut world_input = prism::WorldInput {
            commands: &mut world_cmds,
            camera: Mat4::identity(),
        };
        self.world
            .prepare(&self.ctx, &mut self.resources, &mut world_input);
        self.world.execute(
            &mut encoder,
            &surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            &self.resources,
        );

        let mut vfx_input = prism::VfxInput {
            commands: &vfx_cmds,
            camera: Mat4::identity(),
        };
        self.vfx
            .prepare(&self.ctx, &mut self.resources, &mut vfx_input);
        self.vfx.execute(
            &mut encoder,
            &surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            &self.resources,
        );

        let mut hud_input = prism::HudInput {
            commands: &mut hud_cmds,
            camera: Mat4::identity(),
        };
        self.hud
            .prepare(&self.ctx, &mut self.resources, &mut hud_input);
        self.hud.execute(
            &mut encoder,
            &surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            &self.resources,
        );

        self.ctx.submit(encoder);
        self.ctx.present(surface_texture);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Boucle winit
// ─────────────────────────────────────────────────────────────────────────────

struct App {
    demo: Option<Demo>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.demo.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Prism — Demo")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
                )
                .expect("Impossible de créer la fenêtre"),
        );
        let demo = pollster::block_on(Demo::new(window));
        self.demo = demo.ok();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => event_loop.exit(),

            WindowEvent::Resized(new_size) => {
                if let Some(demo) = &mut self.demo {
                    demo.ctx.resize(new_size.width, new_size.height);
                    demo.window.request_redraw();
                }
            }

            WindowEvent::RedrawRequested => {
                if let Some(demo) = &mut self.demo {
                    demo.render();
                    demo.window.request_redraw();
                }
            }

            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Impossible de créer l'EventLoop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { demo: None };
    event_loop
        .run_app(&mut app)
        .expect("Erreur dans la boucle d'événements");
}

// ─────────────────────────────────────────────────────────────────────────────
// Shader WGSL inline (fallback si shaders/basic.wgsl est absent)
// ─────────────────────────────────────────────────────────────────────────────

const BASIC_WGSL_INLINE: &str = r#"
struct VertexInput {
    @location(0) pos:   vec2<f32>,
    @location(1) uv:    vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
    @location(1)       color:    vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_pos = vec4<f32>(in.pos, 0.0, 1.0);
    out.uv       = in.uv;
    out.color    = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;
