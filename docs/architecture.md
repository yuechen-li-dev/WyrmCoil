# WyrmCoil Architecture Overview

WyrmCoil is the top-level engine-core project.

Dunewyrm is the embedded deterministic control kernel.

Engine is the dense-store / act-bridge layer and now hosts the reintegrated WyrmCoil engine prototype under `src/Engine/`.

See also `docs/Dunewyrm/architecture.md` for the Dunewyrm runtime contract.

See `docs/sdsl-v.md` for the SDSL-V language/design contract and milestone history.

See `docs/sdsl-v-authoring.md` for the current implemented authoring/status guide (what works today vs parse/validate-only surfaces).


## Milestone status

- **M1b (complete):** Dunewyrm is fully reintegrated as an embedded kernel module under `src/Dunewyrm/` and intentionally re-exported from `src/lib.rs` for external use.
- **M2 (complete):** Engine prototype behavior is reintegrated under `src/Engine/` with product-facing names (`Engine`, `World`, `Vec2`, `EntityId`, etc.) replacing legacy `Wc*` prototype prefixes where appropriate.
- **External proof:** Guard Patrol remains available via integration tests that consume public WyrmCoil/Dunewyrm APIs.


Dunewyrm `Dw*` names remain intentionally prefixed to mark embedded-kernel APIs, while Engine-layer product names live unprefixed inside the WyrmCoil namespace.


## M3 timing contract (agentic engine boundary)

WyrmCoil is **not** a render-frame-driven engine loop.

The M3 contract is explicit:

- **Control ticks decide behavior.**
- **Simulation ticks update dense stores.**
- **Render frames observe snapshots.**

Current `Engine` prototype phase boundaries:

- `TickControl()` refreshes selection board state, ticks the Dunewyrm session, and dispatches emitted acts into world command lanes.
- `TickSimulation()` integrates dense world transforms.
- `RenderSnapshot()` returns an immutable world snapshot clone and advances only the render-frame counter.
- `Tick()` remains a convenience wrapper for one control tick followed by one simulation tick.

This pass establishes explicit timing domains and counters (`ControlTick`, `SimulationTick`, `RenderFrame`) but intentionally does **not** add a scheduler, wall-clock cadence model, renderer backend, physics backend, or ECS framework.






## M20 render pipeline layout contract (metadata-only)

M20 adds a renderer-side, plain-data pipeline-layout planning boundary over compiled shader descriptors.

- `RenderPipelineLayoutPlan` combines `CompiledPipelineDesc` with explicit vertex-buffer layout metadata plus color/depth target metadata.
- Layout planning validates common mistakes (empty names, missing buffers, duplicate locations/names, out-of-bounds offsets, and missing shader bytes) and returns structured errors.
- The contract remains deterministic and testable with fake compiled bytes; no DXC tool and no GPU are required for tests.
- This is metadata only: no `wgpu::ShaderModule`, `wgpu::PipelineLayout`, `wgpu::BindGroupLayout`, or `wgpu::RenderPipeline` creation in M20.
- `RenderSnapshot` remains runtime world-observation data, while `RenderPipelineLayoutPlan` is future GPU pipeline metadata.

Still out of scope in M20:

- no shader reflection or shader-driven input-layout extraction
- no material/bind-group system rollout
- no draw submission path


## M21 `wgpu` resource creation probe (descriptor-plan boundary)

M21 adds the first `wgpu`-resource-facing conversion seam from validated M20 layout metadata:

- `RenderPipelineLayoutPlan` now converts into `WgpuRenderPipelineDescriptorPlan` via `BuildWgpuRenderPipelineDescriptorPlan(...)`.
- Vertex attribute formats, vertex step modes, color targets, and depth formats are mapped into `wgpu` equivalents through deterministic mapping helpers.
- The descriptor plan owns converted vertex buffer / attribute data and is testable without creating any `wgpu::Instance`, adapter, device, surface, or window.

M21 remains intentionally narrow:

- no draw pass or render submission path
- no surface/window integration changes
- no `wgpu::ShaderModule` creation helper yet
- no `wgpu::RenderPipeline` creation yet
- no bind-group/material/reflection system rollout

This milestone is a resource descriptor scaffold only, preserving GPU-free testability in normal `cargo test` runs.




## M23 GPU buffer upload scaffold boundary

M23 adds the next deterministic CPU-to-GPU boundary after M22 extraction:

- `BuildVertexBufferUploadPlan(label, &ExtractedRenderBatch)` produces `VertexBufferUploadPlan` plain data (`Label`, packed `Bytes`, `VertexCount`, `StrideBytes`, usage intent).
- Plan construction reuses M22 packing/layout helpers (`PackSpriteVertices`, `SpriteVertexStrideBytes`, `SpriteVertexBufferLayout`) and validates key contract errors (empty labels, byte-length mismatches, stride mismatches).
- Empty batches are intentionally allowed so empty frames still produce valid zero-byte upload plans.
- The boundary remains GPU-free and deterministic: normal tests require no `wgpu::Device`, surface/window, or DXC.

M23 remains intentionally narrow:

- no real `wgpu::Buffer` creation path required yet
- no queue upload/write path
- no command encoder or render pass
- no draw-call submission

Future milestones can consume the upload plan boundary for real buffer creation/submission without changing extraction semantics.

## M22 CPU render extraction boundary

M22 adds the next renderer-side CPU boundary after M5 snapshots and M20/M21 layout planning:

- `ExtractSpriteVertices(&RenderSnapshot)` converts immutable render snapshot items into deterministic `SpriteVertex` values (`X`, `Y`, `SpriteId`) in snapshot item order.
- `PackSpriteVertices(...)` produces explicit little-endian bytes (`f32 x`, `f32 y`, `u32 sprite id`) with a 12-byte stride for future upload.
- `SpriteVertexBufferLayout()` publishes matching layout metadata (`Float32x2` at location 0 offset 0, `Uint32` at location 1 offset 8, step mode `Vertex`, stride 12).
- The pass is CPU only: no `wgpu::Buffer`, no device/surface requirements, no draw submission.

Current scope remains intentionally minimal:

- one vertex per render item (no quad expansion yet)
- no index-buffer generation yet
- no UV/atlas/material/texture system yet
- no camera/projection transform path yet
- no GPU upload or render-pass submission yet

## M9 minimal `wgpu` renderer backend scaffold

M9 adds the first renderer-backend boundary while preserving the existing timing contract.

- `Engine::render::wgpu` introduces a minimal `RenderBackend` plus `RendererConfig`, `ClearColor`, and `RenderStats`.
- The backend consumes immutable `RenderSnapshot` data and reports deterministic observed frame/item stats.
- The backend can build a minimal clear-pass operation descriptor (`wgpu::Operations<wgpu::Color>`) as a bounded foundation for future surface/device submission work.
- Renderer consumption does not mutate `World` and does not advance `ControlTick` or `SimulationTick`.
- Render remains observer-only: control/simulation cadence stays engine-owned and explicit.

Intentional non-goals remain explicit:

- No shader language or SDSL-V pipeline in M9.
- No material/texture asset pipeline.
- No render graph rollout.
- No renderer-owned app loop or simulation clock ownership.

## M7 real `winit` window/input shell

M7 keeps the existing engine-owned timing contract while adding the first real platform event source via `winit`.

- `Engine::backend::winit` maps selected `winit` keyboard keys into `PlatformInput` (`Right/D`, `Left/A`, `Space`, `Q`, `E`).
- The backend then reuses `TranslatePlatformInput(...)` to produce normalized `InputEvent` values.
- Platform events are only allowed to enqueue normalized input (`EnqueueInput` path).
- Queueing from the `winit` boundary does not advance `ControlTick`, `SimulationTick`, or `RenderFrame`.
- Queueing from the `winit` boundary does not mutate world stores directly.
- `TickControl()` remains the sole mailbox bridge boundary.

Intentional non-goals remain explicit:

- No renderer backend and no `wgpu` integration.
- No shader language/compiler pipeline.
- No physics backend or ECS framework.
- No render-frame-owned simulation cadence.
## M6 window/input backend scaffold

M6 adds the first platform-facing backend boundary without adding a renderer or full app loop.

- `Engine::backend` now hosts a small window/input scaffold for future platform adapters.
- Platform-style input events are translated into normalized engine `InputEvent` values (`MoveRightPressed`, `MoveLeftPressed`, `StopPressed`, `AlertGuardPressed`, `NudgeGuardPressed`).
- Translation is deterministic and unknown platform inputs are ignored.
- Backend helpers can enqueue translated input into `Engine` without mutating `World` directly.
- Enqueueing input does not tick control or simulation clocks.
- `TickControl()` remains the only boundary that bridges queued input into Dunewyrm mailbox messages.

Intentional non-goals remain explicit:

- No `wgpu` or renderer integration.
- No shader language/compiler pipeline.
- No real windowed game loop requirement in tests.

## M5 render snapshot / extraction contract

M5 strengthens the render boundary while still keeping rendering backends out of scope.

- `World` now contains an explicit dense `Renderables` lane (`SpriteIds`, `Visible`) aligned to entity indices.
- `RenderSnapshot()` now returns plain snapshot data (`Frame`, `Items`) instead of exposing mutable world stores.
- Snapshot extraction includes only alive + visible entities, reads transform positions, and emits deterministic entity-index order.
- `RenderSnapshot()` advances only `RenderFrame`; it does not advance `ControlTick` or `SimulationTick`.
- Snapshot extraction is observation-only and does not mutate world data.

Timing slogan remains load-bearing:

**Control ticks decide behavior. Simulation ticks update dense stores. Render frames observe snapshots.**

Intentional non-goals remain unchanged for this milestone:

- No `wgpu` integration.
- No shader language/compiler pipeline.
- No windowing/event-loop backend.
- No asset/texture pipeline.


## M4 input boundary contract (mailbox bridge)

M4 adds an engine-owned normalized input boundary without adding any platform backend dependency.

- External adapters enqueue normalized `InputEvent` values into the engine queue.
- `TickControl()` drains that queue at the beginning of the control phase and converts each input event into a Dunewyrm `DwMessage` mailbox message.
- Dunewyrm frame logic consumes mailbox messages during control ticks and emits acts.
- Input does not mutate world stores directly.
- `TickSimulation()` and `RenderSnapshot()` do not process queued input.

Current intentional scope:

- No platform/window backend integration (`winit`, GameInput, Steam Input, etc. are still out of scope).
- No async event loop.
