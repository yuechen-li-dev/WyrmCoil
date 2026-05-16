# WyrmCoil Architecture (M1)

## Thesis

WyrmCoil is a deterministic Rust engine-core prototype. Its purpose is to validate control/runtime structure and module boundaries before renderer, input, audio, shader, or asset backends are introduced.

## Core identity

- **WyrmCoil**: engine-side composition layer.
- **Dunewyrm**: embedded deterministic control kernel.

**Frames decide. Stores iterate. Acts connect. Mailbox reports back. Chunks persist both.**

## Boundary model

### 1) Dunewyrm kernel

Owns deterministic control-runtime mechanics:

- explicit frame PC/runtime progression
- stack semantics
- typed board memory
- mailbox boundary behavior
- trace/persistence chunk primitives
- actuation intent records

### 2) WyrmCoil engine layer

Owns engine-side composition mechanics:

- dense world stores
- engine tick wrapper around kernel ticks
- act bridge between frame intent and world mutation
- world chunk composition with runtime chunks
- prototype/sample gameplay loop shape

### 3) Future backend layers (not implemented yet)

Planned later, intentionally absent in M1:

- renderer
- input backend
- audio backend
- shader compiler/language pipeline
- asset pipeline
- UI layer

## Current implementation status

M1 keeps M0 identity normalization and reintegrates the Dunewyrm kernel module surface:

- repository identity is WyrmCoil at the crate/documentation level
- Dunewyrm kernel modules are active in the crate module graph and re-exported for author-facing use
- Engine remains a placeholder boundary for later engine-layer composition work
- architecture/docs remain aligned to engine-core-first intent
- kernel behavior validation runs through restored Dunewyrm tests under `src/Dunewyrm/`

## What M1 does not do

- introduce new engine/runtime features
- add renderer/physics/shader stacks
- introduce ECS/archetype/query framework architecture
- rewrite Dunewyrm runtime internals
