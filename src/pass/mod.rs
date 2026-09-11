pub mod hud;
pub mod post_process;
pub mod vfx;
pub mod world;

use crate::GpuResources;
use crate::context::GpuContext;
use crate::draw::batch::DrawCommandBuffer;
use utils::math::Mat4;
pub trait Pass {
    type Input<'a>
    where
        Self: 'a;

    /// Prépare les données CPU — tesselle, uploade vers le GPU
    fn prepare<'a>(
        &mut self,
        ctx: &GpuContext,
        gpu_resources: &mut GpuResources,
        input: &mut Self::Input<'a>,
    );

    /// Exécute la passe — crée le render pass, bind pipeline, draw
    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        gpu_resources: &GpuResources,
    );
}

// WorldPass
pub struct WorldInput<'a> {
    pub commands: &'a mut DrawCommandBuffer,
    pub camera: Mat4,
}

// VfxPass
pub struct VfxInput<'a> {
    pub commands: &'a DrawCommandBuffer,
    pub camera: Mat4,
}

// HudPass
pub struct HudInput<'a> {
    pub commands: &'a mut DrawCommandBuffer,
    pub camera: Mat4,
}

// PostProcessPass
pub struct PostProcessInput<'a> {
    pub source: &'a wgpu::TextureView,
    pub target: &'a wgpu::TextureView,
}

use crate::resource::pipeline::BindGroupLayoutEntryKey;
use crate::resource::pipeline::BindingTypeKey;
pub static CAM_BIND_GROUP: &[&[BindGroupLayoutEntryKey]] = &[&[BindGroupLayoutEntryKey {
    binding: 0,
    visibility: wgpu::ShaderStages::VERTEX,
    ty: BindingTypeKey::UniformBuffer,
}]];
pub static TEXTURE_BIND_GROUP: &[&[BindGroupLayoutEntryKey]] = &[
    &[BindGroupLayoutEntryKey {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: BindingTypeKey::UniformBuffer,
    }],
    &[
        BindGroupLayoutEntryKey {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: BindingTypeKey::Texture2D,
        },
        BindGroupLayoutEntryKey {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: BindingTypeKey::Sampler,
        },
    ],
];

pub static MATERIAL_BIND_GROUP: &[&[BindGroupLayoutEntryKey]] = &[
    &[BindGroupLayoutEntryKey {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: BindingTypeKey::UniformBuffer,
    }],
    &[
        BindGroupLayoutEntryKey {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: BindingTypeKey::Texture2D,
        },
        BindGroupLayoutEntryKey {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: BindingTypeKey::Sampler,
        },
    ],
    &[BindGroupLayoutEntryKey {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: BindingTypeKey::DynamicUniformBuffer,
    }],
];
