#![allow(non_snake_case)]

use dunewyrm::{CompareTrace, DwActId, DwControlSummary, DwRunStatus, DwSession, FormatTrace};

#[path = "../samples/guard_patrol.rs"]
mod guard_patrol;

fn RunTicks(session: &mut DwSession, count: usize) {
    for _ in 0..count {
        session.Tick().expect("session tick must succeed");
    }
}

#[test]
fn GuardPatrolPathEmitsPatrolActsAndReturnsToRoot() {
    let mut session = DwSession::New(
        guard_patrol::BuildRegistry(),
        guard_patrol::GuardFrames::Root,
        0,
    )
    .expect("guard sample session must construct");

    let t0 = session.Tick().expect("tick 0 must succeed");
    assert_eq!(
        t0.Control,
        Some(DwControlSummary::Push),
        "root should decide and push a child on tick 0"
    );
    assert_eq!(
        t0.Frame,
        Some(guard_patrol::GuardFrames::Patrol),
        "patrol should be selected without alert"
    );

    let t1 = session.Tick().expect("tick 1 must succeed");
    let act_ids = t1
        .ImmediateActs
        .iter()
        .map(|request| request.Id)
        .collect::<Vec<DwActId>>();
    assert_eq!(
        act_ids,
        vec![guard_patrol::GuardActs::Look, guard_patrol::GuardActs::Step],
        "patrol should emit look and step acts in order"
    );
    assert!(
        t1.DirtySlots.contains(&guard_patrol::Keys::Pressure.Slot),
        "patrol should dirty pressure board slot"
    );

    let t2 = session.Tick().expect("tick 2 must succeed");
    assert_eq!(
        t2.Control,
        Some(DwControlSummary::Pop),
        "patrol should pop back to root"
    );
    assert_eq!(
        t2.Frame,
        Some(guard_patrol::GuardFrames::Root),
        "root should resume after patrol pop"
    );
}

#[test]
fn MailboxAlertSelectsRecoverAndDeferredActMatures() {
    let mut session = DwSession::New(
        guard_patrol::BuildRegistry(),
        guard_patrol::GuardFrames::Root,
        0,
    )
    .expect("guard sample session must construct");

    session
        .MailboxMut()
        .Enqueue(guard_patrol::TargetLostMessage());

    let t0 = session.Tick().expect("tick 0 must succeed");
    assert_eq!(
        t0.Frame,
        Some(guard_patrol::GuardFrames::Recover),
        "recover should be selected after mailbox alert"
    );
    assert!(
        t0.DirtySlots.contains(&guard_patrol::Keys::TargetLost.Slot),
        "root should dirty target-lost slot when consuming alert"
    );
    assert_eq!(
        t0.Decisions.len(),
        1,
        "utility decision record should be present"
    );
    assert_eq!(
        t0.Decisions[0].Selected,
        guard_patrol::GuardFrames::Recover,
        "decision record should select recover frame"
    );

    let t1 = session.Tick().expect("tick 1 must succeed");
    assert!(
        t1.ImmediateActs
            .iter()
            .any(|request| request.Id == guard_patrol::GuardActs::RecoverSweep),
        "recover should emit recover sweep immediate act"
    );
    assert_eq!(
        session
            .Board()
            .GetOr(guard_patrol::Keys::RecoverAttempts, -1),
        1,
        "recover attempts should increment to one"
    );

    let t2 = session.Tick().expect("tick 2 must succeed");
    assert_eq!(
        t2.Control,
        Some(DwControlSummary::Pop),
        "recover should pop back to root"
    );

    let t3 = session.Tick().expect("tick 3 must succeed");
    assert!(
        t3.MaturedDeferredActs
            .iter()
            .any(|request| request.Id == guard_patrol::GuardActs::CallBackup),
        "call-backup deferred act should mature on expected tick"
    );
}

#[test]
fn SaveRestoreProducesEquivalentTrace() {
    let mut full = DwSession::New(
        guard_patrol::BuildRegistry(),
        guard_patrol::GuardFrames::Root,
        0,
    )
    .expect("full session must construct");
    full.MailboxMut().Enqueue(guard_patrol::TargetLostMessage());
    RunTicks(&mut full, 6);
    let full_trace = full.Trace().to_vec();

    let mut split = DwSession::New(
        guard_patrol::BuildRegistry(),
        guard_patrol::GuardFrames::Root,
        0,
    )
    .expect("split session must construct");
    split
        .MailboxMut()
        .Enqueue(guard_patrol::TargetLostMessage());
    RunTicks(&mut split, 3);
    let chunk = split.ExportChunk();

    let mut restored = DwSession::FromChunk(guard_patrol::BuildRegistry(), chunk)
        .expect("restore should succeed with caller registry");
    RunTicks(&mut restored, 3);

    let mut combined_trace = split.Trace().to_vec();
    combined_trace.extend_from_slice(restored.Trace());

    let comparison = CompareTrace(&full_trace, &combined_trace);
    assert!(
        comparison.Matches,
        "split+restore trace should match uninterrupted run:\n{}\nfull:\n{}\ncombined:\n{}",
        dunewyrm::FormatComparison(&comparison),
        FormatTrace(&full_trace),
        FormatTrace(&combined_trace)
    );
    assert_eq!(
        full.Board().GetOr(guard_patrol::Keys::RecoverAttempts, -1),
        restored
            .Board()
            .GetOr(guard_patrol::Keys::RecoverAttempts, -2),
        "restored and full runs should end with same recover attempts"
    );
    let final_status = full_trace
        .last()
        .map(|entry| entry.Status)
        .expect("full trace should not be empty");
    assert_eq!(
        final_status,
        DwRunStatus::Running,
        "sample should remain running as a looping root decision behavior"
    );
}
