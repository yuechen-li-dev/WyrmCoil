# WyrmCoil Architecture Overview

WyrmCoil is the top-level engine-core project.

Dunewyrm is the embedded deterministic control kernel.

Engine is the dense-store / act-bridge layer and now hosts the reintegrated WyrmCoil engine prototype under `src/Engine/`.

See also `docs/Dunewyrm/architecture.md` for the Dunewyrm runtime contract.


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
