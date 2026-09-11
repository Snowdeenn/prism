use std::sync::Arc;

use crate::{GpuShader, GpuTexture};

pub mod buffer;
pub mod material;
pub mod pipeline;
pub mod shader;
pub mod texture;

pub struct GpuResources {
    shader: shader::ShaderManager,
    texture: texture::TextureManager,
    material: material::MaterialManager,
    buffer: buffer::GpuBufferManager,
}

impl GpuResources {
    pub fn new(ctx: &crate::GpuContext) -> Self {
        Self {
            shader: shader::ShaderManager::new(),
            texture: texture::TextureManager::new(ctx),
            material: material::MaterialManager::new(),
            buffer: buffer::GpuBufferManager::new(),
        }
    }
    pub fn load_shader(
        &mut self,
        ctx: &crate::GpuContext,
        path: &str,
    ) -> Result<utils::ids::ShaderId, crate::ShaderError> {
        self.shader.load(ctx, path)
    }
    pub fn load_shader_inline(
        &mut self,
        ctx: &crate::GpuContext,
        source: &str,
        label: &str,
    ) -> utils::ids::ShaderId {
        self.shader.load_inline(ctx, source, label)
    }
    pub fn get_shader(&self, id: utils::ids::ShaderId) -> Option<&crate::GpuShader> {
        self.shader.get(id)
    }
    pub fn get_shader_mut(&mut self, id: utils::ids::ShaderId) -> Option<&mut crate::GpuShader> {
        self.shader.get_mut(id)
    }
    pub fn remove_shader(&mut self, id: utils::ids::ShaderId) -> Option<GpuShader> {
        self.shader.remove(id)
    }
    pub fn reload_shader(
        &mut self,
        ctx: &crate::GpuContext,
        id: utils::ids::ShaderId,
    ) -> Result<(), crate::ShaderError> {
        self.shader.reload(ctx, id)
    }

    pub fn load_texture(
        &mut self,
        ctx: &crate::GpuContext,
        path: &str,
    ) -> Result<utils::ids::TextureId, crate::TextureError> {
        self.texture.load(ctx, path)
    }
    pub fn register_texture(&mut self, texture: GpuTexture) -> utils::ids::TextureId {
        self.texture.register(texture)
    }
    pub fn get_texture(&self, id: utils::ids::TextureId) -> Option<&crate::GpuTexture> {
        self.texture.get(id)
    }
    pub fn get_texture_mut(&mut self, id: utils::ids::TextureId) -> Option<&mut crate::GpuTexture> {
        self.texture.get_mut(id)
    }
    pub fn remove_texture(
        &mut self,
        id: utils::ids::TextureId,
    ) -> Result<GpuTexture, crate::TextureError> {
        self.texture.remove(id)
    }
    pub fn white_texture(&self) -> utils::ids::TextureId {
        self.texture.white_texture()
    }

    pub fn create_material(
        &mut self,
        pipeline: Arc<wgpu::RenderPipeline>,
        bind_groups: Vec<wgpu::BindGroup>,
        uniform_size: usize,
    ) -> utils::ids::MaterialId {
        self.material.create(pipeline, bind_groups, uniform_size)
    }
    pub fn get_material(&self, id: utils::ids::MaterialId) -> Option<&crate::Material> {
        self.material.get(id)
    }
    pub fn get_material_mut(&mut self, id: utils::ids::MaterialId) -> Option<&mut crate::Material> {
        self.material.get_mut(id)
    }
    pub fn remove_material(&mut self, id: utils::ids::MaterialId) -> Option<crate::Material> {
        self.material.remove(id)
    }

    pub fn create_buffer(
        &mut self,
        ctx: &crate::GpuContext,
        label: Option<&str>,
        size: u64,
        usage: wgpu::BufferUsages,
    ) -> Result<utils::ids::BufferId, crate::BufferError> {
        Ok(self.buffer.create_buffer(ctx, label, size, usage)?)
    }
    pub fn get_buffer(&self, id: utils::ids::BufferId) -> Option<&crate::GpuBuffer> {
        self.buffer.get(id)
    }
    pub fn get_buffer_mut(&mut self, id: utils::ids::BufferId) -> Option<&mut crate::GpuBuffer> {
        self.get_buffer_mut(id)
    }
    pub fn write_buffer(
        &mut self,
        ctx: &crate::GpuContext,
        id: utils::ids::BufferId,
        data: &[u8],
    ) -> Result<(), crate::BufferError> {
        self.buffer.write_buffer(ctx, id, data)
    }
    pub fn remove_buffer(
        &mut self,
        id: utils::ids::BufferId,
    ) -> Result<crate::GpuBuffer, crate::BufferError> {
        Ok(self.buffer.remove(id)?)
    }
}
