//! See the [complete V2 guide](crate::guide) for usage, contracts and limitations.
//! Prepare owned frame data, compile the graph, encode, then submit.
mod binding;
mod passes;
use crate::{
    RenderError, RenderResult, frame::v2::*, graph::*, ids::*, pipeline::PipelineCache,
    render_object::RenderPhaseId, resource::manager::ResourceManager,
};
pub use passes::{ClearPass, FullscreenEffect, FullscreenPass, PhasePass};
use std::{collections::HashMap, time::Instant};

#[derive(Debug, Default)]
/// CPU preparation diagnostics and counters. These are not GPU timings or driver
/// memory measurements. See [metric definitions](crate::guide#errors-and-instrumentation).
pub struct FrameReport {
    pub draw_calls: u64,
    pub upload_bytes: u64,
    pub passes: usize,
    pub transient_texture_bytes: u64,
    pub preparation_time: std::time::Duration,
    pub skipped_objects: Vec<RenderError>,
}
/// Services available while preparing a live node, before any GPU command executes.
pub struct PrepareContext<'a> {
    pub device: &'a wgpu::Device,
    pub resources: &'a ResourceManager,
    pub pipelines: &'a mut PipelineCache,
    pub frame: &'a Frame,
    pub report: &'a mut FrameReport,
    pub fallback_material: Option<MaterialHandle>,
}
/// Declaration happens before compilation. Preparation receives only declared
/// graph resources, and encoding cannot look up any additional GPU dependencies.
pub trait GraphPass {
    /// Describe every graph access, explicit dependency and external side effect.
    fn declare(&self) -> PassDescriptor;
    /// Resolve assets and prepare GPU state using only declared graph resources.
    fn prepare(
        &mut self,
        context: &mut PrepareContext<'_>,
        resources: &PassResources<'_>,
    ) -> RenderResult<()>;
    /// Record commands using prepared state and the exact declared graph versions.
    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &PassResources<'_>,
    ) -> RenderResult<()>;
}
/// Prepared command buffer and report, ready for asynchronous queue submission.
pub struct EncodedFrame {
    pub commands: wgpu::CommandBuffer,
    pub report: FrameReport,
}
/// Device-local renderer with pipeline caches and a post-process registry.
/// Binds to the first resource manager it encodes; see the [guide](crate::guide).
pub struct GraphRenderer {
    device: wgpu::Device,
    resource_owner: Option<u64>,
    queue: wgpu::Queue,
    pub pipelines: PipelineCache,
    effects: HashMap<PostProcessId, FullscreenEffect>,
    next_effect: u64,
    pub fallback_material: Option<MaterialHandle>,
    /// None uses the presentation format. HDR clients can select Rgba16Float.
    pub scene_format: Option<wgpu::TextureFormat>,
    pub depth_format: Option<wgpu::TextureFormat>,
    blit: HashMap<wgpu::TextureFormat, (ShaderId, MaterialTemplateHandle)>,
}
impl GraphRenderer {
    /// Create a V2 renderer using the context's device and queue.
    /// The context retains ownership of its GPU and presentation state.
    pub fn from_context(context: &crate::GpuContext) -> Self {
        Self::new(&context.device, &context.queue)
    }

    /// Acquire, render and present through the context without exposing raw GPU objects.
    /// Returns `None` for an unavailable surface or a skipped frame. Lost/outdated
    /// surfaces are reconfigured. Resources and context must use this renderer's device.
    pub fn render(
        &mut self,
        context: &mut crate::GpuContext,
        resources: &mut ResourceManager,
        frame: Frame,
    ) -> RenderResult<Option<FrameReport>> {
        if context.device != self.device {
            return Err(RenderError::new(
                "surface",
                "context belongs to another device",
            ));
        }
        self.check_resources(resources)?;
        if !context.is_ready() {
            return Ok(None);
        }
        let surface = match context.current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                context.reconfigure();
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(RenderError::new("surface", "validation error"));
            }
            _ => return Ok(None),
        };
        let report = self.render_to_texture(resources, frame, &surface.texture)?;
        context.present(surface);
        Ok(Some(report))
    }

    /// Create a renderer for a matching device/queue pair. No optional features are enabled.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            resource_owner: None,
            device: device.clone(),
            queue: queue.clone(),
            pipelines: PipelineCache::new(device),
            effects: HashMap::new(),
            next_effect: 0,
            fallback_material: None,
            scene_format: None,
            depth_format: None,
            blit: HashMap::new(),
        }
    }
    fn check_resources(&mut self, resources: &ResourceManager) -> RenderResult<()> {
        if resources.device() != &self.device {
            return Err(RenderError::new(
                "renderer",
                "resource manager belongs to another device",
            ));
        }
        if self
            .resource_owner
            .is_some_and(|id| id != resources.identity)
        {
            return Err(RenderError::new(
                "renderer",
                "renderer is bound to a different resource manager",
            ));
        }
        self.resource_owner = Some(resources.identity);
        Ok(())
    }
    /// Register an effect without activating it. Asset validation happens during preparation.
    pub fn register_post_process(&mut self, effect: FullscreenEffect) -> PostProcessId {
        let id = PostProcessId(self.next_effect);
        self.next_effect += 1;
        self.effects.insert(id, effect);
        id
    }
    /// Remove a registered effect, returning its definition if it existed.
    pub fn remove_post_process(&mut self, id: PostProcessId) -> Option<FullscreenEffect> {
        self.effects.remove(&id)
    }
    /// Compile, allocate, prepare and encode an explicit graph without submitting.
    /// The supplied graph is authoritative: no default views or effect nodes are inserted.
    pub fn encode_graph(
        &mut self,
        resources: &ResourceManager,
        frame: Frame,
        mut plan: FrameGraph,
    ) -> RenderResult<EncodedFrame> {
        let start = Instant::now();
        self.check_resources(resources)?;
        self.pipelines.retain_shaders(|key| {
            resources
                .shader_revision(key.shader)
                .is_ok_and(|current| current == key)
        });
        let compiled = plan.graph.compile()?;
        let gpu = compiled.prepare(&self.device, plan.imports)?;
        let mut report = FrameReport {
            passes: compiled.order.len(),
            ..Default::default()
        };
        for (i, lifetime) in compiled.lifetimes.iter().enumerate() {
            if lifetime.is_some() {
                // Report allocated transients using their format's block geometry.
                report.transient_texture_bytes += compiled.transient_texture_bytes(i);
            }
        }
        for id in &compiled.order {
            let node = plan
                .nodes
                .iter_mut()
                .find(|(key, _)| key == id)
                .ok_or_else(|| {
                    RenderError::new("graph", "compiled pass has no registered implementation")
                })?;
            let lookup = compiled.pass_resources(*id, &gpu)?;
            node.1.prepare(
                &mut PrepareContext {
                    device: &self.device,
                    resources,
                    pipelines: &mut self.pipelines,
                    frame: &frame,
                    report: &mut report,
                    fallback_material: self.fallback_material,
                },
                &lookup,
            )?;
        }
        report.preparation_time = start.elapsed();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Prism frame"),
            });
        // Uploads are encoded into this command buffer, never queued globally
        // ahead of another unsubmitted frame. Staging owns all CPU bytes.
        encode_uploads(
            &self.device,
            resources,
            &frame.uploads,
            &mut encoder,
            &mut report,
        )?;
        crate::errors::gpu_checked(&self.device, "frame encoding", || -> RenderResult<()> {
            for id in &compiled.order {
                let node = plan
                    .nodes
                    .iter()
                    .find(|(key, _)| key == id)
                    .ok_or_else(|| RenderError::new("graph", "missing node"))?;
                node.1
                    .encode(&mut encoder, &compiled.pass_resources(*id, &gpu)?)?;
            }
            Ok(())
        })??;
        let commands =
            crate::errors::gpu_checked(&self.device, "finish frame", || encoder.finish())?;
        Ok(EncodedFrame { commands, report })
    }
    /// Submit commands and return their report without waiting for GPU completion.
    pub fn submit(&self, frame: EncodedFrame) -> RenderResult<FrameReport> {
        crate::errors::gpu_checked(&self.device, "submit frame", || {
            self.queue.submit([frame.commands])
        })?;
        Ok(frame.report)
    }
    /// Encode default composition and submit it. Does not wait for completion or present.
    pub fn render_to_texture(
        &mut self,
        resources: &mut ResourceManager,
        frame: Frame,
        target: &wgpu::Texture,
    ) -> RenderResult<FrameReport> {
        let encoded = self.encode_to_texture(resources, frame, target)?;
        self.submit(encoded)
    }
    /// Build and encode the default scene/effect/overlay composition without submitting.
    /// The target must be a single-sample, single-layer 2D render attachment.
    pub fn encode_to_texture(
        &mut self,
        resources: &mut ResourceManager,
        mut frame: Frame,
        target: &wgpu::Texture,
    ) -> RenderResult<EncodedFrame> {
        self.check_resources(resources)?;
        if target.sample_count() != 1
            || target.dimension() != wgpu::TextureDimension::D2
            || target.depth_or_array_layers() != 1
        {
            return Err(RenderError::new(
                "target",
                "presentation requires a single-sampled 2D texture",
            ));
        }
        if frame.views.is_empty() {
            frame.views.push(ViewSubmission::default());
        }
        let mut plan = FrameGraph::default();
        let format = self.scene_format.unwrap_or(target.format());
        let mut scene = plan.graph.create_texture(
            "scene",
            TextureDesc::color(target.width(), target.height(), format),
        );
        plan.add_pass(ClearPass::color("clear scene", scene, wgpu::Color::BLACK));
        let depth = if let Some(format) = self.depth_format {
            let mut desc = TextureDesc::color(target.width(), target.height(), format);
            desc.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
            let depth = plan.graph.create_texture("depth", desc);
            plan.add_pass(ClearPass::depth("clear depth", depth, 1.0));
            Some(depth)
        } else {
            None
        };
        let mut current_depth = depth;
        for (view_index, view) in frame.views.iter().enumerate() {
            for phase in &view.phases {
                if phase.phase == RenderPhaseId::OVERLAY
                    && frame.overlay_policy == OverlayPolicy::AfterPostProcess
                {
                    continue;
                }
                scene = plan.graph.next_version(scene)?;
                let write_depth = phase.phase == RenderPhaseId::OPAQUE;
                if write_depth {
                    current_depth = current_depth
                        .map(|d| plan.graph.next_version(d))
                        .transpose()?;
                }
                plan.add_pass(PhasePass::new(
                    scene,
                    current_depth,
                    write_depth,
                    view_index,
                    phase.clone(),
                ));
            }
        }
        for request in &frame.post_process {
            let effect = self
                .effects
                .get(&request.effect)
                .ok_or_else(|| {
                    RenderError::new(
                        "post process",
                        format!("unknown effect {:?}", request.effect),
                    )
                })?
                .clone();
            let output_format = effect.output_format(resources)?;
            let output = plan.graph.create_texture(
                "post process output",
                TextureDesc::color(target.width(), target.height(), output_format),
            );
            plan.add_pass(FullscreenPass::new(
                effect,
                scene,
                output,
                request.parameters.clone(),
            ));
            scene = output;
        }
        let target_desc = TextureDesc {
            size: target.size(),
            format: target.format(),
            usage: target.usage(),
            sample_count: 1,
            mip_level_count: target.mip_level_count(),
            dimension: target.dimension(),
        };
        let surface = plan
            .graph
            .import_texture("presentation", target_desc, false);
        plan.imports.insert(
            surface,
            GraphResource::Texture {
                texture: target.clone(),
                view: target.create_view(&Default::default()),
            },
        );
        let effect = self.blit_effect(resources, target.format())?;
        plan.add_pass(FullscreenPass::new(effect, scene, surface, Vec::new()));
        let mut output = surface;
        if frame.overlay_policy == OverlayPolicy::AfterPostProcess {
            for (view_index, view) in frame.views.iter().enumerate() {
                for phase in view
                    .phases
                    .iter()
                    .filter(|p| p.phase == RenderPhaseId::OVERLAY)
                {
                    output = plan.graph.next_version(output)?;
                    plan.add_pass(PhasePass::new(
                        output,
                        None,
                        false,
                        view_index,
                        phase.clone(),
                    ));
                }
            }
        }
        plan.graph.export(output);
        self.encode_graph(resources, frame, plan)
    }
    fn blit_effect(
        &mut self,
        resources: &mut ResourceManager,
        format: wgpu::TextureFormat,
    ) -> RenderResult<FullscreenEffect> {
        let cached = self
            .blit
            .get(&format)
            .filter(|(shader, template)| {
                resources.shader(*shader).is_ok() && resources.template(*template).is_ok()
            })
            .copied();
        let template = if let Some((_, template)) = cached {
            template
        } else {
            let shader = resources.create_shader("Prism presentation", BLIT_SHADER)?;
            let mut template =
                crate::material::MaterialTemplate::fullscreen("Prism presentation", shader, format);
            template.schema = FullscreenEffect::source_schema();
            let template = resources.create_template(template)?;
            self.blit.insert(format, (shader, template));
            template
        };
        Ok(FullscreenEffect {
            template,
            source_binding: (0, 0),
            sampler_binding: (0, 1),
            parameter_binding: None,
            bindings: Vec::new(),
        })
    }
}
fn encode_uploads(
    device: &wgpu::Device,
    resources: &ResourceManager,
    uploads: &[UploadRequest],
    encoder: &mut wgpu::CommandEncoder,
    report: &mut FrameReport,
) -> RenderResult<()> {
    use wgpu::util::DeviceExt;
    for upload in uploads {
        match upload {
            UploadRequest::Buffer {
                target,
                offset,
                data,
            } => {
                let buffer = resources.buffer(*target)?.raw();
                if !offset.is_multiple_of(4)
                    || !data.len().is_multiple_of(4)
                    || offset
                        .checked_add(data.len() as u64)
                        .is_none_or(|end| end > buffer.size())
                    || !buffer.usage().contains(wgpu::BufferUsages::COPY_DST)
                {
                    return Err(RenderError::new(
                        "upload",
                        "buffer usage, alignment or range is invalid",
                    ));
                }
                if data.is_empty() {
                    continue;
                }
                let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("frame buffer upload"),
                    contents: data,
                    usage: wgpu::BufferUsages::COPY_SRC,
                });
                encoder.copy_buffer_to_buffer(&staging, 0, buffer, *offset, data.len() as u64);
                report.upload_bytes += data.len() as u64;
            }
            UploadRequest::Texture {
                target,
                mip_level,
                origin,
                size,
                bytes_per_row,
                rows_per_image,
                data,
            } => {
                let texture = resources.texture(*target)?.raw();
                if !texture.usage().contains(wgpu::TextureUsages::COPY_DST)
                    || !bytes_per_row.is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
                    || *bytes_per_row == 0
                    || *rows_per_image == 0
                    || data.is_empty()
                {
                    return Err(RenderError::new(
                        "upload",
                        "texture upload needs COPY_DST and padded 256-byte rows",
                    ));
                }
                let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("frame texture upload"),
                    contents: data,
                    usage: wgpu::BufferUsages::COPY_SRC,
                });
                crate::errors::gpu_checked(device, "texture upload", || {
                    encoder.copy_buffer_to_texture(
                        wgpu::TexelCopyBufferInfo {
                            buffer: &staging,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(*bytes_per_row),
                                rows_per_image: Some(*rows_per_image),
                            },
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: *mip_level,
                            origin: *origin,
                            aspect: wgpu::TextureAspect::All,
                        },
                        *size,
                    )
                })?;
                report.upload_bytes += data.len() as u64;
            }
        }
    }
    Ok(())
}
const BLIT_SHADER: &str = r#"
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Output {
    let p = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    var o: Output; o.position = vec4(p[i], 0., 1.); o.uv = p[i] * vec2(.5, -.5) + vec2(.5); return o;
}
@fragment fn fs_main(i: Output) -> @location(0) vec4<f32> { return textureSample(source, source_sampler, i.uv); }
"#;
