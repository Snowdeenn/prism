use crate::arena::Id;

pub struct ShaderTag;
pub struct TextureTag;
pub struct MaterialTag;
pub struct BufferTag;

pub type ShaderId = Id<ShaderTag>;
pub type TextureId = Id<TextureTag>;
pub type MaterialId = Id<MaterialTag>;
pub type BufferId = Id<BufferTag>;
