# WyrmCoil Architecture Overview

WyrmCoil is the top-level engine-core project.

Dunewyrm is the embedded deterministic control kernel.

Engine is the dense-store / act-bridge layer, currently represented by `src/Engine/wyrmcoil.rs`.

See also `docs/Dunewyrm/architecture.md` for the Dunewyrm runtime contract.


## Milestone status

- **M1b (complete):** Dunewyrm is fully reintegrated as an embedded kernel module under `src/Dunewyrm/` and intentionally re-exported from `src/lib.rs` for external use.
- **Engine boundary:** Engine remains a separate layer under `src/Engine/`; broader Engine reintegration is deferred to later milestones.
- **External proof:** Guard Patrol remains available via integration tests that consume public WyrmCoil/Dunewyrm APIs.
