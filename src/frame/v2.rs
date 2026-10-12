//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
//! Owned submissions. Camera transforms and visual simulation belong to the client.
use crate::{graph::*, ids::*, material::BindingSetUse, render_object::*};

#[derive(Debug, Clone)]
/// One phase selected by a view, with its stable ordering policy.
pub struct PhaseSubmission {
    pub phase: RenderPhaseId,
    pub sort: SortPolicy,
}
#[derive(Debug, Clone)]
/// View bindings and ordered phases sharing a target. The default selects opaque,
/// transparent and overlay phases. See the [guide](crate::guide#geometry-instancing-phases-and-views).
pub struct ViewSubmission {
    pub bindings: Vec<BindingSetUse>,
    pub phases: Vec<PhaseSubmission>,
    /// Pixel coordinates; None covers the complete target.
    pub viewport: Option<[f32; 4]>,
}
impl Default for ViewSubmission {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            phases: vec![
                PhaseSubmission {
                    phase: RenderPhaseId::OPAQUE,
                    sort: SortPolicy::Opaque,
                },
                PhaseSubmission {
                    phase: RenderPhaseId::TRANSPARENT,
                    sort: SortPolicy::BackToFront,
                },
                PhaseSubmission {
                    phase: RenderPhaseId::OVERLAY,
                    sort: SortPolicy::SubmissionOrder,
                },
            ],
            viewport: None,
        }
    }
}
#[derive(Debug)]
/// Owned staging data copied before graph passes in the frame command buffer.
/// Buffer copies require four-byte alignment; texture rows require 256-byte alignment.
pub enum UploadRequest {
    Buffer {
        target: BufferId,
        offset: u64,
        data: Vec<u8>,
    },
    Texture {
        target: TextureId,
        mip_level: u32,
        origin: wgpu::Origin3d,
        size: wgpu::Extent3d,
        bytes_per_row: u32,
        rows_per_image: u32,
        data: Vec<u8>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Renderer-local effect registry identifier; do not mix IDs between renderers.
pub struct PostProcessId(pub u64);
#[derive(Debug, Clone)]
/// One execution of a registered effect, in frame list order.
pub struct PostProcessSubmission {
    pub effect: PostProcessId,
    pub parameters: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Whether overlay phases participate in the post-processing chain.
pub enum OverlayPolicy {
    BeforePostProcess,
    #[default]
    AfterPostProcess,
}
#[derive(Debug, Default)]
/// Owned V2 submission. Empty views receive a default only in the high-level texture path.
/// See [upload and submission contracts](crate::guide#frame-uploads-and-submission).
pub struct Frame {
    pub views: Vec<ViewSubmission>,
    pub objects: Vec<RenderObject>,
    pub uploads: Vec<UploadRequest>,
    pub post_process: Vec<PostProcessSubmission>,
    pub overlay_policy: OverlayPolicy,
}
impl Frame {
    /// Empty submission vectors while retaining capacity and preserving overlay policy.
    pub fn clear(&mut self) {
        self.views.clear();
        self.objects.clear();
        self.uploads.clear();
        self.post_process.clear();
    }
}
/// A caller-composed graph can use the same preparation and execution path as
/// the built-in scene graph. Node declarations are registered before compilation.
#[derive(Default)]
/// Executable graph plus external GPU imports. Register nodes with [`Self::add_pass`].
pub struct FrameGraph {
    pub graph: RenderGraph,
    pub imports: GraphImports,
    pub(crate) nodes: Vec<(PassId, Box<dyn crate::render::GraphPass>)>,
}
impl FrameGraph {
    /// Register both the access declaration and executable node implementation.
    pub fn add_pass(&mut self, pass: impl crate::render::GraphPass + 'static) -> PassId {
        let id = self.graph.add_pass(pass.declare());
        self.nodes.push((id, Box::new(pass)));
        id
    }
}
