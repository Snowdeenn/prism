//! Rendering with owned frame submissions, explicit material schemas and a versioned graph.
//!
//! Start with the [V2 integration guide](guide). The main entry points are
//! [`GraphRenderer`], [`Frame`] and [`resource::ResourceManager`].

/// Complete V2 architecture, API usage, lifecycle and migration documentation.
#[doc = include_str!("../../../docs/V2_GUIDE.md")]
pub mod guide {}

pub mod arena;
pub mod bindings;
mod context;
mod errors;
pub mod frame;
pub mod geometry;
pub mod graph;
pub mod ids;
pub mod material;
pub mod math {
    pub use ::math::*;
}
pub mod pipeline;
pub mod render;
pub mod render_object;
pub mod resource;

pub use context::GpuContext;
pub use errors::{GpuContextError, RenderError, RenderResult};
pub use frame::Frame;
pub use frame::Frame as FrameV2;
pub use geometry::{
    mesh::{RawMesh, Vertex},
    shape::{NinePatchMargins, Shape, UvRect},
    tesselator::Tesselator,
};
pub use material::Material;
pub use pipeline::PipelineKey;
pub use render::GraphRenderer;
pub use resource::ResourceManager;
