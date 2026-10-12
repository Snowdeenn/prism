//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
//! Declarative, device-local binding and pipeline layouts.
use crate::{RenderError, RenderResult, errors::gpu_checked};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    sync::Arc,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
/// Owned group schema. Normalization sorts indices without renumbering or adding groups.
pub struct BindingSchema {
    pub groups: Vec<GroupSchema>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Entries at one explicit WGSL group index.
pub struct GroupSchema {
    pub index: u32,
    pub entries: Vec<BindingEntryDesc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// One binding location, visibility, resource type and optional exact array count.
pub struct BindingEntryDesc {
    pub binding: u32,
    pub visibility: wgpu::ShaderStages,
    pub kind: BindingKind,
    pub count: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Resource type and validation requirements for a shader binding.
pub enum BindingKind {
    Uniform {
        min_size: u64,
        dynamic_offset: bool,
    },
    Storage {
        min_size: u64,
        read_only: bool,
        dynamic_offset: bool,
    },
    Texture {
        sample_type: wgpu::TextureSampleType,
        dimension: wgpu::TextureViewDimension,
        multisampled: bool,
    },
    Sampler {
        kind: wgpu::SamplerBindingType,
    },
    StorageTexture {
        format: wgpu::TextureFormat,
        access: wgpu::StorageTextureAccess,
        dimension: wgpu::TextureViewDimension,
    },
}
impl BindingKind {
    pub fn wgpu_type(&self) -> wgpu::BindingType {
        match *self {
            Self::Uniform {
                min_size,
                dynamic_offset,
            } => wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic_offset,
                min_binding_size: wgpu::BufferSize::new(min_size),
            },
            Self::Storage {
                min_size,
                read_only,
                dynamic_offset,
            } => wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: dynamic_offset,
                min_binding_size: wgpu::BufferSize::new(min_size),
            },
            Self::Texture {
                sample_type,
                dimension,
                multisampled,
            } => wgpu::BindingType::Texture {
                sample_type,
                view_dimension: dimension,
                multisampled,
            },
            Self::Sampler { kind } => wgpu::BindingType::Sampler(kind),
            Self::StorageTexture {
                format,
                access,
                dimension,
            } => wgpu::BindingType::StorageTexture {
                format,
                access,
                view_dimension: dimension,
            },
        }
    }
}
impl BindingEntryDesc {
    pub fn wgpu_entry(&self) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding: self.binding,
            visibility: self.visibility,
            ty: self.kind.wgpu_type(),
            count: self.count.and_then(NonZeroU32::new),
        }
    }
}
impl BindingSchema {
    /// Sort entries and group indices, without renumbering shader groups.
    /// Sort groups and entries and reject duplicates, empty visibility and zero array counts.
    pub fn normalized(&self) -> RenderResult<Self> {
        let mut schema = self.clone();
        schema.groups.sort_by_key(|g| g.index);
        let mut groups = HashSet::new();
        for group in &mut schema.groups {
            if !groups.insert(group.index) {
                return Err(RenderError::new(
                    format!("group {}", group.index),
                    "duplicate group",
                ));
            }
            group.entries.sort_by_key(|e| e.binding);
            let mut bindings = HashSet::new();
            for entry in &group.entries {
                let context = format!("group {} binding {}", group.index, entry.binding);
                if !bindings.insert(entry.binding) {
                    return Err(RenderError::new(context, "duplicate binding"));
                }
                if entry.count == Some(0) {
                    return Err(RenderError::new(context, "binding array must not be empty"));
                }
                if entry.visibility.is_empty() {
                    return Err(RenderError::new(context, "empty shader visibility"));
                }
            }
        }
        Ok(schema)
    }
    /// Normalize and validate against enabled device limits/features. Backend validation
    /// still checks shader compatibility and constraints not represented here.
    pub fn validate(
        &self,
        label: &str,
        limits: &wgpu::Limits,
        features: wgpu::Features,
    ) -> RenderResult<Self> {
        let schema = self
            .normalized()
            .map_err(|e| RenderError::new(label, e.to_string()))?;
        for group in &schema.groups {
            if group.index >= limits.max_bind_groups
                || group.entries.len() > limits.max_bindings_per_bind_group as usize
            {
                return Err(RenderError::new(
                    format!("{label} group {}", group.index),
                    "device binding limits exceeded",
                ));
            }
            for entry in &group.entries {
                let fail = |message| {
                    RenderError::new(
                        format!("{label} group {} binding {}", group.index, entry.binding),
                        message,
                    )
                };
                if entry.binding >= limits.max_bindings_per_bind_group {
                    return Err(fail("binding index exceeds device limit"));
                }
                match entry.kind {
                    BindingKind::Uniform { min_size, .. }
                        if min_size > limits.max_uniform_buffer_binding_size =>
                    {
                        return Err(fail("uniform size exceeds device limit"));
                    }
                    BindingKind::Storage { min_size, .. }
                        if min_size > limits.max_storage_buffer_binding_size =>
                    {
                        return Err(fail("storage size exceeds device limit"));
                    }
                    _ => (),
                }
                if entry.count.is_some() {
                    let required = match entry.kind {
                        BindingKind::Uniform { .. } => wgpu::Features::BUFFER_BINDING_ARRAY,
                        BindingKind::Storage { .. } => {
                            wgpu::Features::BUFFER_BINDING_ARRAY
                                | wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY
                        }
                        BindingKind::StorageTexture { .. } => {
                            wgpu::Features::TEXTURE_BINDING_ARRAY
                                | wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY
                        }
                        _ => wgpu::Features::TEXTURE_BINDING_ARRAY,
                    };
                    if !features.contains(required) {
                        return Err(fail("binding array features are not enabled"));
                    }
                }
            }
        }
        Ok(schema)
    }
}

#[derive(Debug, Clone, Copy, Default)]
/// Cumulative successful cache hits and insertions; clearing caches preserves counters.
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}
#[derive(Clone)]
/// Normalized schema and shared device-local layouts; absent group indices remain holes.
pub struct ResolvedLayout {
    pub key: BindingSchema,
    pub groups: Vec<Option<Arc<wgpu::BindGroupLayout>>>,
    pub pipeline: Arc<wgpu::PipelineLayout>,
}
/// Owns the device so a layout can never silently be reused on another device.
/// Device-local structural cache for bind group layouts and pipeline layouts.
pub struct LayoutCache {
    device: wgpu::Device,
    groups: HashMap<Vec<BindingEntryDesc>, Arc<wgpu::BindGroupLayout>>,
    pipelines: HashMap<BindingSchema, ResolvedLayout>,
    pub group_stats: CacheStats,
    pub pipeline_stats: CacheStats,
}
impl LayoutCache {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            groups: HashMap::new(),
            pipelines: HashMap::new(),
            group_stats: CacheStats::default(),
            pipeline_stats: CacheStats::default(),
        }
    }
    /// Validate the schema and reuse or create its group and pipeline layouts.
    pub fn resolve(&mut self, label: &str, schema: &BindingSchema) -> RenderResult<ResolvedLayout> {
        let key = schema.validate(label, &self.device.limits(), self.device.features())?;
        if let Some(layout) = self.pipelines.get(&key) {
            self.pipeline_stats.hits += 1;
            return Ok(layout.clone());
        }
        let mut groups = vec![None; key.groups.last().map_or(0, |g| g.index as usize + 1)];
        for group in &key.groups {
            let layout = if let Some(layout) = self.groups.get(&group.entries) {
                self.group_stats.hits += 1;
                layout.clone()
            } else {
                let entries: Vec<_> = group
                    .entries
                    .iter()
                    .map(BindingEntryDesc::wgpu_entry)
                    .collect();
                let label = format!("{label} group {}", group.index);
                let layout = Arc::new(gpu_checked(&self.device, &label, || {
                    self.device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: Some(&label),
                            entries: &entries,
                        })
                })?);
                self.groups.insert(group.entries.clone(), layout.clone());
                self.group_stats.misses += 1;
                layout
            };
            groups[group.index as usize] = Some(layout);
        }
        let refs: Vec<_> = groups.iter().map(|g| g.as_deref()).collect();
        let pipeline = Arc::new(gpu_checked(&self.device, label, || {
            self.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(label),
                    bind_group_layouts: &refs,
                    immediate_size: 0,
                })
        })?);
        let result = ResolvedLayout {
            key: key.clone(),
            groups,
            pipeline,
        };
        self.pipelines.insert(key, result.clone());
        self.pipeline_stats.misses += 1;
        Ok(result)
    }
    /// Drop cached layouts without resetting statistics or invalidating existing shared owners.
    pub fn clear(&mut self) {
        self.groups.clear();
        self.pipelines.clear();
    }
}
