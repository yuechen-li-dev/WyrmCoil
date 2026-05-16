# WyrmCoil

WyrmCoil is a deterministic Rust engine-core prototype with an embedded Dunewyrm control kernel.

**Current status:** M22 CPU render extraction complete (`RenderSnapshot` -> deterministic packed sprite vertex data with no GPU upload), with prior M21 `wgpu` resource creation probe complete (GPU-free metadata-to-`wgpu` descriptor planning boundary), M20 render pipeline layout contract, M19 compiled shader descriptor scaffold, M9 minimal `wgpu` renderer backend scaffold, M7 real `winit` input shell, M6 backend scaffold, M5 render snapshots, M4 mailbox input bridge, and M3 timing boundaries preserved.

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
