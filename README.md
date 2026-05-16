# WyrmCoil

WyrmCoil is a deterministic Rust engine-core prototype with an embedded Dunewyrm control kernel.

**Current status:** M0 scaffold normalization.

**Architecture slogan:** Frames decide. Stores iterate. Acts connect. Mailbox reports back. Chunks persist both.

## Module layout

- `src/lib.rs`: top-level crate identity and M0 scaffold entrypoints.
- `src/Dunewyrm/`: preserved Dunewyrm kernel/runtime source from the reorg snapshot.
- `src/Engine/`: preserved WyrmCoil prototype engine layer source from the reorg snapshot.
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
