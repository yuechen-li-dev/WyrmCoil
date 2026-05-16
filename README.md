# WyrmCoil

WyrmCoil is a deterministic Rust engine-core prototype with an embedded Dunewyrm control kernel.

**Current status:** M6 window/input backend scaffold complete (platform-style input translation into normalized engine `InputEvent` queue), with M5 render snapshots, M4 mailbox input bridge, and M3 timing boundaries preserved.

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

- No production platform backend yet (`winit`, GameInput, Steam Input, and full window adapters remain future work). M6 adds only a lightweight backend scaffold and platform-input translation helpers.
- No renderer backend yet (`wgpu`/shader pipelines/window loops are intentionally future work; render currently observes snapshots only).
- No physics backend yet.
- No shader language/compiler pipeline yet.
- No ECS/archetype/query framework rollout.
- No production engine claims.
