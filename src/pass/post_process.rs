use crate::{
    GpuResources,
    bindings::*,
    context::GpuContext,
    errors::PassError,
    pass::{Pass, PostProcessInput},
    pipeline::*,
};
use std::{ops::Deref, sync::Arc};
use utils::ids::ShaderId;

#[repr(C)]
#[derive(bytemuck::Zeroable, bytemuck::Pod, Clone, Copy)]
pub struct PostProcessPassId(pub usize);
impl Deref for PostProcessPassId {
    type Target = usize;
    fn deref(&self) -> &usize {
        &self.0
    }
}

pub struct PostProcessPass {
    vert_shader: ShaderId,
    frag_shader: ShaderId,
    pipeline: Arc<wgpu::RenderPipeline>,
    pipelines: PipelineCache,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,
    bind_group: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    surface_format: wgpu::TextureFormat,
    scratch_buffer: wgpu::Buffer,
    scratch_bind_group: wgpu::BindGroup,
    pub enabled: bool,
}
impl PostProcessPass {
    fn schema() -> BindingSchema {
        let mut schema = crate::render::FullscreenEffect::source_schema();
        schema.groups.push(GroupSchema {
            index: 1,
            entries: vec![BindingEntryDesc {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                kind: BindingKind::Uniform {
                    min_size: 0,
                    dynamic_offset: false,
                },
                count: None,
            }],
        });
        schema
    }
    pub fn new(
        ctx: &GpuContext,
        resources: &GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
        surface_format: wgpu::TextureFormat,
    ) -> Result<Self, PassError> {
        let mut pipelines = PipelineCache::new(&ctx.device);
        let schema = Self::schema();
        let layout = pipelines
            .layouts
            .resolve("post process", &schema)
            .map_err(pipeline_error)?;
        let scratch_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post process parameters"),
            size: 4096,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scratch_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post process parameters"),
            layout: layout.groups[1]
                .as_ref()
                .ok_or(PassError::LayoutsNotFound)?,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scratch_buffer.as_entire_binding(),
            }],
        });
        let pipeline = Self::pipeline(
            &mut pipelines,
            resources,
            vert_shader,
            frag_shader,
            surface_format,
        )?;
        Ok(Self {
            vert_shader,
            frag_shader,
            pipeline,
            pipelines,
            bind_group_layout: layout.groups[0].clone().ok_or(PassError::LayoutsNotFound)?,
            bind_group: None,
            sampler: ctx.device.create_sampler(&Default::default()),
            surface_format,
            scratch_buffer,
            scratch_bind_group,
            enabled: true,
        })
    }
    fn pipeline(
        cache: &mut PipelineCache,
        resources: &GpuResources,
        vs: ShaderId,
        fs: ShaderId,
        format: wgpu::TextureFormat,
    ) -> Result<Arc<wgpu::RenderPipeline>, PassError> {
        let vertex = resources
            .get_shader(vs)
            .ok_or(crate::PipelineError::ShaderNotFound { id: vs })?;
        let fragment = resources
            .get_shader(fs)
            .ok_or(crate::PipelineError::ShaderNotFound { id: fs })?;
        cache.retain_shaders(|key| {
            resources
                .get_shader(key.shader)
                .is_some_and(|s| s.revision == key.revision)
        });
        let key = PipelineKey {
            vertex: ShaderStageKey::new(
                ShaderRevisionKey {
                    shader: vs,
                    revision: vertex.revision,
                },
                "vs_main",
            ),
            fragment: Some(ShaderStageKey::new(
                ShaderRevisionKey {
                    shader: fs,
                    revision: fragment.revision,
                },
                "fs_main",
            )),
            layout: Self::schema(),
            vertex_layouts: Vec::new(),
            primitive: Default::default(),
            depth_stencil: None,
            targets: vec![Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            multisample: Default::default(),
            multiview_mask: None,
        };
        cache
            .render("post process", &key, &vertex.module, Some(&fragment.module))
            .map_err(pipeline_error)
    }
    pub fn set_shader(
        &mut self,
        _: &GpuContext,
        resources: &GpuResources,
        vert_shader: ShaderId,
        frag_shader: ShaderId,
    ) -> Result<(), PassError> {
        self.pipeline = Self::pipeline(
            &mut self.pipelines,
            resources,
            vert_shader,
            frag_shader,
            self.surface_format,
        )?;
        self.vert_shader = vert_shader;
        self.frag_shader = frag_shader;
        Ok(())
    }
    pub(crate) fn refresh_pipeline(
        &mut self,
        ctx: &GpuContext,
        resources: &GpuResources,
    ) -> Result<(), PassError> {
        self.set_shader(ctx, resources, self.vert_shader, self.frag_shader)
    }
    pub fn write_scratch_buffer<Data: bytemuck::Pod + bytemuck::Zeroable>(
        &self,
        ctx: &GpuContext,
        data: Data,
    ) {
        let bytes = bytemuck::bytes_of(&data);
        if bytes.len() > self.scratch_buffer.size() as usize || !bytes.len().is_multiple_of(4) {
            tracing::error!("Invalid post-process parameter size");
            return;
        }
        ctx.queue.write_buffer(&self.scratch_buffer, 0, bytes);
    }
}
fn pipeline_error(error: crate::RenderError) -> PassError {
    crate::PipelineError::Validation(error.to_string()).into()
}
impl Pass for PostProcessPass {
    type Input<'a> = PostProcessInput<'a>;
    fn prepare<'a>(&mut self, ctx: &GpuContext, _: &mut GpuResources, input: &mut Self::Input<'a>) {
        self.bind_group = Some(ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post process input"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input.source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
    }
    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        _: &GpuResources,
    ) {
        if !self.enabled {
            return;
        }
        let Some(group) = &self.bind_group else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("post process"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.set_bind_group(1, &self.scratch_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
