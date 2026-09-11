use thiserror::Error;
use utils::ids::BufferId;

pub type Result<T, E = PrismError> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum PrismError {
    #[error("Erreur GPU : {0}")]
    Gpu(#[from] GpuContextError),

    #[error("Erreur de rendu texte : {0}")]
    Text(#[from] TextRendererError),

    #[error("Erreur de shader : {0}")]
    Shader(#[from] ShaderError),

    #[error("Erreur de texture : {0}")]
    Texture(#[from] TextureError),

    #[error("Erreur de buffer : {0}")]
    Buffer(#[from] BufferError),

    #[error("Erreur de pipeline : {0}")]
    Pipeline(#[from] PipelineError),

    #[error("Erreur de passe de rendu : {0}")]
    Pass(#[from] PassError),
}

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

#[derive(Error, Debug)]
pub enum TextRendererError {
    #[error("Impossible de préparer les rendu glyphon")]
    PrepareError(#[from] glyphon::PrepareError),
    #[error("Echec du rendu glyphon")]
    RenderError(#[from] glyphon::RenderError),
}

#[derive(Error, Debug)]
pub enum ShaderError {
    #[error("Impossible de lire le fichier shader '{path}' : {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Shader introuvable (ID : {id})")]
    NotFound { id: utils::ids::ShaderId },

    #[error("Impossible de recharger un shader inline (aucun chemin de fichier associé)")]
    InlineReload,
}

#[derive(Error, Debug)]
pub enum TextureError {
    #[error("Impossible de charger ou décoder l'image '{path}' : {source}")]
    ImageLoad {
        path: String,
        #[source]
        source: image::ImageError,
    },

    #[error("L'image '{path}' a des dimensions invalides ({width}x{height})")]
    InvalidDimensions {
        path: String,
        width: u32,
        height: u32,
    },

    #[error("Texture introuvable (ID : {id})")]
    NotFound { id: utils::ids::TextureId },
}

#[derive(Error, Debug)]
pub enum BufferError {
    #[error("Buffer introuvable (ID : {id})")]
    NotFound { id: BufferId },

    #[error("Taille de buffer invalide : {size} octets")]
    InvalidSize { size: u64 },

    #[error(
        "Capacité du buffer dépassée pour {id} : écriture de {required} octets dans un buffer de {available} octets"
    )]
    Overflow {
        id: BufferId,
        required: u64,
        available: u64,
    },

    #[error("Le buffer {id} ne possède pas le flag COPY_DST requis pour l'écriture")]
    MissingCopyDstUsage { id: BufferId },
}

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Shader introuvable pour la création du pipeline (ID : {id})")]
    ShaderNotFound { id: utils::ids::ShaderId },
}

#[derive(Error, Debug)]
pub enum PassError {
    #[error("Erreur de pipeline dans la passe de rendu : {0}")]
    Pipeline(#[from] PipelineError),

    #[error("Erreur de buffer dans la passe de rendu : {0}")]
    Buffer(#[from] BufferError),

    #[error("Layouts de bind group introuvables pour le pipeline spécifié")]
    LayoutsNotFound,
}
