use prism_v2::{arena::Arena, bindings::*, graph::*, ids::ShaderTag, pipeline::*};

fn color() -> TextureDesc {
    TextureDesc::color(16, 16, wgpu::TextureFormat::Rgba8Unorm)
}
fn uniform(binding: u32) -> BindingEntryDesc {
    BindingEntryDesc {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        kind: BindingKind::Uniform {
            min_size: 64,
            dynamic_offset: false,
        },
        count: None,
    }
}
#[test]
fn schemas_are_structural_normalized_and_keep_group_indices() {
    let a = BindingSchema {
        groups: vec![GroupSchema {
            index: 2,
            entries: vec![uniform(1), uniform(0)],
        }],
    };
    let b = BindingSchema {
        groups: vec![GroupSchema {
            index: 2,
            entries: vec![uniform(0), uniform(1)],
        }],
    };
    assert_eq!(a.normalized().unwrap(), b.normalized().unwrap());
    let mut c = b.clone();
    c.groups[0].index = 0;
    assert_ne!(a.normalized().unwrap(), c.normalized().unwrap());
    let mut duplicate = b.clone();
    duplicate.groups[0].entries.push(uniform(0));
    assert!(
        duplicate
            .normalized()
            .unwrap_err()
            .to_string()
            .contains("binding 0")
    );
    let mut oversized = b;
    oversized.groups[0].index = 99;
    assert!(
        oversized
            .validate("shader", &wgpu::Limits::default(), wgpu::Features::empty())
            .is_err()
    );
}
fn key() -> PipelineKey {
    let id = Arena::<(), ShaderTag>::new().insert(());
    let shader = ShaderRevisionKey {
        shader: id,
        revision: 0,
    };
    PipelineKey {
        vertex: ShaderStageKey::new(shader, "vs_main"),
        fragment: Some(ShaderStageKey::new(shader, "fs_main")),
        layout: BindingSchema::default(),
        vertex_layouts: vec![],
        primitive: Default::default(),
        depth_stencil: None,
        targets: vec![Some(wgpu::TextureFormat::Rgba8Unorm.into())],
        multisample: Default::default(),
        multiview_mask: None,
    }
}
#[test]
fn pipeline_keys_distinguish_every_supported_state() {
    let original = key();
    let mut variants = Vec::new();
    macro_rules! changed {
        ($k:ident, $body:expr) => {{
            let mut $k = original.clone();
            $body;
            variants.push($k);
        }};
    }
    changed!(k, k.vertex.shader.revision += 1);
    changed!(k, k.vertex.entry_point = "another".into());
    changed!(k, k.vertex.constants.push(("mode".into(), 1_f64.to_bits())));
    changed!(k, k.vertex.zero_initialize_workgroup_memory = false);
    changed!(k, k.fragment = None);
    changed!(k, k.fragment.as_mut().unwrap().shader.revision += 1);
    changed!(
        k,
        k.layout.groups.push(GroupSchema {
            index: 0,
            entries: vec![uniform(0)]
        })
    );
    changed!(
        k,
        k.vertex_layouts.push(Some(VertexLayout {
            stride: 8,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: vec![wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0
            }]
        }))
    );
    changed!(k, k.primitive.cull_mode = Some(wgpu::Face::Back));
    changed!(k, k.primitive.topology = wgpu::PrimitiveTopology::LineList);
    changed!(
        k,
        k.depth_stencil = Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default()
        })
    );
    changed!(
        k,
        k.targets[0].as_mut().unwrap().format = wgpu::TextureFormat::Rgba16Float
    );
    changed!(
        k,
        k.targets[0].as_mut().unwrap().blend = Some(wgpu::BlendState::ALPHA_BLENDING)
    );
    changed!(
        k,
        k.targets[0].as_mut().unwrap().write_mask = wgpu::ColorWrites::RED
    );
    changed!(k, k.multisample.count = 4);
    changed!(k, k.multisample.mask = 0);
    changed!(k, k.multisample.alpha_to_coverage_enabled = true);
    changed!(k, k.multiview_mask = std::num::NonZeroU32::new(3));
    let mut keys = std::collections::HashSet::from([original]);
    for variant in variants {
        assert!(keys.insert(variant));
    }
}
#[test]
fn constants_normalize_but_reject_ambiguous_names() {
    let mut a = key();
    let mut b = key();
    a.vertex.constants = vec![("b".into(), 2_f64.to_bits()), ("a".into(), 1_f64.to_bits())];
    b.vertex.constants = a.vertex.constants.iter().rev().cloned().collect();
    assert_eq!(a.normalized().unwrap(), b.normalized().unwrap());
    b.vertex.constants.push(("a".into(), 0));
    assert!(b.normalized().is_err());
}
#[test]
fn graph_orders_versions_and_culls_unobservable_work() {
    let mut g = RenderGraph::new();
    let scene = g.create_texture("scene", color());
    let next = g.next_version(scene).unwrap();
    let dead = g.create_texture("dead", color());
    let overlay = g.add_pass(PassDescriptor::new("overlay").access(next, AccessKind::ColorLoad));
    let world = g.add_pass(PassDescriptor::new("world").access(scene, AccessKind::ColorClear));
    g.add_pass(PassDescriptor::new("dead").access(dead, AccessKind::ColorClear));
    let external = g.add_pass(PassDescriptor::new("external").side_effect());
    g.export(next);
    let compiled = g.compile().unwrap();
    assert_eq!(compiled.order, vec![world, overlay, external]);
    assert_eq!(compiled.lifetimes[scene.resource], Some(0..=1));
    assert_eq!(compiled.lifetimes[dead.resource], None);
}
#[test]
fn graph_rejects_cycles_uninitialized_loads_conflicts_and_usage_mismatches() {
    let mut g = RenderGraph::new();
    let a = g.add_pass(PassDescriptor::new("a").side_effect());
    let b = g.add_pass(PassDescriptor::new("b").after(a));
    g.depends_on(a, b).unwrap();
    assert!(g.compile().unwrap_err().to_string().contains("cycle"));
    let mut g = RenderGraph::new();
    let scene = g.create_texture("scene", color());
    g.add_pass(PassDescriptor::new("read").access(scene, AccessKind::Sampled));
    assert!(
        g.compile()
            .unwrap_err()
            .to_string()
            .contains("uninitialized")
    );
    let mut g = RenderGraph::new();
    let scene = g.create_texture("scene", color());
    g.add_pass(PassDescriptor::new("bad load").access(scene, AccessKind::ColorLoad));
    assert!(g.compile().is_err());
    let mut g = RenderGraph::new();
    let scene = g.import_texture("scene", color(), true);
    let next = g.next_version(scene).unwrap();
    g.add_pass(
        PassDescriptor::new("feedback")
            .access(scene, AccessKind::Sampled)
            .access(next, AccessKind::ColorClear),
    );
    assert!(g.compile().unwrap_err().to_string().contains("conflicting"));
    let mut g = RenderGraph::new();
    let mut desc = color();
    desc.usage = wgpu::TextureUsages::TEXTURE_BINDING;
    let scene = g.create_texture("scene", desc);
    g.add_pass(PassDescriptor::new("bad usage").access(scene, AccessKind::ColorClear));
    assert!(g.compile().is_err());
    let mut g = RenderGraph::new();
    let mut desc = color();
    desc.format = wgpu::TextureFormat::Depth32Float;
    let depth = g.create_texture("depth", desc);
    g.add_pass(PassDescriptor::new("bad format").access(depth, AccessKind::ColorClear));
    assert!(g.compile().is_err());
}
#[test]
fn old_version_readers_precede_in_place_overwrites() {
    let mut g = RenderGraph::new();
    let scene = g.import_texture("scene", color(), true);
    let next = g.next_version(scene).unwrap();
    let writer = g.add_pass(PassDescriptor::new("overwrite").access(next, AccessKind::ColorClear));
    let reader = g.add_pass(
        PassDescriptor::new("read old")
            .access(scene, AccessKind::Sampled)
            .side_effect(),
    );
    g.export(next);
    assert_eq!(g.compile().unwrap().order, vec![reader, writer]);
}
#[test]
fn foreign_handles_and_multiple_writers_are_rejected() {
    let mut a = RenderGraph::new();
    let foreign = a.create_texture("foreign", color());
    let mut b = RenderGraph::new();
    b.export(foreign);
    assert!(b.compile().is_err());
    let own = b.create_texture("own", color());
    b.add_pass(PassDescriptor::new("first").access(own, AccessKind::ColorClear));
    b.add_pass(PassDescriptor::new("second").access(own, AccessKind::ColorClear));
    assert!(b.compile().is_err());
    assert!(b.next_version(foreign).is_err());
}
#[test]
fn empty_graph_and_imported_initialized_output_are_valid() {
    assert!(RenderGraph::new().compile().unwrap().order.is_empty());
    let mut g = RenderGraph::new();
    let imported = g.import_texture("external", color(), true);
    g.export(imported);
    assert!(g.compile().unwrap().order.is_empty());
}

#[test]
fn anti_dependencies_do_not_resurrect_culled_readers_or_overwritten_writers() {
    let mut graph = RenderGraph::new();
    let scene = graph.create_texture("scene", color());
    let unused = graph.create_texture("unused", color());
    let final_scene = graph.next_version(scene).unwrap();
    let clear = graph.add_pass(PassDescriptor::new("clear").access(scene, AccessKind::ColorClear));
    graph.add_pass(
        PassDescriptor::new("unused effect")
            .access(scene, AccessKind::Sampled)
            .access(unused, AccessKind::ColorClear),
    );
    let overlay =
        graph.add_pass(PassDescriptor::new("overlay").access(final_scene, AccessKind::ColorLoad));
    graph.export(final_scene);
    assert_eq!(graph.compile().unwrap().order, vec![clear, overlay]);

    let mut graph = RenderGraph::new();
    let first = graph.create_texture("scene", color());
    let second = graph.next_version(first).unwrap();
    graph.add_pass(PassDescriptor::new("overwritten").access(first, AccessKind::ColorClear));
    let observable =
        graph.add_pass(PassDescriptor::new("last clear").access(second, AccessKind::ColorClear));
    graph.export(second);
    assert_eq!(graph.compile().unwrap().order, vec![observable]);
}
