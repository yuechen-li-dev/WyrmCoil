#![allow(non_snake_case)]

use wyrmcoil::{CompareTrace, DwActRequest, DwBoard, Engine as wyrmcoil_sample_impl};

#[test]
fn DenseStoreMultiEntityVelocityAliveFilteringAndChunkRoundTrip() {
    let mut store = wyrmcoil_sample_impl::TransformStore::New();
    let player = store.Spawn(wyrmcoil_sample_impl::Vec2 { X: 2.0, Y: 3.0 });
    let guard = store.Spawn(wyrmcoil_sample_impl::Vec2 { X: 9.0, Y: 9.0 });

    store.SetVelocity(player, wyrmcoil_sample_impl::Vec2 { X: 1.5, Y: -1.0 });
    store.SetVelocity(guard, wyrmcoil_sample_impl::Vec2 { X: -2.0, Y: 0.5 });
    store.SetAlive(guard, false);
    store.Tick();

    assert_eq!(
        store.Position(player),
        Some(wyrmcoil_sample_impl::Vec2 { X: 3.5, Y: 2.0 }),
        "alive player entity should integrate velocity into deterministic dense position lanes"
    );
    assert_eq!(
        store.Position(guard),
        Some(wyrmcoil_sample_impl::Vec2 { X: 9.0, Y: 9.0 }),
        "non-alive guard entity should be filtered out of world integration loop"
    );

    let chunk = store.ExportChunk();
    let restored = wyrmcoil_sample_impl::TransformStore::FromChunk(chunk);
    assert_eq!(
        restored, store,
        "transform store chunk restore should preserve positions, velocities, and alive lanes exactly"
    );
}

#[test]
fn DenseHealthQuerySelectsLowestAliveWithDeterministicTieAndChunkRoundTrip() {
    let mut world = wyrmcoil_sample_impl::World::New();
    let a = world.SpawnEntity(wyrmcoil_sample_impl::Vec2::Zero(), 20.0);
    let b = world.SpawnEntity(wyrmcoil_sample_impl::Vec2::Zero(), 10.0);
    let c = world.SpawnEntity(wyrmcoil_sample_impl::Vec2::Zero(), 10.0);
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

    let mut world2 = wyrmcoil_sample_impl::World::New();
    let x = world2.SpawnEntity(wyrmcoil_sample_impl::Vec2::Zero(), 3.0);
    world2.Health.SetHealth(x, 2.5);
    let chunk = world2.ExportChunk();
    let restored = wyrmcoil_sample_impl::World::FromChunk(chunk);
    assert_eq!(
        restored, world2,
        "world chunk restore should preserve health and transform lanes used by deterministic selection query"
    );
}

#[test]
fn ActBridgeReadsBoardBackedCommandIntentAndTargetsOnlyRequestedEntity() {
    let mut world = wyrmcoil_sample_impl::World::New();
    let player = world.SpawnEntity(wyrmcoil_sample_impl::Vec2::Zero(), 100.0);
    let guard = world.SpawnEntity(wyrmcoil_sample_impl::Vec2::Zero(), 100.0);

    let mut board = DwBoard::New();
    board
        .Set(wyrmcoil_sample_impl::Keys::CommandEntity, player.0 as i32)
        .expect("command entity write should succeed for player targeting");
    board
        .Set(wyrmcoil_sample_impl::Keys::CommandVelocityX, 1.0)
        .expect("command velocity x write should succeed for player targeting");
    board
        .Set(wyrmcoil_sample_impl::Keys::CommandVelocityY, 0.0)
        .expect("command velocity y write should succeed for player targeting");

    wyrmcoil_sample_impl::DispatchActs(
        &mut world,
        &board,
        &[DwActRequest {
            Id: wyrmcoil_sample_impl::Acts::ApplyVelocityCommand,
        }],
    );

    assert_eq!(
        world.Transforms.Velocity(player),
        Some(wyrmcoil_sample_impl::Vec2 { X: 1.0, Y: 0.0 }),
        "board-backed command intent should set velocity only for the addressed player entity"
    );
    assert_eq!(
        world.Transforms.Velocity(guard),
        Some(wyrmcoil_sample_impl::Vec2::Zero()),
        "player-targeted command intent should not mutate guard velocity lanes"
    );

    board
        .Set(wyrmcoil_sample_impl::Keys::CommandEntity, 99)
        .expect("invalid command entity index write should still be representable on board");
    board
        .Set(wyrmcoil_sample_impl::Keys::CommandVelocityX, 5.0)
        .expect("invalid command velocity x write should succeed on board");
    board
        .Set(wyrmcoil_sample_impl::Keys::CommandVelocityY, 5.0)
        .expect("invalid command velocity y write should succeed on board");

    wyrmcoil_sample_impl::DispatchActs(
        &mut world,
        &board,
        &[DwActRequest {
            Id: wyrmcoil_sample_impl::Acts::ApplyVelocityCommand,
        }],
    );

    assert_eq!(
        world.Transforms.Velocity(player),
        Some(wyrmcoil_sample_impl::Vec2 { X: 1.0, Y: 0.0 }),
        "invalid target index should be ignored by the act bridge instead of mutating arbitrary entity lanes"
    );
}

#[test]
fn EngineTickMailboxCommandWritesBoardAndDispatchesCommandActs() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::MoveRightPressed);
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::NudgeGuardPressed);

    let _t0 = engine.Tick();
    let t1 = engine.Tick();

    assert!(
        t1.Runtime.ImmediateActs.contains(&DwActRequest {
            Id: wyrmcoil_sample_impl::Acts::ApplyVelocityCommand,
        }),
        "player frame should emit ApplyVelocityCommand immediate act after consuming MoveRight mailbox message"
    );
    assert!(
        t1.Runtime.ImmediateActs.contains(&DwActRequest {
            Id: wyrmcoil_sample_impl::Acts::NudgeEntityCommand,
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
        engine
            .Session
            .Board()
            .GetOr(wyrmcoil_sample_impl::Keys::HasSelection, false),
        true,
        "engine tick should leave board-backed selection summary available for frame consumption after deterministic dense query step"
    );
    assert!(
        engine
            .Session
            .Board()
            .GetOr(wyrmcoil_sample_impl::Keys::SelectedEntity, -1)
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
fn InputQueuePreservesOrderAndDrainsAtControlBoundary() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::MoveLeftPressed);
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::StopPressed);
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::MoveRightPressed);

    assert_eq!(
        engine.InputQueueSnapshot(),
        vec![
            wyrmcoil_sample_impl::InputEvent::MoveLeftPressed,
            wyrmcoil_sample_impl::InputEvent::StopPressed,
            wyrmcoil_sample_impl::InputEvent::MoveRightPressed,
        ],
        "input queue snapshot should preserve enqueue order for deterministic adapter-to-runtime bridging"
    );

    let _first_control = engine.TickControl();
    let control_result = engine.TickControl();
    assert_eq!(
        engine.InputQueueLen(),
        0,
        "TickControl should drain every queued normalized input event into the Dunewyrm mailbox before control logic executes"
    );
    assert!(
        control_result.ImmediateActs.contains(&DwActRequest {
            Id: wyrmcoil_sample_impl::Acts::ApplyVelocityCommand,
        }),
        "drained input should become mailbox messages that produce command acts in the same control tick"
    );
}

#[test]
fn InputTimingBoundariesStayIndependentAcrossEnginePhases() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::MoveRightPressed);

    assert_eq!(
        engine.Clock().ControlTick,
        0,
        "enqueueing input must not advance the control clock before TickControl"
    );
    assert_eq!(
        engine.Clock().SimulationTick,
        0,
        "enqueueing input must not advance the simulation clock before TickSimulation"
    );

    engine.TickSimulation();
    assert_eq!(
        engine.InputQueueLen(),
        1,
        "TickSimulation must not process queued input because input enters the control lane only"
    );

    let _snapshot = engine.RenderSnapshot();
    assert_eq!(
        engine.InputQueueLen(),
        1,
        "RenderSnapshot must not process queued input because render frames only observe world snapshots"
    );

    let _first_control = engine.TickControl();
    let control_result = engine.TickControl();
    assert_eq!(
        engine.InputQueueLen(),
        0,
        "TickControl must bridge queued input into the mailbox and consume the queue deterministically"
    );
    assert!(
        control_result.ImmediateActs.contains(&DwActRequest {
            Id: wyrmcoil_sample_impl::Acts::ApplyVelocityCommand,
        }),
        "TickControl must convert normalized input to mailbox traffic that frame logic consumes as control acts"
    );
}

#[test]
fn TickConvenienceProcessesQueuedInputThroughControlPhase() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
    let baseline = engine
        .World
        .Transforms
        .Position(engine.Player)
        .expect("player entity should exist before convenience tick input test");
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::MoveRightPressed);

    let _first = engine.Tick();
    let tick_result = engine.Tick();
    assert_eq!(
        engine.InputQueueLen(),
        0,
        "Tick convenience wrapper should process queued input during its control phase before simulation"
    );
    assert!(
        tick_result.Runtime.ImmediateActs.contains(&DwActRequest {
            Id: wyrmcoil_sample_impl::Acts::ApplyVelocityCommand,
        }),
        "Tick convenience wrapper should expose control acts produced by bridged input"
    );

    let after = engine
        .World
        .Transforms
        .Position(engine.Player)
        .expect("player entity should exist after convenience tick input test");
    assert!(
        after.X > baseline.X,
        "world movement should appear only after Tick simulation phase executes after control consumed input"
    );
}

#[test]
fn EngineChunkPersistsQueuedInputUntilBridgeConsumesIt() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::MoveLeftPressed);
    engine.EnqueueInput(wyrmcoil_sample_impl::InputEvent::StopPressed);

    let chunk = engine.ExportChunk();
    let mut restored = wyrmcoil_sample_impl::Engine::FromChunk(chunk);
    assert_eq!(
        restored.InputQueueSnapshot(),
        vec![
            wyrmcoil_sample_impl::InputEvent::MoveLeftPressed,
            wyrmcoil_sample_impl::InputEvent::StopPressed,
        ],
        "EngineChunk should persist queued unbridged input because it is deterministic engine-owned state"
    );

    let _ = restored.TickControl();
    assert_eq!(
        restored.InputQueueLen(),
        0,
        "restored queued input should bridge into mailbox and drain on the next control tick"
    );
}

#[test]
fn QuerySelectionFeedsControlAndMutatesSelectedEntityOnly() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
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
            .GetOr(wyrmcoil_sample_impl::Keys::HasSelection, false),
        true,
        "selection summary should report true when at least one alive entity exists"
    );
    assert_eq!(
        engine
            .Session
            .Board()
            .GetOr(wyrmcoil_sample_impl::Keys::SelectedEntity, -1),
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
    let mut uninterrupted = wyrmcoil_sample_impl::Engine::New();
    uninterrupted
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil_sample_impl::MoveLeftMessage());
    uninterrupted
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil_sample_impl::AlertGuardMessage());
    uninterrupted
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil_sample_impl::NudgeGuardMessage());
    for _ in 0..10 {
        uninterrupted.Tick();
    }

    let mut split = wyrmcoil_sample_impl::Engine::New();
    split
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil_sample_impl::MoveLeftMessage());
    split
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil_sample_impl::AlertGuardMessage());
    split
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil_sample_impl::NudgeGuardMessage());
    for _ in 0..5 {
        split.Tick();
    }
    let chunk = split.ExportChunk();
    let mut restored = wyrmcoil_sample_impl::Engine::FromChunk(chunk);
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

#[test]
fn EngineTimingPhasesAdvanceIndependently() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
    let initial_clock = engine.Clock();

    let _runtime = engine.TickControl();
    let after_control = engine.Clock();
    assert_eq!(
        after_control.ControlTick,
        initial_clock.ControlTick + 1,
        "control-only tick must advance control clock by exactly one"
    );
    assert_eq!(
        after_control.SimulationTick, initial_clock.SimulationTick,
        "control-only tick must not advance simulation clock"
    );

    engine.TickSimulation();
    let after_simulation = engine.Clock();
    assert_eq!(
        after_simulation.SimulationTick,
        initial_clock.SimulationTick + 1,
        "simulation-only tick must advance simulation clock by exactly one"
    );
    assert_eq!(
        after_simulation.ControlTick,
        initial_clock.ControlTick + 1,
        "simulation-only tick must not advance control clock"
    );

    let _snapshot = engine.RenderSnapshot();
    let after_render = engine.Clock();
    assert_eq!(
        after_render.RenderFrame,
        initial_clock.RenderFrame + 1,
        "render snapshot call should record one observed render frame"
    );
    assert_eq!(
        after_render.ControlTick, after_simulation.ControlTick,
        "render snapshot must not mutate control clock"
    );
    assert_eq!(
        after_render.SimulationTick, after_simulation.SimulationTick,
        "render snapshot must not mutate simulation clock"
    );
}

#[test]
fn EngineTickConvenienceRunsControlThenSimulation() {
    let mut convenience = wyrmcoil_sample_impl::Engine::New();
    let mut explicit = wyrmcoil_sample_impl::Engine::New();

    convenience.Tick();
    explicit.TickControl();
    explicit.TickSimulation();

    assert_eq!(
        convenience.Clock(),
        explicit.Clock(),
        "Tick convenience must be equivalent to one control phase followed by one simulation phase"
    );
    assert_eq!(
        convenience.World, explicit.World,
        "Tick convenience must preserve deterministic world evolution equivalent to explicit phase stepping"
    );
}

#[test]
fn WorldMutationHappensOnSimulationBoundary() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
    engine
        .Session
        .MailboxMut()
        .Enqueue(wyrmcoil_sample_impl::MoveRightMessage());

    let player_before = engine
        .World
        .Transforms
        .Position(engine.Player)
        .expect("player must exist before timing-boundary mutation test");

    let _ = engine.TickControl();
    let _ = engine.TickControl();
    let player_after_control = engine
        .World
        .Transforms
        .Position(engine.Player)
        .expect("player must exist after control-only tick");
    assert_eq!(
        player_after_control, player_before,
        "control-only ticks may set command lanes but must not integrate world positions"
    );

    engine.TickSimulation();
    let player_after_simulation = engine
        .World
        .Transforms
        .Position(engine.Player)
        .expect("player must exist after simulation tick");
    assert_eq!(
        player_after_simulation,
        wyrmcoil_sample_impl::Vec2 {
            X: player_before.X + 1.0,
            Y: player_before.Y
        },
        "simulation tick must integrate the velocity written by control-phase act dispatch"
    );
}

#[test]
fn EngineChunkRoundTripPreservesTimingCounters() {
    let mut engine = wyrmcoil_sample_impl::Engine::New();
    engine.TickControl();
    engine.TickSimulation();
    engine.TickSimulation();
    let _ = engine.RenderSnapshot();

    let before_chunk_clock = engine.Clock();
    let chunk = engine.ExportChunk();
    let restored = wyrmcoil_sample_impl::Engine::FromChunk(chunk);

    assert_eq!(
        restored.Clock(),
        before_chunk_clock,
        "engine chunk restore must preserve control, simulation, and render frame counters exactly"
    );
}
