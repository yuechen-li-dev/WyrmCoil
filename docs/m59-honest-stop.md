# M59 Honest Stop Evidence

Date: 2026-05-17

Attempted M59 fixed-size array seed revealed a structural blocker:

- SDSL-V AST currently hardcodes `SdslvPath` for most type positions (`let`, params, return, fields, aliases).
- Array syntax `array<T, N>` requires a recursive type-ref model.
- Introducing that model ripples through parser, validator, emitter, and runner at many points that currently assume `path.Segments`.
- A bounded patch quickly produced cross-cutting compile failures across emitter/validation/test runner, indicating this is not a safe seed without first landing a dedicated type-ref representation refactor.

Conclusion:

- M59 should be split into a prerequisite milestone that introduces `SdslvTypeRef` end-to-end while preserving existing behavior.
- After that prerequisite lands, array and indexing can be added as a narrow follow-up.

This stop follows AGENTS convergence rule Outcome C (honest stop): further patching in this pass would require overbroad scope expansion and brittle rewrites.
