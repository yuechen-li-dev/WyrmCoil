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









## M39 optional headless WGSL draw smoke probe

M39 adds an optional real-device headless WGSL smoke probe while preserving GPU-free default tests:

- An ignored integration test (`tests/headless_wgsl_draw_smoke.rs`) assembles the offscreen chain end-to-end when explicitly enabled (`WYRMCOIL_RUN_WGPU_TESTS=1`).
- The probe requests adapter/device/queue without a surface, builds a headless target descriptor/resource, constructs a WGSL module/pipeline plan from `MINIMAL_SPRITE_WGSL_FIXTURE`, creates a real `wgpu::RenderPipeline`, extracts one sprite snapshot, builds upload/command/assembly plans, and submits one headless draw command buffer.
- No window/surface/swapchain is created, and no readback is required in M39.
- WGSL is used here as the native `wgpu` shader validation path for smoke coverage; SDSL-V remains the preferred high-level authoring path.

M39 intentionally remains narrow:

- no render loop ownership changes
- no material/bind-group/reflection/render-graph rollout
- no hard GPU requirement in normal `cargo test` runs

## M36 render backend boundary (`wgpu` adapter clarification)

M36 formalizes the render backend seam without adding new rendering behavior:

- WyrmCoil render core contracts remain backend-neutral (snapshot extraction, upload planning/execution policy metadata, command planning, and headless assembly planning).
- `wgpu` is explicitly positioned as the first concrete backend adapter path, not the render architecture itself.
- Backend-facing adapter exports are grouped under `Engine::render::backend::wgpu`, while compatibility re-exports are preserved from `Engine::render` to avoid unnecessary call-site churn.
- A docs-only `Engine::render::backend::vulkan` placeholder marks the future native Vulkan backend seam with no implementation in M36.

WyrmCoil uses wgpu to bootstrap the golden path, but wgpu is a backend adapter, not the engine architecture.

M36 intentionally remains narrow:

- no Vulkan implementation or dependencies
- no window/surface/swapchain work
- no material/bind-group/reflection/render-graph expansion
- no draw-path feature expansion

## M35 shader source strategy policy (implemented)

M35 adds an explicit shader source policy-selection seam without creating shader modules or invoking compilation:

- `SelectShaderSourceStrategy(...)` chooses between `SdslV`, `Wgsl`, and `NoShaderSourceFeasible` using Dunewyrm utility selection (`SelectHighestUtilityTarget`).
- `ShaderSourceStrategyConstraints` controls allow/require behavior plus `PreferWgsl` override while default behavior still prefers SDSL-V when both inputs are feasible.
- `ShaderSourceStrategyDecision` returns structured selection reason, rejected-mode reasons, and explicit feasibility flags for both source paths.
- Availability checks are intentionally minimal (source present and non-whitespace), preserving M35 as policy/metadata only.

M35 intentionally remains narrow:

- no SDSL-V artifact compilation in selector execution
- no DXC invocation
- no WGSL parsing/validation/compilation
- no `wgpu::ShaderModule` creation
- no render pass, pipeline integration, or draw submission changes

Expected next-step seams remain explicit:

- `SdslV` selection continues toward artifact/build pipeline stages
- `Wgsl` selection targets a future direct `wgpu` shader-module path

## M34 optional headless draw submission probe

M34 adds the first optional real-device headless/offscreen draw submission seam while preserving GPU-free default tests:

- `SubmitHeadlessDraw(...)` accepts caller-supplied `wgpu::Device`, `wgpu::Queue`, `WgpuRenderPipelineResource`, `WgpuVertexBufferResource`, `WgpuHeadlessRenderTarget`, and `RenderCommandPlan`.
- The helper creates one command encoder, reuses M31 `RecordWgpuDrawCommand(...)`, finishes one command buffer, and submits exactly one buffer via `queue.submit(...)`.
- `ValidateHeadlessDrawSubmissionOptions(...)` preserves CPU-only validation for option-label policy (`EmptyLabel`) and keeps normal test coverage GPU-free.
- The seam remains offscreen-only: no window/surface/swapchain integration, no presentation, no readback, no material/bind-group/reflection/render-graph expansion.

Future work remains explicit:

- optional ignored/env-gated real-device probe wiring when broader repo policy chooses to exercise runtime GPU creation paths
- optional offscreen readback/verification after submission seam stability
- eventual window/surface path integration from the same snapshot/upload/command seams

## M33 headless draw assembly contract

M33 adds a plain-data compatibility assembly seam for future offscreen draw submission without requiring a GPU in default tests:

- `BuildHeadlessDrawAssemblyPlan(...)` validates `RenderCommandPlan` + `RenderPipelineLayoutPlan` + `VertexBufferUploadPlan` + `HeadlessRenderTargetDesc` metadata compatibility.
- `HeadlessDrawAssemblyPlan` records deterministic draw-ready metadata (names, target dimensions, vertex count, stride, and color format) without owning any `wgpu` handles.
- Structured `HeadlessDrawAssemblyError` variants report readiness failures, command/upload metadata mismatches, target descriptor validation failures, color-format mismatch, and pipeline-identity mismatch.

M33 intentionally remains narrow:

- no command encoder or render pass recording
- no queue submission
- no window/surface/swapchain integration
- no material/bind-group/reflection system

Future work remains explicit:

- optional real-device headless draw probe
- optional offscreen readback
- future surface/swapchain integration
- future material/bind-group rollout

## M32 optional headless/offscreen render-target probe

M32 adds a narrow optional offscreen render-target seam for headless GPU probes while preserving GPU-free default tests:

- `BuildHeadlessRenderTargetDesc(...)` and `ValidateHeadlessRenderTargetDesc(...)` provide deterministic CPU-only validation for label/dimensions/format metadata.
- `CreateWgpuHeadlessRenderTarget(...)` optionally creates a caller-owned-device `wgpu::Texture` + default `wgpu::TextureView` wrapper suitable for future draw probes.
- Target usage is intentionally minimal (`RENDER_ATTACHMENT` + `COPY_SRC`) to support future offscreen clear/draw/readback experiments without introducing readback logic in M32.
- No `wgpu::Instance`, adapter, device, window, surface, or swapchain is created by default tests.

M32 intentionally remains narrow:

- no window/surface/swapchain integration
- no full renderer loop or draw submission expansion
- no material/bind-group/reflection system
- no render graph rollout
- no readback implementation yet

The M31 draw seam can consume the returned texture view in future real-device probes when callers supply compatible pipeline/vertex resources and command-encoder ownership.

## M31 optional `wgpu` render pass / draw-command probe

M31 adds a narrow optional draw-command recording seam while preserving GPU-free default tests:

- `ValidateWgpuDrawInputs(...)` validates `RenderCommandPlan` draw intent against plain pipeline/vertex metadata (status, vertex count, stride, pipeline name, and `u32` draw-range fit) with deterministic structured errors.
- `RecordWgpuDrawCommand(...)` records a minimal color-only render pass and `draw(0..vertex_count, 0..1)` into a caller-supplied `wgpu::CommandEncoder` when the caller also supplies compatible `WgpuRenderPipelineResource`, `WgpuVertexBufferResource`, and `wgpu::TextureView`.
- The helper does not create devices, windows, surfaces, or swapchains, and does not submit command buffers; queue/submission ownership remains caller-controlled.

M31 intentionally remains narrow:

- no window/surface/swapchain integration
- no bind-group/material/reflection system
- no render graph rollout
- no index-buffer, depth-pass, or multi-pass submission policy

`RenderCommandPlan` remains the draw-intent source from M29, and normal `cargo test` stays GPU-free by testing validation/planning behavior without constructing real `wgpu` runtime objects.

## M30 optional `wgpu` render-pipeline creation probe

M30 adds a narrow optional seam for real `wgpu::RenderPipeline` creation while keeping normal tests GPU-free:

- `BuildWgpuRenderPipelineDescriptorPlan(...)` remains the deterministic metadata conversion boundary.
- `ValidateWgpuShaderBytesForPipeline(...)` validates staged shader bytes/entry points without creating a device.
- `CreateWgpuRenderPipelineFromModules(...)` creates `wgpu::PipelineLayout` (empty bind-group list in M30) and a real `wgpu::RenderPipeline` when the caller provides:
  - a real `wgpu::Device`
  - `WgpuRenderPipelineDescriptorPlan` metadata
  - the source `RenderPipelineLayoutPlan`
  - caller-owned compatible vertex/pixel `wgpu::ShaderModule` handles

M30 intentionally avoids forcing SPIR-V shader-module creation policy into this pass. WyrmCoil currently carries DXC/SPIR-V bytes in metadata, but shader-module construction remains caller-owned so default `cargo test` runs stay GPU-free.

M30 remains intentionally narrow:

- no window/surface integration
- no command encoder/render pass/draw submission
- no material/bind-group/reflection rollout




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

## M27 optional `wgpu` vertex buffer creation probe

M27 adds a narrow optional resource-creation seam on top of the M23 upload plan:

- `BuildWgpuVertexBufferCreateDesc(&VertexBufferUploadPlan)` converts validated upload plans into a GPU create descriptor (`Label`, `Bytes`, `Usage`, `VertexCount`, `StrideBytes`) using deterministic CPU-only logic.
- `CreateWgpuVertexBuffer(&wgpu::Device, &VertexBufferUploadPlan)` optionally creates an initialized `wgpu::Buffer` when a caller provides a real device.
- Empty upload plans remain valid at the M23 planning boundary, but M27 rejects empty bytes for real GPU vertex-buffer creation (`EmptyUpload`).
- Default tests remain GPU-free and validate descriptor conversion + structured errors without requiring adapter/device/window/surface/DXC.

M27 remains intentionally narrow:

- no command encoder or render pass
- no draw submission path
- no render-pipeline creation
- no lifecycle-act execution wiring to GPU resources yet

Lifecycle simulation (M25/M26) remains recorded intent and deterministic telemetry/trace behavior; binding those acts to real GPU resource execution is future work.



## M29 render command planning / draw intent (plain-data only)

M29 adds an explicit draw-intent planning seam after pipeline layout and upload execution metadata:

- `BuildRenderCommandPlan(...)` combines `RenderPipelineLayoutPlan`, `VertexBufferUploadPlan`, and optional `UploadExecutionResult` into deterministic plain-data `RenderCommandPlan`.
- Plan status is explicit: `ReadyToDraw`, `NoOpEmptyBatch`, or `Rejected`, with structured reasons (missing execution metadata, stride mismatch, rejected upload, etc.).
- `UploadExecutionMode::CpuRecordOnly` can still yield `ReadyToDraw` draw intent metadata (planning only) and is intentionally distinct from real GPU draw submission.
- `UploadExecutionMode::NoOpEmptyUpload` maps to `NoOpEmptyBatch` (valid no-op).
- This milestone does not create a command encoder, render pass, draw call, or `wgpu::RenderPipeline`.

Future work remains explicit:

- real GPU resource execution/submission wiring
- command encoder creation
- render pass construction
- draw-call submission

## M28 lifecycle act upload executor / utility policy bridge

M28 connects M25/M26 lifecycle intent to M23/M27 upload execution policy without adding draw submission:

- `PlanUploadExecution(...)` consumes lifecycle acts + `VertexBufferUploadPlan` metadata + device-availability and constraints, then uses Dunewyrm utility selection to choose `GpuBufferCreate`, `CpuRecordOnly`, `NoOpEmptyUpload`, or `Rejected`.
- Device presence only makes GPU upload eligible; it does not force hidden renderer-owned execution.
- `ExecuteUploadPlan(...)` keeps normal tests GPU-free by supporting CPU record-only output (`CpuUploadRecord`) and only creating a `wgpu::Buffer` when the caller explicitly supplies a device and the selected mode is GPU.
- Missing upload-intent lifecycle acts reject execution (`MissingLifecycleUploadAct`) so lifecycle acts remain explicit recorded authorization.

M28 remains intentionally narrow:

- no command encoder / render pass / draw submission
- no material system or shader reflection
- no automatic global lifecycle-to-GPU wiring


## M24 render buffering strategy planner (CPU policy only)

M24 adds a deterministic CPU-side buffering strategy planner over `VertexBufferUploadPlan` metadata:

- `PlanVertexBuffering(&VertexBufferUploadPlan, BufferingConstraints)` selects a mode using Dunewyrm utility-style score selection after hard feasibility/safety gates.
- Supported modes are `FixedDoubleDefault`, `PullLagPressure`, `SerialJitSurvival`, and `NoBufferingModeFeasible`.
- WyrmCoil follows the Prometheus three-mode policy: fixed-double by default, pull-lag only under guarded pressure constraints, and serial JIT as a strict one-slot survival fallback.
- Planner output is plain data: selected mode, transition/fallback/hard-failure reason enums, rejected modes, feasibility flags, memory requirements, and deterministic telemetry counters.
- Empty uploads are treated as a no-op success (default mode, zero committed memory, zero active slots).

M24 remains intentionally narrow:

- no real `wgpu::Buffer` creation
- no upload queue submission path
- no command encoder or draw pass integration
- no runtime slot lifecycle manager yet

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


## M25 Dunewyrm buffer slot lifecycle simulator (CPU-only)

M25 adds a Dunewyrm-backed CPU lifecycle simulation pass on top of M24 buffering mode decisions:

- `SimulateBufferSlotLifecycle(&BufferingPlanDecision)` models slot lifecycle intent and invariants for `FixedDoubleDefault`, `PullLagPressure`, `SerialJitSurvival`, and `NoBufferingModeFeasible`.
- Lifecycle evidence is emitted as deterministic Dunewyrm-domain acts (`BeginStage`, `MarkReady`, `ConsumeReady`, `RetireSlot`, cleanup, and rejection).
- Lifecycle telemetry is recorded from Dunewyrm board-backed counters (active slots, WIP depth, committed memory, pressure counters, and cleanup counters).
- The pass is intentionally CPU-only: no `wgpu::Buffer`, no queue uploads, and no draw submission.

This continues the Prometheus strategy chain: fixed-double default, guarded pull-lag under pressure, and serial JIT survival fallback.


## M26 buffer lifecycle save/restore replay proof (CPU-only)

M26 adds deterministic save/restore for the M25 lifecycle simulator:

- `BufferSlotLifecycleSession` now supports lifecycle ticks, `ExportChunk`, and `FromChunk` restore.
- Lifecycle chunks include the persisted `BufferingPlanDecision`, Dunewyrm `DwRuntimeChunk`, lifecycle telemetry, lifecycle acts, status, and pull-lag signal metadata.
- Replay-equivalence tests prove uninterrupted lifecycle execution matches split-run export/restore execution for `FixedDoubleDefault`, `PullLagPressure`, and `SerialJitSurvival`, with hard-failure restore behavior also covered.
- The restore path continues the persisted decision; no planner re-run is performed on restore.

M26 remains intentionally narrow:

- no GPU buffer allocation or persistence
- no upload queue writes
- no command encoder or draw pass integration
- no adaptive re-planning during restore



## M38 WGSL pipeline creation probe (`wgpu` backend)

M38 connects the M37 WGSL shader-module seam to M30 render-pipeline creation under the `wgpu` backend boundary:

- `BuildWgslPipelinePlan(...)` provides CPU-only planning/validation for WGSL pipeline metadata (label/source-name/vertex-entry/pixel-entry), defaulting to `vs_main` and `fs_main`.
- `CreateWgpuRenderPipelineFromWgsl(...)` optionally creates one WGSL `wgpu::ShaderModule` and then a real `wgpu::RenderPipeline` (single module used for both vertex+fragment stages) when the caller supplies a real `wgpu::Device`.
- The helper reuses existing validated render layout/descriptor seams (`RenderPipelineLayoutPlan` + M30 pipeline creation helper) and remains backend-specific under `Engine::render::backend::wgpu`.

M38 preserves policy and scope constraints:

- WGSL remains a valid native shader source path.
- SDSL-V remains the preferred high-level authoring path.
- Normal `cargo test` remains GPU-free.
- No draw submission, window/surface/swapchain, material/reflection/bind-group, or render-graph rollout is added in M38.

## M37 WGSL shader-module path (`wgpu` backend)

M37 adds a backend-specific WGSL shader-module seam under `Engine::render::backend::wgpu`:

- `BuildWgslShaderModulePlan(...)` validates label/source-name/source text into plain-data `WgslShaderModulePlan`.
- `BuildWgslPlanFromStrategyRequest(...)` optionally bridges M35 source-strategy WGSL selection into module planning without coupling shader strategy to `wgpu` runtime handles.
- `CreateWgpuShaderModuleFromWgsl(...)` optionally creates a real `wgpu::ShaderModule` only when a caller supplies a real `wgpu::Device`.

M37 preserves prior policy and scope boundaries:

- SDSL-V remains the preferred high-level authoring path from M35 policy defaults.
- WGSL remains a valid native source path for backend module creation.
- default `cargo test` remains GPU/window/surface/swapchain free.
- no render-pipeline expansion, no draw submission expansion, and no DXC invocation are added in M37.

Future work remains explicit: consume WGSL module plans in later pipeline/draw golden-path integration milestones.
