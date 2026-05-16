# Golden Path Checkpoint (M42/M43)

This document captures the current visible-window golden path status after:

- **M42:** first visible primitive in a real window.
- **M43:** architecture/status documentation checkpoint.

## Status

The current golden path is complete for its intended bootstrap scope:

- Open a real window.
- Run explicit engine control/simulation/render-snapshot phases.
- Extract snapshot render data.
- Upload vertex data.
- Record draw commands.
- Present visible pixels.

This is an **optional manual path**, not a default test path.

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
- **Simulation ticks update dense stores.**
- **Render frames observe snapshots.**

The example loop is a bootstrap integration shell. It does not redefine engine timing as render-frame-driven simulation.

## Temporary/demo-only pieces

The following are intentional scope limits, not omissions by mistake:

- WGSL is the current visible-pixel bootstrap path.
- SDSL-V remains the preferred high-level shader authoring language.
- `BuildVisiblePrimitiveDemoBatch(...)` is a demo bridge.
- No material system.
- No texture system.
- No camera/projection system.
- No asset pipeline.
- No render graph.
- No full application framework.

## Architecture checkpoint summary

Current stack shape:

Dunewyrm kernel
→ engine timing/input/world/render snapshot
→ renderer extraction/upload/buffering/lifecycle seams
→ shader-source strategy (SDSL-V preferred, WGSL valid native path)
→ `wgpu` bootstrap backend adapter
→ visible window primitive

`wgpu` is the current bootstrap backend. A native Vulkan backend remains a future seam and is not implemented.

Backend-neutral render contracts should remain independent of `wgpu` policy details.

## Current limitations (explicit)

- No material bindings.
- No textures.
- No uniform-buffer scene path.
- No camera/projection path.
- No production sprite batching.
- No production renderer lifecycle framework.
- No native Vulkan backend implementation.
- Resize/presentation policy remains basic bootstrap behavior.

## Plausible next-phase options

- Replace demo quad bridge with a real sprite/quad lane.
- Add camera/projection transform path.
- Integrate SDSL-V output flow into `wgpu` module/pipeline creation.
- Seed texture/material binding path.
- Investigate native Vulkan backend adapter path.
- Clean up the window loop around current `winit` deprecation warnings.
- Add optional GPU readback/headless validation where helpful.
