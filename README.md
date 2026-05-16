# WyrmCoil

WyrmCoil is a deterministic Rust engine-core prototype with an embedded Dunewyrm control kernel.

**Current status:** M42 first visible primitive in window complete (opt-in `examples/window_loop_skeleton.rs` plus `examples/window_visible_primitive.rs` owns a minimal `winit` event loop, creates window/surface/device/queue, reuses M40 surface planning+configuration seam, routes keyboard input through normalized M7 backend helper into `Engine.EnqueueInput(...)`, runs explicit `TickControl()` / `TickSimulation()` / `RenderSnapshot()` phases on redraw, and performs clear-only acquire/present while keeping default tests GPU/window-free), with M40 `winit` + `wgpu` surface configuration seam complete (CPU-testable surface configuration planning/selection under the `wgpu` backend plus optional ignored/env-gated real window+surface configure probe, with GPU/window-free default tests and no draw/present loop), with M39 optional headless WGSL draw smoke probe complete (ignored/env-gated real-device offscreen WGSL draw assembly+submit probe through existing extraction/upload/command/assembly seams with no window/surface/swapchain and GPU-free default tests), with M38 WGSL pipeline creation probe complete (CPU-testable WGSL pipeline planning plus optional caller-supplied-device `wgpu::RenderPipeline` creation from WGSL via existing M30 pipeline seam, with GPU-free default tests and no draw/window/surface/swapchain), with M37 WGSL shader-module path for the `wgpu` backend complete (CPU-testable WGSL module planning/validation plus optional caller-supplied-device `wgpu::ShaderModule` creation seam, with GPU-free default tests), with M36 render backend boundary cleanup complete (`wgpu` clarified as the first backend adapter behind backend-neutral render contracts and a documented future Vulkan seam, with behavior/test preservation), with M35 shader source strategy policy complete (Dunewyrm-utility-backed SDSL-V/WGSL source-mode selector with structured feasibility/rejection reasons and CPU-only tests; policy-only, no shader compilation/module creation), with M34 optional headless draw submission probe complete (optional caller-supplied-device+queue offscreen submit seam that reuses existing draw recording/path resources and submits exactly one command buffer without window/surface/swapchain, with GPU-free default tests), with M33 headless draw assembly contract complete (plain-data compatibility validation and metadata assembly across command/pipeline/upload/target with no required GPU tests and no draw submission), with M32 optional headless/offscreen render-target probe complete (GPU-free headless target descriptor validation seam plus optional caller-supplied-device `wgpu` texture/view creation helper for offscreen targets, no default GPU test requirement, and no window/surface/swapchain integration), with M31 optional `wgpu` render-pass / draw command probe complete (GPU-free draw-input validation seam plus optional caller-owned-encoder `wgpu` render-pass/draw recording helper requiring caller-supplied pipeline/vertex-buffer/target-view and no default GPU test requirement), with M30 optional `wgpu` render-pipeline creation probe complete (GPU-free descriptor-plan + validation seam plus optional caller-supplied-shader-module `wgpu::RenderPipeline` creation helper with no default GPU test requirement), with M29 render command planning / draw intent complete (plain-data `RenderCommandPlan` from pipeline layout + upload metadata + upload execution result with `ReadyToDraw` / `NoOpEmptyBatch` / `Rejected` status and deterministic no-GPU tests), with M28 lifecycle-act upload executor / utility policy bridge complete (Dunewyrm lifecycle upload intent + utility-backed execution planning choosing CPU record-only by default and optional caller-owned `wgpu::Device` GPU creation), with M27 optional `wgpu` vertex buffer creation probe complete (validated `VertexBufferUploadPlan` -> GPU-free `WgpuVertexBufferCreateDesc` plus optional caller-owned `wgpu::Device` buffer creation helper), with M26 buffer lifecycle save/restore replay proof complete (CPU-only Dunewyrm-backed lifecycle chunk export/import equivalence), with prior M23 GPU buffer upload scaffold complete (`ExtractedRenderBatch` -> deterministic `VertexBufferUploadPlan` plain data with label/bytes/count/stride and no required real `wgpu::Buffer` creation), with prior M22 CPU render extraction complete (`RenderSnapshot` -> deterministic packed sprite vertex data with no GPU upload), M21 `wgpu` resource creation probe complete (GPU-free metadata-to-`wgpu` descriptor planning boundary), M20 render pipeline layout contract, M19 compiled shader descriptor scaffold, M9 minimal `wgpu` renderer backend scaffold, M7 real `winit` input shell, M6 backend scaffold, M5 render snapshots, M4 mailbox input bridge, and M3 timing boundaries preserved.

**Architecture slogan:** Control ticks decide. Simulation ticks update stores. Render frames observe snapshots. Acts connect control to world. Chunks persist both.

## Module layout

- `src/lib.rs`: top-level crate identity and crate exports for Dunewyrm + Engine boundary.
- `src/Dunewyrm/`: reintegrated Dunewyrm deterministic kernel modules (IDs, phases, registry, session, board, traces, chunks, acts).
- `src/Engine/`: reintegrated WyrmCoil Engine layer (dense stores, query/selection, act bridge, engine tick/chunk behavior).
- `src/Engine/`: includes normalized engine input events and queueing, bridged into Dunewyrm mailbox on the control-tick boundary.
- `src/Engine/`: includes render snapshot extraction (`RenderSnapshot`) so render frames observe immutable plain-data snapshots rather than mutating world state.
- `src/Engine/render/extract.rs`: CPU-side sprite vertex extraction (`ExtractSpriteVertices`) and deterministic little-endian byte packing (`PackSpriteVertices`) for the M22 upload boundary scaffold.
- `docs/architecture.md`: architecture boundary and status document.
- `docs/sdsl-v.md`: SDSL-V language/design reference and milestone contract history.
- `docs/sdsl-v-authoring.md`: practical current-status authoring guide (what parses/validates/emits/runs today).
- `primer/`: repository-authoritative coding and Rust-shape rules.

## Run tests

```bash
cargo test
```

## Current non-goals

- No production renderer-attached platform runtime yet. M7 adds a minimal `winit` keyboard/window shell boundary that normalizes platform key events into engine `InputEvent`s and queues them, but it does not define engine timing or mutate world state directly.
- No full renderer yet: M9 adds a minimal `wgpu` backend scaffold that consumes `RenderSnapshot` and can prepare clear-pass operations, but does not add shaders, materials, render graph, or an app-loop-owned clock.
- No physics backend yet.
- No shader language/compiler pipeline implementation yet (see `docs/sdsl-v.md` for the M0 design contract).
- No ECS/archetype/query framework rollout.
- No production engine claims.
- No native Vulkan backend implementation yet: M36 formalizes only the backend seam and keeps `wgpu` as the bootstrap backend path.


## M43 golden path checkpoint (architecture/status pass)

M43 is a documentation checkpoint that records what M42 proved and what it did **not** prove.

**Golden path status:** complete for the current bootstrap slice.

The current optional/manual golden path is:

1. Open a real `winit` window.
2. Run explicit engine phases (`TickControl()`, `TickSimulation()`, `RenderSnapshot()`).
3. Extract render data from the immutable snapshot.
4. Plan and perform GPU vertex upload.
5. Record a `wgpu` render pass and draw command.
6. Present a visible primitive to the window surface.

Run command:

```bash
cargo run --example window_visible_primitive
```

Environment caveats (manual run):

- Requires a working windowing environment (not headless CI by default).
- Requires a GPU/driver stack that supports the selected `wgpu` backend.
- Uses the current WGSL bootstrap shader path (no DXC required for this example path).

What this example exercises end-to-end:

- `winit` event loop + window lifecycle.
- M40 surface capability/config planning seams (`BuildWgpuSurfaceCapabilitiesInfo`, `BuildWgpuSurfaceConfigPlan`, `BuildWgpuSurfaceConfiguration`).
- Normalized keyboard input routing (`QueueWinitPhysicalKey(...)` -> `Engine.EnqueueInput(...)`).
- Explicit timing boundaries in redraw flow (`TickControl()`, `TickSimulation()`, `RenderSnapshot()`).
- Snapshot extraction bridge (`BuildVisiblePrimitiveDemoBatch(...)`).
- Upload planning + execution (`BuildVertexBufferUploadPlan`, `ExecuteVertexBufferUploadPlan`).
- GPU vertex buffer creation (`CreateWgpuVertexBuffer`).
- WGSL shader-module/pipeline path (`BuildWgslShaderModulePlan`, `CreateWgpuShaderModuleFromWgslPlan`, `BuildWgpuRenderPipelinePlan`, `CreateWgpuRenderPipeline`).
- Draw command planning + recording (`BuildRenderCommandPlan`, `RecordWgpuDrawCommand`).
- Surface acquire/present.

Timing model reminder (unchanged):

- Control ticks decide behavior.
- Simulation ticks update dense stores.
- Render frames observe snapshots.

The window loop is a prototype integration shell; it does **not** redefine WyrmCoil into a render-frame-driven simulation model.

Explicitly temporary / narrow in this checkpoint:

- WGSL is the bootstrap visible-pixel path for M42/M43.
- SDSL-V remains the preferred high-level shader authoring language.
- `BuildVisiblePrimitiveDemoBatch(...)` is a demo bridge, not a full sprite/material renderer.
- No material system, texture system, camera/projection system, asset pipeline, or render graph.
- Not a full application framework or production renderer lifecycle.

Default `cargo test` remains GPU/window-free; this golden-path run is opt-in/manual.

Manual M42 example run:

```bash
cargo run --example window_visible_primitive
```


## Style tooling (`wyrmfmt`)

M44 adds a project-local style checker scaffold focused on Rust function/method definition casing for owned WyrmCoil code.

Run:

```bash
cargo run --bin wyrmfmt -- check --lang rust src tests examples
```

Current scope:

- Checks Rust `fn` definitions for PascalCase names.
- Skips `#[test]` functions to avoid test-name churn.
- Skips trait-impl methods (`impl Trait for Type`) where Rust trait contracts require names like `default` / `fmt`.
- Scans only `.rs` files and skips `target/`, `.git/`, and common third-party/vendor directories.
- Reports violations and exits nonzero when any are found.

Out of scope in M44:

- No auto-rewrite/fix mode yet.
- No rustfmt replacement.
- No SDSL-V/Oct formatter modes yet.
