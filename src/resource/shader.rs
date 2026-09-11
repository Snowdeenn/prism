use utils::arena::Arena;
use utils::ids::{ShaderId, ShaderTag};

use crate::context::GpuContext;
use crate::errors::ShaderError;

pub struct GpuShader {
    pub module: wgpu::ShaderModule,
    pub path: Option<String>, // None si créé depuis du code inline
}

pub struct ShaderManager {
    shaders: Arena<GpuShader, ShaderTag>,
}

impl ShaderManager {
    pub fn new() -> Self {
        Self {
            shaders: Arena::new(),
        }
    }

    pub fn load(&mut self, ctx: &GpuContext, path: &str) -> Result<ShaderId, ShaderError> {
        let _span = tracing::info_span!("ShaderManager::load", path = %path).entered();
        let shader_code = std::fs::read_to_string(path).map_err(|source| {
            tracing::error!("Échec de la lecture du fichier shader '{path}' : {source}");
            ShaderError::Io {
                path: path.to_string(),
                source,
            }
        })?;

        let module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(path),
                source: wgpu::ShaderSource::Wgsl(shader_code.into()),
            });
        let id = self.shaders.insert(GpuShader {
            module,
            path: Some(path.to_string()),
        });
        tracing::info!(?id, "Shader WGSL chargé avec succès");
        Ok(id)
    }

    pub fn load_inline(&mut self, ctx: &GpuContext, source: &str, label: &str) -> ShaderId {
        let module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let id = self.shaders.insert(GpuShader { module, path: None });
        tracing::debug!(?id, label, "Shader WGSL inline enregistré");
        id
    }

    pub fn reload(&mut self, ctx: &GpuContext, id: ShaderId) -> Result<(), ShaderError> {
        let _span = tracing::info_span!("ShaderManager::reload", ?id).entered();

        let path = self
            .shaders
            .get(id)
            .ok_or_else(|| {
                tracing::warn!(?id, "Tentative de recharger un shader inexistant");
                ShaderError::NotFound { id }
            })?
            .path
            .clone()
            .ok_or_else(|| {
                tracing::warn!(?id, "Impossible de recharger un shader créé inline");
                ShaderError::InlineReload
            })?;
        let source = std::fs::read_to_string(&path).map_err(|source| {
        tracing::error!(path = %path, "Échec de lecture lors du rechargement du shader : {source}");
        ShaderError::Io {
            path: path.clone(),
            source,
        }
    })?;
        let module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&path),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        if let Some(shader) = self.shaders.get_mut(id) {
            shader.module = module;
            tracing::info!(?id, path = %path, "Shader rechargé avec succès (Hot-reload)");
            Ok(())
        } else {
            Err(ShaderError::NotFound { id })
        }
    }

    pub fn get(&self, id: ShaderId) -> Option<&GpuShader> {
        self.shaders.get(id)
    }

    pub fn get_mut(&mut self, id: ShaderId) -> Option<&mut GpuShader> {
        self.shaders.get_mut(id)
    }

    pub fn remove(&mut self, id: ShaderId) -> Option<GpuShader> {
        if let Some(shader) = self.shaders.remove(id) {
            tracing::debug!(?id, "Shader supprimé de l'Arena");
            Some(shader)
        } else {
            None
        }
    }
}
