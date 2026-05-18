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

## WorldBlackboard seed (M78)

M78 adds a typed world-level resource layer in `Engine::world`.

### Boundary: `DwBoard` vs `WorldBlackboard`

| Layer | Purpose | Typical data |
|---|---|---|
| Dunewyrm `DwBoard` | control memory for decisions | small typed control facts, TTL/dirty state, decision inputs |
| `Engine::WorldBlackboard` | world-owned subsystem resources | geometry registries, ray query request/result stores, future camera/input resources |

This split is intentional:

- Geometry registries and ray query stores live in world resources, not in `DwBoard`.
- `WorldBlackboard` is not an ECS and not a scene graph.
- `WorldBlackboard` is a typed resource owner for simulation/world subsystems.

### M78 seed contents

`WorldBlackboard` currently owns:

- `Geometry: WorldGeometryRegistry`
- `RayRequests: RayQueryRequestStore`
- `RayResults: RayQueryStore`

`WorldGeometryRegistry` is a deterministic triangle registry keyed by triangle id with explicit duplicate-id rejection.

### Forward seam

M78 prepares the M79 bridge where world geometry can be translated into a ray-query scene. Camera/input resources remain future work.

## WorldBlackboard picking resources (M80)

M80 extends `WorldBlackboard` with minimal picking resources:

- `Camera: Option<WorldCameraResource>`
- `Input: WorldInputResource`

These resources are world-owned picking inputs only. This is not a full camera system, not window-loop input integration, and not renderer camera ownership.

`PickWorldBlackboard(...)` composes:

`WorldBlackboard.Camera + WorldBlackboard.Input + WorldBlackboard.Geometry -> WorldPickResult`

M81 adds an actuatorized world-pick request path:

- Request intent is stored as `RayQueryRequest::WorldPick(WorldPickRayQueryRequest { QueryId })` in `WorldBlackboard.RayRequests`.
- `ExecuteWorldPickRequestById(...)` consumes request-by-id, executes `PickWorldBlackboard(...)`, and stores the result in `WorldBlackboard.RayResults`.
- Hit/miss/failure are stored by query id in `RayQueryOutcome::{Hit, Miss, WorldPickFailure}`.
- Completion remains mailbox-scalar only: `DwMessage::I32(completion_kind, query_id)`.

Scope boundaries remain unchanged: no editor UI, no window event-loop integration, no scene graph/ECS, and no GPU ray tracing.

`WorldBlackboard::Clear()` currently resets all blackboard resources, including camera/input, as a seed-level full reset helper.

## Actuator subsystem pattern (M82)

M82 documents the reusable actuator-subsystem architecture in `docs/actuator-subsystems.md`.

Use this pattern when Dunewyrm control needs domain capability execution without moving rich payloads into acts/mailbox:

- act carries id-only intent,
- request/result payloads live in world-owned stores,
- completion mailbox remains id-only and staged for next-tick visibility.

Margaret world picking (M81) is the canonical worked example of this pattern.
