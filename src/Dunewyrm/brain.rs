//! Typed, resumable frames for Dunewyrm's root-overlay scheduling policy.

use std::collections::VecDeque;

use super::stack::Unwind;

const TRACE_LIMIT: usize = 8;
const STACK_LIMIT: usize = 32;

#[cfg(test)]
#[path = "brain_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReturnKind {
    Returned,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChildReturn<S> {
    pub State: S,
    pub Kind: ReturnKind,
}

pub enum Step<S, D, C> {
    Stay,
    Push(S),
    Goto(S),
    Pop,
    Succeed,
    Fail,
    WaitUntil(u64),
    Decide(D),
    Command(C),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecisionPolicy {
    pub Hysteresis: f32,
    pub MinCommitTicks: u64,
    pub TieEpsilon: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionReason {
    BestIsCurrent,
    MinimumCommitment,
    Hysteresis,
    PreferCurrentTie,
    Switched,
    NoValidOption,
    InvalidUtility,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Utility {
    Score(f32),
    Unavailable,
    Invalid,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecisionReport<S, D> {
    pub Tick: u64,
    pub WorldTick: u64,
    pub Slot: D,
    pub Candidates: Vec<(S, Utility)>,
    pub Current: Option<(S, f32)>,
    pub Best: Option<(S, f32)>,
    pub Selected: Option<S>,
    pub CommitRemaining: u64,
    pub Hysteresis: f32,
    pub Reason: DecisionReason,
}

pub struct Decision<S: 'static> {
    pub Options: &'static [S],
    pub Policy: DecisionPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionKind {
    Interrupt,
    Normal,
}

pub struct Transition<B: Brain> {
    pub Kind: TransitionKind,
    pub Target: B::State,
    /// Push preserves the matched frame after exiting its descendants.
    pub Push: bool,
    /// Zero is unconditional. Other masks require any matching dirty fact.
    pub DependsOn: u64,
    pub When: fn(&B::Observation, &B::Memory, &B::Frame) -> bool,
    pub Reason: &'static str,
}

pub struct Context<'a, B: Brain> {
    pub Observation: &'a B::Observation,
    pub Memory: &'a mut B::Memory,
    pub Child: &'a mut Option<ChildReturn<B::State>>,
    pub Tick: u64,
    pub Resumed: bool,
}

pub trait Brain: Sized + 'static {
    type State: Copy + Eq + 'static;
    type Slot: Copy + Eq + 'static;
    type Frame;
    type Memory;
    type Observation;
    type Command;

    fn States(&self) -> &'static [Self::State];
    fn Slots(&self) -> &'static [Self::Slot];
    fn Root(&self) -> Self::State;
    fn Decision(&self, slot: Self::Slot) -> Decision<Self::State>;
    fn Score(
        &self,
        slot: Self::Slot,
        option: Self::State,
        observation: &Self::Observation,
        memory: &Self::Memory,
    ) -> Option<f32>;
    fn Enter(&self, state: Self::State, memory: &mut Self::Memory) -> Self::Frame;
    fn Exit(&self, _state: Self::State, _frame: Self::Frame, _memory: &mut Self::Memory) {}
    fn Step(
        &self,
        state: Self::State,
        frame: &mut Self::Frame,
        ctx: Context<'_, Self>,
    ) -> Step<Self::State, Self::Slot, Self::Command>;
    fn Transitions(&self, _state: Self::State) -> &'static [Transition<Self>] {
        &[]
    }
}

#[derive(Clone, Copy)]
pub struct Schedule {
    pub KeepRootFrame: bool,
    pub InterruptEvery: u64,
    pub TransitionEvery: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransitionReport<S> {
    pub Tick: u64,
    pub WorldTick: u64,
    pub Before: Vec<S>,
    pub After: Vec<S>,
    pub Reason: &'static str,
    pub Kind: Option<TransitionKind>,
}

struct Active<B: Brain> {
    state: B::State,
    data: B::Frame,
    child: Option<ChildReturn<B::State>>,
    wait: u64,
}

struct Memory<S> {
    selected: Option<S>,
    last_switch: u64,
}

pub struct Agent<B: Brain> {
    brain: B,
    memory: B::Memory,
    stack: Vec<Active<B>>,
    commits: Vec<Memory<B::State>>,
    reports: Vec<Option<DecisionReport<B::State, B::Slot>>>,
    decisions: VecDeque<DecisionReport<B::State, B::Slot>>,
    trace: VecDeque<TransitionReport<B::State>>,
    schedule: Schedule,
    tick: u64,
    world_tick: u64,
    next_interrupt: u64,
    next_transition: u64,
    dirty: u64,
}

impl<B: Brain> Drop for Agent<B> {
    fn drop(&mut self) {
        self.ExitTo(0);
    }
}

impl<B: Brain> Agent<B> {
    pub fn New(brain: B, memory: B::Memory, schedule: Schedule) -> Result<Self, &'static str> {
        let states = brain.States();
        let root = brain.Root();
        if !states.contains(&root) {
            return Err("root must be a registered state");
        }
        for (index, &state) in states.iter().enumerate() {
            if states[..index].contains(&state) {
                return Err("duplicate state");
            }
            for transition in brain.Transitions(state) {
                if !states.contains(&transition.Target) || transition.Target == root {
                    return Err("invalid transition target");
                }
            }
        }
        let slots = brain.Slots();
        for (index, &slot) in slots.iter().enumerate() {
            if slots[..index].contains(&slot) {
                return Err("duplicate decision slot");
            }
            let decision = brain.Decision(slot);
            if ![decision.Policy.Hysteresis, decision.Policy.TieEpsilon]
                .iter()
                .all(|x| x.is_finite() && *x >= 0.0)
            {
                return Err("invalid decision policy");
            }
            for (index, &target) in decision.Options.iter().enumerate() {
                if !states.contains(&target)
                    || target == root
                    || decision.Options[..index].contains(&target)
                {
                    return Err("invalid or duplicate decision option");
                }
            }
        }
        let mut agent = Self {
            brain,
            memory,
            schedule,
            tick: 0,
            world_tick: 0,
            next_interrupt: 0,
            next_transition: 0,
            dirty: 0,
            stack: Vec::new(),
            commits: slots
                .iter()
                .map(|_| Memory {
                    selected: None,
                    last_switch: 0,
                })
                .collect(),
            reports: slots.iter().map(|_| None).collect(),
            decisions: VecDeque::with_capacity(TRACE_LIMIT),
            trace: VecDeque::with_capacity(TRACE_LIMIT),
        };
        agent.Enter(root);
        Ok(agent)
    }

    pub fn Tick(
        &mut self,
        observation: &B::Observation,
        dirty: u64,
        resumed: bool,
    ) -> Result<Option<B::Command>, &'static str> {
        self.TickAt(
            observation,
            dirty,
            resumed,
            self.world_tick.saturating_add(1),
        )
    }

    /// Advance one autonomous tick, recording the caller's simulation tick in diagnostics.
    pub fn TickAt(
        &mut self,
        observation: &B::Observation,
        dirty: u64,
        resumed: bool,
        world_tick: u64,
    ) -> Result<Option<B::Command>, &'static str> {
        if world_tick <= self.world_tick {
            return Err("world tick must advance");
        }
        self.world_tick = world_tick;
        self.tick += 1;
        self.dirty |= dirty;
        // Possession release forces the root to see the new observation on its
        // first eligible tick. Normal ticks retain Dominatus's scan-first order.
        if !resumed && self.Scan(observation) {
            return Ok(None);
        }
        let overlay = self.schedule.KeepRootFrame && self.stack[0].state == self.brain.Root();
        if overlay {
            let (changed, command) = self.RunFrame(0, observation, resumed)?;
            if changed || command.is_some() || self.stack.len() == 1 {
                return Ok(command);
            }
        }
        let (_, command) = self.RunFrame(self.stack.len() - 1, observation, resumed)?;
        Ok(command)
    }

    pub fn Clock(&self) -> u64 {
        self.tick
    }
    pub fn InspectMemory(&self) -> &B::Memory {
        &self.memory
    }
    pub fn Stack(&self) -> Vec<B::State> {
        self.stack.iter().map(|f| f.state).collect()
    }
    pub fn Reports(&self) -> impl Iterator<Item = &DecisionReport<B::State, B::Slot>> {
        self.reports.iter().flatten()
    }
    pub fn DecisionHistory(&self) -> &VecDeque<DecisionReport<B::State, B::Slot>> {
        &self.decisions
    }
    pub fn Trace(&self) -> &VecDeque<TransitionReport<B::State>> {
        &self.trace
    }

    fn Scan(&mut self, observation: &B::Observation) -> bool {
        let interrupt = self.tick >= self.next_interrupt;
        let normal = self.tick >= self.next_transition;
        if !interrupt && !normal {
            return false;
        }
        for index in (0..self.stack.len()).rev() {
            let active = &self.stack[index];
            let transitions = self.brain.Transitions(active.state);
            for (enabled, kind) in [
                (interrupt, TransitionKind::Interrupt),
                (normal, TransitionKind::Normal),
            ] {
                if !enabled {
                    continue;
                }
                for rule in transitions {
                    if rule.Kind == kind
                        && (rule.DependsOn == 0 || self.dirty & rule.DependsOn != 0)
                        && (rule.When)(observation, &self.memory, &active.data)
                    {
                        let before = self.Stack();
                        let keep = if rule.Push || (index == 0 && self.schedule.KeepRootFrame) {
                            index + 1
                        } else {
                            index
                        };
                        if keep >= STACK_LIMIT {
                            continue;
                        }
                        self.ExitTo(keep);
                        self.Enter(rule.Target);
                        self.Record(before, rule.Reason, Some(kind));
                        // Dominatus preserves pending dirt and cadence after a successful scan.
                        return true;
                    }
                }
            }
        }
        if interrupt {
            self.next_interrupt = self.tick.saturating_add(self.schedule.InterruptEvery);
        }
        if normal {
            self.next_transition = self.tick.saturating_add(self.schedule.TransitionEvery);
        }
        self.dirty = 0;
        false
    }

    fn RunFrame(
        &mut self,
        index: usize,
        observation: &B::Observation,
        resumed: bool,
    ) -> Result<(bool, Option<B::Command>), &'static str> {
        let frame = &mut self.stack[index];
        if self.tick < frame.wait {
            return Ok((false, None));
        }
        let step = self.brain.Step(
            frame.state,
            &mut frame.data,
            Context {
                Observation: observation,
                Memory: &mut self.memory,
                Child: &mut frame.child,
                Tick: self.tick,
                Resumed: resumed,
            },
        );
        match step {
            Step::Stay => return Ok((false, None)),
            Step::WaitUntil(until) => {
                self.stack[index].wait = until;
                return Ok((false, None));
            }
            Step::Command(command) => return Ok((false, Some(command))),
            Step::Decide(slot) => return Ok((self.Decide(slot, observation)?, None)),
            _ => {}
        }
        let before = self.Stack();
        let reason = match step {
            Step::Push(target) | Step::Goto(target) => {
                if !self.brain.States().contains(&target) || target == self.brain.Root() {
                    return Err("invalid step target");
                }
                if matches!(step, Step::Push(_)) {
                    if self.stack.len() >= STACK_LIMIT {
                        return Err("state stack is full");
                    }
                    self.stack.last_mut().unwrap().child = None;
                    self.Enter(target);
                    "Push"
                } else {
                    let keep = if self.stack.len() == 1 && self.schedule.KeepRootFrame {
                        1
                    } else {
                        self.stack.len() - 1
                    };
                    self.ExitTo(keep);
                    self.Enter(target);
                    "Goto"
                }
            }
            Step::Pop | Step::Succeed | Step::Fail => {
                let (kind, reason) = match step {
                    Step::Pop => (ReturnKind::Returned, "Pop"),
                    Step::Succeed => (ReturnKind::Succeeded, "Succeed"),
                    _ => (ReturnKind::Failed, "Fail"),
                };
                // An overlay can request a return on behalf of the active leaf.
                let top = self.stack.len() - 1;
                let state = self.stack[top].state;
                self.ExitTo(top);
                if let Some(parent) = self.stack.last_mut() {
                    parent.child = Some(ChildReturn {
                        State: state,
                        Kind: kind,
                    });
                } else {
                    self.Enter(self.brain.Root());
                    self.next_interrupt = 0;
                    self.next_transition = 0;
                    self.dirty = 0;
                }
                reason
            }
            _ => unreachable!(),
        };
        self.Record(before, reason, None);
        Ok((true, None))
    }

    fn Decide(
        &mut self,
        slot: B::Slot,
        observation: &B::Observation,
    ) -> Result<bool, &'static str> {
        let index = self
            .brain
            .Slots()
            .iter()
            .position(|candidate| *candidate == slot)
            .ok_or("decision slot not registered")?;
        let decision = self.brain.Decision(slot);
        let mut candidates = Vec::with_capacity(decision.Options.len());
        let mut best: Option<(B::State, f32)> = None;
        let mut current = None;
        let mut invalid = false;
        let commit = &mut self.commits[index];
        for &option in decision.Options {
            let score = match self.brain.Score(slot, option, observation, &self.memory) {
                None => Utility::Unavailable,
                Some(score) if score.is_finite() => {
                    if best.is_none_or(|(_, best_score)| score > best_score) {
                        best = Some((option, score));
                    }
                    if commit.selected == Some(option) {
                        current = Some((option, score));
                    }
                    Utility::Score(score)
                }
                Some(_) => {
                    invalid = true;
                    Utility::Invalid
                }
            };
            candidates.push((option, score));
        }
        let remaining = decision
            .Policy
            .MinCommitTicks
            .saturating_sub(self.tick - commit.last_switch);
        let (selected, reason) = match (best, current) {
            (None, _) => (
                None,
                if invalid {
                    DecisionReason::InvalidUtility
                } else {
                    DecisionReason::NoValidOption
                },
            ),
            (Some((best, _)), Some((current, _))) if best == current => {
                (Some(current), DecisionReason::BestIsCurrent)
            }
            (Some(_), Some((current, _))) if remaining > 0 => {
                (Some(current), DecisionReason::MinimumCommitment)
            }
            (Some((_, best)), Some((current, score)))
                if best < score + decision.Policy.Hysteresis =>
            {
                (Some(current), DecisionReason::Hysteresis)
            }
            (Some((_, best)), Some((current, score)))
                if (best - score).abs() <= decision.Policy.TieEpsilon =>
            {
                (Some(current), DecisionReason::PreferCurrentTie)
            }
            (Some((best, _)), _) => (Some(best), DecisionReason::Switched),
        };
        let report = DecisionReport {
            Tick: self.tick,
            WorldTick: self.world_tick,
            Slot: slot,
            Candidates: candidates,
            Current: current,
            Best: best,
            Selected: selected,
            CommitRemaining: if reason == DecisionReason::Switched {
                decision.Policy.MinCommitTicks
            } else if current.is_some() {
                remaining
            } else {
                0
            },
            Hysteresis: decision.Policy.Hysteresis,
            Reason: reason,
        };
        if let Some(previous) = self.reports[index].replace(report) {
            if self.decisions.len() == TRACE_LIMIT {
                self.decisions.pop_front();
            }
            self.decisions.push_back(previous);
        }
        if reason != DecisionReason::Switched {
            return Ok(false);
        }
        commit.selected = selected;
        commit.last_switch = self.tick;
        let before = self.Stack();
        let keep = if self.schedule.KeepRootFrame {
            1
        } else {
            self.stack.len() - 1
        };
        self.ExitTo(keep);
        self.Enter(selected.unwrap());
        self.Record(before, "Utility branch selected", None);
        Ok(true)
    }

    fn Enter(&mut self, state: B::State) {
        let data = self.brain.Enter(state, &mut self.memory);
        self.stack.push(Active {
            state,
            data,
            child: None,
            wait: 0,
        });
    }

    fn ExitTo(&mut self, keep: usize) {
        Unwind(&mut self.stack, keep, |frame| {
            self.brain.Exit(frame.state, frame.data, &mut self.memory)
        });
    }

    fn Record(
        &mut self,
        before: Vec<B::State>,
        reason: &'static str,
        kind: Option<TransitionKind>,
    ) {
        if self.trace.len() == TRACE_LIMIT {
            self.trace.pop_front();
        }
        self.trace.push_back(TransitionReport {
            Tick: self.tick,
            WorldTick: self.world_tick,
            Before: before,
            After: self.Stack(),
            Reason: reason,
            Kind: kind,
        });
    }
}
