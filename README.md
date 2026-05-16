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
  world.rs         demo world, stores, frames, acts, keys, mail, input, registry
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

## Run tests

```bash
cargo test
```
