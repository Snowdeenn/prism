//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
//! Immutable rendering families and independently mutable material instances.
use crate::{bindings::BindingSchema, ids::*, pipeline::VertexLayout};

#[derive(Debug, Clone)]
/// Shader handle, entry-point name and floating-point override constants.
pub struct ShaderEntry {
    pub shader: ShaderId,
    pub entry_point: String,
    pub constants: Vec<(String, f64)>,
}
impl ShaderEntry {
    /// Select an entry point with no override constants.
    pub fn new(shader: ShaderId, entry_point: impl Into<String>) -> Self {
        Self {
            shader,
            entry_point: entry_point.into(),
            constants: Vec::new(),
        }
    }
}
#[derive(Debug, Clone)]
/// Immutable rendering family: shaders, schema and complete supported raster state.
/// Clone and register a new template to change state; pipeline creation is lazy.
pub struct MaterialTemplate {
    pub label: String,
    pub vertex: ShaderEntry,
    pub fragment: Option<ShaderEntry>,
    pub schema: BindingSchema,
    pub vertex_layouts: Vec<Option<VertexLayout>>,
    pub primitive: wgpu::PrimitiveState,
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    pub targets: Vec<Option<wgpu::ColorTargetState>>,
    pub multisample: wgpu::MultisampleState,
    pub multiview_mask: Option<std::num::NonZeroU32>,
}
impl MaterialTemplate {
    /// Create a single-color, single-sample triangle-list template using `vs_main` and
    /// `fs_main`. No bindings, vertex layouts, blending or depth are added automatically.
    pub fn fullscreen(
        label: impl Into<String>,
        shader: ShaderId,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            label: label.into(),
            vertex: ShaderEntry::new(shader, "vs_main"),
            fragment: Some(ShaderEntry::new(shader, "fs_main")),
            schema: BindingSchema::default(),
            vertex_layouts: Vec::new(),
            primitive: Default::default(),
            depth_stencil: None,
            targets: vec![Some(format.into())],
            multisample: Default::default(),
            multiview_mask: None,
        }
    }
}
#[derive(Debug, Clone, Default)]
/// Uniform bytes for one binding. Nonempty, four-byte-sized data must already match
/// WGSL layout. A fresh uniform buffer is allocated for each prepared draw.
pub struct ParameterBlock {
    pub group: u32,
    pub binding: u32,
    /// Owned until copied into the frame's immutable GPU uniform buffer.
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone)]
/// Mutable material instance referencing an immutable template.
/// Bindings are collected without overrides; see [resolution rules](crate::guide#materials-and-binding-resolution).
pub struct Material {
    pub template: MaterialTemplateHandle,
    pub bindings: Vec<BindingSetHandle>,
    pub parameters: Option<ParameterBlock>,
}
#[derive(Debug, Clone)]
/// GPU resources for one schema entry. Buffer tuples contain handle, byte offset and
/// nonzero byte size. Arrays must exactly match the schema count.
pub enum BindingResource {
    Buffer {
        handle: BufferId,
        offset: u64,
        size: u64,
    },
    Texture(TextureViewHandle),
    Sampler(SamplerHandle),
    BufferArray(Vec<(BufferId, u64, u64)>),
    TextureArray(Vec<TextureViewHandle>),
    SamplerArray(Vec<SamplerHandle>),
}
#[derive(Debug, Clone)]
/// Entries for one explicit group. Validation is deferred until draw preparation.
pub struct BindingSet {
    pub group: u32,
    pub entries: Vec<(u32, BindingResource)>,
}
#[derive(Debug, Clone)]
/// A binding set with dynamic offsets for this use. Converting a handle supplies none.
pub struct BindingSetUse {
    pub handle: BindingSetHandle,
    /// In ascending binding order, as required by wgpu.
    pub dynamic_offsets: Vec<u32>,
}
impl From<BindingSetHandle> for BindingSetUse {
    fn from(handle: BindingSetHandle) -> Self {
        Self {
            handle,
            dynamic_offsets: Vec::new(),
        }
    }
}
