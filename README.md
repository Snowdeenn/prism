# Prism

Prism is a Rust rendering library built on **wgpu 30**. Its V2 API accepts geometry,
materials, views, instance ranges, uploads, and post-processing requests. Application
simulation, camera behavior, and effect activation remain in the client.

Start with the **[complete V2 guide](docs/V2_GUIDE.md)**. It covers setup, every V2
subsystem, executable examples, resource ownership, graph scheduling, diagnostics,
migration, and current limitations. The same guide is available as `prism::guide`
in generated Rust documentation.

## Run and explore

```sh
cargo run --example v2 -- target/prism-v2-reference.png
cargo doc --no-deps --open
cargo test --doc
```

The [headless V2 example](src/examples/v2.rs) renders a tessellated scene with
multiple material schemas, transparency, and an overlay, then saves a PNG.
The legacy windowed example is available with `cargo run --example demo`.
A compatible GPU adapter or software backend is needed to run rendering examples.

## V2 API map

| Module / entry point | Responsibility |
| --- | --- |
| `resource::ResourceManager` | Generational handles, GPU assets, shader revisions, geometry and material storage |
| `bindings` | Owned binding schemas, validation, reusable bind group and pipeline layouts |
| `pipeline` | Structural raster and compute pipeline keys and caches |
| `material` | Immutable templates, mutable instances, parameter bytes and resource bindings |
| `render_object` | Draw descriptions, phases and stable sorting policies |
| `frame::v2` / `FrameV2` | Owned frame submissions, views, uploads and effect requests |
| `graph` | Resource versions, dependency validation, pass culling and allocation |
| `render` / `GraphRenderer` | Preparation, encoding, submission and built-in graph passes |

Keep `GpuContext` as the GPU and surface owner. Create the V2 services without
accessing its device or queue:

```rust
let mut resources = ResourceManager::from_context(&context);
let mut renderer = GraphRenderer::from_context(&context);
renderer.render(&mut context, &mut resources, frame)?;
```

For offscreen or advanced integration, raw-device constructors remain available.
Create one resource manager and renderer for your context. Register reusable assets,
build a `FrameV2`, then call `render_to_texture`; use `encode_to_texture` and `submit`
when submission must be deferred. For custom render or compute scheduling, implement
`GraphPass` and submit an explicit `FrameGraph`.

## Compatibility

The historical API remains available during migration. `prism::Frame`,
`prism::Material`, and `prism::PipelineKey` are **legacy** names. Use `FrameV2`,
`prism::material::Material`, and `prism::pipeline::PipelineKey` for V2.
`Renderer::render_v2` integrates V2 with the existing surface presentation path.
See the guide's migration section before mixing both APIs.

## Validation

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo test --test render_gpu -- --ignored --nocapture
cargo test --doc
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --no-deps
```

GPU tests are opt-in for environments without an adapter. They exercise pixels,
shader reload, resource replacement, bindings, sorting, post-processing, deferred
uploads, external compute passes, and legacy rendering. Local refactor validation
used Mesa llvmpipe through Vulkan. These deterministic scenes do not replace visual
parity checks against the consuming application's scenes or hardware benchmarks.
