use crate::geometry::mesh::RawMesh;
use crate::geometry::shape::{NinePatchMargins, Shape};
use crate::resource::pipeline::BlendMode;
use utils::ids::{MaterialId, TextureId};

#[derive(Debug)]
pub enum DrawCommand {
    Shape {
        shape: Shape,
        blend: BlendMode,
        layer: u8,
    },
    Mesh {
        mesh: RawMesh,
        blend: BlendMode,
        layer: u8,
    },
    Texture {
        id: TextureId,
        pos: [f32; 2],
        size: [f32; 2],
        rotation: f32,
        uv: Option<crate::geometry::shape::UvRect>,
        tint: [f32; 4],
        blend: BlendMode,
        layer: u8,
    },
    Text {
        content: String,
        pos: [f32; 2],
        size: f32,
        color: [f32; 4],
        layer: u8,
    },
    Material {
        material_id: MaterialId,
        texture_id: Option<TextureId>,
        pos: [f32; 2],
        size: [f32; 2],
        rotation: f32,
        uv: Option<crate::geometry::shape::UvRect>,
        tint: [f32; 4],
        uniform_data: Vec<u8>, // données custom uploadées dans un scratch buffer
        blend: BlendMode,
        layer: u8,
    },
    NinePatch {
        id: TextureId,
        pos: [f32; 2],
        size: [f32; 2],
        texture_size: [f32; 2],
        margins: NinePatchMargins,
        tint: [f32; 4],
        blend: BlendMode,
        layer: u8,
    },
}
