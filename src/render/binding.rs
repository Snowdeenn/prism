use crate::{
    RenderError, RenderResult,
    errors::gpu_checked,
    material::*,
    pipeline::{PipelineCache, PipelineKey, ShaderStageKey},
    resource::manager::ResourceManager,
};
use std::{collections::BTreeMap, sync::Arc};

pub(super) enum ResolvedBinding {
    Buffer(wgpu::Buffer, u64, u64),
    Texture(wgpu::TextureView),
    Sampler(wgpu::Sampler),
    Buffers(Vec<(wgpu::Buffer, u64, u64)>),
    Textures(Vec<wgpu::TextureView>),
    Samplers(Vec<wgpu::Sampler>),
}
pub(super) type Overrides = BTreeMap<(u32, u32), ResolvedBinding>;
pub(super) struct PreparedGroup {
    pub index: u32,
    pub group: wgpu::BindGroup,
    pub offsets: Vec<u32>,
}
pub(super) fn uniform(
    device: &wgpu::Device,
    label: &str,
    bytes: &[u8],
) -> RenderResult<ResolvedBinding> {
    use wgpu::util::DeviceExt;
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return Err(RenderError::new(
            label,
            "uniform bytes must be non-empty and four-byte aligned",
        ));
    }
    let buffer = gpu_checked(device, label, || {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        })
    })?;
    Ok(ResolvedBinding::Buffer(buffer, 0, bytes.len() as u64))
}
fn resolve(
    resources: &ResourceManager,
    resource: &BindingResource,
) -> RenderResult<ResolvedBinding> {
    Ok(match resource {
        BindingResource::Buffer {
            handle,
            offset,
            size,
        } => ResolvedBinding::Buffer(resources.buffer(*handle)?.raw.clone(), *offset, *size),
        BindingResource::Texture(id) => ResolvedBinding::Texture(resources.view(*id)?.raw.clone()),
        BindingResource::Sampler(id) => {
            ResolvedBinding::Sampler(resources.sampler(*id)?.raw.clone())
        }
        BindingResource::BufferArray(ids) => ResolvedBinding::Buffers(
            ids.iter()
                .map(|(id, o, s)| Ok((resources.buffer(*id)?.raw.clone(), *o, *s)))
                .collect::<RenderResult<_>>()?,
        ),
        BindingResource::TextureArray(ids) => ResolvedBinding::Textures(
            ids.iter()
                .map(|id| Ok(resources.view(*id)?.raw.clone()))
                .collect::<RenderResult<_>>()?,
        ),
        BindingResource::SamplerArray(ids) => ResolvedBinding::Samplers(
            ids.iter()
                .map(|id| Ok(resources.sampler(*id)?.raw.clone()))
                .collect::<RenderResult<_>>()?,
        ),
    })
}
fn stage(resources: &ResourceManager, entry: &ShaderEntry) -> RenderResult<ShaderStageKey> {
    Ok(ShaderStageKey {
        shader: resources.shader_revision(entry.shader)?,
        entry_point: entry.entry_point.clone(),
        constants: entry
            .constants
            .iter()
            .map(|(n, v)| (n.clone(), v.to_bits()))
            .collect(),
        zero_initialize_workgroup_memory: true,
    })
}
pub(super) fn prepare_material(
    resources: &ResourceManager,
    cache: &mut PipelineCache,
    template: &MaterialTemplate,
    sets: &[BindingSetUse],
    mut values: Overrides,
) -> RenderResult<(Arc<wgpu::RenderPipeline>, Vec<PreparedGroup>)> {
    let device = resources.device();
    for group in &template.schema.groups {
        for entry in &group.entries {
            if matches!(
                entry.kind,
                crate::bindings::BindingKind::Storage {
                    read_only: false,
                    ..
                } | crate::bindings::BindingKind::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly
                        | wgpu::StorageTextureAccess::ReadWrite
                        | wgpu::StorageTextureAccess::Atomic,
                    ..
                }
            ) {
                return Err(RenderError::new(
                    format!(
                        "{} group {} binding {}",
                        template.label, group.index, entry.binding
                    ),
                    "writable storage requires an explicit GraphPass resource declaration",
                ));
            }
        }
    }
    let key = PipelineKey {
        vertex: stage(resources, &template.vertex)?,
        fragment: template
            .fragment
            .as_ref()
            .map(|e| stage(resources, e))
            .transpose()?,
        layout: template.schema.clone(),
        vertex_layouts: template.vertex_layouts.clone(),
        primitive: template.primitive,
        depth_stencil: template.depth_stencil.clone(),
        targets: template.targets.clone(),
        multisample: template.multisample,
        multiview_mask: template.multiview_mask,
    };
    let pipeline = cache.render(
        &template.label,
        &key,
        &resources.shader(template.vertex.shader)?.module,
        template
            .fragment
            .as_ref()
            .map(|s| resources.shader(s.shader).map(|s| &s.module))
            .transpose()?,
    )?;
    let layout = cache.layouts.resolve(&template.label, &template.schema)?;
    let mut offsets = BTreeMap::new();
    for set_use in sets {
        let set = resources.bindings(set_use.handle)?;
        if offsets
            .insert(set.group, set_use.dynamic_offsets.clone())
            .is_some()
        {
            return Err(RenderError::new(
                &template.label,
                format!("duplicate binding set for group {}", set.group),
            ));
        }
        for (binding, value) in &set.entries {
            if values
                .insert((set.group, *binding), resolve(resources, value)?)
                .is_some()
            {
                return Err(RenderError::new(
                    &template.label,
                    format!("duplicate group {} binding {binding}", set.group),
                ));
            }
        }
    }
    let mut groups = Vec::new();
    for schema in &layout.key.groups {
        let label = format!("{} group {}", template.label, schema.index);
        let dynamic_offsets = offsets.remove(&schema.index).unwrap_or_default();
        let mut dynamic_index = 0;
        let mut resolved = Vec::new();
        for entry in &schema.entries {
            let context = format!("{label} binding {}", entry.binding);
            let value = values
                .remove(&(schema.index, entry.binding))
                .ok_or_else(|| RenderError::new(&context, "missing resource"))?;
            let expected_count = entry.count;
            let actual_count = match &value {
                ResolvedBinding::Buffers(v) => Some(v.len() as u32),
                ResolvedBinding::Textures(v) => Some(v.len() as u32),
                ResolvedBinding::Samplers(v) => Some(v.len() as u32),
                _ => None,
            };
            if actual_count != expected_count {
                return Err(RenderError::new(
                    &context,
                    "resource array count differs from layout",
                ));
            }
            if let wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset,
                min_binding_size,
            } = entry.kind.wgpu_type()
            {
                let alignment = if ty == wgpu::BufferBindingType::Uniform {
                    device.limits().min_uniform_buffer_offset_alignment
                } else {
                    device.limits().min_storage_buffer_offset_alignment
                } as u64;
                let usage = if ty == wgpu::BufferBindingType::Uniform {
                    wgpu::BufferUsages::UNIFORM
                } else {
                    wgpu::BufferUsages::STORAGE
                };
                let dynamic = if has_dynamic_offset {
                    let offset = *dynamic_offsets
                        .get(dynamic_index)
                        .ok_or_else(|| RenderError::new(&context, "missing dynamic offset"))?
                        as u64;
                    dynamic_index += 1;
                    offset
                } else {
                    0
                };
                let check = |buffer: &wgpu::Buffer, offset: u64, size: u64| -> RenderResult<()> {
                    let end = offset
                        .checked_add(dynamic)
                        .and_then(|o| o.checked_add(size));
                    if size == 0
                        || size < min_binding_size.map_or(0, |s| s.get())
                        || !offset.is_multiple_of(alignment)
                        || !dynamic.is_multiple_of(alignment)
                        || end.is_none_or(|end| end > buffer.size())
                        || !buffer.usage().contains(usage)
                    {
                        return Err(RenderError::new(
                            &context,
                            "incompatible buffer usage, size, range or offset alignment",
                        ));
                    }
                    Ok(())
                };
                match &value {
                    ResolvedBinding::Buffer(b, o, s) => check(b, *o, *s)?,
                    ResolvedBinding::Buffers(bs) if !has_dynamic_offset => {
                        for (b, o, s) in bs {
                            check(b, *o, *s)?;
                        }
                    }
                    _ => return Err(RenderError::new(&context, "expected buffer binding")),
                }
            }
            resolved.push((entry.binding, value));
        }
        if dynamic_index != dynamic_offsets.len() {
            return Err(RenderError::new(&label, "unexpected dynamic offsets"));
        }
        // Build array references only after all owned handles have stable locations.
        let buffer_arrays: Vec<Vec<_>> = resolved
            .iter()
            .map(|(_, r)| match r {
                ResolvedBinding::Buffers(v) => v
                    .iter()
                    .map(|(buffer, offset, size)| wgpu::BufferBinding {
                        buffer,
                        offset: *offset,
                        size: wgpu::BufferSize::new(*size),
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        let texture_arrays: Vec<Vec<_>> = resolved
            .iter()
            .map(|(_, r)| match r {
                ResolvedBinding::Textures(v) => v.iter().collect(),
                _ => Vec::new(),
            })
            .collect();
        let sampler_arrays: Vec<Vec<_>> = resolved
            .iter()
            .map(|(_, r)| match r {
                ResolvedBinding::Samplers(v) => v.iter().collect(),
                _ => Vec::new(),
            })
            .collect();
        let entries: Vec<_> = resolved
            .iter()
            .enumerate()
            .map(|(i, (binding, value))| wgpu::BindGroupEntry {
                binding: *binding,
                resource: match value {
                    ResolvedBinding::Buffer(buffer, offset, size) => {
                        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer,
                            offset: *offset,
                            size: wgpu::BufferSize::new(*size),
                        })
                    }
                    ResolvedBinding::Texture(view) => wgpu::BindingResource::TextureView(view),
                    ResolvedBinding::Sampler(sampler) => wgpu::BindingResource::Sampler(sampler),
                    ResolvedBinding::Buffers(_) => {
                        wgpu::BindingResource::BufferArray(&buffer_arrays[i])
                    }
                    ResolvedBinding::Textures(_) => {
                        wgpu::BindingResource::TextureViewArray(&texture_arrays[i])
                    }
                    ResolvedBinding::Samplers(_) => {
                        wgpu::BindingResource::SamplerArray(&sampler_arrays[i])
                    }
                },
            })
            .collect();
        let group = gpu_checked(device, &label, || {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&label),
                layout: layout.groups[schema.index as usize]
                    .as_ref()
                    .expect("resolved schema group"),
                entries: &entries,
            })
        })?;
        groups.push(PreparedGroup {
            index: schema.index,
            group,
            offsets: dynamic_offsets,
        });
    }
    if !values.is_empty() || !offsets.is_empty() {
        return Err(RenderError::new(
            &template.label,
            "resources provided for an undeclared group/binding",
        ));
    }
    Ok((pipeline, groups))
}
