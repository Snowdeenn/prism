use prism::{v1, v2};

#[test]
fn root_keeps_both_frame_apis_available() {
    let legacy: prism::Frame = v1::Frame::new();
    let _: prism_v1::Frame = legacy;

    let frame: prism::FrameV2 = v2::Frame::default();
    let frame: prism::frame::v2::Frame = frame;
    let _: prism_v2::Frame = frame;
}

#[test]
fn both_apis_share_geometry_and_handle_types() {
    let mesh: v1::RawMesh = v2::RawMesh::new();
    let _: prism::RawMesh = mesh;

    let mut arena = v1::arena::Arena::<(), v1::ids::ShaderTag>::new();
    let shader: v2::ids::ShaderId = arena.insert(());
    let _: prism::ids::ShaderId = shader;
    assert!(arena.get(shader).is_some());
}
