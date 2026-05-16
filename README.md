# WyrmCoil

WyrmCoil is a deterministic Rust engine-core prototype with an embedded Dunewyrm control kernel.

**Current status:** M5 render snapshot / extraction contract complete (dense renderable lane + deterministic immutable render snapshots), with M4 input-boundary mailbox bridge and M3 timing boundaries preserved.

**Architecture slogan:** Control ticks decide. Simulation ticks update stores. Render frames observe snapshots. Acts connect control to world. Chunks persist both.

## Module layout

- `src/lib.rs`: top-level crate identity and crate exports for Dunewyrm + Engine boundary.
- `src/Dunewyrm/`: reintegrated Dunewyrm deterministic kernel modules (IDs, phases, registry, session, board, traces, chunks, acts).
- `src/Engine/`: reintegrated WyrmCoil Engine layer (dense stores, query/selection, act bridge, engine tick/chunk behavior).
- `src/Engine/`: includes normalized engine input events and queueing, bridged into Dunewyrm mailbox on the control-tick boundary.
- `src/Engine/`: includes render snapshot extraction (`RenderSnapshot`) so render frames observe immutable plain-data snapshots rather than mutating world state.
- `docs/architecture.md`: architecture boundary and status document.
- `primer/`: repository-authoritative coding and Rust-shape rules.

## Run tests

```bash
cargo test
```

## Current non-goals

- No platform input backend yet (`winit`, GameInput, Steam Input, window adapters are intentionally out of scope).
- No renderer backend yet (`wgpu`/shader pipelines/window loops are intentionally future work; render currently observes snapshots only).
- No physics backend yet.
- No shader language/compiler pipeline yet.
- No ECS/archetype/query framework rollout.
- No production engine claims.
