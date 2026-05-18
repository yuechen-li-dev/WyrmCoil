#![allow(non_snake_case)]

use crate::Engine::primitives::RenderSnapshot;
use crate::{DwActRequest, DwFrameId, DwMessage, DwRuntimeChunk, DwSession, DwTickResult};

pub trait World {
    type Chunk;
    fn RefreshBoard(&self, board: &mut crate::DwBoard);
    fn DispatchActs(&mut self, board: &crate::DwBoard, acts: &[DwActRequest]);
    fn Tick(&mut self);
    fn ExtractRenderSnapshot(&self, frame: u64) -> RenderSnapshot;
    fn ExportChunk(&self) -> Self::Chunk;
    fn FromChunk(chunk: Self::Chunk) -> Self;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineClock {
    pub ControlTick: u64,
    pub SimulationTick: u64,
    pub RenderFrame: u64,
}

pub struct Engine<W: World, I: Copy> {
    pub Session: DwSession,
    pub World: W,
    pub Player: crate::Engine::EntityId,
    pub Guard: crate::Engine::EntityId,
    pub Clock: EngineClock,
    pub InputQueue: Vec<I>,
    _root_frame: DwFrameId,
    _build_registry: fn() -> crate::DwFrameRegistry,
    input_to_message: fn(I) -> DwMessage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EngineChunk<C, I> {
    pub Runtime: DwRuntimeChunk,
    pub World: C,
    pub Player: crate::Engine::EntityId,
    pub Guard: crate::Engine::EntityId,
    pub Clock: EngineClock,
    pub InputQueue: Vec<I>,
}

#[allow(dead_code)]
pub struct TickResult<W: World> {
    pub Runtime: DwTickResult,
    pub World: W,
    pub Clock: EngineClock,
}

impl<W: World, I: Copy> Engine<W, I> {
    pub fn Construct(
        build_registry: fn() -> crate::DwFrameRegistry,
        root_frame: DwFrameId,
        world: W,
        input_to_message: fn(I) -> DwMessage,
    ) -> Self {
        let session = DwSession::New(build_registry(), root_frame, 0)
            .expect("WyrmCoil session should construct");
        Self {
            Session: session,
            World: world,
            Player: crate::Engine::EntityId(0),
            Guard: crate::Engine::EntityId(1),
            Clock: EngineClock::default(),
            InputQueue: Vec::new(),
            _root_frame: root_frame,
            _build_registry: build_registry,
            input_to_message,
        }
    }
    pub fn EnqueueInput(&mut self, event: I) {
        self.InputQueue.push(event);
    }
    pub fn InputQueueLen(&self) -> usize {
        self.InputQueue.len()
    }
    pub fn InputQueueSnapshot(&self) -> Vec<I> {
        self.InputQueue.clone()
    }
    fn BridgeInputIntoMailbox(&mut self) {
        for event in self.InputQueue.drain(..) {
            self.Session
                .MailboxMut()
                .Enqueue((self.input_to_message)(event));
        }
    }
    pub fn Clock(&self) -> EngineClock {
        self.Clock
    }
    pub fn TickControl(&mut self) -> DwTickResult {
        self.BridgeInputIntoMailbox();
        self.World.RefreshBoard(self.Session.BoardMut());
        let runtime = self
            .Session
            .Tick()
            .expect("WyrmCoil control tick should succeed");
        self.World
            .DispatchActs(self.Session.Board(), &runtime.ImmediateActs);
        self.World
            .DispatchActs(self.Session.Board(), &runtime.MaturedDeferredActs);
        self.Clock.ControlTick += 1;
        runtime
    }
    pub fn TickSimulation(&mut self) {
        self.World.Tick();
        self.Clock.SimulationTick += 1;
    }
    pub fn RenderSnapshot(&mut self) -> RenderSnapshot {
        self.Clock.RenderFrame += 1;
        self.World.ExtractRenderSnapshot(self.Clock.RenderFrame)
    }
    pub fn Tick(&mut self) -> TickResult<W>
    where
        W: Clone,
    {
        let runtime = self.TickControl();
        self.TickSimulation();
        TickResult {
            Runtime: runtime,
            World: self.World.clone(),
            Clock: self.Clock,
        }
    }
    pub fn ExportChunk(&self) -> EngineChunk<W::Chunk, I> {
        EngineChunk {
            Runtime: self.Session.ExportChunk(),
            World: self.World.ExportChunk(),
            Player: self.Player,
            Guard: self.Guard,
            Clock: self.Clock,
            InputQueue: self.InputQueue.clone(),
        }
    }
    pub fn FromChunk(
        chunk: EngineChunk<W::Chunk, I>,
        build_registry: fn() -> crate::DwFrameRegistry,
        root_frame: DwFrameId,
        input_to_message: fn(I) -> DwMessage,
    ) -> Self {
        let session = DwSession::FromChunk(build_registry(), chunk.Runtime)
            .expect("WyrmCoil session restore should succeed");
        Self {
            Session: session,
            World: W::FromChunk(chunk.World),
            Player: chunk.Player,
            Guard: chunk.Guard,
            Clock: chunk.Clock,
            InputQueue: chunk.InputQueue,
            _root_frame: root_frame,
            _build_registry: build_registry,
            input_to_message,
        }
    }
}

impl Engine<crate::Demo::World, crate::Demo::InputEvent> {
    pub fn New() -> Self {
        let mut world = crate::Demo::World::New();
        let _player = world.SpawnEntity(crate::Engine::Vec2::Zero(), 100.0);
        let _guard = world.SpawnEntity(crate::Engine::Vec2 { X: 5.0, Y: 5.0 }, 80.0);
        Engine::Construct(
            crate::Demo::BuildRegistry,
            crate::Demo::Frames::Root,
            world,
            crate::Demo::InputToMessage,
        )
    }
    pub fn FromDemoChunk(
        chunk: EngineChunk<crate::Demo::WorldChunk, crate::Demo::InputEvent>,
    ) -> Self {
        Engine::FromChunk(
            chunk,
            crate::Demo::BuildRegistry,
            crate::Demo::Frames::Root,
            crate::Demo::InputToMessage,
        )
    }
}
