#[derive(Debug)]
pub enum Shape {
    Quad {
        pos: [f32; 2],
        size: [f32; 2],
        rotation: f32,
        color: [f32; 4],
        uv: Option<UvRect>,
    },
    Polygon {
        center: [f32; 2],
        sides: u32,
        radius: f32,
        color: [f32; 4],
    },
    SlantedQuad {
        pos: [f32; 2],
        size: [f32; 2],
        skew: f32, // décalage horizontal du haut par rapport au bas
        color: [f32; 4],
    },
    RoundedRect {
        pos: [f32; 2],
        size: [f32; 2],
        radius: f32,
        segments: u32, // nombre de segments par coin
        color: [f32; 4],
    },
    Line {
        start: [f32; 2],
        end: [f32; 2],
        thickness: f32,
        color: [f32; 4],
    },
    Ring {
        center: [f32; 2],
        inner_r: f32,
        outer_r: f32,
        start_angle: f32,
        end_angle: f32,
        resolution: u32,
        color: [f32; 4],
    },
    NinePatch {
        pos: [f32; 2],
        size: [f32; 2],
        texture_size: [f32; 2],
        margins: NinePatchMargins,
        color: [f32; 4],
    },
}

#[derive(Debug, Clone, Copy)]
pub struct UvRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NinePatchMargins {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl NinePatchMargins {
    pub fn uniform(value: f32) -> Self {
        Self {
            top: value,
            bottom: value,
            left: value,
            right: value,
        }
    }
}
