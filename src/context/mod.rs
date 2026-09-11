//! Le [`GpuContext`] est le pont logique entre la code base et le GPU.
//!
//! Il encapsule les 3 objets fondamentaux de wgpu:
//! - [`wgpu::Device`] c'est le GPU logique. Il crée toutes les ressources — textures,
//! buffers, pipelines, shaders. Tu ne parles jamais au GPU directement,
//! tout passes toujours par device.
//!
//! - [`wgpu::Device`] c'est la file d'envoi des commandes. Quand tu as fini d'enregistrer
//! ce que tu veux dessiner, tu envoies tout d'un coup via queue.submit().
//! Le GPU exécute dans l'ordre.
//!
//! - [`wgpu::Surface`] c'est la fenêtre côté GPU. Elle expose les textures du swapchain —
//! les images à renders avant de les afficher à l'écran.

use wgpu::TextureUsages;
use winit::dpi::PhysicalSize;

use crate::errors::{self};
use std::sync::Arc;

pub struct GpuContext {
    _window: Arc<winit::window::Window>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,
    is_surface_configured: bool,
}

impl GpuContext {
    pub async fn new(
        window: Arc<winit::window::Window>,
    ) -> errors::Result<Self, errors::GpuContextError> {
        let _span = tracing::info_span!("GpuContext::init").entered();
        let size = window.inner_size();
        tracing::debug!(
            width = size.width,
            height = size.height,
            "Taille initiale de la fenêtre"
        );

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::DEBUG | wgpu::InstanceFlags::VALIDATION,
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
            display: None,
        });
        let surface = instance
            .create_surface(window.clone())
            .expect("[GpuContext] Echec lors de la créations de la surface");
        let adapter = {
            let _adapter_span = tracing::info_span!("request_adapter").entered();
            match instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                    apply_limit_buckets: true,
                })
                .await
            {
                Ok(adapter) => {
                    let info = adapter.get_info();
                    tracing::info!(
                        gpu = %info.name,
                        backend = ?info.backend,
                        device_type = ?info.device_type,
                        driver = %info.driver_info,
                        "Adaptateur GPU sélectionné"
                    );
                    adapter
                }
                Err(_) => {
                    tracing::error!("Aucun adapteur trouver utilisation du fallback ...");
                    instance
                        .request_adapter(&wgpu::RequestAdapterOptions {
                            power_preference: wgpu::PowerPreference::LowPower,
                            force_fallback_adapter: false,
                            compatible_surface: Some(&surface),
                            apply_limit_buckets: true,
                        })
                        .await?
                }
            }
        };
        if !adapter.is_surface_supported(&surface) {
            tracing::error!(
                "[GpuContext] L'adaptateur sélectionné ne supporte pas cette Surface !"
            );
        }
        let (device, queue) = {
            let _device_and_queue_span = tracing::info_span!("request_device").entered();
            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                    trace: wgpu::Trace::Off,
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                })
                .await?
        };
        device.on_uncaptured_error(Arc::new(|error| {
            tracing::error!(
                target: "prism::gpu::validation",
                error = %error,
                "Erreur de validation WGPU non capturée !"
            );
        }));

        let limits = device.limits();
        tracing::debug!(
            max_texture_dimension_2d = limits.max_texture_dimension_2d,
            max_bind_groups = limits.max_bind_groups,
            max_uniform_buffer_binding_size = limits.max_uniform_buffer_binding_size,
            "Capacité et limites GPU configurées"
        );

        let surface_cap = surface.get_capabilities(&adapter);
        let surface_format = surface_cap
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_cap.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: if surface_cap.present_modes.contains(&wgpu::PresentMode::Fifo) {
                wgpu::PresentMode::Fifo
            } else {
                surface_cap.present_modes[0]
            },
            alpha_mode: surface_cap.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        tracing::info!(
            width = size.width,
            height = size.height,
            format = ?surface_format,
            present_mode = ?surface_config.present_mode,
            alpha_mode = ?surface_config.alpha_mode,
            "Surface GPU initialisée"
        );
        let mut is_surface_configured = false;
        if size.width > 0 && size.height > 0 {
            surface.configure(&device, &surface_config);
            is_surface_configured = true;
        }

        Ok(Self {
            _window: window,
            device,
            queue,
            surface,
            surface_config,
            size,
            is_surface_configured,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            tracing::debug!(width, height, "Redimensionnement de la surface GPU");
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.size.width = width;
            self.size.height = height;
            self.surface.configure(&self.device, &self.surface_config);
            self.is_surface_configured = true;
        } else {
            tracing::warn!(
                width,
                height,
                "Ignoré : tentative de redimensionnement à 0 (fenêtre minimisée ?)"
            );
        }
    }

    pub fn current_texture(&self) -> wgpu::CurrentSurfaceTexture {
        self.surface.get_current_texture()
    }

    pub fn reconfigure(&self) {
        self.surface.configure(&self.device, &self.surface_config);
    }

    pub fn create_encoder(&self, label: &str) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) })
    }

    pub fn submit(&self, encoder: wgpu::CommandEncoder) {
        tracing::trace!("Soumission des commandes au GPU");
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn present(&self, frame: wgpu::SurfaceTexture) {
        tracing::trace!("Présentation de la frame à la surface");
        self.queue.present(frame);
    }

    pub fn is_ready(&self) -> bool {
        self.is_surface_configured
            && self.surface_config.width > 0
            && self.surface_config.height > 0
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }
}
