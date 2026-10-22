use crate::{DrawCommand, DrawCommandBuffer};
use utils::math::Vec2;

/// Représente les données de rendu accumulées durant une frame côté CPU.
///
/// Elle sert de zone de préparation (*staging*) où les différents systèmes enregistrent
/// leurs `DrawCommand` avant l'envoi aux passes GPU respectives.
#[derive(Debug)]
pub struct Frame {
    pub(crate) world: DrawCommandBuffer,
    pub(crate) vfx: DrawCommandBuffer,
    pub(crate) hud: DrawCommandBuffer,
    pub camera_pos: Vec2,
    pub cam_shake_offset: Vec2,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    /// Crée une nouvelle `Frame` avec des capacités pré-allouées adaptées à chaque passe.
    pub fn new() -> Self {
        Self {
            world: DrawCommandBuffer::new(1024),
            vfx: DrawCommandBuffer::new(256),
            hud: DrawCommandBuffer::new(512),
            camera_pos: Vec2::zero(),
            cam_shake_offset: Vec2::zero(),
        }
    }

    /// Réinitialise la frame pour la frame suivante sans réallouer la mémoire des buffers.
    pub fn clear(&mut self) {
        self.world.clear();
        self.vfx.clear();
        self.hud.clear();
        self.cam_shake_offset = Vec2::zero();
    }

    /// Calcule et retourne la position effective de la caméra (position + tremblement).
    #[inline]
    pub fn effective_camera_pos(&self) -> Vec2 {
        self.camera_pos + self.cam_shake_offset
    }

    /// Ajoute une commande de rendu dans la passe World.
    #[inline]
    pub fn push_world(&mut self, cmd: DrawCommand) {
        self.world.push(cmd);
    }

    /// Ajoute une commande de rendu dans la passe VFX.
    #[inline]
    pub fn push_vfx(&mut self, cmd: DrawCommand) {
        self.vfx.push(cmd);
    }

    /// Ajoute une commande de rendu dans la passe HUD.
    #[inline]
    pub fn push_hud(&mut self, cmd: DrawCommand) {
        self.hud.push(cmd);
    }

    /// Accès en lecture seule au buffer de commandes World.
    #[inline]
    pub fn world_commands(&self) -> &DrawCommandBuffer {
        &self.world
    }

    /// Accès en lecture seule au buffer de commandes VFX.
    #[inline]
    pub fn vfx_commands(&self) -> &DrawCommandBuffer {
        &self.vfx
    }

    /// Accès en lecture seule au buffer de commandes HUD.
    #[inline]
    pub fn hud_commands(&self) -> &DrawCommandBuffer {
        &self.hud
    }
}

pub struct FrameContext {
    pub encoder: wgpu::CommandEncoder,
    pub surface_view: wgpu::TextureView,
    pub surface_texture: wgpu::SurfaceTexture,
    pub size: (u32, u32),
}

impl FrameContext {
    pub fn present(self, gpu_ctx: &crate::GpuContext) {
        gpu_ctx.submit(self.encoder);
        gpu_ctx.present(self.surface_texture);
    }
}
