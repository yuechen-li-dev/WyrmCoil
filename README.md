# WyrmCoil

WyrmCoil is a deterministic Rust engine core with an embedded Dunewyrm control kernel.

## Architecture slogan

Control ticks decide. Simulation ticks update stores. Render frames observe snapshots. Acts connect control to world. Chunks persist both.

## Current module map

```text
src/Engine/
  wyrmcoil.rs      generic engine core: World trait, Engine<W, I>, clock, tick phases, chunks
  primitives.rs    Vec2, EntityId, RenderItem, RenderSnapshot
  store.rs         reusable dense-store helper patterns
  render/          render contracts, buffering, upload, backend seams
  shader/          shader source strategy, SDSL-V/WGSL tooling seams

src/Demo/
  world.rs                  demo world, stores, frames, acts, keys, mail, input, registry
  persistent_controller.rs  persistent root controller authoring sample (KeepRootFrame + Steady + typed mail + TTL)
```

## Public API import shape

Use crate-level Engine/Demo exports:

```rust
use wyrmcoil::Engine::{Engine, World, Vec2, EntityId, RenderItem, RenderSnapshot};
use wyrmcoil::Demo::{DemoWorld, BuildRegistry};
```

Notes:

- `Engine` contains the generic orchestration core and reusable primitives.
- `Demo` contains sample/game-specific world code proving the generic engine contract.
- APIs are still evolving; treat current exports as active bootstrap surface, not frozen long-term ABI.

## Custom world quickstart

Implement `wyrmcoil::Engine::World` for your own world data. The engine owns orchestration/timing; your world owns domain state.

```rust
use wyrmcoil::{DwActRequest, DwBoard};
use wyrmcoil::Engine::{RenderSnapshot, World};

#[derive(Clone)]
struct MyWorld {
    Health: i32,
}

#[derive(Clone)]
struct MyWorldChunk {
    Health: i32,
}

impl World for MyWorld {
    type Chunk = MyWorldChunk;

    fn RefreshBoard(&self, board: &mut DwBoard) {
        let _ = board;
    }

    fn DispatchActs(&mut self, board: &DwBoard, acts: &[DwActRequest]) {
        let _ = (board, acts);
    }

    fn Tick(&mut self) {
        self.Health -= 1;
    }

    fn ExtractRenderSnapshot(&self, frame: u64) -> RenderSnapshot {
        let _ = self;
        RenderSnapshot {
            Frame: frame,
            Items: Vec::new(),
        }
    }

    fn ExportChunk(&self) -> Self::Chunk {
        MyWorldChunk { Health: self.Health }
    }

    fn FromChunk(chunk: Self::Chunk) -> Self {
        MyWorld { Health: chunk.Health }
    }
}
```

## Golden path checkpoint

Manual visible-window bootstrap path:

```bash
cargo run --example window_visible_primitive
```

This path uses Engine core contracts plus a demo-visible-primitive bridge (`BuildVisiblePrimitiveDemoBatch(...)`) from `Demo` world data into bootstrap draw inputs. It is intentionally narrow and is not a full sprite/material renderer.

Default `cargo test` remains GPU/window-free.

## Store helper position

`Engine::store` provides reusable dense-store helper patterns. It is **not** an ECS framework, does **not** own demo stores, and exists to keep future dense-store implementations consistent.

## Style tooling (`wyrmfmt`)

```bash
cargo run --bin wyrmfmt -- check --lang rust src tests examples
```

Rust's `non_snake_case` lint is intentionally disabled workspace-wide; `wyrmfmt` is the naming-policy authority for WyrmCoil.

## Run tests

```bash
cargo test
```

## Native material TOML status

Native `.toml` material assets now include parse + structural validation (M91) and an initial semantic validation seed (M92) for core node kinds (`constant_f32`, `constant_float4`, `texture2d`, `multiply`, `add`, `lerp`, `standard_surface`). Semantic validation is still pre-codegen: no SDSL-V generation, no MaterialX import implementation, and no runtime material binding integration yet.


## Native material resource metadata seed (M94)

M94 adds deterministic material resource-requirement extraction after M92 semantic validation:

`material.toml -> validated graph semantics -> MaterialResourceRequirements`

Current M94 output is plain metadata only:

- output-reachable `texture2d` node requirements (node id + deterministic sanitized name),
- texture asset path,
- texture color space (`srgb` default when omitted, or `linear`),
- default sampler requirement (`SamplerPlan::DefaultColor`),
- deterministic future binding names (`tex_<sanitized>` and `samp_<sanitized>`).

No runtime material object ownership, no texture loading, no `wgpu` bind-group creation, and no real texture sampling codegen are added in M94.

## Persistent controller sample (M52)

`Demo::persistent_controller` is a copyable authoring pattern for persistent root-controller flows:

- root session uses `DwRootPolicy::KeepRootFrame`
- idle root returns `Dw::Steady()`
- typed mailbox alert (`DwMessage::I32`) is consumed with `ConsumeFirstKind(...)`
- board state is written with TTL helpers and deterministic expiry
- root pushes a tiny child frame, child emits begin/complete acts, then root resumes steady


## Raw HLSL compatibility path (M68)

SDSL-V remains WyrmCoil's reference shader authoring language. WGSL remains a native backend source path. Raw HLSL is also supported as a compatibility/escape-hatch source path for legacy or direct-DXC workflows.

SDSL-V is HLSL-targeting, not HLSL-compatible, and it is not an HLSL superset. Raw HLSL wrappers require explicit entry metadata (name, stage, target profile). WyrmCoil validates this wrapper metadata only; HLSL parse/semantic diagnostics are owned by DXC.

Example:

```rust
let artifact = BuildHlslShaderArtifact(
    "legacy_flat.hlsl",
    hlsl_source,
    vec![
        HlslEntryPoint::Vertex("VSMain"),
        HlslEntryPoint::Pixel("PSMain"),
    ],
)?;
```


## Margaret subsystem status (M71b)

Margaret is now included as a Cargo workspace subsystem under `src/Margaret/`:

- `margaret-core`
- `margaret-image`
- `margaret-testutil`
- `margaret-cpu`
- `margaret-cli`
- `margaret-vk` (scaffold)

Boundary reminder:

- `Engine::render` remains the raster/window/backend path.
- Margaret is the ray/reference/query subsystem seed.
- Camera/ray-query feature bridges are not integrated yet.
- GPU ray tracing remains deferred; `margaret-vk` is currently scaffold-level.


## Actuator subsystem pattern status (M82)

WyrmCoil now documents the reusable actuator-subsystem pattern at `docs/actuator-subsystems.md`, with Margaret world picking (M81) as the canonical worked example.

Boundary summary:

- Dunewyrm remains the control brain.
- Acts/mailbox messages remain small and often id-only.
- Rich request/result payloads live in world-owned stores/resources.
- Mailbox completion is staged and consumed on the next tick.


## Asset subsystem status (M83)

M83 adds an Engine asset actuator seed for raw file-byte loading only.

- Requests/results live in `WorldBlackboard.Assets` stores.
- Actuator execution is policy-shaped via utility planning (`ImmediateBytesLoad` implemented, deferred unsupported).
- Completion mailbox remains id-only (`AssetRequestId` via `DwMessage::I32`).
- No texture decode, material integration, GPU upload, hot reload, or async jobs yet.


## Asset subsystem status (M84)

M84 adds image decode as the next asset actuator stage after byte loading.

- Requests/results still live in `WorldBlackboard.Assets` stores and execute through utility planning.
- M84 supports deterministic **P6 PPM** decode only (`bytes -> width/height + RGBA8 CPU payload`).
- Completion mailbox remains id-only (`DwMessage::I32` with `AssetRequestId`).
- No GPU texture upload, texture resource, material integration, async jobs, hot reload, or asset database/importer yet.


## Asset subsystem status (M85)

M85 adds the CPU-image-to-texture upload boundary as a deterministic plain-data scaffold.

- Input is `DecodedImageAsset` (`Width`, `Height`, `Rgba8`) from M84 decode results.
- Output is `TextureUploadPlan` with validated label/source/dimensions/byte length and `SampledColor` usage intent.
- Current format choice is `Rgba8UnormSrgb` for decoded color textures.
- Optional helper accepts `AssetResult` and rejects non-decoded variants with structured error.
- No GPU texture creation, sampler policy, bind groups, material integration, async jobs, or hot reload in M85.


## Asset texture resource seam status (M86)

M86 adds an optional `wgpu` texture resource seam after the M85 upload plan boundary.

- `TextureUploadPlan` remains backend-neutral plain data.
- `BuildWgpuTextureUploadDesc(...)` maps plan metadata to deterministic `wgpu` texture/upload descriptor data.
- `CreateWgpuTextureResource(...)` optionally creates a `wgpu::Texture` + default `TextureView` using caller-provided `Device`/`Queue`.
- Default tests remain GPU-free; no sampler, bind group, material system, or render-loop textured draw integration is added in M86.


## Asset sampler plan seam status (M87)

M87 adds a backend-neutral sampler intent boundary after M85/M86 texture storage/upload seams.

- `SamplerPlan` captures filtering/address behavior only (mag/min/mipmap filter + U/V/W address modes).
- `TextureUploadPlan` remains pixel storage/upload metadata; sampler intent is intentionally separate.
- Optional `wgpu` helpers map sampler intent to descriptor data and can create caller-owned `wgpu::Sampler` resources.
- No bind-group/material integration or textured draw-loop integration is added in M87.
- No CPU/reference sampling implementation is added in M87, but Margaret can later consume the same sampler plan intent.
- Color-space conversion remains texture-format-owned (`TexturePixelFormat`, e.g. sRGB); samplers do not perform color conversion.


## Asset texture+sampler binding layout seam status (M88)

M88 adds a backend-neutral texture+sampler binding layout planning boundary after M85/M86/M87:

- `TextureUploadPlan` remains pixel-storage/upload metadata.
- `SamplerPlan` remains filtering/address intent.
- `TextureSamplerBindingLayoutPlan` now owns shader binding slots (`TextureBinding`, `SamplerBinding`) and stage visibility.
- `DefaultSampledColor2D(label)` provides a convenience default of texture binding `0`, sampler binding `1`, and `Pixel` visibility.
- Optional `wgpu` mapping builds a deterministic two-entry bind-group-layout descriptor plan:
  - texture entry: D2, filterable float, non-multisampled;
  - sampler entry: filtering sampler.
- No shader reflection is used; bindings are explicit author-chosen metadata.
- No actual bind-group creation/material ownership/textured draw integration is added in M88.


## Asset texture+sampler bind group seam status (M89)

M89 adds an optional backend-specific `wgpu` bind-group resource seam for one sampled 2D texture + one filtering sampler:

- `TextureUploadPlan` remains backend-neutral upload metadata.
- `SamplerPlan` remains backend-neutral sampling intent metadata.
- `TextureSamplerBindingLayoutPlan` remains explicit backend-neutral binding-slot metadata.
- `BuildWgpuTextureSamplerBindGroupDesc...` provides deterministic GPU-free bind-group descriptor metadata validation.
- `CreateWgpuTextureSamplerBindGroup(...)` optionally creates a caller-owned `wgpu::BindGroup`.
- No material system is added in M89.
- No textured draw integration is added in M89.
- No shader reflection is added in M89.

## Native material TOML schema seed (M90)

M90 defines the first native material asset schema as a flat TOML graph in `docs/material-toml.md`.

- Native editable material source of truth is ordinary `.toml` with `[asset] type = "material"`.
- MaterialX is a compatibility import/export path, not native internal source of truth.
- SDSL-V remains the planned generated/reference shader target for material lowering.
- M90 is docs/schema only: no parser, no material compiler/runtime, and no draw integration changes.

Current staged texture path:

`Decoded image -> TextureUploadPlan -> WgpuTextureResource + SamplerPlan -> WgpuSamplerResource + TextureSamplerBindingLayoutPlan -> WgpuTextureSamplerBindGroupLayoutResource -> WgpuTextureSamplerBindGroupResource`

## Material TOML parser/validator status (M91)

WyrmCoil now includes a native material TOML parser/validator seed in `Engine::material`.

Scope in M91:

- parse `.toml` material assets into typed graph structures,
- validate headers, node identity, edge references, output references, and cycle freedom,
- preserve literal params/editor metadata for future compiler/runtime work.

Out of scope in M91: MaterialX import, SDSL-V codegen, and runtime material binding integration.


## Native material TOML codegen seed status (M93)

M93 adds deterministic native material graph -> SDSL-V source generation after M92 semantic validation.

- Generated output emits a `MaterialSurface` record and `GeneratedMaterial::EvaluateMaterial()` function.
- Supported lowering remains the M92 subset (`constant_f32`, `constant_float4`, `texture2d`, `multiply`, `add`, `lerp`, `standard_surface`).
- `texture2d` currently lowers to deterministic placeholder white sample helper stubs (no real texture binding/sampling yet).
- Output is deterministic (stable topological order, stable identifier sanitization, no timestamps).
- No material runtime object ownership, bind-group integration, textured draw integration, or MaterialX import implementation is added in M93.
