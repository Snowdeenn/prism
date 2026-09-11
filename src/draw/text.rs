use std::collections::HashMap;

use crate::{context::GpuContext, draw::commands::DrawCommand, errors::TextRendererError};

pub struct TextBufferCache {
    pub font_system: glyphon::FontSystem,
    pub buffers: HashMap<(String, u32), glyphon::Buffer>,
}

impl Default for TextBufferCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBufferCache {
    pub fn new() -> Self {
        Self {
            font_system: glyphon::FontSystem::new(),
            buffers: HashMap::new(),
        }
    }

    /// Récupère un buffer mis en cache ou en génère un nouveau via l'Entry API.
    pub fn get_or_create(
        &mut self,
        text: &str,
        size: f32,
        max_width: f32,
        max_height: f32,
    ) -> &glyphon::Buffer {
        let key = (text.to_string(), size.to_bits());
        let font_system = &mut self.font_system;

        self.buffers.entry(key).or_insert_with(|| {
            let metrics = glyphon::Metrics::new(size, size * 1.2);
            let mut buffer = glyphon::Buffer::new(font_system, metrics);
            buffer.set_size(Some(max_width), Some(max_height));
            buffer.set_text(
                text,
                &glyphon::Attrs::new(),
                glyphon::Shaping::Basic,
                Some(glyphon::cosmic_text::Align::Left),
            );
            buffer.shape_until_scroll(font_system, true);
            buffer
        })
    }

    pub fn font_system_mut(&mut self) -> &mut glyphon::FontSystem {
        &mut self.font_system
    }

    /// Vider le cache de buffers de texte.
    pub fn clear(&mut self) {
        self.buffers.clear();
    }
}

pub struct TextRenderer {
    cache_text: TextBufferCache,
    swash_cache: glyphon::SwashCache,
    viewport: glyphon::Viewport,
    atlas: glyphon::TextAtlas,
    renderer: glyphon::TextRenderer,
}

impl TextRenderer {
    pub fn new(ctx: &GpuContext, surface_format: wgpu::TextureFormat) -> Self {
        let mut cache_text = TextBufferCache::new();
        let swash_cache = glyphon::SwashCache::new();
        let cache = &glyphon::Cache::new(&ctx.device);
        let viewport = glyphon::Viewport::new(&ctx.device, cache);
        let mut atlas = glyphon::TextAtlas::new(&ctx.device, &ctx.queue, cache, surface_format);
        let renderer = glyphon::TextRenderer::new(
            &mut atlas,
            &ctx.device,
            wgpu::MultisampleState::default(),
            None,
        );

        cache_text
            .font_system
            .db_mut()
            .load_font_data(include_bytes!("../../../assets/pixel/Pixel Coleco.otf").to_vec());
        cache_text
            .font_system
            .db_mut()
            .set_sans_serif_family("Pixel Coleco");

        Self {
            cache_text,
            swash_cache,
            viewport,
            atlas,
            renderer,
        }
    }

    pub fn load_font_bytes(&mut self, data: Vec<u8>) {
        self.cache_text.font_system.db_mut().load_font_data(data);
    }

    pub fn prepare(
        &mut self,
        ctx: &GpuContext,
        commands: &[DrawCommand],
    ) -> Result<(), TextRendererError> {
        let _span = tracing::trace_span!("TextRenderer::prepare").entered();

        self.viewport.update(
            &ctx.queue,
            glyphon::Resolution {
                width: ctx.size.width,
                height: ctx.size.height,
            },
        );

        let mut text_commands = Vec::with_capacity(commands.len());
        for cmd in commands {
            if let DrawCommand::Text {
                content,
                pos,
                size,
                color,
                ..
            } = cmd
            {
                text_commands.push((content.as_str(), *pos, *size, *color));
            }
        }

        if text_commands.is_empty() {
            return Ok(());
        }

        for (content, _, size, _) in &text_commands {
            self.cache_text.get_or_create(
                content,
                *size,
                ctx.size.width as f32,
                ctx.size.height as f32,
            );
        }

        let mut text_areas = Vec::with_capacity(text_commands.len());
        for (content, pos, size, color) in &text_commands {
            let key = (content.to_string(), size.to_bits());

            if let Some(buffer) = self.cache_text.buffers.get(&key) {
                text_areas.push(glyphon::TextArea {
                    buffer,
                    left: pos[0],
                    top: pos[1],
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: 0,
                        top: 0,
                        right: ctx.size.width as i32,
                        bottom: ctx.size.height as i32,
                    },
                    default_color: glyphon::Color::rgba(
                        (color[0] * 255.0).clamp(0.0, 255.0) as u8,
                        (color[1] * 255.0).clamp(0.0, 255.0) as u8,
                        (color[2] * 255.0).clamp(0.0, 255.0) as u8,
                        (color[3] * 255.0).clamp(0.0, 255.0) as u8,
                    ),
                    custom_glyphs: &[],
                });
            } else {
                tracing::warn!(
                    text = %content,
                    "Buffer de texte introuvable dans le cache lors du prepare"
                );
            }
        }

        self.renderer.prepare(
            &ctx.device,
            &ctx.queue,
            &mut self.cache_text.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        )?;

        Ok(())
    }

    pub fn render<'a>(&mut self, pass: &mut wgpu::RenderPass<'a>) -> Result<(), TextRendererError> {
        let _span = tracing::trace_span!("TextRenderer::render").entered();
        self.renderer.render(&self.atlas, &self.viewport, pass)?;
        Ok(())
    }

    pub fn trim(&mut self) {
        self.atlas.trim();
        self.cache_text.clear();
    }
}
