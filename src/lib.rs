extern crate self as utils;

/// V2 integration guide (expanded in the documentation commit).
pub mod guide {}

pub mod arena;
mod context;
pub mod draw;
mod errors;
pub mod frame;
mod geometry;
pub mod ids;
pub mod math {
    pub use ::math::*;
}
pub mod bindings;
pub mod graph;
pub mod material;
mod pass;
pub mod pipeline;
pub mod render;
pub mod render_object;
pub mod renderer;
pub mod resource;

pub use frame::v2::Frame as FrameV2;
pub use render::GraphRenderer;

pub use context::GpuContext;
pub use draw::{
    batch::DrawCommandBuffer, colors::Color, commands::DrawCommand, text::TextRenderer,
};
pub use errors::*;
pub use errors::{GpuContextError, TextRendererError};
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
