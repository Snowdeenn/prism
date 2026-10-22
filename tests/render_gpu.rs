//! Run explicitly on a machine with a Vulkan/GL adapter (software is sufficient):
//! cargo test --test render_gpu -- --ignored --nocapture
use prism::{
    FrameV2,
    bindings::*,
    frame::v2::*,
    graph::TextureDesc,
    ids::*,
    material::*,
    render::*,
    render_object::*,
    resource::{ResourceManager, manager::Geometry},
};

fn device() -> (wgpu::Device, wgpu::Queue) {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: None,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .expect("GPU integration tests require a wgpu adapter");
        eprintln!("GPU tests: {:?}", adapter.get_info());
        adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap()
    })
}
fn target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let row = (texture.width() * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * texture.height()) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let mut out = Vec::new();
    for row in mapped.chunks(row as usize) {
        out.extend_from_slice(&row[..texture.width() as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    out
}
const VERTEX: &str = "@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> { let p=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));return vec4(p[i],0.,1.); }";
fn template(
    resources: &mut ResourceManager,
    group: Option<u32>,
    color: &str,
) -> (ShaderId, MaterialTemplateHandle) {
    let fragment = if let Some(group) = group {
        format!(
            "@group({group}) @binding(0) var<uniform> color: vec4<f32>; @fragment fn fs_main()->@location(0) vec4<f32> {{return color;}}"
        )
    } else {
        format!("@fragment fn fs_main()->@location(0) vec4<f32> {{return {color};}}")
    };
    let shader = resources
        .create_shader("test shader", &format!("{VERTEX}\n{fragment}"))
        .unwrap();
    let mut template =
        MaterialTemplate::fullscreen("test material", shader, wgpu::TextureFormat::Rgba8Unorm);
    if let Some(group) = group {
        template.schema = BindingSchema {
            groups: vec![GroupSchema {
                index: group,
                entries: vec![BindingEntryDesc {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    kind: BindingKind::Uniform {
                        min_size: 16,
                        dynamic_offset: false,
                    },
                    count: None,
                }],
            }],
        };
    }
    let id = resources.create_template(template).unwrap();
    (shader, id)
}
fn material(
    resources: &mut ResourceManager,
    template: MaterialTemplateHandle,
    color: Option<(u32, [f32; 4])>,
) -> MaterialHandle {
    resources
        .create_material(Material {
            template,
            bindings: Vec::new(),
            parameters: color.map(|(group, color)| ParameterBlock {
                group,
                binding: 0,
                bytes: bytemuck::cast_slice(&color).to_vec(),
            }),
        })
        .unwrap()
}
fn object(
    geometry: GeometryHandle,
    material: MaterialHandle,
    phase: RenderPhaseId,
    sort: u64,
) -> RenderObject {
    RenderObject {
        geometry,
        material,
        instances: 0..1,
        object_bindings: Vec::new(),
        phase,
        sort_key: SortKey(sort),
        bounds: None,
    }
}
fn draw_frame(objects: Vec<RenderObject>) -> FrameV2 {
    FrameV2 {
        objects,
        ..Default::default()
    }
}
fn assert_pixel(actual: &[u8], expected: [u8; 4]) {
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            a.abs_diff(e) <= 1,
            "actual={actual:?}, expected={expected:?}"
        );
    }
}

#[test]
#[ignore = "requires a wgpu adapter; explicitly exercised during renderer validation"]
fn renders_mixed_schemas_hot_reload_resize_post_process_and_empty_frames() {
    let (device, queue) = device();
    let mut resources = ResourceManager::new(&device);
    let mut renderer = GraphRenderer::new(&device, &queue);
    let target = target(&device, 8, 8);
    let geometry = resources.create_geometry(Geometry::fullscreen());
    let (plain_shader, plain_template) = template(&mut resources, None, "vec4(1.,0.,0.,1.)");
    let (_, uniform_template) = template(&mut resources, Some(2), "");
    let red = material(&mut resources, plain_template, None);
    let green = material(
        &mut resources,
        uniform_template,
        Some((2, [0., 1., 0., 1.])),
    );
    let opaque = RenderPhaseId::OPAQUE;
    let frame = || {
        draw_frame(vec![
            object(geometry, red, opaque, 0),
            object(geometry, green, opaque, 1),
        ])
    };
    let report = renderer
        .render_to_texture(&mut resources, frame(), &target)
        .unwrap();
    assert!(
        report.skipped_objects.is_empty(),
        "{:?}",
        report.skipped_objects
    );
    assert_eq!(report.draw_calls, 3);
    assert_pixel(&pixels(&device, &queue, &target)[..4], [0, 255, 0, 255]);
    let misses = renderer.pipelines.render_stats.misses;
    renderer
        .render_to_texture(&mut resources, frame(), &target)
        .unwrap();
    assert_eq!(renderer.pipelines.render_stats.misses, misses);
    resources.reload_shader(plain_shader,&format!("{VERTEX}\n@fragment fn fs_main()->@location(0) vec4<f32> {{return vec4(0.,0.,1.,1.);}}")).unwrap();
    renderer
        .render_to_texture(
            &mut resources,
            draw_frame(vec![object(geometry, red, opaque, 0)]),
            &target,
        )
        .unwrap();
    assert_pixel(&pixels(&device, &queue, &target)[..4], [0, 0, 255, 255]);
    assert_eq!(renderer.pipelines.render_stats.misses, misses + 1);
    let revision = resources.shader_revision(plain_shader).unwrap();
    assert!(
        resources
            .reload_shader(plain_shader, "invalid WGSL")
            .is_err()
    );
    assert_eq!(resources.shader_revision(plain_shader).unwrap(), revision);
    let resized = self::target(&device, 12, 4);
    renderer
        .render_to_texture(
            &mut resources,
            draw_frame(vec![object(geometry, green, opaque, 0)]),
            &resized,
        )
        .unwrap();
    assert!(
        pixels(&device, &queue, &resized)
            .chunks(4)
            .all(|p| p == [0, 255, 0, 255])
    );
    // An optional fullscreen node, registered solely through the public API.
    let shader=resources.create_shader("invert",&format!("{VERTEX}\n@group(0) @binding(0) var source:texture_2d<f32>; @group(0) @binding(1) var s:sampler; @fragment fn fs_main(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {{ let c=textureLoad(source,vec2<i32>(p.xy),0); return vec4(vec3(1.)-c.rgb,1.); }}")).unwrap();
    let mut effect_template =
        MaterialTemplate::fullscreen("invert", shader, wgpu::TextureFormat::Rgba8Unorm);
    effect_template.schema = FullscreenEffect::source_schema();
    let effect_template = resources.create_template(effect_template).unwrap();
    let effect = renderer.register_post_process(FullscreenEffect {
        template: effect_template,
        source_binding: (0, 0),
        sampler_binding: (0, 1),
        parameter_binding: None,
        bindings: Vec::new(),
    });
    let mut post_frame = draw_frame(vec![object(geometry, green, opaque, 0)]);
    post_frame.post_process.push(PostProcessSubmission {
        effect,
        parameters: Vec::new(),
    });
    renderer
        .render_to_texture(&mut resources, post_frame, &target)
        .unwrap();
    assert_pixel(&pixels(&device, &queue, &target)[..4], [255, 0, 255, 255]);
    renderer
        .render_to_texture(&mut resources, FrameV2::default(), &target)
        .unwrap();
    assert!(
        pixels(&device, &queue, &target)
            .chunks(4)
            .all(|p| p == [0, 0, 0, 255])
    );
    // Stale views cannot resolve to a replacement allocation after resize.
    let texture = resources
        .create_texture(
            "asset",
            &TextureDesc::color(2, 2, wgpu::TextureFormat::Rgba8Unorm),
        )
        .unwrap();
    let view = resources.create_view(texture, &Default::default()).unwrap();
    resources.remove_texture(texture);
    let replacement = resources
        .create_texture(
            "resized asset",
            &TextureDesc::color(4, 4, wgpu::TextureFormat::Rgba8Unorm),
        )
        .unwrap();
    assert_ne!(texture, replacement);
    assert!(resources.view(view).is_err());
}

#[test]
#[ignore = "requires a wgpu adapter"]
fn transparent_sorting_overlay_policy_and_encoded_uploads_preserve_visual_order() {
    let (device, queue) = device();
    let mut resources = ResourceManager::new(&device);
    let mut renderer = GraphRenderer::new(&device, &queue);
    let target = target(&device, 4, 4);
    let geometry = resources.create_geometry(Geometry::fullscreen());
    let (_, base) = template(&mut resources, Some(0), "");
    let mut translucent = resources.template(base).unwrap().clone();
    translucent.targets[0].as_mut().unwrap().blend = Some(wgpu::BlendState::ALPHA_BLENDING);
    let translucent = resources.create_template(translucent).unwrap();
    let red = material(&mut resources, translucent, Some((0, [1., 0., 0., 0.5])));
    let blue = material(&mut resources, translucent, Some((0, [0., 0., 1., 0.5])));
    let frame = draw_frame(vec![
        object(geometry, blue, RenderPhaseId::TRANSPARENT, 1),
        object(geometry, red, RenderPhaseId::TRANSPARENT, 10),
    ]);
    let report = renderer
        .render_to_texture(&mut resources, frame, &target)
        .unwrap();
    assert!(
        report.skipped_objects.is_empty(),
        "{:?}",
        report.skipped_objects
    );
    assert_pixel(&pixels(&device, &queue, &target)[..4], [64, 0, 128, 255]);
    // Equal-depth transparency remains in submission order.
    let frame = draw_frame(vec![
        object(geometry, blue, RenderPhaseId::TRANSPARENT, 1),
        object(geometry, red, RenderPhaseId::TRANSPARENT, 1),
    ]);
    renderer
        .render_to_texture(&mut resources, frame, &target)
        .unwrap();
    assert_pixel(&pixels(&device, &queue, &target)[..4], [128, 0, 64, 255]);
    // Two frames can be encoded before either is submitted without uploading B's
    // data into A's buffer ahead of A's draw.
    let buffer = resources
        .create_buffer(
            "color",
            256,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        )
        .unwrap();
    let bindings = resources.create_bindings(BindingSet {
        group: 0,
        entries: vec![(
            0,
            BindingResource::Buffer {
                handle: buffer,
                offset: 0,
                size: 16,
            },
        )],
    });
    let material = resources
        .create_material(Material {
            template: base,
            bindings: vec![bindings],
            parameters: None,
        })
        .unwrap();
    let make_frame = |color: [f32; 4]| FrameV2 {
        objects: vec![object(geometry, material, RenderPhaseId::OPAQUE, 0)],
        uploads: vec![UploadRequest::Buffer {
            target: buffer,
            offset: 0,
            data: bytemuck::cast_slice(&color).to_vec(),
        }],
        ..Default::default()
    };
    let a = renderer
        .encode_to_texture(&mut resources, make_frame([1., 0., 0., 1.]), &target)
        .unwrap();
    let b = renderer
        .encode_to_texture(&mut resources, make_frame([0., 1., 0., 1.]), &target)
        .unwrap();
    renderer.submit(a).unwrap();
    assert_pixel(&pixels(&device, &queue, &target)[..4], [255, 0, 0, 255]);
    renderer.submit(b).unwrap();
    assert_pixel(&pixels(&device, &queue, &target)[..4], [0, 255, 0, 255]);
}

#[test]
#[ignore = "requires a wgpu adapter"]
fn layouts_dynamic_offsets_and_shader_mismatch_are_validated() {
    let (device, queue) = device();
    let mut resources = ResourceManager::new(&device);
    let mut renderer = GraphRenderer::new(&device, &queue);
    let target = target(&device, 4, 4);
    let geometry = resources.create_geometry(Geometry::fullscreen());
    let (_, base) = template(&mut resources, Some(0), "");
    let mut dynamic = resources.template(base).unwrap().clone();
    dynamic.schema.groups[0].entries[0].kind = BindingKind::Uniform {
        min_size: 16,
        dynamic_offset: true,
    };
    let layout = renderer
        .pipelines
        .layouts
        .resolve("dynamic", &dynamic.schema)
        .unwrap();
    let same = renderer
        .pipelines
        .layouts
        .resolve("dynamic duplicate", &dynamic.schema)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&layout.pipeline, &same.pipeline));
    let mut other = dynamic.schema.clone();
    other.groups[0].index = 2;
    let other = renderer
        .pipelines
        .layouts
        .resolve("other group", &other)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(
        layout.groups[0].as_ref().unwrap(),
        other.groups[2].as_ref().unwrap()
    ));
    assert!(!std::sync::Arc::ptr_eq(&layout.pipeline, &other.pipeline));
    let dynamic = resources.create_template(dynamic).unwrap();
    let material = material(&mut resources, dynamic, None);
    let alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
    let buffer = resources
        .create_buffer(
            "dynamic uniform",
            (alignment + 16) as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        )
        .unwrap();
    let bindings = resources.create_bindings(BindingSet {
        group: 0,
        entries: vec![(
            0,
            BindingResource::Buffer {
                handle: buffer,
                offset: 0,
                size: 16,
            },
        )],
    });
    let mut object = object(geometry, material, RenderPhaseId::OPAQUE, 0);
    object.object_bindings.push(BindingSetUse {
        handle: bindings,
        dynamic_offsets: vec![alignment as u32],
    });
    let mut data = vec![0; alignment + 16];
    data[alignment..].copy_from_slice(bytemuck::cast_slice(&[1_f32, 0., 1., 1.]));
    let mut frame = draw_frame(vec![object.clone()]);
    frame.uploads.push(UploadRequest::Buffer {
        target: buffer,
        offset: 0,
        data,
    });
    let report = renderer
        .render_to_texture(&mut resources, frame, &target)
        .unwrap();
    assert!(
        report.skipped_objects.is_empty(),
        "{:?}",
        report.skipped_objects
    );
    assert_pixel(&pixels(&device, &queue, &target)[..4], [255, 0, 255, 255]);
    object.object_bindings[0].dynamic_offsets = vec![1];
    let report = renderer
        .render_to_texture(&mut resources, draw_frame(vec![object.clone()]), &target)
        .unwrap();
    assert_eq!(report.skipped_objects.len(), 1);
    assert!(report.skipped_objects[0].to_string().contains("binding 0"));
    object.object_bindings[0].dynamic_offsets = vec![alignment as u32 * 2];
    assert!(
        !renderer
            .render_to_texture(&mut resources, draw_frame(vec![object]), &target)
            .unwrap()
            .skipped_objects
            .is_empty()
    );
    let mut wrong = resources.template(base).unwrap().clone();
    wrong.schema.groups[0].entries[0].kind = BindingKind::Storage {
        min_size: 16,
        read_only: true,
        dynamic_offset: false,
    };
    let wrong = resources.create_template(wrong).unwrap();
    let wrong = resources
        .create_material(Material {
            template: wrong,
            bindings: Vec::new(),
            parameters: None,
        })
        .unwrap();
    let report = renderer
        .render_to_texture(
            &mut resources,
            draw_frame(vec![self::object(
                geometry,
                wrong,
                RenderPhaseId::OPAQUE,
                0,
            )]),
            &target,
        )
        .unwrap();
    assert_eq!(report.skipped_objects.len(), 1);
    assert!(report.skipped_objects[0].to_string().contains("shader"));
    // A renderer cannot confuse generation/index pairs from unrelated stores.
    let mut foreign = ResourceManager::new(&device);
    assert!(
        renderer
            .render_to_texture(&mut foreign, FrameV2::default(), &target)
            .is_err()
    );
}

#[test]
#[ignore = "requires a wgpu adapter"]
fn overlay_before_and_after_post_process_is_explicit() {
    let (device, queue) = device();
    let mut resources = ResourceManager::new(&device);
    let mut renderer = GraphRenderer::new(&device, &queue);
    let target = target(&device, 4, 4);
    let geometry = resources.create_geometry(Geometry::fullscreen());
    let (_, base) = template(&mut resources, None, "vec4(1.,0.,0.,1.)");
    let red = material(&mut resources, base, None);
    let source = format!(
        "{VERTEX}\n@group(0) @binding(0) var source:texture_2d<f32>; @group(0) @binding(1) var s:sampler; @group(1) @binding(0) var<uniform> strength:vec4<f32>; @fragment fn fs_main(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {{let c=textureLoad(source,vec2<i32>(p.xy),0);return vec4(mix(c.rgb,vec3(1.)-c.rgb,strength.x),1.);}}"
    );
    let shader = resources
        .create_shader("invert with parameters", &source)
        .unwrap();
    let mut t = MaterialTemplate::fullscreen(
        "invert with parameters",
        shader,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    t.schema = FullscreenEffect::source_schema();
    t.schema.groups.push(GroupSchema {
        index: 1,
        entries: vec![BindingEntryDesc {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            kind: BindingKind::Uniform {
                min_size: 16,
                dynamic_offset: false,
            },
            count: None,
        }],
    });
    let t = resources.create_template(t).unwrap();
    let effect = renderer.register_post_process(FullscreenEffect {
        template: t,
        source_binding: (0, 0),
        sampler_binding: (0, 1),
        parameter_binding: Some((1, 0)),
        bindings: Vec::new(),
    });
    let frame = |policy| FrameV2 {
        objects: vec![object(geometry, red, RenderPhaseId::OVERLAY, 0)],
        post_process: vec![PostProcessSubmission {
            effect,
            parameters: bytemuck::cast_slice(&[1_f32, 0., 0., 0.]).to_vec(),
        }],
        overlay_policy: policy,
        ..Default::default()
    };
    renderer
        .render_to_texture(
            &mut resources,
            frame(OverlayPolicy::BeforePostProcess),
            &target,
        )
        .unwrap();
    assert_pixel(&pixels(&device, &queue, &target)[..4], [0, 255, 255, 255]);
    renderer
        .render_to_texture(
            &mut resources,
            frame(OverlayPolicy::AfterPostProcess),
            &target,
        )
        .unwrap();
    assert_pixel(&pixels(&device, &queue, &target)[..4], [255, 0, 0, 255]);
    let mut invalid = frame(OverlayPolicy::AfterPostProcess);
    invalid.post_process[0].parameters = vec![0; 4];
    assert!(
        renderer
            .render_to_texture(&mut resources, invalid, &target)
            .is_err()
    );
    let mut twice = frame(OverlayPolicy::BeforePostProcess);
    twice.post_process.push(twice.post_process[0].clone());
    renderer
        .render_to_texture(&mut resources, twice, &target)
        .unwrap();
    assert_pixel(&pixels(&device, &queue, &target)[..4], [255, 0, 0, 255]);
}

#[test]
#[ignore = "requires a wgpu adapter"]
fn a_custom_compute_pass_can_be_added_without_changing_the_renderer() {
    use prism::{graph::*, pipeline::*};
    struct Compute {
        output: ResourceVersion,
        shader: ShaderId,
        pipeline: Option<std::sync::Arc<wgpu::ComputePipeline>>,
        group: Option<wgpu::BindGroup>,
    }
    impl GraphPass for Compute {
        fn declare(&self) -> PassDescriptor {
            PassDescriptor::new("user compute").access(self.output, AccessKind::StorageWrite)
        }
        fn prepare(
            &mut self,
            ctx: &mut PrepareContext<'_>,
            graph: &PassResources<'_>,
        ) -> prism::RenderResult<()> {
            let schema = BindingSchema {
                groups: vec![GroupSchema {
                    index: 0,
                    entries: vec![BindingEntryDesc {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        kind: BindingKind::Storage {
                            min_size: 4,
                            read_only: false,
                            dynamic_offset: false,
                        },
                        count: None,
                    }],
                }],
            };
            let key = ComputePipelineKey {
                stage: ShaderStageKey::new(ctx.resources.shader_revision(self.shader)?, "main"),
                layout: schema.clone(),
            };
            self.pipeline = Some(ctx.pipelines.compute(
                "custom compute",
                &key,
                ctx.resources.shader(self.shader)?.module(),
            )?);
            let layouts = ctx.pipelines.layouts.resolve("custom compute", &schema)?;
            self.group = Some(ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: layouts.groups[0].as_ref().unwrap(),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: graph.buffer(self.output)?.as_entire_binding(),
                }],
            }));
            Ok(())
        }
        fn encode(
            &self,
            encoder: &mut wgpu::CommandEncoder,
            _: &PassResources<'_>,
        ) -> prism::RenderResult<()> {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(self.pipeline.as_ref().unwrap());
            pass.set_bind_group(0, self.group.as_ref().unwrap(), &[]);
            pass.dispatch_workgroups(1, 1, 1);
            Ok(())
        }
    }
    struct Copy {
        source: ResourceVersion,
        target: ResourceVersion,
    }
    impl GraphPass for Copy {
        fn declare(&self) -> PassDescriptor {
            PassDescriptor::new("readback")
                .access(self.source, AccessKind::CopyRead)
                .access(self.target, AccessKind::CopyWrite)
        }
        fn prepare(
            &mut self,
            _: &mut PrepareContext<'_>,
            _: &PassResources<'_>,
        ) -> prism::RenderResult<()> {
            Ok(())
        }
        fn encode(
            &self,
            e: &mut wgpu::CommandEncoder,
            g: &PassResources<'_>,
        ) -> prism::RenderResult<()> {
            e.copy_buffer_to_buffer(g.buffer(self.source)?, 0, g.buffer(self.target)?, 0, 4);
            Ok(())
        }
    }
    let (device, queue) = device();
    let mut resources = ResourceManager::new(&device);
    let mut renderer = GraphRenderer::new(&device, &queue);
    let shader=resources.create_shader("compute","@group(0) @binding(0) var<storage,read_write> value:u32; @compute @workgroup_size(1) fn main() {value=42u;}").unwrap();
    let mut plan = FrameGraph::default();
    let output = plan.graph.create_buffer(
        "compute output",
        4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let imported = plan
        .graph
        .import_buffer("readback", 4, readback.usage(), false);
    plan.imports
        .insert(imported, GraphResource::Buffer(readback.clone()));
    // Register consumer first to exercise dependency-driven scheduling.
    plan.add_pass(Copy {
        source: output,
        target: imported,
    });
    plan.add_pass(Compute {
        output,
        shader,
        pipeline: None,
        group: None,
    });
    plan.graph.export(imported);
    let frame = renderer
        .encode_graph(&resources, FrameV2::default(), plan)
        .unwrap();
    renderer.submit(frame).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 42);
}

#[test]
#[ignore = "requires a wgpu adapter"]
fn compatibility_graph_preserves_world_vfx_hud_and_optional_post_processing() {
    let (device, queue) = device();
    let mut ctx = prism::GpuContext::from_device(
        device.clone(),
        queue.clone(),
        16,
        16,
        wgpu::TextureFormat::Rgba8Unorm,
    )
    .unwrap();
    let mut resources = prism::GpuResources::new(&ctx);
    let shader=resources.load_shader_inline(&ctx,r#"
        struct Out { @builtin(position) position:vec4<f32>, @location(0) color:vec4<f32> }
        @vertex fn vs_main(@location(0) p:vec2<f32>,@location(1) uv:vec2<f32>,@location(2) color:vec4<f32>)->Out {var o:Out;o.position=vec4(p,0.,1.);o.color=color;return o;}
        @fragment fn fs_main(i:Out)->@location(0) vec4<f32>{return i.color;}
    "#,"legacy test");
    let mut renderer =
        prism::Renderer::new(&ctx, &mut resources, shader, shader, shader, shader).unwrap();
    let target = target(&device, 16, 16);
    let shape = |color| prism::DrawCommand::Shape {
        shape: prism::Shape::Quad {
            pos: [-1., -1.],
            size: [2., 2.],
            rotation: 0.,
            color,
            uv: None,
        },
        blend: prism::BlendMode::Alpha,
        layer: 0,
    };
    let frame = || {
        let mut f = prism::Frame::new();
        f.push_world(shape([1., 0., 0., 1.]));
        f
    };
    let draw = |renderer: &mut prism::Renderer, resources: &mut prism::GpuResources, frame| {
        let commands = renderer
            .render_offscreen(&ctx, resources, frame, &target)
            .unwrap();
        queue.submit([commands]);
        pixels(&device, &queue, &target)
    };
    assert_pixel(
        &draw(&mut renderer, &mut resources, frame())[..4],
        [255, 0, 0, 255],
    );
    let mut combined = frame();
    combined.push_vfx(shape([0., 0.25, 0., 0.]));
    combined.push_hud(shape([0., 0., 1., 0.5]));
    assert_pixel(
        &draw(&mut renderer, &mut resources, combined)[..4],
        [128, 32, 128, 255],
    );
    let shader=resources.load_shader_inline(&ctx,&format!("{VERTEX}\n@group(0) @binding(0) var source:texture_2d<f32>;@group(0) @binding(1) var s:sampler;@fragment fn fs_main(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{{let c=textureLoad(source,vec2<i32>(p.xy),0);return vec4(vec3(1.)-c.rgb,1.);}}"),"legacy invert");
    let a = renderer
        .add_post_process_pass::<[f32; 4]>(&ctx, &resources, shader, shader, None)
        .unwrap();
    let b = renderer
        .add_post_process_pass::<[f32; 4]>(&ctx, &resources, shader, shader, None)
        .unwrap();
    let c = renderer
        .add_post_process_pass::<[f32; 4]>(&ctx, &resources, shader, shader, None)
        .unwrap();
    assert_pixel(
        &draw(&mut renderer, &mut resources, frame())[..4],
        [0, 255, 255, 255],
    );
    renderer.disable_post_process_pass(b);
    assert_pixel(
        &draw(&mut renderer, &mut resources, frame())[..4],
        [255, 0, 0, 255],
    );
    renderer.disable_post_process_pass(c);
    assert_pixel(
        &draw(&mut renderer, &mut resources, frame())[..4],
        [0, 255, 255, 255],
    );
    renderer.disable_post_process_pass(a);
    assert_pixel(
        &draw(&mut renderer, &mut resources, frame())[..4],
        [255, 0, 0, 255],
    );
    assert_pixel(
        &draw(&mut renderer, &mut resources, prism::Frame::new())[..4],
        [0, 0, 0, 255],
    );
    renderer.resize(&mut ctx, 0, 0);
    assert!(!ctx.is_ready());
    renderer.resize(&mut ctx, 8, 8);
    let resized = self::target(&device, 8, 8);
    let commands = renderer
        .render_offscreen(&ctx, &mut resources, frame(), &resized)
        .unwrap();
    queue.submit([commands]);
    assert_pixel(&pixels(&device, &queue, &resized)[..4], [255, 0, 0, 255]);
}

#[test]
#[ignore = "requires a wgpu adapter"]
fn depth_versions_support_opaque_writes_and_transparent_reads() {
    let (device, queue) = device();
    let mut resources = ResourceManager::new(&device);
    let mut renderer = GraphRenderer::new(&device, &queue);
    renderer.depth_format = Some(wgpu::TextureFormat::Depth32Float);
    let target = target(&device, 4, 4);
    let geometry = resources.create_geometry(Geometry::fullscreen());
    let (_, base) = template(&mut resources, Some(0), "");
    let mut opaque = resources.template(base).unwrap().clone();
    opaque.depth_stencil = Some(wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: Default::default(),
        bias: Default::default(),
    });
    let mut transparent = opaque.clone();
    transparent
        .depth_stencil
        .as_mut()
        .unwrap()
        .depth_write_enabled = Some(false);
    transparent.depth_stencil.as_mut().unwrap().depth_compare =
        Some(wgpu::CompareFunction::LessEqual);
    transparent.targets[0].as_mut().unwrap().blend = Some(wgpu::BlendState::ALPHA_BLENDING);
    let opaque = resources.create_template(opaque).unwrap();
    let transparent = resources.create_template(transparent).unwrap();
    let red = material(&mut resources, opaque, Some((0, [1., 0., 0., 1.])));
    let green = material(&mut resources, opaque, Some((0, [0., 1., 0., 1.])));
    let blue = material(&mut resources, transparent, Some((0, [0., 0., 1., 0.5])));
    let frame = draw_frame(vec![
        object(geometry, red, RenderPhaseId::OPAQUE, 0),
        object(geometry, green, RenderPhaseId::OPAQUE, 1),
        object(geometry, blue, RenderPhaseId::TRANSPARENT, 0),
    ]);
    let report = renderer
        .render_to_texture(&mut resources, frame, &target)
        .unwrap();
    assert!(
        report.skipped_objects.is_empty(),
        "{:?}",
        report.skipped_objects
    );
    assert_pixel(&pixels(&device, &queue, &target)[..4], [128, 0, 128, 255]);
}

#[test]
#[ignore = "requires a wgpu adapter"]
fn shared_view_bindings_do_not_force_groups_on_other_material_families() {
    let (device, queue) = device();
    let mut resources = ResourceManager::new(&device);
    let mut renderer = GraphRenderer::new(&device, &queue);
    let target = target(&device, 4, 4);
    let geometry = resources.create_geometry(Geometry::fullscreen());
    let (_, plain_template) = template(&mut resources, None, "vec4(1.,0.,0.,1.)");
    let (_, view_template) = template(&mut resources, Some(0), "");
    let plain = material(&mut resources, plain_template, None);
    let view_material = material(&mut resources, view_template, None);
    let buffer = resources
        .create_buffer(
            "view",
            16,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        )
        .unwrap();
    let binding = resources.create_bindings(BindingSet {
        group: 0,
        entries: vec![(
            0,
            BindingResource::Buffer {
                handle: buffer,
                offset: 0,
                size: 16,
            },
        )],
    });
    let frame = FrameV2 {
        views: vec![ViewSubmission {
            bindings: vec![binding.into()],
            ..Default::default()
        }],
        objects: vec![
            object(geometry, plain, RenderPhaseId::OPAQUE, 0),
            object(geometry, view_material, RenderPhaseId::OPAQUE, 1),
        ],
        uploads: vec![UploadRequest::Buffer {
            target: buffer,
            offset: 0,
            data: bytemuck::cast_slice(&[0_f32, 1., 0., 1.]).to_vec(),
        }],
        ..Default::default()
    };
    let report = renderer
        .render_to_texture(&mut resources, frame, &target)
        .unwrap();
    assert!(
        report.skipped_objects.is_empty(),
        "{:?}",
        report.skipped_objects
    );
    assert_eq!(report.draw_calls, 3);
    assert_pixel(&pixels(&device, &queue, &target)[..4], [0, 255, 0, 255]);
}

#[test]
#[ignore = "requires a wgpu adapter"]
fn context_constructs_v2_services_and_skips_headless_presentation() {
    let (device, queue) = device();
    let mut context =
        prism::GpuContext::from_device(device, queue, 8, 8, wgpu::TextureFormat::Rgba8Unorm)
            .unwrap();
    let mut resources = ResourceManager::from_context(&context);
    let mut renderer = GraphRenderer::from_context(&context);
    assert!(
        renderer
            .render(&mut context, &mut resources, FrameV2::default())
            .unwrap()
            .is_none()
    );
    let output = target(resources.device(), 8, 8);
    let report = renderer
        .render_to_texture(&mut resources, FrameV2::default(), &output)
        .unwrap();
    assert!(report.skipped_objects.is_empty());
    let rgba = pixels(&context.device, &context.queue, &output);
    assert!(rgba.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 255]));
}
