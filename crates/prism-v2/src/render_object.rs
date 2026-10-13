//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
use crate::{ids::*, material::BindingSetUse};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Extensible phase identifier. Views choose which phases to draw.
pub struct RenderPhaseId(pub u32);
impl RenderPhaseId {
    /// Default opaque phase; writes depth in the high-level composition.
    pub const OPAQUE: Self = Self(0);
    /// Default transparent phase, sorted back to front.
    pub const TRANSPARENT: Self = Self(1);
    /// Default overlay phase, placed according to the frame overlay policy.
    pub const OVERLAY: Self = Self(2);
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
/// Client-computed layer or depth ordering key; interpretation depends on sort policy.
pub struct SortKey(pub u64);
#[derive(Debug, Clone, Copy)]
/// Optional axis-aligned bounds metadata. Built-in passes do not perform bounds culling.
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
#[derive(Debug, Clone)]
/// One geometry/material draw with a half-open instance range.
/// Objects are selected by phase for every matching view; empty ranges are skipped.
pub struct RenderObject {
    pub geometry: GeometryHandle,
    pub material: MaterialHandle,
    pub instances: Range<u32>,
    pub object_bindings: Vec<BindingSetUse>,
    pub phase: RenderPhaseId,
    /// Painter/depth ordering is client supplied. Transparent objects retain ties.
    pub sort_key: SortKey,
    pub bounds: Option<Bounds>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Stable ordering within a phase; equal comparison keys preserve submission order.
pub enum SortPolicy {
    /// Group by template and material handle, then ascending sort key.
    Opaque,
    /// Sort by descending sort key supplied by the client.
    BackToFront,
    /// Sort by ascending layer key, retaining submission order for ties.
    SubmissionOrder,
}
