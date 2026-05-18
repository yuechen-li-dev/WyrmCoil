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
