use dunewyrm::brain::{
    Agent, Brain, Context, Decision, DecisionPolicy, DecisionReport, Schedule, Step, Transition,
    TransitionKind, Utility,
};
use wyrmcoil_game::{AgentDecision, AgentInspection, AgentTransition, CommandOutcome, ControlMode};

use super::{
    Actor, Cell, Distance, Order, PARTY, STARTS, STATS, STEPS, Subtract, Visible, Walkable,
};

pub(super) const HEAVY_CHANGED: u64 = 1;

#[derive(Clone, Copy)]
pub(super) struct Fact {
    cell: Cell,
    facing: Cell,
    target: Option<usize>,
    health: u16,
    strike: u16,
    cooldown: u16,
    stun: u16,
    slow: u16,
    block: u16,
    travel: u16,
}

impl From<&Actor> for Fact {
    fn from(actor: &Actor) -> Self {
        Self {
            cell: actor.cell,
            facing: actor.facing,
            target: actor.target,
            health: actor.health,
            strike: actor.strike,
            cooldown: actor.cooldown,
            stun: actor.stun,
            slow: actor.slow,
            block: actor.block,
            travel: actor.travel,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Battle {
    pub actors: [Fact; 6],
    pub leader: Option<usize>,
    previous_proposals: [Option<(Order, CommandOutcome)>; PARTY],
}

impl Battle {
    pub fn Read(
        actors: &[Actor; 6],
        leader: Option<usize>,
        previous_proposals: [Option<(Order, CommandOutcome)>; PARTY],
    ) -> Self {
        Self {
            actors: actors.each_ref().map(Fact::from),
            leader,
            previous_proposals,
        }
    }

    pub fn Heavy(&self, index: usize) -> bool {
        let captain = self.actors[5];
        let actor = self.actors[index];
        captain.health > 0
            && captain.stun == 0
            && captain.travel <= 2
            && captain.strike <= 2
            && captain.target == Some(index)
            && actor.block == 0
            && actor.travel == 0
            && actor.stun == 0
            && Distance(actor.cell, captain.cell) <= STATS[5].range
            && Visible(actor.cell, captain.cell)
    }

    fn LiveEnemy(&self, target: usize) -> bool {
        target >= PARTY
            && self
                .actors
                .get(target)
                .is_some_and(|actor| actor.health > 0)
    }

    fn Near(&self, index: usize) -> Option<usize> {
        let cell = self.actors[index].cell;
        (PARTY..6)
            .filter(|&id| self.LiveEnemy(id))
            .min_by_key(|&id| (Distance(cell, self.actors[id].cell), id))
    }

    fn MageThreat(&self) -> Option<usize> {
        let mage = self.actors[2];
        if mage.health == 0 {
            return None;
        }
        (PARTY..6)
            .filter(|&id| {
                let enemy = self.actors[id];
                self.LiveEnemy(id)
                    && (enemy.target == Some(2) || Distance(enemy.cell, mage.cell) <= 2)
            })
            .min_by_key(|&id| {
                (
                    self.actors[id].stun > 0,
                    Distance(self.actors[id].cell, mage.cell),
                    id,
                )
            })
    }

    fn CanStand(&self, index: usize, cell: Cell) -> bool {
        Walkable(cell)
            && !self
                .actors
                .iter()
                .enumerate()
                .any(|(other, a)| other != index && a.health > 0 && a.cell == cell)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(super) enum Role {
    Alda,
    Merrit,
    Vey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    Root,
    ProtectParty,
    Engage,
    Retreat,
    Regroup,
    Flank,
    Assassinate,
    Evade,
    MaintainDistance,
    CrowdControl,
    ProtectSelf,
    SelectTarget,
    SelectOpportunity,
    Approach,
    FindFlankingPosition,
    FindPosition,
    AttackCycle,
    RangedAttack,
    ShieldBash,
    Backstab,
    FrostBolt,
    BlockThreat,
    Recover,
}

const STATES: &[State] = &[
    State::Root,
    State::ProtectParty,
    State::Engage,
    State::Retreat,
    State::Regroup,
    State::Flank,
    State::Assassinate,
    State::Evade,
    State::MaintainDistance,
    State::CrowdControl,
    State::ProtectSelf,
    State::SelectTarget,
    State::SelectOpportunity,
    State::Approach,
    State::FindFlankingPosition,
    State::FindPosition,
    State::AttackCycle,
    State::RangedAttack,
    State::ShieldBash,
    State::Backstab,
    State::FrostBolt,
    State::BlockThreat,
    State::Recover,
];

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Slot {
    Intent,
}

#[derive(Default)]
pub(super) struct PartyMemory {
    target: Option<usize>,
    blocking: bool,
    next_evaluation: u64,
}

#[derive(Default)]
pub(super) enum Frame {
    #[default]
    Begin,
    Ready,
    Flanking(Cell),
    Attacked,
    Recovering,
}

type PartyStep = Step<State, Slot, Order>;
pub(super) type PartyAgent = Agent<Role>;

impl Role {
    pub fn Start(self) -> PartyAgent {
        Agent::New(
            self,
            PartyMemory::default(),
            Schedule {
                KeepRootFrame: true,
                InterruptEvery: 1,
                TransitionEvery: 3,
            },
        )
        .expect("valid party brain")
    }

    fn Target(self, battle: &Battle, intent: State) -> Option<usize> {
        if matches!(intent, State::ProtectParty | State::CrowdControl) {
            if let Some(target) = battle.MageThreat() {
                return Some(target);
            }
        }
        let index = self as usize;
        let own = battle.actors[index];
        (PARTY..6)
            .filter(|&enemy| battle.LiveEnemy(enemy))
            .min_by_key(|&enemy| {
                let other = battle.actors[enemy];
                let distance = Distance(own.cell, other.cell);
                let distracted = other.target.is_some_and(|target| target != index);
                let direction = Subtract(other.cell, own.cell);
                let behind = direction[0] * other.facing[0] + direction[1] * other.facing[1] > 0;
                let isolation = (PARTY..6)
                    .filter(|&id| {
                        id != enemy
                            && battle.LiveEnemy(id)
                            && Distance(other.cell, battle.actors[id].cell) <= 3
                    })
                    .count() as i32;
                let rank = match self {
                    Self::Alda => {
                        distance
                            - if other.target.is_some_and(|id| id != index) {
                                2
                            } else {
                                0
                            }
                    }
                    Self::Merrit => {
                        distance + isolation
                            - if distracted || behind { 4 } else { 0 }
                            - if other.health <= 46 { 2 } else { 0 }
                    }
                    Self::Vey => {
                        distance + if !Visible(own.cell, other.cell) { 3 } else { 0 }
                            - if other.target == Some(2) { 4 } else { 0 }
                    }
                };
                (rank, other.health, enemy)
            })
    }

    fn Ability(self, battle: &Battle, target: usize) -> bool {
        let own = battle.actors[self as usize];
        let enemy = battle.actors[target];
        if own.cooldown != 0 {
            return false;
        }
        match self {
            Self::Alda => {
                enemy.stun == 0
                    && enemy.health > STATS[0].damage
                    && Distance(own.cell, enemy.cell) <= 2
            }
            Self::Merrit => {
                enemy.target.is_some_and(|id| id != 1) || enemy.health > STATS[1].damage
            }
            Self::Vey => enemy.slow == 0 && enemy.health > STATS[2].damage,
        }
    }

    fn Branch(self, state: State, data: &mut Frame, ctx: Context<'_, Self>) -> PartyStep {
        let index = self as usize;
        let own = ctx.Observation.actors[index];
        match state {
            State::Retreat | State::Evade | State::MaintainDistance | State::ProtectSelf => {
                return Step::Push(State::FindPosition);
            }
            State::Regroup => {
                ctx.Memory.target = None;
                if let Some(leader) = ctx.Observation.leader.filter(|id| *id != index) {
                    let anchor = ctx.Observation.actors[leader].cell;
                    if Distance(own.cell, anchor) > 3
                        && let Some(goal) = STEPS
                            .iter()
                            .map(|step| [anchor[0] + step[0], anchor[1] + step[1]])
                            .filter(|&cell| ctx.Observation.CanStand(index, cell))
                            .min_by_key(|&cell| Distance(own.cell, cell))
                    {
                        return Step::Command(Order::Move(goal));
                    }
                } else if own.cell != STARTS[index] {
                    return Step::Command(Order::Move(STARTS[index]));
                }
                return Step::Stay;
            }
            _ => {}
        }
        if matches!(data, Frame::Begin)
            || ctx
                .Memory
                .target
                .is_none_or(|id| !ctx.Observation.LiveEnemy(id))
        {
            ctx.Memory.target = self.Target(ctx.Observation, state);
            *data = Frame::Ready;
            return Step::Push(if self == Self::Merrit {
                State::SelectOpportunity
            } else {
                State::SelectTarget
            });
        }
        let target = ctx.Memory.target.unwrap();
        let enemy = ctx.Observation.actors[target];
        let _ = ctx.Child.take();
        if matches!(state, State::Flank)
            && matches!(data, Frame::Ready)
            && Distance(own.cell, enemy.cell) > 1
        {
            *data = Frame::Attacked;
            return Step::Push(State::FindFlankingPosition);
        }
        if Distance(own.cell, enemy.cell) > STATS[index].range || !Visible(own.cell, enemy.cell) {
            return Step::Push(State::Approach);
        }
        Step::Push(if self == Self::Vey {
            State::RangedAttack
        } else {
            State::AttackCycle
        })
    }

    fn AttackCycle(self, data: &mut Frame, ctx: Context<'_, Self>, target: usize) -> PartyStep {
        let own = ctx.Observation.actors[self as usize];
        if matches!(data, Frame::Recovering) {
            return Step::Succeed;
        }
        if matches!(data, Frame::Attacked) {
            *data = Frame::Recovering;
            return Step::Push(State::Recover);
        }
        if own.strike > 0 {
            return Step::Stay;
        }
        *data = Frame::Attacked;
        if self.Ability(ctx.Observation, target) {
            return Step::Push(match self {
                Self::Alda => State::ShieldBash,
                Self::Merrit => State::Backstab,
                Self::Vey => State::FrostBolt,
            });
        }
        Step::Command(Order::Attack(target))
    }

    fn Position(
        self,
        battle: &Battle,
        data: &mut Frame,
        flank: bool,
        target: Option<usize>,
    ) -> PartyStep {
        let index = self as usize;
        let own = battle.actors[index];
        let Some(target) = target
            .filter(|&id| battle.LiveEnemy(id))
            .or_else(|| battle.Near(index))
        else {
            return Step::Fail;
        };
        let opponent = battle.actors[target];
        if flank {
            let destination = if let Frame::Flanking(cell) = data {
                *cell
            } else {
                let direction = opponent.facing.map(i32::signum);
                let cell = Subtract(opponent.cell, direction);
                if direction == [0, 0] || Distance(own.cell, cell) > 4 {
                    return Step::Succeed;
                }
                *data = Frame::Flanking(cell);
                cell
            };
            if own.cell == destination || !battle.CanStand(index, destination) {
                return Step::Succeed;
            }
            return Step::Command(Order::Move(destination));
        }
        let nearest = |cell| {
            (PARTY..6)
                .filter(|&id| battle.LiveEnemy(id))
                .map(|id| Distance(cell, battle.actors[id].cell))
                .min()
                .unwrap_or(0)
        };
        let distance = nearest(own.cell);
        if distance >= 4 {
            return Step::Succeed;
        }
        for step in STEPS {
            let cell = [own.cell[0] + step[0], own.cell[1] + step[1]];
            if battle.CanStand(index, cell) && nearest(cell) > distance {
                return Step::Command(Order::Move(cell));
            }
        }
        Step::Succeed
    }
}

fn ScoreAlda(option: State, battle: &Battle, own: Fact, near: Fact, distance: i32) -> f32 {
    match option {
        State::ProtectParty => battle.MageThreat().map_or(0.0, |enemy| {
            let enemy = battle.actors[enemy];
            0.98 - Distance(own.cell, enemy.cell).min(12) as f32 * 0.014
                - if enemy.stun > 0 { 0.22 } else { 0.0 }
        }),
        State::Engage => {
            0.69 + if own.cooldown == 0 { 0.06 } else { 0.0 }
                + if near.target.is_some() { 0.04 } else { 0.0 }
        }
        State::Retreat if own.health <= 32 && distance <= 3 => 0.99,
        State::Regroup => 0.35,
        _ => 0.0,
    }
}

fn ScoreMerrit(option: State, battle: &Battle, own: Fact, opportunity: Fact, distance: i32) -> f32 {
    let distracted = opportunity.target.is_some_and(|id| id != 1);
    match option {
        State::Flank if distracted && own.cooldown == 0 => 0.93,
        State::Assassinate if opportunity.health <= 46 && own.cooldown == 0 => 0.84,
        State::Engage => {
            0.67 + if battle.MageThreat().is_some() {
                0.06
            } else {
                0.0
            }
        }
        State::Evade if own.health <= 17 && distance <= 2 => 1.0,
        State::Regroup => 0.35,
        _ => 0.0,
    }
}

fn ScoreVey(option: State, battle: &Battle, own: Fact, distance: i32) -> f32 {
    match option {
        State::ProtectSelf if own.health <= 21 && distance <= 2 => 1.0,
        State::MaintainDistance if distance <= 2 => 0.96,
        State::CrowdControl if own.cooldown == 0 && battle.MageThreat().is_some() => 0.88,
        State::Engage => 0.71,
        State::Regroup => 0.35,
        _ => 0.0,
    }
}

const ALDA_RULES: &[Transition<Role>] = &[Transition {
    Kind: TransitionKind::Interrupt,
    Target: State::BlockThreat,
    Push: true,
    DependsOn: HEAVY_CHANGED,
    When: |battle, memory, _| !memory.blocking && battle.Heavy(0),
    Reason: "Incoming heavy attack",
}];

impl Brain for Role {
    type State = State;
    type Slot = Slot;
    type Frame = Frame;
    type Memory = PartyMemory;
    type Observation = Battle;
    type Command = Order;

    fn States(&self) -> &'static [State] {
        STATES
    }
    fn Slots(&self) -> &'static [Slot] {
        &[Slot::Intent]
    }
    fn Root(&self) -> State {
        State::Root
    }

    fn Decision(&self, _: Slot) -> Decision<State> {
        let options: &'static [State] = match self {
            Self::Alda => &[
                State::ProtectParty,
                State::Engage,
                State::Retreat,
                State::Regroup,
            ],
            Self::Merrit => &[
                State::Flank,
                State::Assassinate,
                State::Engage,
                State::Evade,
                State::Regroup,
            ],
            Self::Vey => &[
                State::MaintainDistance,
                State::CrowdControl,
                State::Engage,
                State::ProtectSelf,
                State::Regroup,
            ],
        };
        Decision {
            Options: options,
            Policy: DecisionPolicy {
                Hysteresis: 0.08,
                MinCommitTicks: 12,
                TieEpsilon: 0.01,
            },
        }
    }

    fn Score(&self, _: Slot, option: State, battle: &Battle, memory: &PartyMemory) -> Option<f32> {
        let index = *self as usize;
        let own = battle.actors[index];
        let Some(nearest) = battle.Near(index) else {
            return (option == State::Regroup).then_some(0.86);
        };
        let near = battle.actors[nearest];
        let distance = Distance(own.cell, near.cell);
        let score = match self {
            Self::Alda => ScoreAlda(option, battle, own, near, distance),
            Self::Merrit => ScoreMerrit(
                option,
                battle,
                own,
                battle.actors[self.Target(battle, State::Flank).unwrap_or(nearest)],
                distance,
            ),
            Self::Vey => ScoreVey(option, battle, own, distance),
        };
        let committed_target = memory.target.is_some_and(|id| battle.LiveEnemy(id));
        (score > 0.0).then_some(
            score
                + if committed_target && option == State::Engage {
                    0.02
                } else {
                    0.0
                },
        )
    }

    fn Enter(&self, state: State, memory: &mut PartyMemory) -> Frame {
        if state == State::BlockThreat {
            memory.blocking = true;
        }
        Frame::Begin
    }

    fn Exit(&self, state: State, _: Frame, memory: &mut PartyMemory) {
        if state == State::BlockThreat {
            memory.blocking = false;
        }
    }

    fn Transitions(&self, state: State) -> &'static [Transition<Self>] {
        if *self == Self::Alda && state == State::AttackCycle {
            ALDA_RULES
        } else {
            &[]
        }
    }

    fn Step(&self, state: State, data: &mut Frame, ctx: Context<'_, Self>) -> PartyStep {
        if state == State::Root {
            if ctx.Resumed || ctx.Tick >= ctx.Memory.next_evaluation {
                ctx.Memory.next_evaluation = ctx.Tick + 6;
                return Step::Decide(Slot::Intent);
            }
            return Step::Stay;
        }
        let target = ctx
            .Memory
            .target
            .filter(|&id| ctx.Observation.LiveEnemy(id));
        let own = ctx.Observation.actors[*self as usize];
        if own.travel > 0 || own.stun > 0 {
            return Step::Stay;
        }
        match state {
            State::SelectTarget | State::SelectOpportunity => {
                if target.is_some() {
                    Step::Succeed
                } else {
                    Step::Fail
                }
            }
            State::FindPosition => self.Position(ctx.Observation, data, false, target),
            State::FindFlankingPosition => self.Position(ctx.Observation, data, true, target),
            State::BlockThreat => {
                if matches!(data, Frame::Begin) {
                    *data = Frame::Ready;
                    Step::Command(Order::Block)
                } else {
                    Step::Succeed
                }
            }
            State::Recover => {
                if own.strike == 0 {
                    Step::Succeed
                } else {
                    Step::WaitUntil(ctx.Tick + 2)
                }
            }
            State::Approach => match target {
                Some(target)
                    if Distance(own.cell, ctx.Observation.actors[target].cell)
                        <= STATS[*self as usize].range
                        && Visible(own.cell, ctx.Observation.actors[target].cell) =>
                {
                    Step::Succeed
                }
                Some(target) => Step::Command(Order::Approach(target)),
                None => Step::Fail,
            },
            State::AttackCycle | State::RangedAttack => match target {
                Some(target) => self.AttackCycle(data, ctx, target),
                None => Step::Fail,
            },
            State::ShieldBash | State::Backstab | State::FrostBolt => {
                if matches!(data, Frame::Ready)
                    && let Some((Order::UseAbility(proposed), outcome)) =
                        ctx.Observation.previous_proposals[*self as usize]
                    && ctx.Memory.target == Some(proposed)
                {
                    match outcome {
                        CommandOutcome::Completed => return Step::Succeed,
                        CommandOutcome::Rejected => return Step::Fail,
                        CommandOutcome::InProgress => {}
                    }
                }
                match target {
                    Some(_) if own.cooldown > 0 => Step::Fail,
                    Some(target) if own.strike == 0 => {
                        *data = Frame::Ready;
                        Step::Command(Order::UseAbility(target))
                    }
                    Some(_) => Step::Stay,
                    None => Step::Fail,
                }
            }
            _ => self.Branch(state, data, ctx),
        }
    }
}

pub(super) fn Inspect(index: usize, mode: ControlMode, agent: &PartyAgent) -> AgentInspection {
    let names = |states: &[State]| states.iter().map(|state| format!("{state:?}")).collect();
    AgentInspection {
        Actor: index as u16,
        Mode: mode,
        Clock: agent.Clock(),
        Stack: names(&agent.Stack()),
        Target: agent.InspectMemory().target.map(|index| index as u16),
        Blocking: agent.InspectMemory().blocking,
        Decision: agent.Reports().next().map(DecisionView),
        Decisions: agent.DecisionHistory().iter().map(DecisionView).collect(),
        Transitions: agent
            .Trace()
            .iter()
            .map(|record| AgentTransition {
                Tick: record.Tick,
                WorldTick: record.WorldTick,
                Before: names(&record.Before),
                After: names(&record.After),
                Reason: record.Reason.into(),
                Interrupt: record.Kind == Some(TransitionKind::Interrupt),
            })
            .collect(),
        Commands: Vec::new(),
    }
}

fn DecisionView(report: &DecisionReport<State, Slot>) -> AgentDecision {
    let mut invalid = Vec::new();
    let mut candidates = Vec::with_capacity(report.Candidates.len());
    for (state, score) in &report.Candidates {
        let name = format!("{state:?}");
        let score = match score {
            Utility::Score(score) => Some(*score),
            Utility::Unavailable => None,
            Utility::Invalid => {
                invalid.push(name.clone());
                None
            }
        };
        candidates.push((name, score));
    }
    AgentDecision {
        Tick: report.Tick,
        WorldTick: report.WorldTick,
        Candidates: candidates,
        Invalid: invalid,
        Current: report
            .Current
            .map(|(state, score)| (format!("{state:?}"), score)),
        Selected: report.Selected.map(|state| format!("{state:?}")),
        Hysteresis: report.Hysteresis,
        CommitRemaining: report.CommitRemaining,
        Reason: format!("{:?}", report.Reason),
    }
}

#[cfg(test)]
mod profiling {
    use super::*;
    use crate::encounter::Encounter;
    use std::{hint::black_box, time::Instant};

    #[test]
    #[ignore = "release-mode CPU timing probe"]
    fn profile_party_planning_and_inspection() {
        const TICKS: u64 = 24_000;
        let mut game = Encounter::New();
        game.actors[3].cell = [2, 4];
        game.actors[3].target = Some(2);
        let facts = Battle::Read(&game.actors, None, [None; PARTY]);
        let mut agents = [Role::Alda, Role::Merrit, Role::Vey].map(Role::Start);
        let start = Instant::now();
        for _ in 0..TICKS {
            black_box(Battle::Read(black_box(&game.actors), None, [None; PARTY]));
        }
        let snapshot = start.elapsed();

        let start = Instant::now();
        for _ in 0..TICKS {
            for role in [Role::Alda, Role::Merrit, Role::Vey] {
                for &option in role.Decision(Slot::Intent).Options {
                    black_box(role.Score(
                        Slot::Intent,
                        option,
                        black_box(&facts),
                        &PartyMemory::default(),
                    ));
                }
            }
        }
        let scoring = start.elapsed();

        let start = Instant::now();
        for tick in 1..=TICKS {
            for agent in &mut agents {
                black_box(agent.TickAt(black_box(&facts), 0, false, tick).unwrap());
            }
        }
        let stepping = start.elapsed();

        let start = Instant::now();
        let mut report_bytes = 0;
        for _ in 0..TICKS {
            for (index, agent) in agents.iter().enumerate() {
                let report = Inspect(index, ControlMode::Autonomous, black_box(agent));
                let mut encoded = Vec::new();
                wyrmcoil_game::Wire::WriteFrame(&mut encoded, report).unwrap();
                report_bytes = encoded.len();
                black_box(report_bytes);
            }
        }
        let reporting = start.elapsed();
        let nanos = |duration: std::time::Duration| duration.as_nanos() / u128::from(TICKS);
        println!(
            "per party iteration snapshot={}ns score_all_options={}ns step_three={}ns inspect_and_encode_three={}ns last_agent_report={}B inline_agent={}B frame_payload={}B",
            nanos(snapshot),
            nanos(scoring),
            nanos(stepping),
            nanos(reporting),
            report_bytes,
            size_of::<PartyAgent>(),
            size_of::<Frame>()
        );
    }
}
