#![allow(non_snake_case)]

use dunewyrm::{CompareTrace, DwActRequest};

#[path = "../samples/wyrmcoil.rs"]
mod wyrmcoil;

#[test]
fn DenseStoreMultiEntityVelocityAliveFilteringAndChunkRoundTrip() {
    let mut store = wyrmcoil::WcTransformStore::New();
    let player = store.Spawn(wyrmcoil::WcVec2 { X: 2.0, Y: 3.0 });
    let guard = store.Spawn(wyrmcoil::WcVec2 { X: 9.0, Y: 9.0 });

    store.SetVelocity(player, wyrmcoil::WcVec2 { X: 1.5, Y: -1.0 });
    store.SetVelocity(guard, wyrmcoil::WcVec2 { X: -2.0, Y: 0.5 });
    store.SetAlive(guard, false);
    store.Tick();

    assert_eq!(
        store.Position(player),
        Some(wyrmcoil::WcVec2 { X: 3.5, Y: 2.0 }),
        "alive player entity should integrate velocity into deterministic dense position lanes"
    );
    assert_eq!(
        store.Position(guard),
        Some(wyrmcoil::WcVec2 { X: 9.0, Y: 9.0 }),
        "non-alive guard entity should be filtered out of world integration loop"
    );

    let chunk = store.ExportChunk();
    let restored = wyrmcoil::WcTransformStore::FromChunk(chunk);
    assert_eq!(
        restored, store,
        "transform store chunk restore should preserve positions, velocities, and alive lanes exactly"
    );
}

#[test]
fn DenseHealthQuerySelectsLowestAliveWithDeterministicTieAndChunkRoundTrip() {
    let mut world = wyrmcoil::WcWorld::New();
    let a = world.SpawnEntity(wyrmcoil::WcVec2::Zero(), 20.0);
    let b = world.SpawnEntity(wyrmcoil::WcVec2::Zero(), 10.0);
    let c = world.SpawnEntity(wyrmcoil::WcVec2::Zero(), 10.0);
    world.Transforms.SetAlive(b, false);

    let selected = world.FindLowestHealthAliveEntity();
    assert_eq!(
        selected,
        Some(c),
        "lowest-health query should ignore dead entities and select the lowest-health alive entity with lowest index tie-break among alive lanes"
    );

    world.Transforms.SetAlive(c, false);
    world.Transforms.SetAlive(a, false);
    assert_eq!(
        world.FindLowestHealthAliveEntity(),
        None,
        "lowest-health query should return no selection when all entities are non-alive"
    );

    let mut world2 = wyrmcoil::WcWorld::New();
    let x = world2.SpawnEntity(wyrmcoil::WcVec2::Zero(), 3.0);
    world2.Health.SetHealth(x, 2.5);
    let chunk = world2.ExportChunk();
    let restored = wyrmcoil::WcWorld::FromChunk(chunk);
    assert_eq!(
        restored, world2,
        "world chunk restore should preserve health and transform lanes used by deterministic selection query"
    );
}

#[test]
fn ActBridgeReadsBoardBackedCommandIntentAndTargetsOnlyRequestedEntity() {
    let mut world = wyrmcoil::WcWorld::New();
    let player = world.SpawnEntity(wyrmcoil::WcVec2::Zero(), 100.0);
    let guard = world.SpawnEntity(wyrmcoil::WcVec2::Zero(), 100.0);

    let mut board = dunewyrm::DwBoard::New();
    board
        .Set(wyrmcoil::WcKeys::CommandEntity, player.0 as i32)
        .expect("command entity write should succeed for player targeting");
    board
        .Set(wyrmcoil::WcKeys::CommandVelocityX, 1.0)
        .expect("command velocity x write should succeed for player targeting");
    board
        .Set(wyrmcoil::WcKeys::CommandVelocityY, 0.0)
        .expect("command velocity y write should succeed for player targeting");

    wyrmcoil::DispatchActs(
        &mut world,
        &board,
        &[DwActRequest {
            Id: wyrmcoil::WcActs::ApplyVelocityCommand,
        }],
    );

    assert_eq!(
        world.Transforms.Velocity(player),
        Some(wyrmcoil::WcVec2 { X: 1.0, Y: 0.0 }),
        "board-backed command intent should set velocity only for the addressed player entity"
    );
    assert_eq!(
        world.Transforms.Velocity(guard),
        Some(wyrmcoil::WcVec2::Zero()),
        "player-targeted command intent should not mutate guard velocity lanes"
    );

    board
        .Set(wyrmcoil::WcKeys::CommandEntity, 99)
        .expect("invalid command entity index write should still be representable on board");
    board
        .Set(wyrmcoil::WcKeys::CommandVelocityX, 5.0)
        .expect("invalid command velocity x write should succeed on board");
    board
        .Set(wyrmcoil::WcKeys::CommandVelocityY, 5.0)
        .expect("invalid command velocity y write should succeed on board");

    wyrmcoil::DispatchActs(
        &mut world,
        &board,
        &[DwActRequest {
            Id: wyrmcoil::WcActs::ApplyVelocityCommand,
        }],
    );

    assert_eq!(
        world.Transforms.Velocity(player),
        Some(wyrmcoil::WcVec2 { X: 1.0, Y: 0.0 }),
        "invalid target index should be ignored by the act bridge instead of mutating arbitrary entity lanes"
    );
}

#[test]
fn EngineTickMailboxCommandWritesBoardAndDispatchesCommandActs() {
    let mut engine = wyrmcoil::WcEngine::New();
    engine
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil::MoveRightMessage());
    engine
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil::NudgeGuardMessage());

    let _t0 = engine.Tick();
    let t1 = engine.Tick();

    assert!(
        t1.Runtime.ImmediateActs.contains(&DwActRequest {
            Id: wyrmcoil::WcActs::ApplyVelocityCommand,
        }),
        "player frame should emit ApplyVelocityCommand immediate act after consuming MoveRight mailbox message"
    );
    assert!(
        t1.Runtime.ImmediateActs.contains(&DwActRequest {
            Id: wyrmcoil::WcActs::NudgeEntityCommand,
        }),
        "player frame should emit NudgeEntityCommand immediate act after consuming nudge mailbox message"
    );
    assert!(
        t1.Runtime.DirtySlots.contains(&21)
            && t1.Runtime.DirtySlots.contains(&22)
            && t1.Runtime.DirtySlots.contains(&23),
        "typed command board keys should be marked dirty when command intent is written by frame logic"
    );
    assert_eq!(
        engine.Session.Board().GetOr(wyrmcoil::WcKeys::HasSelection, false),
        true,
        "engine tick should leave board-backed selection summary available for frame consumption after deterministic dense query step"
    );
    assert!(
        engine
            .Session
            .Board()
            .GetOr(wyrmcoil::WcKeys::SelectedEntity, -1)
            >= 0,
        "engine tick should leave selected entity index on board after deterministic dense query step"
    );

    let before_guard = engine
        .World
        .Transforms
        .Position(engine.Guard)
        .expect("guard entity should exist before deterministic loop steps");

    for _ in 0..4 {
        let _ = engine.Tick();
    }

    let after_guard = engine
        .World
        .Transforms
        .Position(engine.Guard)
        .expect("guard entity should exist after deterministic loop steps");
    assert!(
        after_guard.Y > before_guard.Y,
        "guard position should advance after guard command acts and world integration execute"
    );
}

#[test]
fn QuerySelectionFeedsControlAndMutatesSelectedEntityOnly() {
    let mut engine = wyrmcoil::WcEngine::New();
    let selected = engine.Guard;
    let non_selected = engine.Player;
    engine.World.Health.SetHealth(engine.Player, 90.0);
    engine.World.Health.SetHealth(engine.Guard, 10.0);
    engine
        .World
        .RefreshSelectionBoard(engine.Session.BoardMut());
    assert_eq!(
        engine
            .Session
            .Board()
            .GetOr(wyrmcoil::WcKeys::HasSelection, false),
        true,
        "selection summary should report true when at least one alive entity exists"
    );
    assert_eq!(
        engine
            .Session
            .Board()
            .GetOr(wyrmcoil::WcKeys::SelectedEntity, -1),
        selected.0 as i32,
        "selection summary should report the deterministic lowest-health alive entity index on the board"
    );

    let baseline_selected_position = engine
        .World
        .Transforms
        .Position(selected)
        .expect("selected entity should exist before query-to-control test tick");
    let baseline_other_position = engine
        .World
        .Transforms
        .Position(non_selected)
        .expect("non-selected entity should exist before query-to-control test tick");

    for _ in 0..8 {
        engine.Tick();
    }

    let updated_selected_position = engine
        .World
        .Transforms
        .Position(selected)
        .expect("selected entity should exist after query-to-control test ticks");
    let updated_other_position = engine
        .World
        .Transforms
        .Position(non_selected)
        .expect("non-selected entity should exist after query-to-control test ticks");
    assert_ne!(
        updated_selected_position, baseline_selected_position,
        "guard frame should route ApplyVelocityCommand through board-backed selected entity key and move selected position lane"
    );
    assert_eq!(
        updated_other_position, baseline_other_position,
        "query-driven command routing should leave non-selected entity position unchanged in this bounded pressure test"
    );
}

#[test]
fn EngineChunkRestoreMatchesUninterruptedMultiEntityCommandExecution() {
    let mut uninterrupted = wyrmcoil::WcEngine::New();
    uninterrupted
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil::MoveLeftMessage());
    uninterrupted
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil::AlertGuardMessage());
    uninterrupted
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil::NudgeGuardMessage());
    for _ in 0..10 {
        uninterrupted.Tick();
    }

    let mut split = wyrmcoil::WcEngine::New();
    split
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil::MoveLeftMessage());
    split
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil::AlertGuardMessage());
    split
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil::NudgeGuardMessage());
    for _ in 0..5 {
        split.Tick();
    }
    let chunk = split.ExportChunk();
    let mut restored = wyrmcoil::WcEngine::FromChunk(chunk);
    for _ in 0..5 {
        restored.Tick();
    }

    assert_eq!(
        uninterrupted.World.Transforms.Positions, restored.World.Transforms.Positions,
        "restored WyrmCoil world positions should match uninterrupted multi-entity command execution"
    );
    assert_eq!(
        uninterrupted.World.Transforms.Velocities, restored.World.Transforms.Velocities,
        "restored WyrmCoil world velocities should match uninterrupted command bridge output"
    );

    let mut combined_trace = split.Session.Trace().to_vec();
    combined_trace.extend_from_slice(restored.Session.Trace());
    let comparison = CompareTrace(uninterrupted.Session.Trace(), &combined_trace);
    assert!(
        comparison.Matches,
        "restored runtime trace should match uninterrupted trace for multi-entity command-pressure continuation"
    );
}
