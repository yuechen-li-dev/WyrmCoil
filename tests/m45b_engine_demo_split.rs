#![allow(non_snake_case)]

use wyrmcoil::Engine::{
    DenseAliveCount, DenseAliveIndices, DenseLaneSafeLen, Engine, RenderItem, RenderSnapshot, Vec2,
    World,
};
use wyrmcoil::{DwActRequest, DwBoard, DwFrameDef, DwFrameId, DwFrameRegistry, DwMessage};

#[derive(Clone, Debug, Default, PartialEq)]
struct FakeWorld {
    pub TickCount: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct FakeChunk {
    pub TickCount: u64,
}

impl World for FakeWorld {
    type Chunk = FakeChunk;
    fn RefreshBoard(&self, _board: &mut DwBoard) {}
    fn DispatchActs(&mut self, _board: &DwBoard, _acts: &[DwActRequest]) {}
    fn Tick(&mut self) {
        self.TickCount += 1;
    }
    fn ExtractRenderSnapshot(&self, frame: u64) -> RenderSnapshot {
        RenderSnapshot {
            Frame: frame,
            Items: vec![RenderItem {
                Entity: wyrmcoil::Engine::EntityId(0),
                Position: Vec2 {
                    X: self.TickCount as f32,
                    Y: 0.0,
                },
                SpriteId: 1,
            }],
        }
    }
    fn ExportChunk(&self) -> Self::Chunk {
        FakeChunk {
            TickCount: self.TickCount,
        }
    }
    fn FromChunk(chunk: Self::Chunk) -> Self {
        Self {
            TickCount: chunk.TickCount,
        }
    }
}

fn BuildFakeRegistry() -> DwFrameRegistry {
    fn Root(_ctx: &mut wyrmcoil::DwFrameCtx) -> wyrmcoil::DwControl {
        wyrmcoil::Dw::Pop()
    }
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: DwFrameId {
                Domain: 7,
                Local: 1,
            },
            Step: Root,
            DebugName: "Root",
        })
        .expect("register");
    registry
}

fn FakeInputToMessage(_i: u8) -> DwMessage {
    DwMessage { Kind: 0, Value: 0 }
}

#[test]
fn GenericEngineCoreWorksWithoutDemoWorld() {
    let mut engine = Engine::<FakeWorld, u8>::Construct(
        BuildFakeRegistry,
        DwFrameId {
            Domain: 7,
            Local: 1,
        },
        FakeWorld::default(),
        FakeInputToMessage,
    );
    let _ = engine.TickControl();
    engine.TickSimulation();
    let snapshot = engine.RenderSnapshot();
    assert_eq!(snapshot.Frame, 1);
    assert_eq!(snapshot.Items.len(), 1);
    assert_eq!(engine.World.TickCount, 1);
}

#[test]
fn DemoEngineIntegrationProducesRenderSnapshot() {
    let mut engine = wyrmcoil::Engine::Engine::New();
    let snapshot = engine.RenderSnapshot();
    assert!(!snapshot.Items.is_empty());
}

#[test]
fn PrimitiveReexportsAreEngineOwned() {
    let item = RenderItem {
        Entity: wyrmcoil::Engine::EntityId(3),
        Position: Vec2 { X: 1.0, Y: 2.0 },
        SpriteId: 9,
    };
    let snapshot = RenderSnapshot {
        Frame: 9,
        Items: vec![item],
    };
    assert_eq!(snapshot.Items[0].SpriteId, 9);
}

#[test]
fn StoreHelpersAreDeterministic() {
    let alive = vec![true, false, true, true, false];
    assert_eq!(DenseAliveCount(&alive), 3);
    assert_eq!(DenseAliveIndices(&alive), vec![0, 2, 3]);
    assert_eq!(DenseLaneSafeLen(&[5, 3, 9]), 3);
    assert_eq!(DenseLaneSafeLen(&[]), 0);
}
