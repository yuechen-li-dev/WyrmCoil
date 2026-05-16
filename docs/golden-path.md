# Golden Path Checkpoint (M42/M43, post M45b split)

This document captures the current visible-window bootstrap path after the Engine/Demo split.

## Status

The current golden path is complete for its intended bootstrap scope:

- Open a real window.
- Run explicit engine control/simulation/render-snapshot phases.
- Extract snapshot render data.
- Upload vertex data.
- Record draw commands.
- Present visible pixels.

This is an **optional manual path**, not a default test path.

## Split-aware ownership

- `Engine` owns generic timing/orchestration contracts and render/shader/backend seams.
- `Demo` owns sample world/game-specific data and demo bootstrap flow.
- The golden-path example combines both: generic Engine core + Demo world/bootstrap bridge.

## Run command

```bash
cargo run --example window_visible_primitive
```

Notes:

- Requires a windowing environment and GPU/driver support.
- Depends on available `wgpu` backend support on the host.
- Uses WGSL bootstrap shaders for this path; DXC is not required.

## Seams exercised by `window_visible_primitive`

The example exercises these integration seams in order:

1. `winit` window/event loop ownership.
2. Surface capability + config seam from M40:
   - `BuildWgpuSurfaceCapabilitiesInfo(...)`
   - `BuildWgpuSurfaceConfigPlan(...)`
   - `BuildWgpuSurfaceConfiguration(...)`
3. Normalized input routing:
   - `QueueWinitPhysicalKey(...)`
   - `Engine.EnqueueInput(...)`
4. Explicit phase boundaries:
   - `TickControl()`
   - `TickSimulation()`
   - `RenderSnapshot()`
5. Snapshot extraction bridge:
   - `BuildVisiblePrimitiveDemoBatch(...)`
6. Upload planning/execution:
   - `BuildVertexBufferUploadPlan(...)`
   - `ExecuteVertexBufferUploadPlan(...)`
   - `CreateWgpuVertexBuffer(...)`
7. WGSL shader/pipeline bootstrap:
   - `BuildWgslShaderModulePlan(...)`
   - `CreateWgpuShaderModuleFromWgslPlan(...)`
   - `BuildWgpuRenderPipelinePlan(...)`
   - `CreateWgpuRenderPipeline(...)`
8. Draw planning/recording/present:
   - `BuildRenderCommandPlan(...)`
   - `RecordWgpuDrawCommand(...)`
   - surface acquire + `present()`

## Timing model reminder

WyrmCoil timing semantics remain:

- **Control ticks decide behavior.**
- **Simulation ticks update stores.**
- **Render frames observe snapshots.**

The example loop is a bootstrap integration shell. It does not redefine engine timing as render-frame-driven simulation.

## Temporary/demo-only pieces

The following are intentional scope limits, not omissions by mistake:

- WGSL is the current visible-pixel bootstrap path.
- SDSL-V remains the preferred high-level shader authoring language.
- `BuildVisiblePrimitiveDemoBatch(...)` remains a demo visible-primitive bridge, not a full sprite/material renderer.
- No material system.
- No texture system.
- No camera/projection system.
- No asset pipeline.
- No render graph.
- No full application framework.

## Architecture checkpoint summary

Current stack shape:

Dunewyrm kernel
→ Engine generic timing/input/world/render snapshot contracts
→ Engine render extraction/upload/command/backends seams
→ Engine shader-source strategy seams (SDSL-V preferred, WGSL bootstrap path)
→ `wgpu` bootstrap backend adapter
→ Demo visible-primitive bridge (`BuildVisiblePrimitiveDemoBatch`)
→ visible window primitive

`wgpu` is the current bootstrap backend. A native Vulkan backend remains a future seam and is not implemented.
