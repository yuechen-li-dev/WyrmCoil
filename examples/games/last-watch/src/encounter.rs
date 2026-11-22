use std::collections::VecDeque;
use wyrmcoil_game::{
    ActorMotion, GameState, Scene::SceneDocument, TacticalActor, TacticalCommand, TacticalGame,
    TacticalOrder, TacticalView,
};

use crate::scene::{BuildScene, PIERS, RELIC};

type Cell = [i32; 2];
const STEPS: [Cell; 4] = [[0, -1], [-1, 0], [1, 0], [0, 1]];
const SIZE: usize = 17 * 15;
const PARTY: usize = 3;
const LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    Move(Cell),
    Attack(usize),
    UseAbility(usize),
    Hold,
}

#[derive(Clone, Copy)]
struct Stats {
    name: &'static str,
    health: u16,
    damage: u16,
    range: i32,
    attack_delay: u16,
    travel: u16,
}

const STATS: [Stats; 6] = [
    Stats {
        name: "Alda · Warrior",
        health: 144,
        damage: 17,
        range: 1,
        attack_delay: 72,
        travel: 17,
    },
    Stats {
        name: "Merrit · Rogue",
        health: 94,
        damage: 14,
        range: 1,
        attack_delay: 47,
        travel: 13,
    },
    Stats {
        name: "Vey · Mage",
        health: 86,
        damage: 14,
        range: 5,
        attack_delay: 86,
        travel: 18,
    },
    Stats {
        name: "Gate Guard",
        health: 94,
        damage: 12,
        range: 1,
        attack_delay: 85,
        travel: 19,
    },
    Stats {
        name: "Hall Guard",
        health: 94,
        damage: 12,
        range: 1,
        attack_delay: 85,
        travel: 19,
    },
    Stats {
        name: "Watch Captain",
        health: 185,
        damage: 23,
        range: 1,
        attack_delay: 68,
        travel: 21,
    },
];

const STARTS: [Cell; 6] = [[0, 5], [-2, 5], [2, 5], [-4, 0], [4, 0], [0, -5]];

#[derive(Clone)]
struct Actor {
    cell: Cell,
    last: Cell,
    travel: u16,
    travel_total: u16,
    facing: Cell,
    health: u16,
    strike: u16,
    cooldown: u16,
    stun: u16,
    slow: u16,
    motion: ActorMotion,
    motion_ticks: u16,
    queue: VecDeque<Order>,
    target: Option<usize>,
    patrol_out: bool,
}

impl Actor {
    fn Position(&self) -> [f32; 2] {
        let fraction = if self.travel_total == 0 {
            0.0
        } else {
            f32::from(self.travel) / f32::from(self.travel_total)
        };
        [
            self.cell[0] as f32 + (self.last[0] - self.cell[0]) as f32 * fraction,
            self.cell[1] as f32 + (self.last[1] - self.cell[1]) as f32 * fraction,
        ]
    }

    fn Motion(&mut self, motion: ActorMotion) {
        self.motion = motion;
        self.motion_ticks = 0;
    }
}

pub struct Encounter {
    actors: [Actor; 6],
    frame: u64,
    paused: bool,
    selected: usize,
    target: Option<usize>,
    state: GameState,
    notice: &'static str,
}

impl Encounter {
    pub fn New() -> Self {
        Self {
            actors: std::array::from_fn(|index| Actor {
                cell: STARTS[index],
                last: STARTS[index],
                travel: 0,
                travel_total: 0,
                facing: [0, if index < PARTY { -1 } else { 1 }],
                health: STATS[index].health,
                strike: 0,
                cooldown: 0,
                stun: 0,
                slow: 0,
                motion: ActorMotion::Idle,
                motion_ticks: 0,
                queue: VecDeque::new(),
                target: None,
                patrol_out: false,
            }),
            frame: 0,
            paused: true,
            selected: 0,
            target: None,
            state: GameState::Playing,
            notice: "Defeat the watch and reach the relic.",
        }
    }

    fn Command(&mut self, command: TacticalCommand) {
        match command {
            TacticalCommand::TogglePause => self.paused = !self.paused,
            TacticalCommand::SelectParty(slot) if usize::from(slot) < PARTY => {
                self.selected = usize::from(slot)
            }
            TacticalCommand::Inspect(id) if usize::from(id) < self.actors.len() => {
                let index = usize::from(id);
                if index < PARTY {
                    self.selected = index;
                } else {
                    self.target = Some(index);
                }
            }
            TacticalCommand::Clear => {
                self.actors[self.selected].queue.clear();
                self.notice = "Orders cleared.";
            }
            TacticalCommand::Hold => self.Enqueue(Order::Hold),
            TacticalCommand::MoveTo(position) if position.into_iter().all(f32::is_finite) => {
                let cell = [position[0].round() as i32, position[1].round() as i32];
                if Walkable(cell) {
                    self.Enqueue(Order::Move(cell));
                } else {
                    self.notice = "That tile is blocked.";
                }
            }
            TacticalCommand::Attack(id) if self.LiveEnemy(usize::from(id)) => {
                self.target = Some(usize::from(id));
                self.Enqueue(Order::Attack(usize::from(id)));
            }
            TacticalCommand::Ability(slot) if usize::from(slot) < PARTY => {
                self.selected = usize::from(slot);
                if let Some(target) = self.target.filter(|&index| self.LiveEnemy(index)) {
                    self.Enqueue(Order::UseAbility(target));
                } else {
                    self.notice = "Inspect an enemy before queuing an ability.";
                }
            }
            _ => {}
        }
    }

    fn LiveEnemy(&self, index: usize) -> bool {
        index >= PARTY && self.actors.get(index).is_some_and(|actor| actor.health > 0)
    }

    fn Enqueue(&mut self, order: Order) {
        let actor = &mut self.actors[self.selected];
        if actor.health == 0 {
            self.notice = "That adventurer is down.";
        } else if actor.queue.len() >= LIMIT {
            self.notice = "Order queue full. Clear it with C.";
        } else {
            actor.queue.push_back(order);
            self.notice = "Order queued.";
        }
    }

    fn Advance(&mut self) {
        self.frame += 1;
        for actor in &mut self.actors {
            actor.motion_ticks = actor.motion_ticks.saturating_add(1);
            if actor.health == 0 {
                continue;
            }
            actor.travel = actor.travel.saturating_sub(1);
            actor.strike = actor.strike.saturating_sub(1);
            actor.cooldown = actor.cooldown.saturating_sub(1);
            actor.stun = actor.stun.saturating_sub(1);
            actor.slow = actor.slow.saturating_sub(1);
            let duration = match actor.motion {
                ActorMotion::Attack | ActorMotion::Ability => 23,
                ActorMotion::Hit => 12,
                ActorMotion::Walk => actor.travel_total,
                _ => u16::MAX,
            };
            if actor.motion_ticks >= duration {
                actor.Motion(ActorMotion::Idle);
            }
        }
        for index in 0..self.actors.len() {
            let actor = &self.actors[index];
            if actor.health == 0 || actor.travel != 0 || actor.stun > 0 {
                continue;
            }
            if index < PARTY {
                self.PartyTurn(index);
            } else {
                self.EnemyTurn(index);
            }
        }
        if self.actors[..PARTY].iter().all(|actor| actor.health == 0) {
            self.state = GameState::Lost;
            self.notice = "The watch has fallen silent. Restart to try again.";
        } else if self.actors[PARTY..].iter().all(|actor| actor.health == 0) {
            if self.actors[..PARTY]
                .iter()
                .any(|actor| actor.health > 0 && actor.travel == 0 && actor.cell == RELIC)
            {
                self.state = GameState::Won;
                self.notice = "Relic recovered. The Last Watch is over.";
            } else {
                self.notice = "The guards are down. Send a survivor to the gold relic.";
            }
        }
    }

    fn PartyTurn(&mut self, index: usize) {
        let Some(order) = self.actors[index].queue.front().copied() else {
            return;
        };
        let done = match order {
            Order::Move(goal) => {
                if self.actors[index].cell == goal {
                    true
                } else {
                    self.Approach(index, goal, 0);
                    false
                }
            }
            Order::Attack(target) => {
                !self.LiveEnemy(target) || {
                    let struck = self.Fight(index, target, false);
                    !self.LiveEnemy(target) || (struck && self.actors[index].queue.len() > 1)
                }
            }
            Order::UseAbility(target) => !self.LiveEnemy(target) || self.Fight(index, target, true),
            Order::Hold => {
                if let Some(target) = self.FindTarget(index, PARTY..6, STATS[index].range) {
                    self.Fight(index, target, false);
                }
                false
            }
        };
        if done {
            self.actors[index].queue.pop_front();
        }
    }

    fn EnemyTurn(&mut self, index: usize) {
        let target = self.actors[index]
            .target
            .filter(|&target| self.actors[target].health > 0)
            .or_else(|| self.FindTarget(index, 0..PARTY, 4));
        self.actors[index].target = target;
        if let Some(target) = target {
            self.Fight(index, target, false);
            return;
        }
        let anchor = STARTS[index];
        let mut goal = if self.actors[index].patrol_out {
            [anchor[0], anchor[1] + 2]
        } else {
            anchor
        };
        if self.actors[index].cell == goal {
            self.actors[index].patrol_out = !self.actors[index].patrol_out;
            goal = if self.actors[index].patrol_out {
                [anchor[0], anchor[1] + 2]
            } else {
                anchor
            };
        }
        self.Approach(index, goal, 0);
    }

    fn FindTarget(
        &self,
        index: usize,
        candidates: std::ops::Range<usize>,
        range: i32,
    ) -> Option<usize> {
        let cell = self.actors[index].cell;
        candidates
            .filter(|&candidate| {
                let actor = &self.actors[candidate];
                actor.health > 0 && Distance(cell, actor.cell) <= range && Visible(cell, actor.cell)
            })
            .min_by_key(|&candidate| (Distance(cell, self.actors[candidate].cell), candidate))
    }

    fn Fight(&mut self, index: usize, target: usize, ability: bool) -> bool {
        let cell = self.actors[index].cell;
        let goal = self.actors[target].cell;
        let stats = STATS[index];
        if Distance(cell, goal) > stats.range || !Visible(cell, goal) {
            self.Approach(index, goal, stats.range);
            return false;
        }
        if self.actors[index].strike > 0 || (ability && self.actors[index].cooldown > 0) {
            return false;
        }
        let direction = Subtract(goal, cell);
        self.actors[index].facing = direction;
        self.actors[index].strike = stats.attack_delay;
        let mut damage = stats.damage;
        if ability {
            self.actors[index].Motion(ActorMotion::Ability);
            match index {
                0 => {
                    self.actors[index].cooldown = 300;
                    self.actors[target].stun = 115;
                    damage = 21;
                    self.notice = "Shield Bash interrupts the target.";
                }
                1 => {
                    self.actors[index].cooldown = 245;
                    let facing = self.actors[target].facing;
                    let behind = direction[0] * facing[0] + direction[1] * facing[1] > 0;
                    let distracted = self.actors[target]
                        .target
                        .is_some_and(|target| target != index);
                    damage = if behind || distracted { 46 } else { 21 };
                    self.notice = if damage == 46 {
                        "Backstab found an opening."
                    } else {
                        "Backstab hit from the front."
                    };
                }
                _ => {
                    self.actors[index].cooldown = 340;
                    self.actors[target].slow = 170;
                    damage = 36;
                    self.notice = "Frost Bolt slows the target.";
                }
            }
        } else {
            self.actors[index].Motion(ActorMotion::Attack);
        }
        let victim = &mut self.actors[target];
        victim.health = victim.health.saturating_sub(damage);
        if victim.health == 0 {
            victim.queue.clear();
            victim.travel = 0;
            victim.Motion(ActorMotion::Death);
        } else {
            victim.Motion(ActorMotion::Hit);
        }
        true
    }

    fn Approach(&mut self, index: usize, goal: Cell, range: i32) {
        let occupied = |cell| {
            self.actors.iter().enumerate().any(|(other, actor)| {
                other != index
                    && actor.health > 0
                    && (actor.cell == cell || (actor.travel > 0 && actor.last == cell))
            })
        };
        let actor = &self.actors[index];
        let Some(next) = NextStep(actor.cell, goal, range, occupied) else {
            return;
        };
        let actor = &mut self.actors[index];
        actor.last = actor.cell;
        actor.cell = next;
        actor.facing = Subtract(next, actor.last);
        actor.travel_total = STATS[index].travel * if actor.slow > 0 { 2 } else { 1 };
        actor.travel = actor.travel_total;
        actor.Motion(ActorMotion::Walk);
    }

    fn View(&self) -> TacticalView {
        let actors = self
            .actors
            .iter()
            .enumerate()
            .map(|(index, actor)| TacticalActor {
                Id: index as u16,
                Name: STATS[index].name.into(),
                PartySlot: (index < PARTY).then_some(index as u8),
                Position: actor.Position(),
                Yaw: (actor.facing[0] as f32).atan2(actor.facing[1] as f32),
                Health: actor.health,
                MaxHealth: STATS[index].health,
                Cooldown: actor.cooldown,
                Motion: actor.motion,
                MotionTicks: actor.motion_ticks,
            })
            .collect();
        let orders = self.actors[self.selected]
            .queue
            .iter()
            .map(|order| match *order {
                Order::Move([x, z]) => TacticalOrder::MoveTo([x as f32, z as f32]),
                Order::Attack(id) => TacticalOrder::Attack(id as u16),
                Order::UseAbility(id) => TacticalOrder::Ability(id as u16),
                Order::Hold => TacticalOrder::Hold,
            })
            .collect();
        TacticalView {
            Frame: self.frame,
            Paused: self.paused,
            State: self.state,
            Objective: self.notice.into(),
            Selected: self.selected as u16,
            Target: self.target.map(|target| target as u16),
            Actors: actors,
            Orders: orders,
            Relic: [RELIC[0] as f32, RELIC[1] as f32],
        }
    }
}

impl TacticalGame for Encounter {
    fn Start(&mut self) -> Result<(SceneDocument, TacticalView), String> {
        *self = Self::New();
        Ok((BuildScene(), self.View()))
    }

    fn FixedUpdate(&mut self, commands: &[TacticalCommand]) -> Result<TacticalView, String> {
        if self.state == GameState::Playing {
            for &command in commands {
                self.Command(command);
            }
            if !self.paused {
                self.Advance();
            }
        }
        Ok(self.View())
    }

    fn Art(&self) -> Option<wyrmcoil_game::TacticalArt> {
        Some(crate::scene::BuildArt())
    }
}

fn Walkable([x, z]: Cell) -> bool {
    (-8..=8).contains(&x) && (-7..=7).contains(&z) && !PIERS.contains(&[x, z])
}

fn Subtract(a: Cell, b: Cell) -> Cell {
    [a[0] - b[0], a[1] - b[1]]
}

fn Distance(a: Cell, b: Cell) -> i32 {
    let [x, z] = Subtract(a, b);
    x.abs() + z.abs()
}

fn Visible(start: Cell, end: Cell) -> bool {
    let [dx, dz] = Subtract(end, start);
    let length = dx.abs().max(dz.abs()) * 2;
    for step in 1..length {
        let x = start[0] as f32 + (dx * step) as f32 / length as f32;
        let z = start[1] as f32 + (dz * step) as f32 / length as f32;
        if !Walkable([x.round() as i32, z.round() as i32]) {
            return false;
        }
    }
    true
}

fn NextStep(
    start: Cell,
    destination: Cell,
    range: i32,
    occupied: impl Fn(Cell) -> bool,
) -> Option<Cell> {
    let slot = |[x, z]: Cell| ((z + 7) * 17 + x + 8) as usize;
    let mut open = VecDeque::from([(start, start)]);
    let mut seen = [false; SIZE];
    seen[slot(start)] = true;
    while let Some((cell, first)) = open.pop_front() {
        if Distance(cell, destination) <= range && Visible(cell, destination) {
            return (cell != start).then_some(first);
        }
        for [x, z] in STEPS {
            let next = [cell[0] + x, cell[1] + z];
            if !Walkable(next) || seen[slot(next)] || occupied(next) {
                continue;
            }
            seen[slot(next)] = true;
            open.push_back((next, if cell == start { next } else { first }));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use TacticalCommand::*;

    fn Tick(game: &mut Encounter, ticks: usize) {
        for _ in 0..ticks {
            game.FixedUpdate(&[]).unwrap();
        }
    }

    #[test]
    fn tactical_pause_holds_patrol_positions_cooldowns_and_animation_but_accepts_orders() {
        let mut game = Encounter::New();
        let initial = game.Start().unwrap().1;
        assert!(initial.Paused);
        assert_eq!(initial.Actors.len(), 6);
        game.actors[1].cooldown = 12;
        game.actors[1].Motion(ActorMotion::Ability);
        let paused = game
            .FixedUpdate(&[SelectParty(1), MoveTo([-2.0, 4.0]), MoveTo([-2.0, 3.0])])
            .unwrap();
        Tick(&mut game, 300);
        assert_eq!(game.View(), paused);
        assert_eq!(paused.Selected, 1);
        assert_eq!(paused.Orders.len(), 2);
        game.FixedUpdate(&[TogglePause]).unwrap();
        Tick(&mut game, 13);
        let resumed = game.View();
        assert_eq!(resumed.Frame, 14);
        assert_eq!(resumed.Actors[1].Position, [-2.0, 4.0]);
        assert_eq!(resumed.Actors[1].Cooldown, 0);
        assert_eq!(resumed.Orders, vec![TacticalOrder::MoveTo([-2.0, 3.0])]);
        Tick(&mut game, 14);
        assert!(game.View().Orders.is_empty());
    }

    #[test]
    fn abilities_apply_their_own_fixed_step_effects_and_dead_targets_are_removed() {
        for (slot, damage) in [(0, 21), (1, 46), (2, 36)] {
            let mut game = Encounter::New();
            game.actors[slot].cell = [-4, 1];
            game.actors[3].target = Some((slot + 1) % 3);
            let before = game.actors[3].health;
            let updated = game
                .FixedUpdate(&[Inspect(3), Ability(slot as u8), TogglePause])
                .unwrap();
            assert_eq!(updated.Actors[3].Health, before - damage);
            assert_eq!(updated.Actors[slot].Motion, ActorMotion::Ability);
            assert!(updated.Actors[slot].Cooldown > 0);
            assert!(updated.Orders.is_empty());
            match slot {
                0 => assert_eq!(game.actors[3].stun, 115),
                1 => assert!(updated.Objective.contains("opening")),
                _ => assert_eq!(game.actors[3].slow, 170),
            }
            game.actors[3].health = 1;
            game.actors[3].stun = 100;
            game.actors[3].travel = 0;
            game.actors[slot].cell = [game.actors[3].cell[0], game.actors[3].cell[1] + 1];
            game.actors[slot].travel = 0;
            game.actors[slot].strike = 0;
            let result = game.FixedUpdate(&[Attack(3)]).unwrap();
            assert_eq!(result.Actors[3].Health, 0);
            assert_eq!(result.Actors[3].Motion, ActorMotion::Death);
            assert!(result.Orders.is_empty());
        }
    }

    #[test]
    fn navigation_uses_stable_ties_and_routes_around_the_pier() {
        let next = NextStep([-3, 1], [-3, -3], 0, |_| false);
        assert_eq!(next, Some([-4, 1]));
        assert_eq!(
            NextStep([-3, 1], [-3, -3], 0, |cell| cell == [-4, 1]),
            Some([-2, 1])
        );
        assert!(!Visible([-3, 1], [-3, -3]));
    }

    fn Replay() -> TacticalView {
        let mut game = Encounter::New();
        for target in 3..6 {
            for slot in 0..3 {
                game.FixedUpdate(&[
                    SelectParty(slot),
                    Clear,
                    Inspect(target),
                    Ability(slot),
                    Attack(target),
                ])
                .unwrap();
            }
            if game.paused {
                game.FixedUpdate(&[TogglePause]).unwrap();
            }
            for _ in 0..1500 {
                if game.actors[target as usize].health == 0 || game.state != GameState::Playing {
                    break;
                }
                Tick(&mut game, 1);
            }
            assert_eq!(game.actors[target as usize].health, 0, "{:?}", game.View());
            game.FixedUpdate(&[TogglePause]).unwrap();
        }
        let survivor = (0..3)
            .find(|&slot| game.actors[slot as usize].health > 0)
            .unwrap();
        game.FixedUpdate(&[
            SelectParty(survivor),
            Clear,
            MoveTo([0.0, -6.0]),
            TogglePause,
        ])
        .unwrap();
        Tick(&mut game, 700);
        let won = game.View();
        assert_eq!(won.State, GameState::Won);
        assert_eq!(game.Start().unwrap().1, Encounter::New().View());
        won
    }

    #[test]
    fn tactical_party_can_win_and_replay_the_same_plan() {
        assert_eq!(Replay(), Replay());
    }

    #[test]
    fn abandoning_the_party_at_the_captain_leads_to_defeat() {
        let mut game = Encounter::New();
        game.FixedUpdate(&[
            MoveTo([0.0, -3.0]),
            SelectParty(1),
            MoveTo([-1.0, -3.0]),
            SelectParty(2),
            MoveTo([1.0, -3.0]),
            TogglePause,
        ])
        .unwrap();
        Tick(&mut game, 3000);
        let outcome = game.View();
        assert_eq!(outcome.State, GameState::Lost);
        assert!(outcome.Actors[..3].iter().all(|actor| actor.Health == 0));
    }
}
