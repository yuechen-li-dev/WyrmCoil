#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use crate::Engine::primitives::{EntityId, RenderItem, RenderSnapshot, Vec2};
use crate::{
    Dw, DwActRequest, DwControl, DwFrameCtx, DwFrameDef, DwFrameId, DwFrameRegistry, DwKey,
    DwMessage, DwPhase,
};

#[derive(Clone, Debug, PartialEq)]
pub struct TransformStore {
    pub Positions: Vec<Vec2>,
    pub Velocities: Vec<Vec2>,
    pub Alive: Vec<bool>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HealthStore {
    pub Health: Vec<f32>,
}
impl HealthStore {
    pub fn New() -> Self {
        Self { Health: Vec::new() }
    }
    pub fn Spawn(&mut self, health: f32) {
        self.Health.push(health);
    }
    pub fn SetHealth(&mut self, id: EntityId, health: f32) {
        if id.0 < self.Health.len() {
            self.Health[id.0] = health;
        }
    }
    pub fn ExportChunk(&self) -> HealthStoreChunk {
        HealthStoreChunk {
            Health: self.Health.clone(),
        }
    }
    pub fn FromChunk(chunk: HealthStoreChunk) -> Self {
        Self {
            Health: chunk.Health,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct HealthStoreChunk {
    pub Health: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderableStore {
    pub SpriteIds: Vec<u32>,
    pub Visible: Vec<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderableStoreChunk {
    pub SpriteIds: Vec<u32>,
    pub Visible: Vec<bool>,
}

impl RenderableStore {
    pub fn New() -> Self {
        Self {
            SpriteIds: Vec::new(),
            Visible: Vec::new(),
        }
    }

    pub fn Spawn(&mut self, sprite_id: u32) {
        self.SpriteIds.push(sprite_id);
        self.Visible.push(true);
    }

    pub fn SetSprite(&mut self, id: EntityId, sprite_id: u32) {
        if id.0 < self.SpriteIds.len() {
            self.SpriteIds[id.0] = sprite_id;
        }
    }

    pub fn SetVisible(&mut self, id: EntityId, visible: bool) {
        if id.0 < self.Visible.len() {
            self.Visible[id.0] = visible;
        }
    }

    pub fn ExportChunk(&self) -> RenderableStoreChunk {
        RenderableStoreChunk {
            SpriteIds: self.SpriteIds.clone(),
            Visible: self.Visible.clone(),
        }
    }

    pub fn FromChunk(chunk: RenderableStoreChunk) -> Self {
        Self {
            SpriteIds: chunk.SpriteIds,
            Visible: chunk.Visible,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransformStoreChunk {
    pub Positions: Vec<Vec2>,
    pub Velocities: Vec<Vec2>,
    pub Alive: Vec<bool>,
}

impl TransformStore {
    pub fn New() -> Self {
        Self {
            Positions: Vec::new(),
            Velocities: Vec::new(),
            Alive: Vec::new(),
        }
    }
    pub fn Spawn(&mut self, position: Vec2) -> EntityId {
        let id = EntityId(self.Positions.len());
        self.Positions.push(position);
        self.Velocities.push(Vec2::Zero());
        self.Alive.push(true);
        id
    }
    pub fn SetAlive(&mut self, id: EntityId, alive: bool) {
        if id.0 < self.Alive.len() {
            self.Alive[id.0] = alive;
        }
    }
    pub fn SetVelocity(&mut self, id: EntityId, velocity: Vec2) {
        if id.0 < self.Velocities.len() && self.Alive[id.0] {
            self.Velocities[id.0] = velocity;
        }
    }
    pub fn Position(&self, id: EntityId) -> Option<Vec2> {
        self.Positions.get(id.0).copied()
    }
    pub fn Velocity(&self, id: EntityId) -> Option<Vec2> {
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
    pub fn ExportChunk(&self) -> TransformStoreChunk {
        TransformStoreChunk {
            Positions: self.Positions.clone(),
            Velocities: self.Velocities.clone(),
            Alive: self.Alive.clone(),
        }
    }
    pub fn FromChunk(chunk: TransformStoreChunk) -> Self {
        Self {
            Positions: chunk.Positions,
            Velocities: chunk.Velocities,
            Alive: chunk.Alive,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DemoWorld {
    pub Transforms: TransformStore,
    pub Health: HealthStore,
    pub Renderables: RenderableStore,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DemoWorldChunk {
    pub Transforms: TransformStoreChunk,
    pub Health: HealthStoreChunk,
    pub Renderables: RenderableStoreChunk,
}
impl DemoWorld {
    pub fn New() -> Self {
        Self {
            Transforms: TransformStore::New(),
            Health: HealthStore::New(),
            Renderables: RenderableStore::New(),
        }
    }
    pub fn SpawnEntity(&mut self, position: Vec2, health: f32) -> EntityId {
        let entity = self.Transforms.Spawn(position);
        self.Health.Spawn(health);
        self.Renderables.Spawn(0);
        entity
    }

    pub fn ExtractRenderItems(&self) -> Vec<RenderItem> {
        let mut items = Vec::new();
        let count = self
            .Transforms
            .Positions
            .len()
            .min(self.Transforms.Alive.len())
            .min(self.Renderables.SpriteIds.len())
            .min(self.Renderables.Visible.len());
        for index in 0..count {
            if !self.Transforms.Alive[index] || !self.Renderables.Visible[index] {
                continue;
            }
            items.push(RenderItem {
                Entity: EntityId(index),
                Position: self.Transforms.Positions[index],
                SpriteId: self.Renderables.SpriteIds[index],
            });
        }
        items
    }
    pub fn FindLowestHealthAliveEntity(&self) -> Option<EntityId> {
        let mut selected: Option<EntityId> = None;
        let mut selected_health = 0.0_f32;
        let count = self.Transforms.Alive.len().min(self.Health.Health.len());
        for index in 0..count {
            if !self.Transforms.Alive[index] {
                continue;
            }
            let health = self.Health.Health[index];
            if selected.is_none() || health < selected_health {
                selected = Some(EntityId(index));
                selected_health = health;
            }
        }
        selected
    }
    pub fn RefreshSelectionBoard(&self, board: &mut crate::DwBoard) {
        let selected = self.FindLowestHealthAliveEntity();
        if let Some(entity) = selected {
            board
                .Set(Keys::HasSelection, true)
                .expect("selection flag should write when query finds alive entity");
            board
                .Set(Keys::SelectedEntity, entity.0 as i32)
                .expect("selected entity should write when query finds alive entity");
            board
                .Set(Keys::SelectedHealth, self.Health.Health[entity.0])
                .expect("selected health should write when query finds alive entity");
        } else {
            board
                .Set(Keys::HasSelection, false)
                .expect("selection flag should write when query does not find alive entity");
            board
                .Set(Keys::SelectedEntity, -1)
                .expect("selected entity sentinel should write when query finds no alive entity");
            board
                .Set(Keys::SelectedHealth, -1.0)
                .expect("selected health sentinel should write when query finds no alive entity");
        }
    }
    pub fn Tick(&mut self) {
        self.Transforms.Tick();
    }
    pub fn ExportChunk(&self) -> DemoWorldChunk {
        DemoWorldChunk {
            Transforms: self.Transforms.ExportChunk(),
            Health: self.Health.ExportChunk(),
            Renderables: self.Renderables.ExportChunk(),
        }
    }
    pub fn FromChunk(chunk: DemoWorldChunk) -> Self {
        Self {
            Transforms: TransformStore::FromChunk(chunk.Transforms),
            Health: HealthStore::FromChunk(chunk.Health),
            Renderables: RenderableStore::FromChunk(chunk.Renderables),
        }
    }
}

pub mod Frames {
    use crate::DwFrameId;
    pub const Domain: u64 = 310;
    pub const Root: DwFrameId = DwFrameId { Domain, Local: 1 };
    pub const Player: DwFrameId = DwFrameId { Domain, Local: 2 };
    pub const Guard: DwFrameId = DwFrameId { Domain, Local: 3 };
}
pub mod Acts {
    use crate::DwActId;
    pub const Domain: u64 = 311;
    pub const ApplyVelocityCommand: DwActId = DwActId { Domain, Local: 1 };
    pub const NudgeEntityCommand: DwActId = DwActId { Domain, Local: 2 };
    pub const GuardStep: DwActId = DwActId { Domain, Local: 3 };
}
pub mod Keys {
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
pub mod MailKinds {
    pub const MovePlayerRight: u32 = 1;
    pub const MovePlayerLeft: u32 = 2;
    pub const StopPlayer: u32 = 3;
    pub const AlertGuard: u32 = 4;
    pub const NudgeGuardUp: u32 = 5;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputEvent {
    MoveRightPressed,
    MoveLeftPressed,
    StopPressed,
    AlertGuardPressed,
    NudgeGuardPressed,
}

pub fn MoveRightMessage() -> DwMessage {
    DwMessage::I32(MailKinds::MovePlayerRight, 1)
}
pub fn MoveLeftMessage() -> DwMessage {
    DwMessage::I32(MailKinds::MovePlayerLeft, 1)
}
pub fn StopMessage() -> DwMessage {
    DwMessage::I32(MailKinds::StopPlayer, 1)
}
pub fn AlertGuardMessage() -> DwMessage {
    DwMessage::I32(MailKinds::AlertGuard, 1)
}
pub fn NudgeGuardMessage() -> DwMessage {
    DwMessage::I32(MailKinds::NudgeGuardUp, 1)
}
pub fn InputToMessage(event: InputEvent) -> DwMessage {
    match event {
        InputEvent::MoveRightPressed => MoveRightMessage(),
        InputEvent::MoveLeftPressed => MoveLeftMessage(),
        InputEvent::StopPressed => StopMessage(),
        InputEvent::AlertGuardPressed => AlertGuardMessage(),
        InputEvent::NudgeGuardPressed => NudgeGuardMessage(),
    }
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
        Some(RootPhase::Player) => Dw::Push(Frames::Player, RootPhase::Guard),
        Some(RootPhase::Guard) => Dw::Push(Frames::Guard, RootPhase::Loop),
        Some(RootPhase::Loop) => Dw::Continue(RootPhase::Player),
        None => Dw::Fail("wyrmcoil root phase invalid"),
    }
}

fn QueueVelocityCommand(ctx: &mut DwFrameCtx, entity: i32, x: f32, y: f32) {
    ctx.BoardMut()
        .Set(Keys::CommandEntity, entity)
        .expect("command entity key write should succeed");
    ctx.BoardMut()
        .Set(Keys::CommandVelocityX, x)
        .expect("command velocity x key write should succeed");
    ctx.BoardMut()
        .Set(Keys::CommandVelocityY, y)
        .expect("command velocity y key write should succeed");
    ctx.Immediate(Acts::ApplyVelocityCommand);
}

fn Player(ctx: &mut DwFrameCtx) -> DwControl {
    match ctx.Phase::<UnitPhase>() {
        Some(UnitPhase::Enter) => {
            while let Some(message) = ctx.MailboxMut().ConsumeFront() {
                if message.Kind == MailKinds::MovePlayerRight {
                    QueueVelocityCommand(ctx, 0, 1.0, 0.0);
                } else if message.Kind == MailKinds::MovePlayerLeft {
                    QueueVelocityCommand(ctx, 0, -1.0, 0.0);
                } else if message.Kind == MailKinds::StopPlayer {
                    QueueVelocityCommand(ctx, 0, 0.0, 0.0);
                } else if message.Kind == MailKinds::AlertGuard {
                    ctx.BoardMut()
                        .Set(Keys::GuardAlert, true)
                        .expect("guard alert key write should succeed");
                } else if message.Kind == MailKinds::NudgeGuardUp {
                    ctx.BoardMut()
                        .Set(Keys::CommandEntity, 1)
                        .expect("nudge command entity write should succeed");
                    ctx.BoardMut()
                        .Set(Keys::CommandDeltaX, 0.0)
                        .expect("nudge command delta x write should succeed");
                    ctx.BoardMut()
                        .Set(Keys::CommandDeltaY, 2.0)
                        .expect("nudge command delta y write should succeed");
                    ctx.Immediate(Acts::NudgeEntityCommand);
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
            if ctx.Board().GetOr(Keys::HasSelection, false) {
                let selected_entity = ctx.Board().GetOr(Keys::SelectedEntity, -1);
                if selected_entity >= 0 {
                    ctx.BoardMut()
                        .Set(Keys::CommandEntity, selected_entity)
                        .expect("guard query selected entity write should succeed");
                    velocity_x = 0.5;
                    velocity_y = 0.5;
                }
            } else {
                ctx.BoardMut()
                    .Set(Keys::CommandEntity, 1)
                    .expect("guard fallback command entity write should succeed");
            }
            QueueVelocityCommand(
                ctx,
                ctx.Board().GetOr(Keys::CommandEntity, 1),
                velocity_x,
                velocity_y,
            );
            if ctx.Board().GetOr(Keys::GuardAlert, false)
                && ctx.Board().GetOr(Keys::HasSelection, false)
            {
                ctx.BoardMut()
                    .Set(Keys::CommandVelocityX, 1.0)
                    .expect("guard command velocity x write should succeed");
                ctx.BoardMut()
                    .Set(Keys::CommandVelocityY, 1.0)
                    .expect("guard command velocity y write should succeed");
                ctx.Deferred(Acts::ApplyVelocityCommand, 1);
            }
            ctx.Immediate(Acts::GuardStep);
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
            Id: Frames::Root,
            Step: Root,
            DebugName: "Root",
        })
        .expect("Root should register exactly once");
    registry
        .Register(DwFrameDef {
            Id: Frames::Player,
            Step: Player,
            DebugName: "Player",
        })
        .expect("Player should register exactly once");
    registry
        .Register(DwFrameDef {
            Id: Frames::Guard,
            Step: Guard,
            DebugName: "Guard",
        })
        .expect("Guard should register exactly once");
    registry
}

pub fn DispatchActs(world: &mut World, board: &crate::DwBoard, acts: &[DwActRequest]) {
    for act in acts {
        if act.Id == Acts::ApplyVelocityCommand {
            let entity = board.GetOr(Keys::CommandEntity, -1);
            let velocity_x = board.GetOr(Keys::CommandVelocityX, 0.0);
            let velocity_y = board.GetOr(Keys::CommandVelocityY, 0.0);
            if entity >= 0 {
                world.Transforms.SetVelocity(
                    EntityId(entity as usize),
                    Vec2 {
                        X: velocity_x,
                        Y: velocity_y,
                    },
                );
            }
        } else if act.Id == Acts::NudgeEntityCommand {
            let entity = board.GetOr(Keys::CommandEntity, -1);
            let delta_x = board.GetOr(Keys::CommandDeltaX, 0.0);
            let delta_y = board.GetOr(Keys::CommandDeltaY, 0.0);
            if entity >= 0 {
                let target = EntityId(entity as usize);
                if let Some(position) = world.Transforms.Position(target) {
                    if target.0 < world.Transforms.Alive.len() && world.Transforms.Alive[target.0] {
                        world.Transforms.Positions[target.0] = Vec2 {
                            X: position.X + delta_x,
                            Y: position.Y + delta_y,
                        };
                    }
                }
            }
        }
    }
}

impl crate::Engine::wyrmcoil::World for DemoWorld {
    type Chunk = DemoWorldChunk;
    fn RefreshBoard(&self, board: &mut crate::DwBoard) {
        self.RefreshSelectionBoard(board);
    }
    fn DispatchActs(&mut self, board: &crate::DwBoard, acts: &[DwActRequest]) {
        DispatchActs(self, board, acts);
    }
    fn Tick(&mut self) {
        DemoWorld::Tick(self);
    }
    fn ExtractRenderSnapshot(&self, frame: u64) -> RenderSnapshot {
        RenderSnapshot {
            Frame: frame,
            Items: self.ExtractRenderItems(),
        }
    }
    fn ExportChunk(&self) -> Self::Chunk {
        self.ExportChunk()
    }
    fn FromChunk(chunk: Self::Chunk) -> Self {
        DemoWorld::FromChunk(chunk)
    }
}

pub type World = DemoWorld;
pub type WorldChunk = DemoWorldChunk;
