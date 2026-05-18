# WyrmCoil

WyrmCoil is a deterministic Rust engine core with an embedded Dunewyrm control kernel.

## Architecture slogan

Control ticks decide. Simulation ticks update stores. Render frames observe snapshots. Acts connect control to world. Chunks persist both.

## Current module map

```text
src/Engine/
  wyrmcoil.rs      generic engine core: World trait, Engine<W, I>, clock, tick phases, chunks
  primitives.rs    Vec2, EntityId, RenderItem, RenderSnapshot
  store.rs         reusable dense-store helper patterns
  render/          render contracts, buffering, upload, backend seams
  shader/          shader source strategy, SDSL-V/WGSL tooling seams

src/Demo/
  world.rs                  demo world, stores, frames, acts, keys, mail, input, registry
  persistent_controller.rs  persistent root controller authoring sample (KeepRootFrame + Steady + typed mail + TTL)
```

## Public API import shape

Use crate-level Engine/Demo exports:

```rust
use wyrmcoil::Engine::{Engine, World, Vec2, EntityId, RenderItem, RenderSnapshot};
use wyrmcoil::Demo::{DemoWorld, BuildRegistry};
```

Notes:

- `Engine` contains the generic orchestration core and reusable primitives.
- `Demo` contains sample/game-specific world code proving the generic engine contract.
- APIs are still evolving; treat current exports as active bootstrap surface, not frozen long-term ABI.

## Custom world quickstart

Implement `wyrmcoil::Engine::World` for your own world data. The engine owns orchestration/timing; your world owns domain state.

```rust
use wyrmcoil::{DwActRequest, DwBoard};
use wyrmcoil::Engine::{RenderSnapshot, World};

#[derive(Clone)]
struct MyWorld {
    Health: i32,
}

#[derive(Clone)]
struct MyWorldChunk {
    Health: i32,
}

impl World for MyWorld {
    type Chunk = MyWorldChunk;

    fn RefreshBoard(&self, board: &mut DwBoard) {
        let _ = board;
    }

    fn DispatchActs(&mut self, board: &DwBoard, acts: &[DwActRequest]) {
        let _ = (board, acts);
    }

    fn Tick(&mut self) {
        self.Health -= 1;
    }

    fn ExtractRenderSnapshot(&self, frame: u64) -> RenderSnapshot {
        let _ = self;
        RenderSnapshot {
            Frame: frame,
            Items: Vec::new(),
        }
    }

    fn ExportChunk(&self) -> Self::Chunk {
        MyWorldChunk { Health: self.Health }
    }

    fn FromChunk(chunk: Self::Chunk) -> Self {
        MyWorld { Health: chunk.Health }
    }
}
```

## Golden path checkpoint

Manual visible-window bootstrap path:

```bash
cargo run --example window_visible_primitive
```

This path uses Engine core contracts plus a demo-visible-primitive bridge (`BuildVisiblePrimitiveDemoBatch(...)`) from `Demo` world data into bootstrap draw inputs. It is intentionally narrow and is not a full sprite/material renderer.

Default `cargo test` remains GPU/window-free.

## Store helper position

`Engine::store` provides reusable dense-store helper patterns. It is **not** an ECS framework, does **not** own demo stores, and exists to keep future dense-store implementations consistent.

## Style tooling (`wyrmfmt`)

```bash
cargo run --bin wyrmfmt -- check --lang rust src tests examples
```

Rust's `non_snake_case` lint is intentionally disabled workspace-wide; `wyrmfmt` is the naming-policy authority for WyrmCoil.

## Run tests

```bash
cargo test
```

## Persistent controller sample (M52)

`Demo::persistent_controller` is a copyable authoring pattern for persistent root-controller flows:

- root session uses `DwRootPolicy::KeepRootFrame`
- idle root returns `Dw::Steady()`
- typed mailbox alert (`DwMessage::I32`) is consumed with `ConsumeFirstKind(...)`
- board state is written with TTL helpers and deterministic expiry
- root pushes a tiny child frame, child emits begin/complete acts, then root resumes steady


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


## Margaret subsystem status (M71b)

Margaret is now included as a Cargo workspace subsystem under `src/Margaret/`:

- `margaret-core`
- `margaret-image`
- `margaret-testutil`
- `margaret-cpu`
- `margaret-cli`
- `margaret-vk` (scaffold)

Boundary reminder:

- `Engine::render` remains the raster/window/backend path.
- Margaret is the ray/reference/query subsystem seed.
- Camera/ray-query feature bridges are not integrated yet.
- GPU ray tracing remains deferred; `margaret-vk` is currently scaffold-level.


## Actuator subsystem pattern status (M82)

WyrmCoil now documents the reusable actuator-subsystem pattern at `docs/actuator-subsystems.md`, with Margaret world picking (M81) as the canonical worked example.

Boundary summary:

- Dunewyrm remains the control brain.
- Acts/mailbox messages remain small and often id-only.
- Rich request/result payloads live in world-owned stores/resources.
- Mailbox completion is staged and consumed on the next tick.
