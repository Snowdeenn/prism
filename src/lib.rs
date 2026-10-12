//! Rendering with owned frame submissions, explicit material schemas and a versioned graph.
//!
//! Start with the [V2 integration guide](guide) for architecture, examples and contracts.
//! The main V2 entry points are [`GraphRenderer`], [`FrameV2`] and
//! [`resource::ResourceManager`]. Historical root reexports such as [`Frame`],
//! [`Material`] and [`PipelineKey`] remain legacy types; use [`material::Material`]
//! and [`pipeline::PipelineKey`] for V2.

extern crate self as utils;

/// Complete V2 architecture, API usage, lifecycle and migration documentation.
#[doc = include_str!("../docs/V2_GUIDE.md")]
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
