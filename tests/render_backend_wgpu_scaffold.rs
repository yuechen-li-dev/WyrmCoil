#![allow(non_snake_case)]

use wyrmcoil::Engine as wyrmcoil_engine;

#[test]
fn RenderBackendConsumesEmptySnapshotDeterministically() {
    let mut renderer = wyrmcoil_engine::RenderBackend::New(wyrmcoil_engine::RendererConfig {
        ClearColor: wyrmcoil_engine::ClearColor {
            R: 0.1,
            G: 0.2,
            B: 0.3,
            A: 1.0,
        },
    });

    let snapshot = wyrmcoil_engine::RenderSnapshot {
        Frame: 77,
        Items: Vec::new(),
    };

    let stats = renderer.RenderSnapshot(&snapshot);

    assert_eq!(stats.SnapshotFrame, 77);
    assert_eq!(stats.RenderItems, 0);
    assert_eq!(stats.ClearColor, renderer.Config().ClearColor);
    assert_eq!(renderer.LastStats(), Some(stats));
}

#[test]
fn RenderBackendConsumesSnapshotWithItemsAndDoesNotMutateIt() {
    let mut renderer =
        wyrmcoil_engine::RenderBackend::New(wyrmcoil_engine::RendererConfig::default());
    let snapshot = wyrmcoil_engine::RenderSnapshot {
        Frame: 9,
        Items: vec![wyrmcoil_engine::RenderItem {
            Entity: wyrmcoil_engine::EntityId(2),
            Position: wyrmcoil_engine::Vec2 { X: 4.0, Y: 6.0 },
            SpriteId: 12,
        }],
    };
    let before_snapshot = snapshot.clone();

    let stats = renderer.RenderSnapshot(&snapshot);

    assert_eq!(stats.SnapshotFrame, 9);
    assert_eq!(stats.RenderItems, 1);
    assert_eq!(snapshot, before_snapshot);
}

#[test]
fn RenderBackendConsumptionDoesNotAdvanceControlOrSimulationOrMutateWorld() {
    let mut engine = wyrmcoil_engine::Engine::New();
    let mut renderer =
        wyrmcoil_engine::RenderBackend::New(wyrmcoil_engine::RendererConfig::default());

    let before_clock = engine.Clock();
    let before_world = engine.World.clone();
    let snapshot = engine.RenderSnapshot();
    let after_snapshot_clock = engine.Clock();

    let stats = renderer.RenderSnapshot(&snapshot);

    assert_eq!(stats.SnapshotFrame, snapshot.Frame);
    assert_eq!(stats.RenderItems, snapshot.Items.len());
    assert_eq!(engine.World, before_world);
    assert_eq!(engine.Clock().ControlTick, before_clock.ControlTick);
    assert_eq!(engine.Clock().SimulationTick, before_clock.SimulationTick);
    assert_eq!(engine.Clock().RenderFrame, after_snapshot_clock.RenderFrame);
}

#[test]
fn RepeatedRenderConsumptionTracksIndependentSnapshots() {
    let mut engine = wyrmcoil_engine::Engine::New();
    let mut renderer =
        wyrmcoil_engine::RenderBackend::New(wyrmcoil_engine::RendererConfig::default());

    let first = engine.RenderSnapshot();
    let first_stats = renderer.RenderSnapshot(&first);

    let _control = engine.TickControl();
    engine.TickSimulation();
    let second = engine.RenderSnapshot();
    let second_stats = renderer.RenderSnapshot(&second);

    assert_eq!(first_stats.SnapshotFrame, first.Frame);
    assert_eq!(second_stats.SnapshotFrame, second.Frame);
    assert!(second_stats.SnapshotFrame > first_stats.SnapshotFrame);
    assert_eq!(engine.Clock().ControlTick, 1);
    assert_eq!(engine.Clock().SimulationTick, 1);
}

#[test]
fn RendererConfigClearColorIsReflectedInClearOperation() {
    let config = wyrmcoil_engine::RendererConfig {
        ClearColor: wyrmcoil_engine::ClearColor {
            R: 0.6,
            G: 0.4,
            B: 0.2,
            A: 1.0,
        },
    };
    let renderer = wyrmcoil_engine::RenderBackend::New(config);
    let clear = renderer.BuildClearOp();

    assert_eq!(renderer.Config(), config);
    assert_eq!(clear.store, wgpu::StoreOp::Store);
    match clear.load {
        wgpu::LoadOp::Clear(color) => {
            assert_eq!(color.r, config.ClearColor.R);
            assert_eq!(color.g, config.ClearColor.G);
            assert_eq!(color.b, config.ClearColor.B);
            assert_eq!(color.a, config.ClearColor.A);
        }
        _ => panic!("clear op should use LoadOp::Clear for M9 scaffold"),
    }
}
