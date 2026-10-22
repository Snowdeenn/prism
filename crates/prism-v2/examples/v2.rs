//! Headless reference scene: cargo run -p prism-v2 --example v2 -- target/prism-v2-reference.png
use prism_v2::{
    bindings::*,
    frame::v2::UploadRequest,
    material::*,
    pipeline::VertexLayout,
    render_object::*,
    resource::{
        ResourceManager,
        manager::{Geometry, IndexBuffer, VertexBuffer},
    },
    *,
};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let context = GpuContext::from_device(
        device.clone(),
        queue.clone(),
        640,
        360,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )?;
    let mut resources = ResourceManager::from_context(&context);
    let mut renderer = GraphRenderer::from_context(&context);
    let shader = resources.create_shader("vertex colors", COLOR_SHADER)?;
    let mut colored =
        MaterialTemplate::fullscreen("vertex colors", shader, wgpu::TextureFormat::Rgba8UnormSrgb);
    colored.vertex_layouts = vec![Some(VertexLayout {
        stride: 32,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32x4].to_vec(),
    })];
    let colored = resources.create_template(colored)?;
    let plain = resources.create_material(material::Material {
        template: colored,
        bindings: Vec::new(),
        parameters: None,
    })?;
    let shader = resources.create_shader("uniform tint", TINT_SHADER)?;
    let mut tinted = resources.template(colored)?.clone();
    tinted.label = "uniform tint".into();
    tinted.vertex.shader = shader;
    tinted.fragment.as_mut().unwrap().shader = shader;
    tinted.schema = BindingSchema {
        groups: vec![GroupSchema {
            index: 2,
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
    tinted.targets[0].as_mut().unwrap().blend = Some(wgpu::BlendState::ALPHA_BLENDING);
    let tinted = resources.create_template(tinted)?;
    let tint = resources.create_material(material::Material {
        template: tinted,
        bindings: Vec::new(),
        parameters: Some(ParameterBlock {
            group: 2,
            binding: 0,
            bytes: bytemuck::cast_slice(&[0.18_f32, 0.7, 1., 0.72]).to_vec(),
        }),
    })?;
    let mut frame = FrameV2::default();
    let shapes = [
        (
            Shape::Quad {
                pos: [-1., -1.],
                size: [2., 2.],
                rotation: 0.,
                color: [0.012, 0.018, 0.035, 1.],
                uv: None,
            },
            plain,
            RenderPhaseId::OPAQUE,
            0,
        ),
        (
            Shape::RoundedRect {
                pos: [-0.86, -0.66],
                size: [0.88, 1.32],
                radius: 0.1,
                segments: 16,
                color: [0.09, 0.16, 0.28, 1.],
            },
            plain,
            RenderPhaseId::OPAQUE,
            1,
        ),
        (
            Shape::Polygon {
                center: [-0.41, 0.04],
                sides: 6,
                radius: 0.33,
                color: [1., 0.25, 0.09, 1.],
            },
            plain,
            RenderPhaseId::OPAQUE,
            2,
        ),
        (
            Shape::Ring {
                center: [0.46, 0.02],
                inner_r: 0.30,
                outer_r: 0.39,
                start_angle: 0.,
                end_angle: std::f32::consts::TAU,
                resolution: 96,
                color: [0.24, 0.9, 0.58, 1.],
            },
            plain,
            RenderPhaseId::OPAQUE,
            3,
        ),
        (
            Shape::Quad {
                pos: [-0.30, -0.30],
                size: [0.9, 0.45],
                rotation: 0.2,
                color: [1.; 4],
                uv: None,
            },
            tint,
            RenderPhaseId::TRANSPARENT,
            0,
        ),
        (
            Shape::Quad {
                pos: [-0.86, 0.78],
                size: [1.72, 0.025],
                rotation: 0.,
                color: [0.7, 0.82, 1., 1.],
                uv: None,
            },
            plain,
            RenderPhaseId::OVERLAY,
            0,
        ),
    ];
    for (shape, material, phase, sort) in shapes {
        let mut mesh = RawMesh::default();
        Tesselator::tesselate(&shape, &mut mesh);
        let vertices = resources.create_buffer(
            "mesh vertices",
            mesh.vertices_bytes().len() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        )?;
        let indices = resources.create_buffer(
            "mesh indices",
            mesh.indices_bytes().len() as u64,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        )?;
        let geometry = resources.create_geometry(Geometry {
            vertices: vec![VertexBuffer {
                slot: 0,
                buffer: vertices,
                range: 0..mesh.vertices_bytes().len() as u64,
            }],
            indices: Some(IndexBuffer {
                buffer: indices,
                range: 0..mesh.indices_bytes().len() as u64,
                format: wgpu::IndexFormat::Uint32,
            }),
            elements: 0..mesh.indices().len() as u32,
            base_vertex: 0,
        });
        frame.uploads.push(UploadRequest::Buffer {
            target: vertices,
            offset: 0,
            data: mesh.vertices_bytes().to_vec(),
        });
        frame.uploads.push(UploadRequest::Buffer {
            target: indices,
            offset: 0,
            data: mesh.indices_bytes().to_vec(),
        });
        frame.objects.push(RenderObject {
            geometry,
            material,
            instances: 0..1,
            object_bindings: Vec::new(),
            phase,
            sort_key: SortKey(sort),
            bounds: None,
        });
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reference"),
        size: wgpu::Extent3d {
            width: 640,
            height: 360,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let report = renderer.render_to_texture(&mut resources, frame, &texture)?;
    if !report.skipped_objects.is_empty() {
        return Err(format!("{:?}", report.skipped_objects).into());
    }
    let row = 640 * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: row * 360,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row as u32),
                rows_per_image: Some(360),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(30)),
    })?;
    rx.recv()??;
    let bytes = buffer.slice(..).get_mapped_range()?;
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/prism-v2-reference.png".into());
    image::save_buffer(&path, &bytes, 640, 360, image::ColorType::Rgba8)?;
    println!(
        "{path}: {} draws, {} upload bytes, {} passes, {} pipeline creations",
        report.draw_calls,
        report.upload_bytes,
        report.passes,
        renderer.pipelines.render_stats.misses
    );
    Ok(())
}
const COLOR_SHADER: &str = r#"
struct Out { @builtin(position) position:vec4<f32>, @location(0) color:vec4<f32> }
@vertex fn vs_main(@location(0) p:vec2<f32>,@location(1) uv:vec2<f32>,@location(2) color:vec4<f32>)->Out { var o:Out;o.position=vec4(p,0.,1.);o.color=color;return o; }
@fragment fn fs_main(i:Out)->@location(0) vec4<f32>{return i.color;}
"#;
const TINT_SHADER: &str = r#"
struct Out { @builtin(position) position:vec4<f32>, @location(0) color:vec4<f32> }
@group(2) @binding(0) var<uniform> tint:vec4<f32>;
@vertex fn vs_main(@location(0) p:vec2<f32>,@location(1) uv:vec2<f32>,@location(2) color:vec4<f32>)->Out { var o:Out;o.position=vec4(p,0.,1.);o.color=color;return o; }
@fragment fn fs_main(i:Out)->@location(0) vec4<f32>{return i.color*tint;}
"#;
