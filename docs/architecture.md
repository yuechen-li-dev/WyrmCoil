# WyrmCoil Architecture Overview

WyrmCoil is the top-level engine-core project.

Dunewyrm is the embedded deterministic control kernel.

See also:

- `docs/Dunewyrm/architecture.md` for Dunewyrm runtime contract history.
- `docs/sdsl-v.md` for SDSL-V language/design contract and milestone history.
- `docs/sdsl-v-authoring.md` for current authoring status.
- `docs/golden-path.md` for visible-window bootstrap checkpoint details.

## Current product shape (M45b+)

```text
Engine = generic core + primitives + store helpers + render/shader/backend contracts
Demo   = sample/demo world + stores + frames + acts + input + registry
```

### Module map

```text
src/Engine/
  wyrmcoil.rs      World trait + Engine<W, I> orchestration + clock/tick/chunk boundaries
  primitives.rs    shared primitives (Vec2, EntityId, RenderItem, RenderSnapshot)
  store.rs         reusable dense-store helper patterns
  render/          extraction/upload/command/pipeline/backend seams
  shader/          source strategy seams (SDSL-V preferred, WGSL bootstrap path)

src/Demo/
  world.rs         demo world implementation and demo registry/input scaffolding
```

## Public API surface (current)

Typical imports:

```rust
use wyrmcoil::Engine::{Engine, World, Vec2, EntityId, RenderItem, RenderSnapshot};
use wyrmcoil::Demo::{DemoWorld, BuildRegistry};
```

Notes:

- `Engine` is generic orchestration and reusable contracts.
- `Demo` is sample/game-specific proof code.
- Do not treat `Demo` as engine infrastructure.
- Public API is intentionally usable now but still evolving.

## World contract boundary

The generic `World` trait lives in `Engine::wyrmcoil` and is re-exported from `Engine`.

- `Engine` owns timing/orchestration (`TickControl`, `TickSimulation`, `RenderSnapshot`).
- `World` implementations own domain data and act handling.
- Chunk export/import persists both runtime and world state.

## Store policy

`Engine::store` is a reusable dense-store helper layer:

- It provides helper patterns and utilities for dense lanes.
- It does not implement a full ECS.
- It does not own demo/game stores.
- It exists to keep future dense-store usage consistent.

## Golden-path relation

The current golden path (`window_visible_primitive`) demonstrates Engine + Demo integration:

- Engine timing and render contracts are generic.
- Demo world provides sample data.
- `BuildVisiblePrimitiveDemoBatch(...)` remains a demo visible-primitive bridge, not a full renderer.

## Style policy tooling

`wyrmfmt` checks project naming policy:

```bash
cargo run --bin wyrmfmt -- check --lang rust src tests examples
```


## Dunewyrm mailbox (M48)

Dunewyrm mailbox remains a deterministic two-queue model:

- `Visible` messages are readable/consumable this tick.
- `Staged` messages are enqueued during this tick and only promote on the next `BeginTick`.
- FIFO order is preserved across visible consume and staged promotion.
- Runtime chunks persist both queues.

Messages now carry a bounded typed payload enum (`None`, `Bool`, `I32`, `F32`, `PairI32`) while preserving `Kind: u32` routing. Mailbox helpers support filtered cursor operations on the visible queue (`HasKind`, `PeekFirstKind`, `ConsumeFirstKind`, `ConsumeAllKind`).

Non-goals remain unchanged: no async event bus, no dynamic/object payload transport, and no arbitrary reflection-driven message channel.


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

## WorldBlackboard seed (M78)

M78 adds a typed world-level resource layer in `Engine::world`.

### Boundary: `DwBoard` vs `WorldBlackboard`

| Layer | Purpose | Typical data |
|---|---|---|
| Dunewyrm `DwBoard` | control memory for decisions | small typed control facts, TTL/dirty state, decision inputs |
| `Engine::WorldBlackboard` | world-owned subsystem resources | geometry registries, ray query request/result stores, future camera/input resources |

This split is intentional:

- Geometry registries and ray query stores live in world resources, not in `DwBoard`.
- `WorldBlackboard` is not an ECS and not a scene graph.
- `WorldBlackboard` is a typed resource owner for simulation/world subsystems.

### M78 seed contents

`WorldBlackboard` currently owns:

- `Geometry: WorldGeometryRegistry`
- `RayRequests: RayQueryRequestStore`
- `RayResults: RayQueryStore`

`WorldGeometryRegistry` is a deterministic triangle registry keyed by triangle id with explicit duplicate-id rejection.

### Forward seam

M78 prepares the M79 bridge where world geometry can be translated into a ray-query scene. Camera/input resources remain future work.

## WorldBlackboard picking resources (M80)

M80 extends `WorldBlackboard` with minimal picking resources:

- `Camera: Option<WorldCameraResource>`
- `Input: WorldInputResource`

These resources are world-owned picking inputs only. This is not a full camera system, not window-loop input integration, and not renderer camera ownership.

`PickWorldBlackboard(...)` composes:

`WorldBlackboard.Camera + WorldBlackboard.Input + WorldBlackboard.Geometry -> WorldPickResult`

M81 adds an actuatorized world-pick request path:

- Request intent is stored as `RayQueryRequest::WorldPick(WorldPickRayQueryRequest { QueryId })` in `WorldBlackboard.RayRequests`.
- `ExecuteWorldPickRequestById(...)` consumes request-by-id, executes `PickWorldBlackboard(...)`, and stores the result in `WorldBlackboard.RayResults`.
- Hit/miss/failure are stored by query id in `RayQueryOutcome::{Hit, Miss, WorldPickFailure}`.
- Completion remains mailbox-scalar only: `DwMessage::I32(completion_kind, query_id)`.

Scope boundaries remain unchanged: no editor UI, no window event-loop integration, no scene graph/ECS, and no GPU ray tracing.

`WorldBlackboard::Clear()` currently resets all blackboard resources, including camera/input, as a seed-level full reset helper.

## Actuator subsystem pattern (M82)

M82 documents the reusable actuator-subsystem architecture in `docs/actuator-subsystems.md`.

Use this pattern when Dunewyrm control needs domain capability execution without moving rich payloads into acts/mailbox:

- act carries id-only intent,
- request/result payloads live in world-owned stores,
- completion mailbox remains id-only and staged for next-tick visibility.

Margaret world picking (M81) is the canonical worked example of this pattern.


## Asset actuator subsystem seed (M83)

M83 introduces asset byte-load as the second actuator-subsystem example. `WorldBlackboard` now includes grouped `Assets` resources with deterministic request/result stores. Asset execution is utility-planned (`ImmediateBytesLoad` vs `NoAssetExecutionFeasible`), but only immediate synchronous `std::fs::read` is implemented in M83. Mailbox completion remains request-id-only.


## Asset image decode seed (M84)

M84 extends the asset actuator subsystem with an image decode stage. Decode requests own source bytes and run through the same utility-planned immediate execution path as byte loading. The result store now includes CPU-side decoded image payloads (`Width`, `Height`, `Rgba8`) and structured decode failures.

Current decode scope is intentionally narrow and deterministic: **P6 PPM (max value 255)** only. Mailbox completion remains request-id-only (`DwMessage::I32`).

Non-goals remain unchanged in M84: no GPU upload, no texture resources, no material system integration, no async/deferred asset jobs, and no hot reload/import database path.


## Asset texture upload plan seed (M85)

M85 adds a deterministic CPU-to-upload boundary for images after M84 decode:

`DecodedImageAsset -> TextureUploadPlan`

`TextureUploadPlan` is plain data only (label, source name, dimensions, pixel format, bytes, usage intent) and validates width/height/byte-length consistency with overflow-safe checks.

Current M85 scope uses `TexturePixelFormat::Rgba8UnormSrgb` with `TextureUsageIntent::SampledColor` for decoded color textures.

Non-goals in M85 remain unchanged: no GPU texture resource creation, no sampler/bind-group/material path, and no async/deferred upload jobs.


## Asset `wgpu` texture resource seam (M86)

M86 extends the staged asset texture path with an optional backend-specific helper:

`TextureUploadPlan -> BuildWgpuTextureUploadDesc(...) -> CreateWgpuTextureResource(...)`

Boundary rules remain explicit:

- `TextureUploadPlan` stays backend-neutral and plain data.
- `wgpu` types are isolated in backend module seams.
- Real texture creation/upload requires caller-provided `wgpu::Device` and `wgpu::Queue`.
- Default test coverage for descriptor mapping remains GPU-free.
- No sampler/bind group/material integration yet.


## Asset sampler plan seam (M87)

M87 adds a backend-neutral sampler boundary:

`SamplerPlan -> BuildWgpuSamplerDesc(...) -> CreateWgpuSamplerResource(...)`

Boundary rules:

- `SamplerPlan` is filtering/address intent only.
- `TextureUploadPlan` remains storage/upload data only.
- Samplers do not own color conversion; storage/color interpretation (for example sRGB) stays with texture format metadata.
- Mipmap filter is present as intent metadata, but mipmap generation remains future work.
- `wgpu` sampler creation is optional and caller-owned.
- No bind groups, material system, or textured draw-loop integration are included in M87.
- Margaret/reference sampling may later consume the same sampler intent without changing this boundary.


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
