//! Historical Prism drawing API, backed by the V2 graph and pipeline caches.
//! Use `prism-v2` directly for new integrations.

extern crate self as utils;

use prism_v2::geometry;
pub use prism_v2::{FrameV2, GpuContext, GraphRenderer, arena, ids, math};

mod context {
    pub use prism_v2::GpuContext;
}

pub mod draw;
mod errors;
pub mod frame;
mod pass;
pub mod renderer;
pub mod resource;

pub use draw::{
    batch::DrawCommandBuffer, colors::Color, commands::DrawCommand, text::TextRenderer,
};
pub use errors::*;
pub use frame::{frame::Frame, frame::FrameContext, manager::FrameManager};
pub use geometry::{
    mesh::{RawMesh, Vertex},
    shape::{NinePatchMargins, Shape, UvRect},
    tesselator::Tesselator,
};
pub use pass::{
    HudInput, Pass, PostProcessInput, VfxInput, WorldInput,
    hud::HudPass,
    post_process::{PostProcessPass, PostProcessPassId},
    vfx::VfxPass,
    world::WorldPass,
};
pub use renderer::{DoubleBufferIndex, Renderer};
pub use resource::{
    GpuResources,
    buffer::{GpuBuffer, GpuBufferManager},
    material::{Material, MaterialManager},
    pipeline::{
        BindGroupLayoutEntryKey, BindingTypeKey, BlendMode, PipelineKey, PipelineManager,
        VertexFormat,
    },
    shader::{GpuShader, ShaderManager},
    texture::{GpuTexture, TextureManager},
};

pub use pass::CAM_BIND_GROUP;
pub use pass::MATERIAL_BIND_GROUP;
pub use pass::TEXTURE_BIND_GROUP;
