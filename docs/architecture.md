# WyrmCoil Architecture (M0)

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

Planned later, intentionally absent in M0:

- renderer
- input backend
- audio backend
- shader compiler/language pipeline
- asset pipeline
- UI layer

## Current implementation status

M0 normalizes project identity and scaffold:

- repository identity is WyrmCoil at the crate/documentation level
- Dunewyrm and Engine source trees are preserved as explicit subtrees
- architecture/docs are aligned to engine-core-first intent
- compile/test baseline is restored for the scaffold crate while deeper runtime reintegration is tracked as follow-up work

## What M0 does not do

- introduce new engine/runtime features
- add renderer/physics/shader stacks
- introduce ECS/archetype/query framework architecture
- rewrite Dunewyrm runtime internals
