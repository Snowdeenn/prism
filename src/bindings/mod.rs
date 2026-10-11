pub struct BindingSchema {
    pub groups: Vec<GroupSchema>,
}

pub struct GroupSchema {
    pub index: u32,
    pub entries: Vec<BindingEntryDesc>,
}

pub struct BindingEntryDesc {
    pub binding: u32,
    pub visibility: wgpu::ShaderStages,
    pub kind: BindingKind,
    pub count: Option<u32>,
}

pub enum BindingKind {
    Uniform { min_size: u64, dynamic_offset: bool },
    Storage { min_size: u64, read_only: bool, dynamic_offset: bool },
    Texture { sample_type: wgpu::TextureSampleType, dimension: wgpu::TextureViewDimension },
    Sampler { kind: wgpu::SamplerBindingType },
    StorageTexture { format: wgpu::TextureFormat, access: wgpu::StorageTextureAccess },
}
