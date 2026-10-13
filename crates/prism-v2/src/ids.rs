use crate::arena::Id;

pub struct ShaderTag;
pub struct TextureTag;
pub struct MaterialTag;
pub struct BufferTag;

pub type ShaderId = Id<ShaderTag>;
pub type TextureId = Id<TextureTag>;
pub type MaterialId = Id<MaterialTag>;
pub type BufferId = Id<BufferTag>;

pub struct GeometryTag;
pub struct MaterialTemplateTag;
pub struct MaterialInstanceTag;
pub struct BindingSetTag;
pub struct SamplerTag;
pub struct TextureViewTag;
pub type GeometryHandle = Id<GeometryTag>;
pub type MaterialTemplateHandle = Id<MaterialTemplateTag>;
pub type MaterialHandle = Id<MaterialInstanceTag>;
pub type BindingSetHandle = Id<BindingSetTag>;
pub type SamplerHandle = Id<SamplerTag>;
pub type TextureViewHandle = Id<TextureViewTag>;
