# Dunewyrm Authoring Guide (M10)

This guide explains how to write real Dunewyrm frame code with the current public API.

## 1) What Dunewyrm code looks like

Dunewyrm frame authoring is **explicit-PC Rust**:

- Frames are plain Rust functions.
- A frame takes `&mut DwFrameCtx`.
- A frame returns `DwControl` (usually via `Dw::...` helpers).
- You normally decode numeric PC (`u32`) into a typed phase enum with `DwPhase`.
- Runtime persistence truth stays numeric (`Pc`) so sessions can save/restore deterministically.

Compact shape:

```rust
fn Patrol(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<PatrolPhase>() {
        Some(PatrolPhase::Enter) => Dw::Continue(PatrolPhase::Finish),
        Some(PatrolPhase::Finish) => Dw::Pop(),
        None => Dw::Fail("guard patrol phase invalid"),
    }
}
```

## 2) Naming convention

Dunewyrm intentionally uses CamelCase for public and author-facing calls.

Examples:

- `Dw::Continue`
- `Dw::WaitTicks`
- `Dw::Steady`
- `DwSession::New`
- `ctx.Phase`
- `ctx.BoardMut`

This is a project convention across the runtime family.

## 3) Domain-scoped IDs (frames and acts)

Define frame and act IDs inside your domain module.

```rust
mod GuardFrames {
    use dunewyrm::DwFrameId;

    pub const Domain: u64 = 100;
    pub const Root: DwFrameId = DwFrameId { Domain, Local: 1 };
    pub const Patrol: DwFrameId = DwFrameId { Domain, Local: 2 };
    pub const Recover: DwFrameId = DwFrameId { Domain, Local: 3 };
}

mod GuardActs {
    use dunewyrm::DwActId;

    pub const Domain: u64 = 200;
    pub const Look: DwActId = DwActId { Domain, Local: 1 };
    pub const Step: DwActId = DwActId { Domain, Local: 2 };
}
```

Why this shape:

- No global enum coordination across unrelated domains.
- `Local` values are only required to be unique inside one `Domain`.
- Runtime identity is numeric (`Domain`, `Local`) and persists cleanly.

## 4) Board keys

Board memory is typed and intentionally small-surface.

```rust
mod Keys {
    use dunewyrm::DwKey;

    pub const TargetLost: DwKey<bool> = DwKey::New("TargetLost", 1);
    pub const RecoverAttempts: DwKey<i32> = DwKey::New("RecoverAttempts", 2);
    pub const Pressure: DwKey<f32> = DwKey::New("Pressure", 3);
}
```

Supported key value types now:

- `bool`
- `i32`
- `f32`

Board behavior:

- Behavior-local working memory shared by active stack frames.
- Not a generic object store.
- Dirty tracking is automatic on successful `Set`.
- Slot metadata collisions are diagnosed (`DwSlotCollision`).

## 5) Typed phases (`DwPhase`)

Write a small enum and map it to numeric PC.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootPhase {
    Start,
    Decide,
    Done,
}

impl DwPhase for RootPhase {
    fn ToPc(self) -> u32 {
        match self {
            RootPhase::Start => 0,
            RootPhase::Decide => 1,
            RootPhase::Done => 2,
        }
    }

    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(RootPhase::Start),
            1 => Some(RootPhase::Decide),
            2 => Some(RootPhase::Done),
            _ => None,
        }
    }
}
```

Guideline:

- `ToPc` is what persists/restores.
- `FromPc` is what keeps frame code readable.
- Unknown PC is usually a runtime contract failure -> `Dw::Fail("bad phase")`.

## 6) Writing frame functions

Typical frame body pattern:

```rust
fn ExampleFrame(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<ExamplePhase>() {
        Some(ExamplePhase::Start) => {
            let retries = ctx.Board().GetOr(Keys::RecoverAttempts, 0);
            ctx.BoardMut().Set(Keys::RecoverAttempts, retries + 1)
                .expect("RecoverAttempts write must succeed");
            Dw::Continue(ExamplePhase::Wait)
        }
        Some(ExamplePhase::Wait) => Dw::WaitTicks(1, ExamplePhase::PushChild),
        Some(ExamplePhase::PushChild) => Dw::Push(GuardFrames::Recover, ExamplePhase::AfterChild),
        Some(ExamplePhase::AfterChild) => Dw::Pop(),
        None => Dw::Fail("bad phase"),
    }
}
```

Important timing contract:

- Runtime executes **one frame function call per tick**.
- The returned `DwControl` updates runtime state.
- Pushed/replaced child starts on a later tick, not inside same call.
- Parent resumes on a later tick after child pop/complete.

## 7) Stack semantics

Core stack controls:

- `Dw::Push(target, resume_phase)`:
  - Parent stays on stack with `resume_phase` PC.
  - Child becomes top-of-stack at PC `0` on later tick.
- `Dw::Pop()`:
  - Pops current frame and resumes parent later.
- `Dw::Replace(target)`:
  - Drops current top frame and installs target at PC `0` later.
- `Dw::Complete()`:
  - If current frame is child, it behaves like successful pop.
  - If current frame is root, session status becomes `Completed`.
- `Dw::Steady()`:
  - Leaves the current frame active with the same PC.
  - Reports a quiescent `Steady` tick status.
  - Does not start a wait timer and does not change stack shape.
  - Preferred for persistent root/controller idle loops instead of fake long waits.

Again: one frame call per tick. A control return schedules transitions; it does not execute multiple frame steps in one tick.

## 8) Mailbox: visible vs staged

Mailbox has deterministic two-queue behavior:

- `Visible`: messages readable this tick.
- `Staged`: messages enqueued during this tick.

Rules:

- Tick start promotes staged -> visible.
- `ctx.MailboxMut().Enqueue(...)` writes to staged.
- New staged messages are **not** visible until a later tick.
- FIFO order is preserved.

Example:

```rust
while let Some(message) = ctx.MailboxMut().ConsumeFront() {
    if message.Kind == MailKinds::TargetLost {
        ctx.BoardMut().Set(Keys::TargetLost, true)
            .expect("TargetLost write must succeed");
    }
}
```

## 9) Utility decisions (`Dw::Decide`)

Utility uses plain scorer functions:

- scorer signature: `fn(&DwFrameCtx) -> f32`
- score clamp: runtime clamps to `[0.0, 1.0]`
- candidates built with `Dw::When(target, scorer)`
- evaluated by `Dw::Decide(ctx, candidates, options)`

Shared utility policy selection also exposes a pure diagnostics surface for non-frame engine policy paths:

- `SelectHighestUtilityTarget(scored)` preserves existing max-score selection semantics.
- `SelectHighestUtilityTargetWithReport(scored)` returns structured diagnostics:
  - selected target (or none),
  - per-candidate raw/clamped score + rank + selected marker,
  - deterministic tie metadata,
  - selection reason (`HighestScore`, `TieBreakFirst`, `NoCandidates`, `NoPositiveScore`).
- This report surface is for observability/policy diagnostics; it does not change `Dw::Decide` behavior.

Options (`DwDecideOptions`):

- `Hysteresis`
- `MinCommitTicks`
- `TieBreak` (`KeepCurrent` or `First`)

Timing and memory:

- Decision tick is real: decide call returns a `Push` of selected child.
- Child starts on a later tick.
- Parent resumes same PC after child returns.
- Commitment memory (selected target + age per frame/pc) is runtime-owned and persisted.

## 10) Actuation

Actuation is currently **recorded intent**, not side-effect handlers.

- IDs are domain-scoped via `DwActId`.
- Emit immediate act: `ctx.Immediate(act_id)`.
- Schedule deferred act: `ctx.Deferred(act_id, delay_ticks)`.

Timing:

- Immediate acts appear in this tick result/trace.
- Deferred acts mature at deterministic tick boundaries.
- A deferred act with `delay_ticks = 0` matures on next tick.
- Deferred queue persists through runtime chunk export/import.

## 11) Sessions and ticking

Build registry, create session, tick it:

```rust
let mut registry = DwFrameRegistry::New();
registry.Register(DwFrameDef { Id: GuardFrames::Root, Step: Root, DebugName: "GuardRoot" })?;

let mut session = DwSession::New(registry, GuardFrames::Root, 0)?;
let tick = session.Tick()?;
```

Useful per-tick data (`DwTickResult`):

- `Tick`, `Status`, `Frame`, `Pc`
- `Stack`, `StackDepth`
- `Control` summary
- board dirty slots
- mailbox visible/staged snapshots
- decision entries
- immediate/matured/pending acts

## 12) Persistence chunks

Persistence API is plain runtime structs:

- `session.ExportChunk()`
- `DwSession::FromChunk(registry, chunk)`

Properties:

- No serde/file format layer yet.
- Restore requires caller-provided registry.
- Chunks represent runtime state at tick/result boundaries.

## 13) Traces and comparison

Diagnostics APIs:

- `session.Trace()` -> structured `DwTickTraceEntry` list
- `CompareTrace(expected, actual)` -> first mismatch details
- `FormatTrace(trace)` / `FormatTraceEntry(entry)`
- `FormatComparison(comparison)`

Trace entries are structured runtime data first; formatting helpers are for human diagnostics.

## 14) Guard Patrol sample

Use `samples/guard_patrol.rs` plus `tests/m9_guard_patrol_sample.rs` as the canonical compact example.

It demonstrates:

- external-style library usage
- domain-owned frame/act IDs
- typed board keys and writes
- mailbox visible/staged behavior
- utility selection with commitment rules
- immediate + deferred acts
- save/restore chunk equivalence
- trace comparison
