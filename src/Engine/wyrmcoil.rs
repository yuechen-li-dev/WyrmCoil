#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use wyrmcoil::{
    Dw, DwActRequest, DwControl, DwFrameCtx, DwFrameDef, DwFrameRegistry, DwKey, DwMessage,
    DwPhase, DwRuntimeChunk, DwSession, DwTickResult,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WcVec2 {
    pub X: f32,
    pub Y: f32,
}

impl WcVec2 {
    pub fn Zero() -> Self {
        Self { X: 0.0, Y: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WcEntityId(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub struct WcTransformStore {
    pub Positions: Vec<WcVec2>,
    pub Velocities: Vec<WcVec2>,
    pub Alive: Vec<bool>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WcHealthStore {
    pub Health: Vec<f32>,
}
impl WcHealthStore {
    pub fn New() -> Self {
        Self { Health: Vec::new() }
    }
    pub fn Spawn(&mut self, health: f32) {
        self.Health.push(health);
    }
    pub fn SetHealth(&mut self, id: WcEntityId, health: f32) {
        if id.0 < self.Health.len() {
            self.Health[id.0] = health;
        }
    }
    pub fn ExportChunk(&self) -> WcHealthStoreChunk {
        WcHealthStoreChunk {
            Health: self.Health.clone(),
        }
    }
    pub fn FromChunk(chunk: WcHealthStoreChunk) -> Self {
        Self {
            Health: chunk.Health,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct WcHealthStoreChunk {
    pub Health: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WcTransformStoreChunk {
    pub Positions: Vec<WcVec2>,
    pub Velocities: Vec<WcVec2>,
    pub Alive: Vec<bool>,
}

impl WcTransformStore {
    pub fn New() -> Self {
        Self {
            Positions: Vec::new(),
            Velocities: Vec::new(),
            Alive: Vec::new(),
        }
    }
    pub fn Spawn(&mut self, position: WcVec2) -> WcEntityId {
        let id = WcEntityId(self.Positions.len());
        self.Positions.push(position);
        self.Velocities.push(WcVec2::Zero());
        self.Alive.push(true);
        id
    }
    pub fn SetAlive(&mut self, id: WcEntityId, alive: bool) {
        if id.0 < self.Alive.len() {
            self.Alive[id.0] = alive;
        }
    }
    pub fn SetVelocity(&mut self, id: WcEntityId, velocity: WcVec2) {
        if id.0 < self.Velocities.len() && self.Alive[id.0] {
            self.Velocities[id.0] = velocity;
        }
    }
    pub fn Position(&self, id: WcEntityId) -> Option<WcVec2> {
        self.Positions.get(id.0).copied()
    }
    pub fn Velocity(&self, id: WcEntityId) -> Option<WcVec2> {
        self.Velocities.get(id.0).copied()
    }
    pub fn Tick(&mut self) {
        for index in 0..self.Positions.len() {
            if self.Alive[index] {
                self.Positions[index].X += self.Velocities[index].X;
                self.Positions[index].Y += self.Velocities[index].Y;
            }
        }
    }
    pub fn ExportChunk(&self) -> WcTransformStoreChunk {
        WcTransformStoreChunk {
            Positions: self.Positions.clone(),
            Velocities: self.Velocities.clone(),
            Alive: self.Alive.clone(),
        }
    }
    pub fn FromChunk(chunk: WcTransformStoreChunk) -> Self {
        Self {
            Positions: chunk.Positions,
            Velocities: chunk.Velocities,
            Alive: chunk.Alive,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WcWorld {
    pub Transforms: WcTransformStore,
    pub Health: WcHealthStore,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WcWorldChunk {
    pub Transforms: WcTransformStoreChunk,
    pub Health: WcHealthStoreChunk,
}
impl WcWorld {
    pub fn New() -> Self {
        Self {
            Transforms: WcTransformStore::New(),
            Health: WcHealthStore::New(),
        }
    }
    pub fn SpawnEntity(&mut self, position: WcVec2, health: f32) -> WcEntityId {
        let entity = self.Transforms.Spawn(position);
        self.Health.Spawn(health);
        entity
    }
    pub fn FindLowestHealthAliveEntity(&self) -> Option<WcEntityId> {
        let mut selected: Option<WcEntityId> = None;
        let mut selected_health = 0.0_f32;
        let count = self.Transforms.Alive.len().min(self.Health.Health.len());
        for index in 0..count {
            if !self.Transforms.Alive[index] {
                continue;
            }
            let health = self.Health.Health[index];
            if selected.is_none() || health < selected_health {
                selected = Some(WcEntityId(index));
                selected_health = health;
            }
        }
        selected
    }
    pub fn RefreshSelectionBoard(&self, board: &mut wyrmcoil::DwBoard) {
        let selected = self.FindLowestHealthAliveEntity();
        if let Some(entity) = selected {
            board
                .Set(WcKeys::HasSelection, true)
                .expect("selection flag should write when query finds alive entity");
            board
                .Set(WcKeys::SelectedEntity, entity.0 as i32)
                .expect("selected entity should write when query finds alive entity");
            board
                .Set(WcKeys::SelectedHealth, self.Health.Health[entity.0])
                .expect("selected health should write when query finds alive entity");
        } else {
            board
                .Set(WcKeys::HasSelection, false)
                .expect("selection flag should write when query does not find alive entity");
            board
                .Set(WcKeys::SelectedEntity, -1)
                .expect("selected entity sentinel should write when query finds no alive entity");
            board
                .Set(WcKeys::SelectedHealth, -1.0)
                .expect("selected health sentinel should write when query finds no alive entity");
        }
    }
    pub fn Tick(&mut self) {
        self.Transforms.Tick();
    }
    pub fn ExportChunk(&self) -> WcWorldChunk {
        WcWorldChunk {
            Transforms: self.Transforms.ExportChunk(),
            Health: self.Health.ExportChunk(),
        }
    }
    pub fn FromChunk(chunk: WcWorldChunk) -> Self {
        Self {
            Transforms: WcTransformStore::FromChunk(chunk.Transforms),
            Health: WcHealthStore::FromChunk(chunk.Health),
        }
    }
}

pub mod WcFrames {
    use wyrmcoil::DwFrameId;
    pub const Domain: u64 = 310;
    pub const Root: DwFrameId = DwFrameId { Domain, Local: 1 };
    pub const Player: DwFrameId = DwFrameId { Domain, Local: 2 };
    pub const Guard: DwFrameId = DwFrameId { Domain, Local: 3 };
}
pub mod WcActs {
    use wyrmcoil::DwActId;
    pub const Domain: u64 = 311;
    pub const ApplyVelocityCommand: DwActId = DwActId { Domain, Local: 1 };
    pub const NudgeEntityCommand: DwActId = DwActId { Domain, Local: 2 };
    pub const GuardStep: DwActId = DwActId { Domain, Local: 3 };
}
pub mod WcKeys {
    use super::DwKey;
    pub const GuardAlert: DwKey<bool> = DwKey::New("GuardAlert", 20);
    pub const CommandEntity: DwKey<i32> = DwKey::New("CommandEntity", 21);
    pub const CommandVelocityX: DwKey<f32> = DwKey::New("CommandVelocityX", 22);
    pub const CommandVelocityY: DwKey<f32> = DwKey::New("CommandVelocityY", 23);
    pub const CommandDeltaX: DwKey<f32> = DwKey::New("CommandDeltaX", 24);
    pub const CommandDeltaY: DwKey<f32> = DwKey::New("CommandDeltaY", 25);
    pub const HasSelection: DwKey<bool> = DwKey::New("HasSelection", 26);
    pub const SelectedEntity: DwKey<i32> = DwKey::New("SelectedEntity", 27);
    pub const SelectedHealth: DwKey<f32> = DwKey::New("SelectedHealth", 28);
}
pub mod WcMailKinds {
    pub const MovePlayerRight: u32 = 1;
    pub const MovePlayerLeft: u32 = 2;
    pub const StopPlayer: u32 = 3;
    pub const AlertGuard: u32 = 4;
    pub const NudgeGuardUp: u32 = 5;
}

#[derive(Clone, Copy)]
enum RootPhase {
    Player,
    Guard,
    Loop,
}
impl DwPhase for RootPhase {
    fn ToPc(self) -> u32 {
        match self {
            RootPhase::Player => 0,
            RootPhase::Guard => 1,
            RootPhase::Loop => 2,
        }
    }
    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(RootPhase::Player),
            1 => Some(RootPhase::Guard),
            2 => Some(RootPhase::Loop),
            _ => None,
        }
    }
}
#[derive(Clone, Copy)]
enum UnitPhase {
    Enter,
    Finish,
}
impl DwPhase for UnitPhase {
    fn ToPc(self) -> u32 {
        match self {
            UnitPhase::Enter => 0,
            UnitPhase::Finish => 1,
        }
    }
    fn FromPc(pc: u32) -> Option<Self> {
        match pc {
            0 => Some(UnitPhase::Enter),
            1 => Some(UnitPhase::Finish),
            _ => None,
        }
    }
}

fn Root(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<RootPhase>() {
        Some(RootPhase::Player) => Dw::Push(WcFrames::Player, RootPhase::Guard),
        Some(RootPhase::Guard) => Dw::Push(WcFrames::Guard, RootPhase::Loop),
        Some(RootPhase::Loop) => Dw::Continue(RootPhase::Player),
        None => Dw::Fail("wyrmcoil root phase invalid"),
    }
}

fn QueueVelocityCommand(ctx: &mut DwFrameCtx, entity: i32, x: f32, y: f32) {
    ctx.BoardMut()
        .Set(WcKeys::CommandEntity, entity)
        .expect("command entity key write should succeed");
    ctx.BoardMut()
        .Set(WcKeys::CommandVelocityX, x)
        .expect("command velocity x key write should succeed");
    ctx.BoardMut()
        .Set(WcKeys::CommandVelocityY, y)
        .expect("command velocity y key write should succeed");
    ctx.Immediate(WcActs::ApplyVelocityCommand);
}

fn Player(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<UnitPhase>() {
        Some(UnitPhase::Enter) => {
            while let Some(message) = ctx.MailboxMut().ConsumeFront() {
                if message.Kind == WcMailKinds::MovePlayerRight {
                    QueueVelocityCommand(ctx, 0, 1.0, 0.0);
                } else if message.Kind == WcMailKinds::MovePlayerLeft {
                    QueueVelocityCommand(ctx, 0, -1.0, 0.0);
                } else if message.Kind == WcMailKinds::StopPlayer {
                    QueueVelocityCommand(ctx, 0, 0.0, 0.0);
                } else if message.Kind == WcMailKinds::AlertGuard {
                    ctx.BoardMut()
                        .Set(WcKeys::GuardAlert, true)
                        .expect("guard alert key write should succeed");
                } else if message.Kind == WcMailKinds::NudgeGuardUp {
                    ctx.BoardMut()
                        .Set(WcKeys::CommandEntity, 1)
                        .expect("nudge command entity write should succeed");
                    ctx.BoardMut()
                        .Set(WcKeys::CommandDeltaX, 0.0)
                        .expect("nudge command delta x write should succeed");
                    ctx.BoardMut()
                        .Set(WcKeys::CommandDeltaY, 2.0)
                        .expect("nudge command delta y write should succeed");
                    ctx.Immediate(WcActs::NudgeEntityCommand);
                }
            }
            Dw::Continue(UnitPhase::Finish)
        }
        Some(UnitPhase::Finish) => Dw::Pop(),
        None => Dw::Fail("wyrmcoil player phase invalid"),
    }
}
fn Guard(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<UnitPhase>() {
        Some(UnitPhase::Enter) => {
            let mut velocity_x = 0.0;
            let mut velocity_y = 1.0;
            if ctx.Board().GetOr(WcKeys::HasSelection, false) {
                let selected_entity = ctx.Board().GetOr(WcKeys::SelectedEntity, -1);
                if selected_entity >= 0 {
                    ctx.BoardMut()
                        .Set(WcKeys::CommandEntity, selected_entity)
                        .expect("guard query selected entity write should succeed");
                    velocity_x = 0.5;
                    velocity_y = 0.5;
                }
            } else {
                ctx.BoardMut()
                    .Set(WcKeys::CommandEntity, 1)
                    .expect("guard fallback command entity write should succeed");
            }
            QueueVelocityCommand(
                ctx,
                ctx.Board().GetOr(WcKeys::CommandEntity, 1),
                velocity_x,
                velocity_y,
            );
            if ctx.Board().GetOr(WcKeys::GuardAlert, false)
                && ctx.Board().GetOr(WcKeys::HasSelection, false)
            {
                ctx.BoardMut()
                    .Set(WcKeys::CommandVelocityX, 1.0)
                    .expect("guard command velocity x write should succeed");
                ctx.BoardMut()
                    .Set(WcKeys::CommandVelocityY, 1.0)
                    .expect("guard command velocity y write should succeed");
                ctx.Deferred(WcActs::ApplyVelocityCommand, 1);
            }
            ctx.Immediate(WcActs::GuardStep);
            Dw::Continue(UnitPhase::Finish)
        }
        Some(UnitPhase::Finish) => Dw::Pop(),
        None => Dw::Fail("wyrmcoil guard phase invalid"),
    }
}

pub fn BuildRegistry() -> DwFrameRegistry {
    let mut registry = DwFrameRegistry::New();
    registry
        .Register(DwFrameDef {
            Id: WcFrames::Root,
            Step: Root,
            DebugName: "WcRoot",
        })
        .expect("WcRoot should register exactly once");
    registry
        .Register(DwFrameDef {
            Id: WcFrames::Player,
            Step: Player,
            DebugName: "WcPlayer",
        })
        .expect("WcPlayer should register exactly once");
    registry
        .Register(DwFrameDef {
            Id: WcFrames::Guard,
            Step: Guard,
            DebugName: "WcGuard",
        })
        .expect("WcGuard should register exactly once");
    registry
}

pub fn DispatchActs(world: &mut WcWorld, board: &wyrmcoil::DwBoard, acts: &[DwActRequest]) {
    for act in acts {
        if act.Id == WcActs::ApplyVelocityCommand {
            let entity = board.GetOr(WcKeys::CommandEntity, -1);
            let velocity_x = board.GetOr(WcKeys::CommandVelocityX, 0.0);
            let velocity_y = board.GetOr(WcKeys::CommandVelocityY, 0.0);
            if entity >= 0 {
                world.Transforms.SetVelocity(
                    WcEntityId(entity as usize),
                    WcVec2 {
                        X: velocity_x,
                        Y: velocity_y,
                    },
                );
            }
        } else if act.Id == WcActs::NudgeEntityCommand {
            let entity = board.GetOr(WcKeys::CommandEntity, -1);
            let delta_x = board.GetOr(WcKeys::CommandDeltaX, 0.0);
            let delta_y = board.GetOr(WcKeys::CommandDeltaY, 0.0);
            if entity >= 0 {
                let target = WcEntityId(entity as usize);
                if let Some(position) = world.Transforms.Position(target) {
                    if target.0 < world.Transforms.Alive.len() && world.Transforms.Alive[target.0] {
                        world.Transforms.Positions[target.0] = WcVec2 {
                            X: position.X + delta_x,
                            Y: position.Y + delta_y,
                        };
                    }
                }
            }
        }
    }
}

pub struct WcEngine {
    pub Session: DwSession,
    pub World: WcWorld,
    pub Player: WcEntityId,
    pub Guard: WcEntityId,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WcEngineChunk {
    pub Runtime: DwRuntimeChunk,
    pub World: WcWorldChunk,
    pub Player: WcEntityId,
    pub Guard: WcEntityId,
}
pub struct WcTickResult {
    pub Runtime: DwTickResult,
    pub World: WcWorld,
}

impl WcEngine {
    pub fn New() -> Self {
        let mut world = WcWorld::New();
        let player = world.SpawnEntity(WcVec2::Zero(), 100.0);
        let guard = world.SpawnEntity(WcVec2 { X: 5.0, Y: 5.0 }, 80.0);
        let session = DwSession::New(BuildRegistry(), WcFrames::Root, 0)
            .expect("WyrmCoil session should construct");
        Self {
            Session: session,
            World: world,
            Player: player,
            Guard: guard,
        }
    }
    pub fn Tick(&mut self) -> WcTickResult {
        self.World.RefreshSelectionBoard(self.Session.BoardMut());
        let runtime = self
            .Session
            .Tick()
            .expect("WyrmCoil engine tick should succeed");
        DispatchActs(
            &mut self.World,
            self.Session.Board(),
            &runtime.ImmediateActs,
        );
        DispatchActs(
            &mut self.World,
            self.Session.Board(),
            &runtime.MaturedDeferredActs,
        );
        self.World.Tick();
        WcTickResult {
            Runtime: runtime,
            World: self.World.clone(),
        }
    }
    pub fn ExportChunk(&self) -> WcEngineChunk {
        WcEngineChunk {
            Runtime: self.Session.ExportChunk(),
            World: self.World.ExportChunk(),
            Player: self.Player,
            Guard: self.Guard,
        }
    }
    pub fn FromChunk(chunk: WcEngineChunk) -> Self {
        let session = DwSession::FromChunk(BuildRegistry(), chunk.Runtime)
            .expect("WyrmCoil session restore should succeed");
        Self {
            Session: session,
            World: WcWorld::FromChunk(chunk.World),
            Player: chunk.Player,
            Guard: chunk.Guard,
        }
    }
}

pub fn MoveRightMessage() -> DwMessage {
    DwMessage {
        Kind: WcMailKinds::MovePlayerRight,
        Value: 1,
    }
}
pub fn MoveLeftMessage() -> DwMessage {
    DwMessage {
        Kind: WcMailKinds::MovePlayerLeft,
        Value: 1,
    }
}
pub fn StopMessage() -> DwMessage {
    DwMessage {
        Kind: WcMailKinds::StopPlayer,
        Value: 1,
    }
}
pub fn AlertGuardMessage() -> DwMessage {
    DwMessage {
        Kind: WcMailKinds::AlertGuard,
        Value: 1,
    }
}
pub fn NudgeGuardMessage() -> DwMessage {
    DwMessage {
        Kind: WcMailKinds::NudgeGuardUp,
        Value: 1,
    }
}
