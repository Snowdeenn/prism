#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Debug, Default, Clone)]
pub struct RawMesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl RawMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn with_capacity(verts: usize, index: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(verts),
            indices: Vec::with_capacity(index),
        }
    }

    pub fn push_vertex(&mut self, v: Vertex) -> u32 {
        let idx = self.vertices.len();
        self.vertices.push(v);
        idx as u32
    }

    pub fn push_triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.push(a);
        self.indices.push(b);
        self.indices.push(c);
    }

    pub fn clear(&mut self) {
        self.indices.clear();
        self.vertices.clear();
    }

    pub fn vertices_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.vertices)
    }

    pub fn indices_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.indices)
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn append(&mut self, other: &RawMesh) {
        let offset = self.vertices.len() as u32;
        self.vertices.extend_from_slice(other.vertices());
        self.indices
            .extend(other.indices().iter().map(|i| i + offset));
    }
}
