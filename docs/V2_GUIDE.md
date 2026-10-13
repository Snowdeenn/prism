# Prism V2: architecture and integration guide

This guide documents the implemented V2 API, built on Rust 2024 and wgpu 30.
It distinguishes supported behavior from extension points and future optimizations.
The public Rust modules provide the exact signatures; the examples below are checked
by `cargo test --workspace --doc` through the `prism_v2::guide` module.

## Contents

1. [Architecture and ownership](#architecture-and-ownership)
2. [First frame](#first-frame)
3. [Resource management and lifetime](#resource-management-and-lifetime)
4. [Binding schemas and layouts](#binding-schemas-and-layouts)
5. [Materials and binding resolution](#materials-and-binding-resolution)
6. [Geometry, instancing, phases and views](#geometry-instancing-phases-and-views)
7. [Frame uploads and submission](#frame-uploads-and-submission)
8. [Default composition, depth and HDR](#default-composition-depth-and-hdr)
9. [Post-processing](#post-processing)
10. [Render graph semantics](#render-graph-semantics)
11. [Custom render and compute passes](#custom-render-and-compute-passes)
12. [Pipeline caches and shader reload](#pipeline-caches-and-shader-reload)
13. [Errors and instrumentation](#errors-and-instrumentation)
14. [Migration from the legacy API](#migration-from-the-legacy-api)
15. [Validation and current limits](#validation-and-current-limits)

## Architecture and ownership

Prism receives rendering data. It does not own game entities, simulate effects,
compute camera shake, or decide which effects are active. The client computes final
view data and submits objects and effect requests for each frame.

The data flow is:

```text
Device + Queue
    ├── ResourceManager: persistent assets and CPU descriptions
    └── GraphRenderer: device-local caches and effect registry
             ↑
       FrameV2: views, objects, uploads, effect requests
             ↓
       default graph OR explicit FrameGraph
             ↓
       validate → schedule → allocate → prepare → encode
             ↓
       EncodedFrame { commands, report } → submit → GPU execution
```

A `ResourceManager` stores reusable assets. A `FrameV2` owns its submission vectors
and upload bytes, but references assets through handles. A `FrameGraph` owns pass
implementations and logical graph descriptions. An `EncodedFrame` owns a wgpu command
buffer and its preparation report.

Use `GpuContext::new` for standard windowed initialization. For custom features or
limits, create the device and queue yourself and optionally wrap them with
`GpuContext::from_device`. Prism does not request optional features implicitly. Use the same device
for the resource manager, renderer, imported resources and raw GPU objects. A renderer
binds itself to the first resource manager used for encoding and rejects a different
manager. Recreate both when recreating the device.

Handle types separate resource categories; they do not encode manager identity.
Never transfer handles between managers, even if their numeric values happen to match.
Likewise, `PostProcessId` belongs to the renderer that registered the effect.

## Using GpuContext as the entry point

`GpuContext` remains the owner of the device, queue, window surface, surface
configuration and size. For ordinary windowed rendering, create V2 services from
it and let the renderer acquire and present frames through it:

```no_run
use std::sync::Arc;
use prism_v2::{GpuContext, GraphRenderer, FrameV2};
use prism_v2::resource::ResourceManager;

async fn initialize_and_render(window: Arc<winit::window::Window>)
    -> Result<(), Box<dyn std::error::Error>>
{
    let mut context = GpuContext::new(window).await?;
    let mut resources = ResourceManager::from_context(&context);
    let mut renderer = GraphRenderer::from_context(&context);
    let format = context.surface_format();
    // Register shaders, geometry and materials using `resources` and `format`.
    let frame = FrameV2::default();
    if let Some(report) = renderer.render(&mut context, &mut resources, frame)? {
        assert!(report.skipped_objects.is_empty());
    }
    Ok(())
}
```

Keep these three objects across frames. Forward window size changes to
`context.resize(width, height)`. `render` returns `None` when no surface is ready
or acquisition must be skipped, and reconfigures lost/outdated surfaces. A headless
context has no surface, so use `render_to_texture` for offscreen rendering.

The constructors share internal wgpu handles; they do not create another device or
borrow the context for their lifetime. All services must still use the same device.
Raw `new(device, queue)` / `new(device)` constructors and the context's public GPU
fields remain available for advanced integrations and compatibility. They are not
needed in the windowed flow above. Device recreation requires recreating the services.

## First frame

This complete headless example renders an orange procedural triangle. `no_run`
means the documentation checks compilation without requiring a GPU in CI.

```no_run
use prism_v2::{FrameV2, GraphRenderer};
use prism_v2::material::{Material, MaterialTemplate};
use prism_v2::render_object::{RenderObject, RenderPhaseId, SortKey};
use prism_v2::resource::{ResourceManager, manager::Geometry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(
        wgpu::InstanceDescriptor::new_without_display_handle(),
    );
    let adapter = pollster::block_on(instance.request_adapter(
        &wgpu::RequestAdapterOptions::default(),
    ))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor::default(),
    ))?;
    let mut resources = ResourceManager::new(&device);
    let mut renderer = GraphRenderer::new(&device, &queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let shader = resources.create_shader("triangle", r#"
        @vertex fn vs_main(@builtin(vertex_index) i: u32)
            -> @builtin(position) vec4<f32> {
            let positions = array<vec2<f32>, 3>(
                vec2(-0.7, -0.6), vec2(0.7, -0.6), vec2(0.0, 0.7));
            return vec4(positions[i], 0.0, 1.0);
        }
        @fragment fn fs_main() -> @location(0) vec4<f32> {
            return vec4(1.0, 0.35, 0.08, 1.0);
        }
    "#)?;
    let template = resources.create_template(
        MaterialTemplate::fullscreen("triangle", shader, format),
    )?;
    let material = resources.create_material(Material {
        template,
        bindings: vec![],
        parameters: None,
    })?;
    // This geometry issues three vertices; the shader determines their positions.
    let geometry = resources.create_geometry(Geometry::fullscreen());
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("output"),
        size: wgpu::Extent3d {
            width: 640, height: 360, depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut frame = FrameV2::default();
    frame.objects.push(RenderObject {
        geometry, material,
        instances: 0..1,
        object_bindings: vec![],
        phase: RenderPhaseId::OPAQUE,
        sort_key: SortKey(0),
        bounds: None,
    });
    let report = renderer.render_to_texture(&mut resources, frame, &target)?;
    assert!(report.skipped_objects.is_empty());
    // Submission is asynchronous. Readback requires a copy, map and device poll.
    Ok(())
}
```

`MaterialTemplate::fullscreen` is a convenience descriptor, not a shader generator:
it selects `vs_main` / `fs_main`, no vertex buffers or bindings, triangle-list
primitives, one color target, no blending, no depth, and one sample. The shader may
produce a full-screen triangle or any other procedural three-vertex geometry.

For indexed CPU tessellation, multiple schemas and PNG readback, see the repository's
`crates/prism-v2/examples/v2.rs`. Readback is an explicit wgpu operation; graph export is unrelated
to downloading pixels.

## Resource management and lifetime

`ResourceManager` owns generational arenas for these resource categories:

| Handle | Stored value | Creation / access |
| --- | --- | --- |
| `ShaderId` | Validated shader module, label and revision | `create_shader`, `shader`, `shader_revision` |
| `BufferId` | GPU buffer | `create_buffer`, `buffer` |
| `TextureId` | GPU texture | `create_texture`, `texture` |
| `TextureViewHandle` | View, parent texture handle, dimension and format | `create_view`, `view` |
| `SamplerHandle` | Sampler and binding classification | `create_sampler`, `sampler` |
| `GeometryHandle` | Vertex/index buffer references and draw range | `create_geometry`, `geometry` |
| `MaterialTemplateHandle` | Immutable shader/layout/render-state description | `create_template`, `template` |
| `MaterialHandle` | Template reference, binding references and optional parameters | `create_material`, `material`, `material_mut` |
| `BindingSetHandle` | Entries for one bind group | `create_bindings`, `bindings`, `bindings_mut` |

Each category has a corresponding `remove_*` method returning whether the handle
was live. Reusing a slot changes its generation. Lookups of stale or missing handles
return `RenderError`. Registration does not always validate every dependency:
geometry and binding sets are checked when preparing a draw, and material creation
checks its template while bindings are resolved later. Template creation validates
its schema and shader handles; actual pipeline compilation happens on demand.

Removal is not recursive. Removing a geometry does not remove its buffers; removing
a material does not remove its template or binding sets. Removing a texture makes
its existing views invalid for manager lookup, although their arena entries remain
until explicitly removed. Update bindings after replacing a texture and its views.

Encoded wgpu commands retain their GPU resources. Removing a CPU handle does not
retroactively invalidate a command buffer already encoded. Raw object accessors
(`raw()` and shader `module()`) are intended for custom GPU integration; explicitly
calling raw `destroy()` bypasses this lifetime contract and must be coordinated by
the application.

### Resize and device replacement

The default graph sizes scene, depth and post-process textures from the target on
each frame. To resize a persistent texture, create a replacement and its views,
update the binding sets that refer to them, then remove obsolete handles. There is
no in-place manager resize operation. Recheck material target formats if the surface
format changes. Shader/template handles and pipeline caches are device-specific;
none survive a device replacement as usable assets.

`TextureDesc::color(width, height, format)` creates a descriptor for one 2D layer,
one mip and one sample, with `RENDER_ATTACHMENT | TEXTURE_BINDING` usage. Add copy
or storage usage explicitly when needed. Manager texture creation uses no alternate
view formats.

## Binding schemas and layouts

A `BindingSchema` is owned data: a list of `GroupSchema` values, each containing an
explicit group index and `BindingEntryDesc` entries. There are no reserved camera,
material or object group numbers in V2.

Each entry specifies the WGSL binding number, shader visibility, binding kind and an
optional array count. Supported kinds are uniform buffers, read-only or writable
storage buffers, sampled textures, samplers and storage textures. Buffer kinds carry
minimum binding sizes and dynamic-offset flags; textures carry sample type,
dimension and multisampling or storage access/format.

```rust
use prism_v2::bindings::{BindingSchema, GroupSchema, BindingEntryDesc, BindingKind};

let schema = BindingSchema {
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
let normalized = schema.normalized()?;
assert_eq!(normalized.groups[0].index, 2); // No renumbering.
# Ok::<(), prism_v2::RenderError>(())
```

This schema corresponds to `@group(2) @binding(0) var<uniform> tint: vec4<f32>`.
Normalization sorts groups and binding numbers and rejects duplicates, empty stage
visibility, and zero-length binding arrays. `validate(label, limits, features)` also
checks supported limits and array features and returns a normalized schema. It does
not perform shader reflection; final compatibility is checked by wgpu when layouts,
bind groups and pipelines are created.

Use `count: None` for a scalar binding and `Some(n)` for an array with exactly `n`
resources. Buffer arrays require `BUFFER_BINDING_ARRAY`; storage buffer arrays also
require `STORAGE_RESOURCE_BINDING_ARRAY`. Texture and sampler arrays require
`TEXTURE_BINDING_ARRAY`; storage texture arrays additionally require
`STORAGE_RESOURCE_BINDING_ARRAY`. The device must have enabled all required features.
Other backend restrictions still apply.

A buffer minimum size of zero defers the minimum-size check to later validation.
Low-level `wgpu_type()` and `wgpu_entry()` are conversions, not replacements for
schema validation; in particular, do not pass an unvalidated zero array count.

### Layout caching

`LayoutCache::resolve` returns `ResolvedLayout` with the normalized key, a vector
of optional group layouts and the pipeline layout. Sparse indices are preserved:
a schema containing only group 2 has empty slots before it. An empty schema has no
forced groups. Identical entry lists can share a bind group layout even when used
at different group indices; pipeline layouts are keyed by the complete normalized
schema. Layouts are specific to the cache's device.

The cache exposes cumulative `group_stats` and `pipeline_stats`. `clear()` drops
cached entries but does not reset those counters. Resolved layouts are `Arc` values;
existing users may retain them after cache eviction.

## Materials and binding resolution

A `MaterialTemplate` describes everything needed to select a render pipeline:
shader entry points and constants, schema, vertex layouts, primitive state,
depth/stencil state, color targets, multisampling and multiview mask. Registered
templates are immutable. Clone a template and register a new one to change state.

A `Material` selects a template and supplies persistent binding-set handles plus an
optional `ParameterBlock`. Materials and binding sets can be changed through their
manager accessors. Changes are checked at the next preparation; they do not modify
command buffers already encoded.

### Parameter bytes

`ParameterBlock { group, binding, bytes }` supplies a uniform binding at an explicit
location. The bytes must be nonempty, their length must be divisible by four, and
they must satisfy the schema and WGSL memory layout. Prism does not serialize Rust
structs, insert padding, or transpose matrices. Use a suitable `#[repr(C)]` / Pod
representation and account for WGSL alignment yourself.

```rust
use prism_v2::material::ParameterBlock;
let tint = ParameterBlock {
    group: 2,
    binding: 0,
    bytes: bytemuck::cast_slice(&[1.0_f32, 0.3, 0.1, 1.0]).to_vec(),
};
assert_eq!(tint.bytes.len(), 16);
```

Each prepared draw receives a fresh uniform buffer for its material parameters.
There is currently no shared parameter arena or per-material deduplication. Changing
these bytes does not alter the pipeline key. `ParameterBlock::default()` has empty
bytes and is not a valid supplied uniform block until populated.

### Explicit binding sets

`BindingSet { group, entries }` represents one group; entries pair a binding number
with `BindingResource`. Resource variants reference a buffer range, texture view,
sampler, or arrays of those. Buffer ranges use explicit byte offsets and nonzero
sizes. Their usage, minimum size, bounds and alignment must match the schema.
Array lengths must match the declared count exactly.

Resolution collects:

1. View bindings whose group is declared by the template.
2. Material binding sets.
3. Object binding sets.
4. Generated material parameters, when present.

This is collection, **not an override hierarchy**. Two binding sets for the same
group are rejected. Duplicate binding entries are rejected, including collisions
with generated parameters. A parameter block can fill a missing entry alongside
one binding set for its group. Every required entry must be supplied; extra material
or object groups/entries are errors. A stale view binding handle is an error even
if its group would otherwise be ignored.

### Dynamic offsets

A `BindingSetUse` adds `dynamic_offsets` to a binding-set handle. Offsets follow
ascending binding-number order among that group's dynamic buffer entries. Their
count must match, and each must meet the device's uniform or storage offset alignment.
The base range plus dynamic offset must remain inside the buffer.

```rust
use prism_v2::material::{BindingResource, BindingSet, BindingSetUse};
use prism_v2::resource::ResourceManager;
use prism_v2::ids::BufferId;

fn select_uniform_record(
    resources: &mut ResourceManager,
    buffer: BufferId,
    record: u32,
) -> BindingSetUse {
    let alignment = resources.device().limits().min_uniform_buffer_offset_alignment;
    let handle = resources.create_bindings(BindingSet {
        group: 0,
        entries: vec![(0, BindingResource::Buffer {
            handle: buffer, offset: 0, size: 16,
        })],
    });
    // The caller allocates enough space and declares this uniform as dynamic.
    BindingSetUse { handle, dynamic_offsets: vec![record * alignment] }
}
```

View and object submissions accept `BindingSetUse`. `Material::bindings` stores plain
handles and supplies no dynamic offsets. Converting a handle with `.into()` creates
a use with an empty offset list.

Built-in phase and full-screen passes reject writable storage bindings. Such writes
must be represented by explicit custom graph passes so scheduling sees their resource
dependencies. Bind groups are rebuilt during preparation; only their layouts and
pipelines are cached.

## Geometry, instancing, phases and views

`Geometry` describes buffers without owning their handles exclusively:

| Field | Meaning |
| --- | --- |
| `vertices` | Buffer slot, buffer handle and byte range for each vertex/instance stream |
| `indices` | Optional index buffer, byte range and `Uint16` or `Uint32` format |
| `elements` | Half-open index range for indexed geometry, otherwise vertex range |
| `base_vertex` | Signed vertex offset for indexed draws |

`VertexLayout` in the template gives stride, `VertexStepMode`, and attributes for
each slot. Optional slots are preserved. Slots must be unique; required buffers
must exist, carry the right usage and have valid ranges. Vertex buffer starts must
be four-byte aligned; index starts follow their element size. The renderer validates
range sizes but does not read index contents back to find the largest referenced
vertex. `Geometry::fullscreen()` has no buffers and selects vertices `0..3`.

A `RenderObject` adds material, half-open instance range, object bindings, phase,
sort key and optional bounds. Empty instance ranges are skipped. For instance
attributes, use `VertexStepMode::Instance`; for shader-managed instance records,
use suitable read-only storage bindings. Prism does not merge separate objects into
one draw. `Bounds` is currently metadata: no automatic frustum culling uses it.

### Stable ordering

`RenderPhaseId(pub u32)` is extensible. Built-in IDs are `OPAQUE`, `TRANSPARENT`
and `OVERLAY`. A view lists the phases it wants to render and their sort policy.

| Policy | Order |
| --- | --- |
| `Opaque` | Template handle, then material handle, then ascending sort key |
| `BackToFront` | Descending sort key; equal keys retain submission order |
| `SubmissionOrder` | Ascending sort key; equal keys retain submission order |

Despite its name, `SubmissionOrder` honors layer keys before preserving ties. Set
all keys equal for literal submission order. The client computes depth keys for
transparency. Opaque material grouping can reorder layered 2D objects; choose a
layer-oriented policy when painter order matters.

### Views

`ViewSubmission` contains bindings, an ordered phase list and an optional viewport
`[x, y, width, height]` in pixels. Viewports must be finite, positive in size and fit
the target. The default view renders opaque, transparent, then overlay phases using
the policies in the table.

All views select from the same frame object list by phase; there is no object view
mask. Multiple views may render the same object. The client uploads final camera
matrices and other view uniforms. All views in the default composition share scene
color and depth; use a custom graph for independently cleared view targets.

## Frame uploads and submission

`FrameV2` owns `views`, `objects`, `uploads`, `post_process` and `overlay_policy`.
`clear()` empties the four vectors while retaining their capacities and **preserves**
the overlay policy. The high-level texture path inserts a default view when the list
is empty. The explicit graph path does not insert views or interpret effect requests.

### Buffer uploads

`UploadRequest::Buffer` copies owned bytes to a manager buffer. The buffer must have
`COPY_DST`; offset and byte count must be multiples of four and remain in bounds.
An empty in-bounds request produces no copy.

```rust
use prism_v2::{FrameV2, ids::BufferId};
use prism_v2::frame::v2::UploadRequest;
fn upload_vertices(frame: &mut FrameV2, target: BufferId, values: &[f32]) {
    frame.uploads.push(UploadRequest::Buffer {
        target, offset: 0,
        data: bytemuck::cast_slice(values).to_vec(),
    });
}
```

### Texture uploads

`UploadRequest::Texture` specifies target, mip, origin, extent, row layout and bytes.
The texture must have `COPY_DST`. The payload is nonempty, `bytes_per_row` must be a
positive multiple of 256, and `rows_per_image` must be positive. Backend validation
checks the mip, bounds, format/block restrictions and payload size. Copies use the
full texture aspect. Prism does not pad rows, convert formats or generate mips.

This example pads tightly packed RGBA8 rows for a single 2D layer:

```rust
use prism_v2::frame::v2::UploadRequest;
use prism_v2::ids::TextureId;
fn rgba8_upload(target: TextureId, width: u32, height: u32, rgba: &[u8]) -> UploadRequest {
    assert!(width > 0 && height > 0);
    let packed = width * 4;
    assert_eq!(rgba.len(), packed as usize * height as usize);
    let padded = packed.div_ceil(256) * 256;
    let mut data = vec![0; padded as usize * height as usize];
    for row in 0..height as usize {
        data[row * padded as usize..row * padded as usize + packed as usize]
            .copy_from_slice(&rgba[row * packed as usize..(row + 1) * packed as usize]);
    }
    UploadRequest::Texture {
        target, mip_level: 0, origin: wgpu::Origin3d::ZERO,
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        bytes_per_row: padded, rows_per_image: height, data,
    }
}
```

### Encode now, submit later

Uploads are staging-buffer copies encoded before graph passes in the same command
buffer, in request order. They are not immediate `queue.write_*` calls. It is safe
to encode frames A and B before submitting either: each command buffer retains its
own upload bytes. Submit them in the intended order when they share persistent
resources. Later GPU reads still observe queue execution order, not isolated copies
of every persistent asset.

```no_run
# use prism_v2::{FrameV2, GraphRenderer, RenderResult};
# use prism_v2::resource::ResourceManager;
fn deferred(
    renderer: &mut GraphRenderer,
    resources: &mut ResourceManager,
    target: &wgpu::Texture,
    a: FrameV2,
    b: FrameV2,
) -> RenderResult<()> {
    let a = renderer.encode_to_texture(resources, a, target)?;
    let b = renderer.encode_to_texture(resources, b, target)?;
    let report_a = renderer.submit(a)?;
    let report_b = renderer.submit(b)?;
    assert!(report_a.skipped_objects.is_empty());
    assert!(report_b.skipped_objects.is_empty());
    Ok(())
}
```

`render_to_texture` is the encode-and-submit convenience operation. `submit` does
not wait for completion or present a surface. The application manages surface
acquisition/presentation and GPU readback, or uses the legacy renderer's `render_v2`
bridge described below.

## Default composition, depth and HDR

`encode_to_texture` builds this graph for a single-sample, single-layer 2D target:

1. Clear scene color to opaque black.
2. Clear optional depth to 1.0.
3. Render each view's phases in supplied order, deferring overlay when requested.
4. Apply requested post-process effects in their submission order.
5. Draw the built-in identity blit into the output target.
6. Render deferred overlay phases directly into the output.

The target must support `RENDER_ATTACHMENT`; use a conventional one-mip render
texture. Existing target contents are replaced. An empty frame still clears and
produces an output. The final blit adds a draw even with no effects.

`OverlayPolicy::AfterPostProcess` is the default. It keeps overlays outside the
effect chain and renders them without depth. `BeforePostProcess` renders overlay
with the scene, making it subject to the effects. Overlay templates must match the
attachment format and depth mode where they actually execute.

Set `renderer.depth_format` to enable scene depth. Materials used there need matching
depth/stencil state and attachment format. In the default path, only `OPAQUE` phases
write depth; other phase IDs use read-only depth. Transparent materials must disable
depth writes and respect read-only stencil requirements. For other depth policies,
construct explicit `PhasePass` nodes. The built-in path is not a complete stencil
composition system and provides no MSAA resolve, MRT or multiview rendering.

`renderer.scene_format` defaults to the output format. Set it to an appropriate HDR
format, such as `Rgba16Float`, to retain HDR scene values. Register a tone-mapping
effect explicitly when needed: the final blit performs sampling/output conversion,
not an implicit exposure or tone-mapping curve. Each scene material must match the
scene format; each effect template determines its own output format.

## Post-processing

`FullscreenEffect` describes an immutable template handle, generated source texture
and sampler binding locations, an optional generated parameter binding and additional
binding-set uses. `register_post_process` returns a renderer-local ID; registration
alone does not activate the effect or validate all of its assets.

Each `PostProcessSubmission` contains the ID and owned parameter bytes. Requests are
executed in list order, including repeated requests for the same effect. Removing
an effect returns its definition; a later request for that removed ID is an error.

`FullscreenEffect::source_schema()` describes a filterable, non-multisampled D2
float texture at group 0 binding 0 and a filtering sampler at group 0 binding 1,
both visible to the fragment stage. `MaterialTemplate::fullscreen` does not add
that schema automatically.

```no_run
use prism_v2::{FrameV2, GraphRenderer, RenderResult};
use prism_v2::resource::ResourceManager;
use prism_v2::material::MaterialTemplate;
use prism_v2::render::FullscreenEffect;
use prism_v2::frame::v2::PostProcessSubmission;

fn add_invert(
    resources: &mut ResourceManager,
    renderer: &mut GraphRenderer,
    frame: &mut FrameV2,
    format: wgpu::TextureFormat,
) -> RenderResult<()> {
    let shader = resources.create_shader("invert", r#"
        struct Out {
            @builtin(position) position: vec4<f32>,
            @location(0) uv: vec2<f32>,
        }
        @group(0) @binding(0) var source: texture_2d<f32>;
        @group(0) @binding(1) var source_sampler: sampler;
        @vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {
            let p = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
            var o: Out;
            o.position = vec4(p[i], 0., 1.);
            o.uv = p[i] * vec2(0.5, -0.5) + vec2(0.5);
            return o;
        }
        @fragment fn fs_main(i: Out) -> @location(0) vec4<f32> {
            let c = textureSample(source, source_sampler, i.uv);
            return vec4(vec3(1.) - c.rgb, c.a);
        }
    "#)?;
    let mut template = MaterialTemplate::fullscreen("invert", shader, format);
    template.schema = FullscreenEffect::source_schema();
    let template = resources.create_template(template)?;
    let effect = renderer.register_post_process(FullscreenEffect {
        template,
        source_binding: (0, 0),
        sampler_binding: (0, 1),
        parameter_binding: None,
        bindings: vec![],
    });
    frame.post_process.push(PostProcessSubmission { effect, parameters: vec![] });
    Ok(())
}
```

To add parameters, add a uniform entry to the template schema and set
`parameter_binding` to its location. Supply nonempty, four-byte-aligned uniform
bytes in each request. With `parameter_binding: None`, request bytes must be empty.
Generated source, sampler and parameter entries cannot collide with each other or
with additional bindings.

The built-in full-screen pass requires one color target, one sample, no vertex
layouts, no depth and no multiview. It creates a nearest-filtered, clamp-to-edge
sampler during preparation and draws vertices `0..3`, instances `0..1`. The shader
owns its UV convention. Custom graphs may use different source/output sizes; the
default chain uses the output target's dimensions for intermediate textures.
Sampling and writing must use different logical textures, not versions of one.

## Render graph semantics

`RenderGraph` is a CPU description, independent of GPU allocation. A graph resource
has a graph-local identity, descriptor and sequence of `ResourceVersion` values.
`PassId` is also graph-local. Never mix IDs from different graphs.

### Resources and versions

`create_texture` / `create_buffer` declare uninitialized transient version zero.
`import_texture` / `import_buffer` declare external version zero, with an
`initialized` flag: setting it to true promises that valid contents already exist
before graph execution. Actual GPU objects are supplied separately in `GraphImports`.

`next_version(previous)` creates the next logical version and requires the newest
version of that resource. It neither allocates memory nor declares a writer.
**All versions of a resource share the same physical allocation.** Versions are
ordering constraints, not immutable image or buffer snapshots.

A pass declares accesses to precise versions:

| Access | Resource / required usage | Meaning |
| --- | --- | --- |
| `Sampled` | Texture / `TEXTURE_BINDING` | Read the declared version |
| `StorageRead` | Texture / `STORAGE_BINDING`, or buffer / `STORAGE` | Read |
| `StorageWrite` | Texture / `STORAGE_BINDING`, or buffer / `STORAGE` | Write |
| `ColorClear` | Color texture / `RENDER_ATTACHMENT` | Write without loading old contents |
| `ColorLoad` | Color texture / `RENDER_ATTACHMENT` | Read previous version and write declared successor |
| `DepthClear` | Depth texture / `RENDER_ATTACHMENT` | Write without loading old contents |
| `DepthLoad` | Depth texture / `RENDER_ATTACHMENT` | Read previous version and write declared successor |
| `DepthRead` | Depth texture / `RENDER_ATTACHMENT` | Read |
| `Uniform` | Buffer / `UNIFORM` | Read |
| `Vertex` | Buffer / `VERTEX` | Read |
| `Index` | Buffer / `INDEX` | Read |
| `Indirect` | Buffer / `INDIRECT` | Read |
| `CopyRead` | Texture or buffer / `COPY_SRC` | Read |
| `CopyWrite` | Texture or buffer / `COPY_DST` | Write |

A pass may declare only one access per logical resource. Tracking is whole-resource,
not per mip, layer or buffer slice. `ColorLoad` and `DepthLoad` require a successor
version and retain its predecessor's producer. `StorageWrite` does not imply reading
old contents. For a compute transform, prefer separate input/output resources;
do not assume automatic in-place read/write hazard modeling.

```rust
use prism_v2::graph::{RenderGraph, TextureDesc, PassDescriptor, AccessKind};
let mut graph = RenderGraph::new();
let first = graph.create_texture("scene", TextureDesc::color(
    64, 64, wgpu::TextureFormat::Rgba8Unorm,
));
let clear = graph.add_pass(PassDescriptor::new("clear")
    .access(first, AccessKind::ColorClear));
let composed = graph.next_version(first)?;
let draw = graph.add_pass(PassDescriptor::new("draw")
    .access(composed, AccessKind::ColorLoad));
graph.export(composed);
let compiled = graph.compile()?;
assert_eq!(compiled.order, vec![clear, draw]);
# Ok::<(), prism_v2::RenderError>(())
```

### Scheduling and culling

Compilation validates resource IDs, usages, access kinds, unique writers, initialized
reads, load predecessors and explicit dependencies. It detects cycles. These checks
apply to all declared nodes, even nodes that would later be culled.

Dependencies come from readers and their producers, plus explicit `after` or
`depends_on` edges. Ordering also prevents overwriting an older version before its
readers finish. The topological order is stable: otherwise-ready nodes use registration
order. Exported outputs and passes marked `side_effect` form the observable roots.
Only their semantic dependencies stay live; ordering hazards alone do not resurrect
dead readers or overwritten writes.

`export(version)` keeps its result observable to graph compilation. It does **not**
copy pixels to the CPU, return an allocation, present, or transfer ownership. A pass
with external effects and no exported output must explicitly declare `side_effect()`
to avoid culling. An initialized imported resource can be exported without any pass.

### Allocation and imports

`CompiledGraph::lifetimes` records each active logical resource's inclusive first
and last scheduled use. Unused entries are `None`. These ranges currently describe
usage only: preparation allocates every active transient resource, without aliasing,
pooling or recycling storage between nonoverlapping lifetimes.

`GraphImports::insert` maps **version zero** to `GraphResource::Texture { texture,
view }` or `GraphResource::Buffer`. Later versions use that same allocation. Supply
a view belonging to the supplied texture and appropriate for the declared accesses;
Prism cannot independently prove that a raw view belongs to that texture. Import
insertion replaces an existing mapping for the same key.

Active texture imports must match dimensions, size, format, mip count and samples;
usage may be a superset. Active buffer imports must be at least the declared size
and have the required usage. An active physical object cannot be imported under
two different logical resource IDs. Unused imports need not be validated.

`CompiledGraph::prepare` performs allocation and resolves imports. `GraphResources`
exposes typed lookup. `pass_resources` narrows this to `PassResources`, where only
exact versions declared by that node are accessible. This is an API correctness
boundary, not a sandbox: custom code can still retain unrelated raw handles, so the
author remains responsible for accurate access declarations.

## Custom render and compute passes

Use `FrameGraph::add_pass` to register both a pass descriptor and its implementation.
Calling only `plan.graph.add_pass` creates metadata; encoding fails if a live node
has no implementation. Implement the three `GraphPass` methods:

| Method | Responsibility |
| --- | --- |
| `declare` | Return name, accesses, explicit dependencies and side-effect status |
| `prepare` | Resolve assets, create pipelines/bind groups and validate inputs |
| `encode` | Record commands using prepared state and declared graph resources |

`PrepareContext` exposes the device, resource manager, mutable pipeline cache, frame,
report and optional fallback material. All live nodes are prepared before any node
is encoded. Upload commands precede node execution, but preparation must not assume
those uploads have executed on the GPU. Preparation is synchronous and uses native
validation error scopes; an asynchronous browser integration is not provided.

This custom copy pass demonstrates imports, transient output, export and executable
registration without depending on the default frame composition:

```no_run
use prism_v2::{FrameV2, GraphRenderer, RenderResult};
use prism_v2::frame::v2::FrameGraph;
use prism_v2::graph::{
    AccessKind, GraphResource, PassDescriptor, PassResources, ResourceVersion,
};
use prism_v2::render::{GraphPass, PrepareContext};
use prism_v2::resource::ResourceManager;

struct CopyBuffer {
    source: ResourceVersion,
    destination: ResourceVersion,
    bytes: u64,
}
impl GraphPass for CopyBuffer {
    fn declare(&self) -> PassDescriptor {
        PassDescriptor::new("copy buffer")
            .access(self.source, AccessKind::CopyRead)
            .access(self.destination, AccessKind::CopyWrite)
    }
    fn prepare(&mut self, _: &mut PrepareContext<'_>, _: &PassResources<'_>)
        -> RenderResult<()> { Ok(()) }
    fn encode(&self, encoder: &mut wgpu::CommandEncoder, resources: &PassResources<'_>)
        -> RenderResult<()> {
        encoder.copy_buffer_to_buffer(
            resources.buffer(self.source)?, 0,
            resources.buffer(self.destination)?, 0, self.bytes,
        );
        Ok(())
    }
}
fn copy_frame(
    renderer: &mut GraphRenderer,
    resources: &ResourceManager,
    source: wgpu::Buffer,
) -> RenderResult<()> {
    // The caller provides an initialized source with COPY_SRC and at least 16 bytes.
    let mut plan = FrameGraph::default();
    let input = plan.graph.import_buffer("input", 16, wgpu::BufferUsages::COPY_SRC, true);
    let output = plan.graph.create_buffer("output", 16, wgpu::BufferUsages::COPY_DST);
    plan.imports.insert(input, GraphResource::Buffer(source));
    plan.add_pass(CopyBuffer { source: input, destination: output, bytes: 16 });
    plan.graph.export(output);
    let encoded = renderer.encode_graph(resources, FrameV2::default(), plan)?;
    renderer.submit(encoded)?;
    Ok(())
}
```

The exported transient above demonstrates retention only. To read a result outside
the graph, write/copy it into an imported buffer retained by the caller and follow
wgpu's mapping rules.

For compute, prepare a `ComputePipelineKey` with a revisioned shader stage and binding
schema, resolve it through `context.pipelines.compute`, build the required bind groups,
then begin a compute pass and dispatch during `encode`. Declare input/output buffers
or textures with their storage access kinds. The existing `tests/render_gpu.rs`
contains a complete external compute pass. Prism provides scheduling and caching,
not a built-in particle simulation or dispatch policy.

The built-in `ClearPass`, `PhasePass` and `FullscreenPass` can also be inserted into
custom graphs. A phase requires a valid view index in the supplied frame, a successor
color version for `ColorLoad`, and matching attachment/material state. Depth writes
use `DepthLoad` on a successor; read-only depth uses `DepthRead`. Full-screen source
and target must be distinct logical textures.

## Pipeline caches and shader reload

The V2 `pipeline::PipelineKey` describes all supported raster pipeline identity:

- Vertex and optional fragment shader IDs, revisions, entry points, override constants
  and workgroup-memory initialization flags.
- Normalized binding schema and vertex layouts, preserving buffer slots.
- Primitive state, depth/stencil state and optional color targets including blending
  and write masks.
- Multisample state and multiview mask.

Vertex attributes are normalized by shader location. Override constants are sorted
by name and duplicate names are rejected. `ShaderStageKey::constants` stores each
`f64` as its exact `to_bits()` representation, not a numeric integer conversion.
`ShaderEntry` accepts ordinary `f64` values at the material level. Parameter bytes,
buffer handles and texture handles are not part of the pipeline key.

`ComputePipelineKey` combines a shader stage and schema. `PipelineCache::render`
and `compute` return shared pipeline objects. When using these low-level methods,
the caller must supply modules corresponding to the shader IDs and revisions in the
key, on the same device. The cache does not independently identify arbitrary modules.
The fragment module's presence must match the raster key.

`reload_shader` validates replacement WGSL before changing the stored module. Success
increments the revision; failure preserves both old code and revision. At subsequent
frame encoding, the renderer evicts pipelines referring to missing or superseded
shader revisions. The next use lazily creates the replacement pipeline. Shader reload
is explicit: there is no built-in file watcher.

`invalidate_shader` removes that shader's raster and compute entries;
`retain_shaders` supports custom retention policies. `clear()` also clears layout
caches. None resets cumulative hit/miss counters. `render_count()` reports currently
retained raster entries, not total creations. The cache fixes immediate-data size to
zero and does not expose every possible future wgpu pipeline feature.

## Errors and instrumentation

V2 methods return `RenderResult<T>`, an alias for `Result<T, RenderError>`.
`RenderError` contains public `context` and `message` strings and displays both.
Labels should identify meaningful assets or passes so diagnostics are actionable.

| Failure | Behavior |
| --- | --- |
| Invalid object geometry, material, bindings or pipeline during phase preparation | Record a diagnostic and skip that draw, or try configured fallback material |
| Fallback also fails | Record its diagnostic too and omit the draw |
| Invalid graph, import, upload, full-screen effect or pass-level operation | Abort encoding with an error |
| Shader replacement fails validation | Return error and preserve the previous shader |
| Submission validation fails | Return error; successful submission still does not imply GPU completion |

A fallback reuses the original geometry, view and object bindings with another
material. It must therefore be compatible with those inputs and attachments. The
original error remains in `skipped_objects` even if fallback succeeds; this vector
is a diagnostics list, not an exact count of objects that disappeared.

GPU creation and encoding paths capture wgpu validation errors into contextual
results. This is not a general guarantee that device loss, out-of-memory conditions
or every possible raw-wgpu misuse becomes `RenderError`; integrate the device's own
error and recovery mechanisms where needed.

### Reading `FrameReport`

| Field | Interpretation |
| --- | --- |
| `draw_calls` | Prepared built-in draws, including full-screen blits and successful fallback draws |
| `upload_bytes` | Requested frame upload bytes, including supplied row padding |
| `passes` | Number of live scheduled graph passes |
| `transient_texture_bytes` | Estimate for active transient texture allocations, including mips, layers and samples |
| `preparation_time` | CPU time for graph compilation, resource preparation and pass preparation |
| `skipped_objects` | Per-object errors and possible fallback errors |

Custom nodes must add their own draw accounting if desired. Parameter-block uniform
allocations are not counted in `upload_bytes`. Texture estimates exclude persistent
or imported textures and all buffers, and are not driver memory usage or an aliased
peak. Preparation timing excludes high-level default-graph construction, upload and
pass encoding, command-buffer finishing, submission and GPU execution. Use external
GPU profiling for actual execution timing.

Useful checks when output is wrong:

1. Inspect the returned error and `skipped_objects` before investigating shader math.
2. Check scene/output/effect formats and depth state against their actual attachments.
3. Confirm binding group numbers, WGSL layout, buffer ranges and dynamic offsets.
4. Check sort policy and layer/depth keys, especially for painter-style 2D rendering.
5. For custom graphs, verify exports, side effects, initialized imports and exact versions.
6. Verify uploads have correct usage and padding, and submit shared-resource frames in order.

## Migration from the legacy API

The implementations live in separate workspace crates: `prism-v1` and `prism-v2`.
Use `prism_v1` for historical types and `prism_v2` for the new API. The root `prism`
crate retains the original imports and exposes both APIs as `prism::v1` and `prism::v2`.

| Legacy | V2 |
| --- | --- |
| `prism_v1::Frame` | `prism_v2::FrameV2` or `prism_v2::frame::v2::Frame` |
| `prism_v1::Material` | `prism_v2::material::Material` |
| `prism_v1::PipelineKey` | `prism_v2::pipeline::PipelineKey` |
| `prism_v1::GpuResources` | `prism_v2::resource::ResourceManager` |
| `prism_v1::Renderer` | `prism_v2::GraphRenderer` for direct V2 use |

A practical migration sequence is:

1. Keep `GpuContext`; create the V2 manager and renderer with `from_context`.
2. Register shaders and explicit material templates, including every binding group.
3. Register geometry and shared buffers; move writes into frame upload requests.
4. Convert client draws into objects with explicit phases and instance ranges.
5. Move final camera/view uniforms into view binding sets.
6. Register effects once and submit only their active IDs/parameter bytes per frame.
7. Inspect reports and compare the consuming application's representative scenes.

`Renderer::graph_renderer()` exposes the V2 renderer owned by the legacy wrapper.
`Renderer::render_v2(context, resources, frame)` acquires the existing surface,
encodes/submits V2 and presents. Its `Option<FrameReport>` may be `None` when the
surface is not ready or a frame must be skipped; lost/outdated surfaces are
reconfigured. Validation failures return errors. The legacy constructor still
requires its historical setup; new integrations can use `GraphRenderer::from_context` and `GraphRenderer::render`
directly. The legacy bridge delegates to that same presentation path.

`GpuContext::from_device` and `Renderer::render_offscreen` support legacy headless
rendering. The old path retains queue-based upload behavior: submit/present its
frame before preparing the next historical frame. Do not infer V2's deferred-upload
snapshot guarantees for legacy preparation.

Historical World, VFX, HUD and post-process adapters remain. Their known conventions
(camera group 0, sprite texture/sampler group 1, HUD dynamic uniforms group 2) are
not V2 requirements. Legacy HUD/glyphon remains the available text path; V2 has no
built-in text submission API. Use a custom pass or appropriate geometry/textures
when implementing text directly in V2. The legacy HUD uniform block remains 256
bytes; V2 uniform limits come from the device.

## Validation and current limits

Run these checks from the repository root; add `--offline` when dependencies are
already cached and network access is unnecessary:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo test --test render_gpu -- --ignored --nocapture
cargo test --workspace --doc
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace --no-deps
cargo run -p prism-v2 --example v2 -- target/prism-v2-reference.png
```

CPU tests cover schema/key normalization, graph versions, ordering, cycles, usage
checks, culling and lifetimes. GPU tests are explicitly opt-in and cover rendering,
heterogeneous schemas, shader reload, resize, dynamic offsets, stable transparency,
overlay placement, empty frames, deferred uploads, external compute and legacy passes.
Documentation examples check Rust API compatibility; `no_run` examples do not prove
that WGSL or GPU behavior is correct on every adapter.

Current boundaries to consider when designing a client:

- Transient allocation happens per frame; no texture pooling, lifetime-based memory
  aliasing, upload ring buffer or bind-group cache is implemented.
- Bounds do not drive culling, and objects are not automatically merged into batches.
- Graph hazards track whole resources and do not support arbitrary simultaneous
  subresource accesses or general in-place storage read/write semantics.
- The default render path is single-target and single-sample; custom passes are needed
  for broader rendering techniques. Low-level key fields alone do not make those
  techniques work in built-in passes.
- Tone mapping, camera simulation, VFX logic, text integration, feature selection,
  surface management and device recovery belong to the application or explicit adapters.
- Shader reload is explicit and synchronous; there is no filesystem watcher or
  browser-oriented asynchronous preparation API.

Local refactor validation used software Vulkan through Mesa llvmpipe. Deterministic
reference scenes and cache counters establish useful regression coverage, but client
scene parity and target-hardware performance still need application-level validation
before removing legacy adapters.
