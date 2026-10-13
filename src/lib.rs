//! Prism rendering APIs.
//!
//! Use [`v1`] for historical drawing commands and [`v2`] for owned frame
//! submissions and the versioned render graph. The implementations live in the
//! `prism-v1` and `prism-v2` workspace crates, which can also be used directly.
//! Existing root imports remain available for compatibility.

pub use prism_v1 as v1;
pub use prism_v2 as v2;

pub use prism_v1::{
    BindGroupLayoutEntryKey, BindingTypeKey, BlendMode, BufferError, CAM_BIND_GROUP, Color,
    DoubleBufferIndex, DrawCommand, DrawCommandBuffer, Frame, FrameContext, FrameManager,
    GpuBuffer, GpuBufferManager, GpuResources, GpuShader, GpuTexture, HudInput, HudPass,
    MATERIAL_BIND_GROUP, Material, MaterialManager, Pass, PassError, PipelineError, PipelineKey,
    PipelineManager, PostProcessInput, PostProcessPass, PostProcessPassId, PrismError, Renderer,
    Result, ShaderError, ShaderManager, TEXTURE_BIND_GROUP, TextRenderer, TextRendererError,
    TextureError, TextureManager, VertexFormat, VfxInput, VfxPass, WorldInput, WorldPass, draw,
    renderer,
};
pub use prism_v2::{
    FrameV2, GpuContext, GpuContextError, GraphRenderer, NinePatchMargins, RawMesh, RenderError,
    RenderResult, Shape, Tesselator, UvRect, Vertex, arena, bindings, graph, guide, ids, material,
    math, pipeline, render, render_object,
};

/// Historical frame services and the V2 frame submission module.
pub mod frame {
    pub use prism_v1::frame::*;
    pub use prism_v2::frame::v2;
}

/// Historical GPU resources and the V2 resource manager.
pub mod resource {
    pub use prism_v1::resource::*;
    pub use prism_v2::resource::{ResourceManager, manager};
}
