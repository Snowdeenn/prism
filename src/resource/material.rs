use std::sync::Arc;
use utils::arena::Arena;
use utils::ids::{MaterialId, MaterialTag};

pub struct Material {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    // Bind groups custom du matériau — group(2+)
    // Vides pour un matériau sans uniforms custom
    pub bind_groups: Vec<wgpu::BindGroup>,
    // Taille en octets du bloc d'uniforms custom attendu par ce matériau
    // 0 si le matériau n'a pas d'uniforms custom
    pub uniform_size: usize,
}

pub struct MaterialManager {
    materials: Arena<Material, MaterialTag>,
}

impl MaterialManager {
    pub fn new() -> Self {
        Self {
            materials: Arena::new(),
        }
    }

    pub fn create(
        &mut self,
        pipeline: Arc<wgpu::RenderPipeline>,
        bind_groups: Vec<wgpu::BindGroup>,
        uniform_size: usize,
    ) -> MaterialId {
        self.materials.insert(Material {
            pipeline,
            bind_groups,
            uniform_size,
        })
    }

    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id)
    }

    pub fn get_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.get_mut(id)
    }

    pub fn remove(&mut self, id: MaterialId) -> Option<Material> {
        self.materials.remove(id)
    }
}
