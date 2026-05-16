#![allow(non_snake_case)]

use wyrmcoil::{
    Demo::persistent_controller::{
        self, Acts, AlertMessage, BuildPersistentControllerRegistry, Frames, Keys,
        NewPersistentControllerSession,
    },
    DwControlSummary, DwMessage, DwRootPolicy, DwRunStatus, DwSession,
};

#[test]
fn IdleRootReturnsSteadyAndRemainsOnRootFrame() {
    let mut session = NewPersistentControllerSession();

    let first = session.Tick().expect("idle tick should succeed");
    assert_eq!(
        first.Status,
        DwRunStatus::Steady,
        "expected idle root tick to return Steady"
    );
    assert_eq!(
        first.StackDepth, 1,
        "expected idle steady root to keep stack depth at one"
    );
    assert_eq!(
        first.Frame,
        Some(Frames::ControllerRoot),
        "expected steady root frame to remain active"
    );
    assert_eq!(
        first.Pc,
        Some(0),
        "expected root program counter to remain in poll phase while idle"
    );
    assert_eq!(
        first.Status,
        DwRunStatus::Steady,
        "expected steady status to indicate non-terminal persistent root session"
    );
}

#[test]
fn TypedEventConsumptionPushesChildAndResumesToSteady() {
    let mut session = NewPersistentControllerSession();
    session.MailboxMut().Enqueue(AlertMessage(2));

    let staged_before = session.Mailbox().StagedSnapshot();
    assert_eq!(
        staged_before,
        vec![DwMessage::I32(persistent_controller::MailKinds::Alert, 2)],
        "expected typed alert payload to be staged before tick boundary"
    );

    let root_consumes = session.Tick().expect("alert consume tick should succeed");
    assert_eq!(
        root_consumes.Status,
        DwRunStatus::Running,
        "expected root to run when staged alert promotes and is consumed"
    );
    assert_eq!(
        root_consumes.Frame,
        Some(Frames::HandleAlert),
        "expected push result to advance current frame to child handler"
    );
    assert_eq!(
        root_consumes.StackDepth, 2,
        "expected root alert consume to push child frame"
    );

    let child_start = session.Tick().expect("child start tick should succeed");
    assert_eq!(
        child_start
            .ImmediateActs
            .iter()
            .map(|a| a.Id)
            .collect::<Vec<_>>(),
        vec![Acts::BeginHandleAlert],
        "expected child start phase to emit BeginHandleAlert act"
    );

    let child_done = session.Tick().expect("child done tick should succeed");
    assert_eq!(
        child_done
            .ImmediateActs
            .iter()
            .map(|a| a.Id)
            .collect::<Vec<_>>(),
        vec![Acts::CompleteHandleAlert],
        "expected child done phase to emit CompleteHandleAlert act"
    );
    assert_eq!(
        child_done.StackDepth, 1,
        "expected child pop to resume root as sole stack frame"
    );

    let resume_to_poll = session.Tick().expect("resume tick should succeed");
    assert_eq!(
        resume_to_poll.Status,
        DwRunStatus::Running,
        "expected root wait-for-child phase to continue back to poll"
    );

    let steady_again = session.Tick().expect("steady tick should succeed");
    assert_eq!(
        steady_again.Status,
        DwRunStatus::Steady,
        "expected root to return Steady once alert child work completes"
    );
    assert_eq!(
        steady_again.StackDepth, 1,
        "expected KeepRootFrame root to remain stack depth one after child work"
    );
}

#[test]
fn BoardTtlRefreshAndExpiryResetValues() {
    let mut session = NewPersistentControllerSession();
    session.MailboxMut().Enqueue(AlertMessage(7));

    let _ = session.Tick().expect("tick should succeed");
    assert_eq!(
        session.Board().GetOr(Keys::AlertActive, false),
        true,
        "expected alert active flag to be true after alert consume"
    );
    assert_eq!(
        session.Board().GetOr(Keys::AlertLevel, 0),
        7,
        "expected alert level to match consumed typed payload"
    );

    let ttl_entries = session.Board().TtlSnapshot().Entries;
    assert_eq!(
        ttl_entries.len(),
        2,
        "expected both alert keys to register TTL entries"
    );

    let _ = session.Tick().expect("tick should succeed");
    let _ = session.Tick().expect("tick should succeed");
    assert_eq!(
        session.Board().GetOr(Keys::AlertActive, true),
        false,
        "expected alert active bool to expire to false after deterministic ttl ticks"
    );
    assert_eq!(
        session.Board().GetOr(Keys::AlertLevel, 99),
        0,
        "expected alert level i32 to expire to zero after deterministic ttl ticks"
    );
    assert!(
        session.Board().IsDirty(Keys::AlertActive),
        "expected bool slot dirty flag to be set on ttl expiry write"
    );
    assert!(
        session.Board().IsDirty(Keys::AlertLevel),
        "expected i32 slot dirty flag to be set on ttl expiry write"
    );
}

#[test]
fn KeepRootFramePreservesRootAfterAccidentalComplete() {
    let mut registry = BuildPersistentControllerRegistry();
    registry
        .Register(wyrmcoil::DwFrameDef {
            Id: wyrmcoil::DwFrameId {
                Domain: Frames::Domain,
                Local: 99,
            },
            Step: |_| wyrmcoil::Dw::Complete(),
            DebugName: "CompletingRoot",
        })
        .expect("custom complete root should register");

    let mut session = DwSession::NewWithRootPolicy(
        registry,
        wyrmcoil::DwFrameId {
            Domain: Frames::Domain,
            Local: 99,
        },
        0,
        DwRootPolicy::KeepRootFrame,
    )
    .expect("keep-root session with completing root should construct");

    let tick = session.Tick().expect("completing root tick should succeed");
    assert_eq!(
        tick.Status,
        DwRunStatus::Steady,
        "expected KeepRootFrame to transform root complete into Steady"
    );
    assert_eq!(
        tick.Control,
        Some(DwControlSummary::Complete),
        "expected control summary to preserve completing root signal"
    );
    assert_eq!(
        tick.StackDepth, 1,
        "expected KeepRootFrame to preserve root frame on stack after complete transform"
    );
}
