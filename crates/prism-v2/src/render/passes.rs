//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
use super::{GraphPass, PrepareContext, binding::*};
use crate::{
    RenderError, RenderResult, bindings::*, frame::v2::PhaseSubmission, graph::*, ids::*,
    material::BindingSetUse, render_object::*, resource::manager::ResourceManager,
};
use std::{collections::BTreeMap, ops::Range, sync::Arc};

/// Clear a complete color or depth attachment; no viewport or scissor is applied.
pub struct ClearPass {
    name: String,
    target: ResourceVersion,
    color: Option<wgpu::Color>,
    depth: f32,
}
impl ClearPass {
    pub fn color(name: impl Into<String>, target: ResourceVersion, color: wgpu::Color) -> Self {
        Self {
            name: name.into(),
            target,
            color: Some(color),
            depth: 1.0,
        }
    }
    pub fn depth(name: impl Into<String>, target: ResourceVersion, depth: f32) -> Self {
        Self {
            name: name.into(),
            target,
            color: None,
            depth,
        }
    }
}
impl GraphPass for ClearPass {
    fn declare(&self) -> PassDescriptor {
        PassDescriptor::new(&self.name).access(
            self.target,
            if self.color.is_some() {
                AccessKind::ColorClear
            } else {
                AccessKind::DepthClear
            },
        )
    }
    fn prepare(&mut self, _: &mut PrepareContext<'_>, _: &PassResources<'_>) -> RenderResult<()> {
        Ok(())
    }
    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &PassResources<'_>,
    ) -> RenderResult<()> {
        let view = resources.view(self.target)?;
        let color = self
            .color
            .map(|value| color_attachment(view, wgpu::LoadOp::Clear(value)));
        let colors = [color];
        let depth = if self.color.is_none() {
            Some(wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.depth),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            })
        } else {
            None
        };
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&self.name),
            color_attachments: if self.color.is_some() { &colors } else { &[] },
            depth_stencil_attachment: depth,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        Ok(())
    }
}
fn color_attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    }
}
struct Draw {
    pipeline: Arc<wgpu::RenderPipeline>,
    groups: Vec<PreparedGroup>,
    vertices: Vec<(u32, wgpu::Buffer, Range<u64>)>,
    index: Option<(wgpu::Buffer, Range<u64>, wgpu::IndexFormat)>,
    elements: Range<u32>,
    base_vertex: i32,
    instances: Range<u32>,
}
impl Draw {
    fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        for group in &self.groups {
            pass.set_bind_group(group.index, &group.group, &group.offsets);
        }
        for (slot, buffer, range) in &self.vertices {
            pass.set_vertex_buffer(*slot, buffer.slice(range.clone()));
        }
        if let Some((buffer, range, format)) = &self.index {
            pass.set_index_buffer(buffer.slice(range.clone()), *format);
            pass.draw_indexed(
                self.elements.clone(),
                self.base_vertex,
                self.instances.clone(),
            );
        } else {
            pass.draw(self.elements.clone(), self.instances.clone());
        }
    }
}
/// Prepare and draw objects matching one view phase with stable sorting.
/// Color uses a load successor; depth can be read-only or a load successor for writes.
pub struct PhasePass {
    pub target: ResourceVersion,
    pub depth: Option<ResourceVersion>,
    pub write_depth: bool,
    pub view: usize,
    pub phase: PhaseSubmission,
    draws: Vec<Draw>,
    viewport: Option<[f32; 4]>,
}
impl PhasePass {
    pub fn new(
        target: ResourceVersion,
        depth: Option<ResourceVersion>,
        write_depth: bool,
        view: usize,
        phase: PhaseSubmission,
    ) -> Self {
        Self {
            target,
            depth,
            write_depth,
            view,
            phase,
            draws: Vec::new(),
            viewport: None,
        }
    }
}
impl GraphPass for PhasePass {
    fn declare(&self) -> PassDescriptor {
        let mut desc =
            PassDescriptor::new(format!("view {} phase {}", self.view, self.phase.phase.0))
                .access(self.target, AccessKind::ColorLoad);
        if let Some(depth) = self.depth {
            desc = desc.access(
                depth,
                if self.write_depth {
                    AccessKind::DepthLoad
                } else {
                    AccessKind::DepthRead
                },
            );
        }
        desc
    }
    fn prepare(
        &mut self,
        context: &mut PrepareContext<'_>,
        graph: &PassResources<'_>,
    ) -> RenderResult<()> {
        self.draws.clear();
        let view = context
            .frame
            .views
            .get(self.view)
            .ok_or_else(|| RenderError::new("phase", "missing view"))?;
        self.viewport = view.viewport;
        let GraphResource::Texture {
            texture: target, ..
        } = graph.get(self.target)?
        else {
            return Err(RenderError::new("phase", "color target must be texture"));
        };
        if let Some([x, y, w, h]) = self.viewport
            && ([x, y, w, h].iter().any(|v| !v.is_finite())
                || x < 0.0
                || y < 0.0
                || w <= 0.0
                || h <= 0.0
                || x + w > target.width() as f32
                || y + h > target.height() as f32)
        {
            return Err(RenderError::new("view", "invalid viewport"));
        }
        let depth_format = self
            .depth
            .map(|d| match graph.get(d)? {
                GraphResource::Texture { texture, .. } => {
                    if texture.size() != target.size()
                        || texture.sample_count() != target.sample_count()
                    {
                        return Err(RenderError::new(
                            "phase",
                            "depth attachment dimensions/sample count differ",
                        ));
                    }
                    Ok(texture.format())
                }
                _ => Err(RenderError::new("phase", "depth target must be texture")),
            })
            .transpose()?;
        let mut objects: Vec<_> = context
            .frame
            .objects
            .iter()
            .filter(|o| o.phase == self.phase.phase && !o.instances.is_empty())
            .collect();
        match self.phase.sort {
            SortPolicy::Opaque => objects.sort_by_key(|o| {
                let template = context
                    .resources
                    .material(o.material)
                    .map(|m| (m.template.index, m.template.generation))
                    .unwrap_or((usize::MAX, 0));
                (
                    template,
                    o.material.index,
                    o.material.generation,
                    o.sort_key,
                )
            }),
            SortPolicy::BackToFront => objects.sort_by_key(|o| std::cmp::Reverse(o.sort_key)),
            SortPolicy::SubmissionOrder => objects.sort_by_key(|o| o.sort_key),
        }
        for object in objects {
            let prepare = |material| {
                prepare_object(
                    context.resources,
                    context.pipelines,
                    object,
                    material,
                    &view.bindings,
                    target.format(),
                    target.sample_count(),
                    depth_format,
                    self.write_depth,
                )
            };
            let mut prepare = prepare;
            match prepare(object.material) {
                Ok(draw) => self.draws.push(draw),
                Err(error) => {
                    context.report.skipped_objects.push(error);
                    if let Some(fallback) = context.fallback_material {
                        match prepare_object(
                            context.resources,
                            context.pipelines,
                            object,
                            fallback,
                            &view.bindings,
                            target.format(),
                            target.sample_count(),
                            depth_format,
                            self.write_depth,
                        ) {
                            Ok(draw) => self.draws.push(draw),
                            Err(error) => context.report.skipped_objects.push(error),
                        }
                    }
                }
            }
        }
        context.report.draw_calls += self.draws.len() as u64;
        Ok(())
    }
    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &PassResources<'_>,
    ) -> RenderResult<()> {
        let colors = [Some(color_attachment(
            resources.view(self.target)?,
            wgpu::LoadOp::Load,
        ))];
        let depth = self
            .depth
            .map(|d| {
                resources
                    .view(d)
                    .map(|view| wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: self.write_depth.then_some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    })
            })
            .transpose()?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("phase"),
            color_attachments: &colors,
            depth_stencil_attachment: depth,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some([x, y, w, h]) = self.viewport {
            pass.set_viewport(x, y, w, h, 0., 1.);
        }
        for draw in &self.draws {
            draw.encode(&mut pass);
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
fn prepare_object(
    resources: &ResourceManager,
    pipelines: &mut crate::pipeline::PipelineCache,
    object: &RenderObject,
    material_id: MaterialHandle,
    view_bindings: &[BindingSetUse],
    format: wgpu::TextureFormat,
    samples: u32,
    depth: Option<wgpu::TextureFormat>,
    write_depth: bool,
) -> RenderResult<Draw> {
    let material = resources.material(material_id)?;
    let template = resources.template(material.template)?;
    validate_target(template, format, samples)?;
    if template.depth_stencil.as_ref().map(|d| d.format) != depth
        || (!write_depth
            && template.depth_stencil.as_ref().is_some_and(|d| {
                d.depth_write_enabled == Some(true)
                    || !d.stencil.is_read_only(template.primitive.cull_mode)
            }))
    {
        return Err(RenderError::new(
            &template.label,
            "incompatible depth state or write to read-only depth",
        ));
    }
    // Shared view bindings are defaults for families that declare those groups.
    // A procedural/fullscreen family may legitimately omit the view group.
    let mut sets = Vec::new();
    for binding in view_bindings {
        let group = resources.bindings(binding.handle)?.group;
        if template
            .schema
            .groups
            .iter()
            .any(|schema| schema.index == group)
        {
            sets.push(binding.clone());
        }
    }
    sets.extend(material.bindings.iter().copied().map(Into::into));
    sets.extend(object.object_bindings.clone());
    let mut overrides = BTreeMap::new();
    if let Some(p) = &material.parameters {
        overrides.insert(
            (p.group, p.binding),
            uniform(resources.device(), &template.label, &p.bytes)?,
        );
    }
    let (pipeline, groups) = prepare_material(resources, pipelines, template, &sets, overrides)?;
    let geometry = resources.geometry(object.geometry)?;
    let mut vertices = Vec::new();
    let mut slots = std::collections::HashSet::new();
    for vertex in &geometry.vertices {
        let buffer = resources.buffer(vertex.buffer)?.raw();
        if !slots.insert(vertex.slot)
            || !buffer.usage().contains(wgpu::BufferUsages::VERTEX)
            || vertex.range.is_empty()
            || vertex.range.end > buffer.size()
            || !vertex.range.start.is_multiple_of(4)
        {
            return Err(RenderError::new(
                "geometry",
                "invalid vertex buffer slot/range/usage",
            ));
        }
        vertices.push((vertex.slot, buffer.clone(), vertex.range.clone()));
    }
    for (slot, layout) in template.vertex_layouts.iter().enumerate() {
        if let Some(layout) = layout {
            let (_, _, range) = vertices
                .iter()
                .find(|(s, _, _)| *s == slot as u32)
                .ok_or_else(|| RenderError::new("geometry", "missing vertex/instance buffer"))?;
            let count = if layout.step_mode == wgpu::VertexStepMode::Instance {
                object.instances.end as u64
            } else if geometry.indices.is_none() {
                geometry.elements.end as u64
            } else {
                0
            };
            let last_attribute = layout
                .attributes
                .iter()
                .map(|a| a.offset + a.format.size())
                .max()
                .unwrap_or(0);
            let required = if count == 0 {
                0
            } else {
                (count - 1)
                    .checked_mul(layout.stride)
                    .and_then(|n| n.checked_add(last_attribute))
                    .unwrap_or(u64::MAX)
            };
            if required > range.end - range.start {
                return Err(RenderError::new(
                    "geometry",
                    "vertex/instance range exceeds bound buffer",
                ));
            }
        }
    }
    let index = geometry
        .indices
        .as_ref()
        .map(|index| {
            let buffer = resources.buffer(index.buffer)?.raw();
            let size = if index.format == wgpu::IndexFormat::Uint32 {
                4
            } else {
                2
            };
            if !buffer.usage().contains(wgpu::BufferUsages::INDEX)
                || index.range.is_empty()
                || index.range.end > buffer.size()
                || !index.range.start.is_multiple_of(size)
                || geometry.elements.end as u64 * size > index.range.end - index.range.start
            {
                return Err(RenderError::new(
                    "geometry",
                    "invalid index buffer range/usage",
                ));
            }
            Ok((buffer.clone(), index.range.clone(), index.format))
        })
        .transpose()?;
    Ok(Draw {
        pipeline,
        groups,
        vertices,
        index,
        elements: geometry.elements.clone(),
        base_vertex: geometry.base_vertex,
        instances: object.instances.clone(),
    })
}
fn validate_target(
    template: &crate::material::MaterialTemplate,
    format: wgpu::TextureFormat,
    samples: u32,
) -> RenderResult<()> {
    if template.targets.len() != 1
        || template.targets[0]
            .as_ref()
            .is_none_or(|t| t.format != format)
        || template.multisample.count != samples
        || template.multiview_mask.is_some()
    {
        return Err(RenderError::new(
            &template.label,
            "pipeline color target/sample count differs from pass",
        ));
    }
    Ok(())
}
#[derive(Debug, Clone)]
/// Registered single-input effect definition with explicit generated binding locations.
pub struct FullscreenEffect {
    pub template: MaterialTemplateHandle,
    pub source_binding: (u32, u32),
    pub sampler_binding: (u32, u32),
    pub parameter_binding: Option<(u32, u32)>,
    pub bindings: Vec<BindingSetUse>,
}
impl FullscreenEffect {
    /// Return fragment bindings for a filterable D2 source at g0/b0 and sampler at g0/b1.
    pub fn source_schema() -> BindingSchema {
        BindingSchema {
            groups: vec![GroupSchema {
                index: 0,
                entries: vec![
                    BindingEntryDesc {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        kind: BindingKind::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    BindingEntryDesc {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        kind: BindingKind::Sampler {
                            kind: wgpu::SamplerBindingType::Filtering,
                        },
                        count: None,
                    },
                ],
            }],
        }
    }
    /// Return the sole color target format, or error if the template is not single-target.
    pub fn output_format(&self, resources: &ResourceManager) -> RenderResult<wgpu::TextureFormat> {
        let template = resources.template(self.template)?;
        if template.targets.len() != 1 {
            return Err(RenderError::new(
                &template.label,
                "fullscreen needs exactly one color target",
            ));
        }
        template.targets[0]
            .as_ref()
            .map(|t| t.format)
            .ok_or_else(|| RenderError::new(&template.label, "missing fullscreen color target"))
    }
}
/// Sample one logical texture and draw a procedural triangle into a distinct texture.
pub struct FullscreenPass {
    effect: FullscreenEffect,
    source: ResourceVersion,
    target: ResourceVersion,
    parameters: Vec<u8>,
    draw: Option<Draw>,
}
impl FullscreenPass {
    pub fn new(
        effect: FullscreenEffect,
        source: ResourceVersion,
        target: ResourceVersion,
        parameters: Vec<u8>,
    ) -> Self {
        Self {
            effect,
            source,
            target,
            parameters,
            draw: None,
        }
    }
}
impl GraphPass for FullscreenPass {
    fn declare(&self) -> PassDescriptor {
        PassDescriptor::new("fullscreen")
            .access(self.source, AccessKind::Sampled)
            .access(self.target, AccessKind::ColorClear)
    }
    fn prepare(
        &mut self,
        context: &mut PrepareContext<'_>,
        graph: &PassResources<'_>,
    ) -> RenderResult<()> {
        let template = context.resources.template(self.effect.template)?;
        let GraphResource::Texture { texture, .. } = graph.get(self.target)? else {
            return Err(RenderError::new("fullscreen", "expected texture"));
        };
        validate_target(template, texture.format(), texture.sample_count())?;
        if template.depth_stencil.is_some() || !template.vertex_layouts.is_empty() {
            return Err(RenderError::new(
                &template.label,
                "fullscreen requires procedural vertices and no depth state",
            ));
        }
        let sampler = context
            .device
            .create_sampler(&wgpu::SamplerDescriptor::default());
        let mut values = BTreeMap::new();
        values.insert(
            self.effect.source_binding,
            ResolvedBinding::Texture(graph.view(self.source)?.clone()),
        );
        if values
            .insert(
                self.effect.sampler_binding,
                ResolvedBinding::Sampler(sampler),
            )
            .is_some()
        {
            return Err(RenderError::new(
                &template.label,
                "overlapping source and sampler bindings",
            ));
        }
        match self.effect.parameter_binding {
            Some(binding) => {
                if values
                    .insert(
                        binding,
                        uniform(context.device, &template.label, &self.parameters)?,
                    )
                    .is_some()
                {
                    return Err(RenderError::new(
                        &template.label,
                        "overlapping parameter binding",
                    ));
                }
            }
            None if !self.parameters.is_empty() => {
                return Err(RenderError::new(
                    &template.label,
                    "effect does not accept parameters",
                ));
            }
            None => (),
        }
        let (pipeline, groups) = prepare_material(
            context.resources,
            context.pipelines,
            template,
            &self.effect.bindings,
            values,
        )?;
        self.draw = Some(Draw {
            pipeline,
            groups,
            vertices: Vec::new(),
            index: None,
            elements: 0..3,
            instances: 0..1,
            base_vertex: 0,
        });
        context.report.draw_calls += 1;
        Ok(())
    }
    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        graph: &PassResources<'_>,
    ) -> RenderResult<()> {
        let draw = self
            .draw
            .as_ref()
            .ok_or_else(|| RenderError::new("fullscreen", "pass was not prepared"))?;
        let colors = [Some(color_attachment(
            graph.view(self.target)?,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        ))];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fullscreen"),
            color_attachments: &colors,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        draw.encode(&mut pass);
        Ok(())
    }
}
