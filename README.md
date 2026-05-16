# WyrmCoil

WyrmCoil is a deterministic Rust engine-core prototype with an embedded Dunewyrm control kernel.

**Current status:** M1b full Dunewyrm kernel reintegration (module exports + restored behavior tests + Guard Patrol external API proof).

**Architecture slogan:** Frames decide. Stores iterate. Acts connect. Mailbox reports back. Chunks persist both.

## Module layout

- `src/lib.rs`: top-level crate identity and crate exports for Dunewyrm + Engine boundary.
- `src/Dunewyrm/`: reintegrated Dunewyrm deterministic kernel modules (IDs, phases, registry, session, board, traces, chunks, acts).
- `src/Engine/`: WyrmCoil engine-layer placeholder module for later milestones.
- `docs/architecture.md`: architecture boundary and status document.
- `primer/`: repository-authoritative coding and Rust-shape rules.

## Run tests

```bash
cargo test
```

## Current non-goals

- No renderer backend yet.
- No physics backend yet.
- No shader language/compiler pipeline yet.
- No ECS/archetype/query framework rollout.
- No production engine claims.
