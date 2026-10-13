use utils::{
    arena::Arena,
    ids::{TextureId, TextureTag},
};

use crate::TextureError;
use crate::context::GpuContext;

pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub size: (u32, u32),
}

pub struct TextureManager {
    textures: Arena<GpuTexture, TextureTag>,
    white_texture: TextureId,
}

impl TextureManager {
    pub fn new(ctx: &GpuContext) -> Self {
        let mut textures = Arena::new();
        let white_texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Default White Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            mip_level_count: 1,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = white_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &white_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let white_texture_id = textures.insert(GpuTexture {
            texture: white_texture,
            view,
            sampler,
            size: (1, 1),
        });
        tracing::debug!(id = %white_texture_id, "Texture blanche par défaut initialisée");
        Self {
            textures,
            white_texture: white_texture_id,
        }
    }

    pub fn load(&mut self, ctx: &GpuContext, path: &str) -> Result<TextureId, TextureError> {
        let _span = tracing::info_span!("TextureManager::load", path = %path).entered();

        let image = image::open(path)
            .map_err(|source| {
                tracing::error!("Échec du chargement de la texture '{path}' : {source}");
                TextureError::ImageLoad {
                    path: path.to_string(),
                    source,
                }
            })?
            .to_rgba8();

        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            tracing::error!(width, height, "Dimensions d'image invalides");
            return Err(TextureError::InvalidDimensions {
                path: path.to_string(),
                width,
                height,
            });
        }

        let bytes = image.as_raw();

        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(path),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            mip_level_count: 1,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let id = self.textures.insert(GpuTexture {
            texture,
            view,
            sampler,
            size: (width, height),
        });

        tracing::info!(id = %id, width, height, "Texture GPU créée avec succès");
        Ok(id)
    }

    pub fn register(&mut self, texture: GpuTexture) -> TextureId {
        let (w, h) = texture.size;
        let id = self.textures.insert(texture);
        tracing::debug!(id = %id, width = w, height = h, "Texture GPU manuelle enregistrée");
        id
    }

    pub fn remove(&mut self, id: TextureId) -> Result<GpuTexture, TextureError> {
        if let Some(texture) = self.textures.remove(id) {
            tracing::debug!(id = %id, "Texture retirée de l'Arena");
            Ok(texture)
        } else {
            tracing::warn!(id = %id, "Tentative de suppression d'une texture inexistante");
            Err(TextureError::NotFound { id })
        }
    }

    pub fn get(&self, id: TextureId) -> Option<&GpuTexture> {
        self.textures.get(id)
    }

    pub fn get_mut(&mut self, id: TextureId) -> Option<&mut GpuTexture> {
        self.textures.get_mut(id)
    }

    pub fn white_texture(&self) -> TextureId {
        self.white_texture
    }
}
