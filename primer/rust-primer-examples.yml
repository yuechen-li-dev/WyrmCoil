# Dunewyrm Architecture Contract (M9)

Dunewyrm is a Rust-native sibling of DragonGod, not a line-by-line port.

## Runtime truth preserved

- Explicit state and explicit PC/phase progression.
- Deterministic ticks and stack-based control.
- Typed board memory, dirty tracking, and deterministic mailbox boundaries.
- Persistence chunks and trace comparison.

## Rust posture

- Use Rust features where they clarify design (enums, matching, typed phases, helper functions).
- Prefer owned runtime state over lifetime-heavy borrowed architectures.
- Keep authoring friendly and explicit; avoid trait/object/generic theater.

## Early constraints

- No async/generators/nightly/proc-macro dependency in early runtime stages.
- `FrameId` and `ActId` should evolve as domain-scoped numeric identities from the start.
- Board memory should be typed and bounded, not a generic object store.


## M2 stack semantics

- Runtime uses an explicit frame stack with one frame-step call per tick.
- `Push` stores parent resume PC and schedules child start at PC 0 on a later tick.
- `Pop` removes top frame and resumes parent on a later tick.
- `Replace` removes current top frame and installs target frame at PC 0 on a later tick.
- Child `Complete` is treated as a successful pop; root `Complete` ends the session.
- `WaitTicks` applies only to the current top frame and blocks parent execution while waiting.
- Stack runtime is still pre-board, pre-mailbox, pre-utility, and pre-actuation.


## M3 typed board memory

- Session-owned board shared by all frames in the active stack.
- Typed keys (`DwKey<T>`) are currently closed to `bool`, `i32`, and `f32`.
- Board supports `Set`, `TryGet`, `GetOr`, `IsDirty`, `DirtySlots`, and `ClearDirty`.
- Dirty tracking is automatic for successful writes and reset at tick start.
- Slot collisions are diagnosed when a slot is reused with different name or type.
- Board is control working memory only, not a generic object store and not persistence.

## M4 mailbox visible/staged semantics

- Session-owned mailbox has deterministic FIFO visible and staged queues.
- `BeginTick` promotes staged messages into visible before wait processing or frame execution.
- Frame code reads and mutates mailbox through `DwFrameCtx` (`Mailbox` / `MailboxMut`).
- `Enqueue` during a tick appends only to staged, never to same-tick visible reads.
- Visible messages remain in FIFO order until consumed.
- Mailbox is synchronous runtime state only: no async/event loop machinery.

## M5 persistence chunks

- Runtime chunk export/import exists as plain Rust data structs, not file serialization.
- Restore requires caller-provided `DwFrameRegistry`; no global registry lookup is used.
- Chunk boundaries are tick/result boundaries; mid-frame serialization is not supported.
- Chunks persist session tick/status/failure, wait state, runtime stack, board state, dirty slots, and mailbox visible/staged queues.
- Persistence currently avoids serde and versioned storage formats by design.


## M6 tick trace and comparison

- Session records one structured `DwTickTraceEntry` per tick in memory.
- Trace entries include tick/status/frame/pc/stack/control/dirty slots/mailbox snapshots/failure reason.
- `CompareTrace` reports first mismatch index and reason, including expected/actual entries when available.
- Formatting helpers (`FormatTraceEntry`, `FormatTrace`, `FormatComparison`) are diagnostics only.
- No trace file artifact writer or serde serialization is added in M6.


## M7 utility decisions

- Decision authoring remains explicit (`Dw::When`, `Dw::Decide`) with plain scorer function pointers.
- Scores clamp to `[0,1]` before ranking.
- Tie-break, min-commit, and hysteresis provide anti-thrashing arbitration.
- Decision helper pushes selected child and resumes parent at the same PC on later ticks (decision tick is real).
- Utility commitment memory is runtime-owned, keyed by frame+PC, and persisted through chunk export/import.
- Tick traces include structured decision entries for deterministic diagnostics.
- M8 includes actuation intent recording only (no side-effect execution):
  - `DwActId` is domain-scoped numeric identity (`Domain`, `Local`).
  - Frames can emit immediate acts and schedule deferred acts through frame context.
  - Deferred acts mature at tick boundary start; acts scheduled during tick `N` with `delay=0` mature on tick `N+1`.
  - Tick results/traces include immediate and matured deferred acts.
  - Pending deferred acts (with due ticks) persist through runtime chunks.


## M9 first external sample

- A tiny Guard Patrol / Recover sample exists outside core runtime modules at `samples/guard_patrol.rs`.
- The sample is authored like downstream library usage and relies only on public `dunewyrm` APIs.
- It exercises stack transitions, typed board writes, mailbox consumption, utility decisions (`Dw::Decide`), immediate and deferred actuation, trace determinism, and chunk restore equivalence via integration tests.
- M9 intentionally does not add broad sample frameworks or runtime redesign.


## M11/M12 WyrmCoil prototype scaffold

- WyrmCoil is a sample/prototype layer outside core runtime modules (`samples/wyrmcoil.rs`).
- Dunewyrm remains the behavioral control spine (frames/stack/mailbox/acts/chunks).
- World data lanes stay dense and typed in explicit stores (no ECS/archetype/query framework in M11).
- Engine chunk composition is explicit: `WcEngineChunk = DwRuntimeChunk + WcWorldChunk`.
- M11 intentionally excludes renderer/physics/UI systems and act payload redesign.


### M12 entity command pressure extension

- WyrmCoil extends the prototype with multi-entity command routing using sample-local typed board intent.
- Dunewyrm core actuation stays unchanged: act IDs remain payload-free and generic.
- The M12 bridge reads board keys to resolve entity-targeted command parameters and updates dense world lanes explicitly.
- This pass is intentionally bounded to sample/prototype behavior and exists to measure pressure for any later core payload design.

### M13 query / selection pressure extension

- WyrmCoil extends the sample/prototype with deterministic dense-store query/selection over explicit health lanes.
- Query and selection remain data-layer responsibilities in sample-local world helpers, not core runtime APIs.
- Query results are summarized into explicit board keys and consumed by frame control logic on later runtime steps.
- Existing act payload-free Dunewyrm actuation remains unchanged; selected entities flow through board-backed command intent.
- This pass does not add ECS, archetype storage, dynamic component queries, renderer, physics, or UI layers.
- **Stores query. Frames decide. Acts connect. Mailbox reports back. Chunks persist both.**
