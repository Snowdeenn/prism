use thiserror::Error;

#[derive(Error, Debug)]
pub enum GpuContextError {
    #[error("Impossible de créer la surface d'affichage WGPU : {0}")]
    SurfaceCreation(#[from] wgpu::CreateSurfaceError),

    #[error("Aucun adaptateur GPU compatible n'a été trouvé sur cette machine")]
    AdapterNotFound(#[from] wgpu::RequestAdapterError),

    #[error("L'adaptateur sélectionné ne supporte pas la surface de la fenêtre")]
    AdapterNotSupported,

    #[error("Échec lors de la récupération du périphérique GPU (Device) : {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),

    #[error("Taille de fenêtre invalide à l'initialisation ({width}x{height})")]
    InvalidWindowSize { width: u32, height: u32 },

    #[error("Erreur de reconfiguration de la surface d'affichage")]
    SurfaceConfigFailed,
}

/// Recoverable description, resource, graph or GPU validation error in the V2 API.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("{context}: {message}")]
pub struct RenderError {
    pub context: String,
    pub message: String,
}

impl RenderError {
    pub fn new(context: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            context: context.into(),
            message: message.into(),
        }
    }
}

pub type RenderResult<T> = std::result::Result<T, RenderError>;

pub(crate) fn gpu_checked<T>(
    device: &wgpu::Device,
    label: &str,
    create: impl FnOnce() -> T,
) -> RenderResult<T> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let value = create();
    match pollster::block_on(scope.pop()) {
        Some(error) => Err(RenderError::new(label, error.to_string())),
        None => Ok(value),
    }
}
