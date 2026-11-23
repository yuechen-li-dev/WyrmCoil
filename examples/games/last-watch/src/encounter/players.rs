use super::*;
use wyrmcoil_game::{
    BattleActor, HUMAN_SEAT, MAX_PLAYER_EVENTS, PLAYER_REVISION_AGE, PLAYER_SEATS, PlayerAction,
    PlayerError, PlayerEvent, PlayerKind, PlayerRequest, PlayerResult, PlayerSeat, PlayerView,
};

#[derive(Clone, Copy, Default)]
struct Seat {
    claimed: bool,
    character: Option<usize>,
    target: Option<u16>,
    last: Option<PlayerEvent>,
}

pub(super) struct Controllers {
    seats: [Seat; PLAYER_SEATS],
    events: VecDeque<PlayerEvent>,
    sequence: u64,
    pub revision: u64,
}

impl Controllers {
    pub fn New() -> Self {
        let mut seats = [Seat::default(); PLAYER_SEATS];
        seats[0].claimed = true;
        seats[0].character = Some(0);
        Self {
            seats,
            events: VecDeque::with_capacity(MAX_PLAYER_EVENTS),
            sequence: 0,
            revision: 0,
        }
    }

    pub fn Human(&self) -> Option<usize> {
        self.seats[0].character
    }

    pub fn Owner(&self, index: usize) -> Option<u8> {
        self.seats
            .iter()
            .position(|seat| seat.character == Some(index))
            .map(|slot| slot as u8)
    }

    pub fn Guests(&self) -> bool {
        self.seats[1..].iter().any(|seat| seat.claimed)
    }

    pub fn Event(
        &mut self,
        frame: u64,
        request: PlayerRequest,
        actor: Option<usize>,
        result: PlayerResult,
    ) {
        self.sequence += 1;
        let event = PlayerEvent {
            Sequence: self.sequence,
            Tick: frame,
            Seat: request.Seat,
            Receipt: request.Receipt,
            Actor: actor.map(|id| id as u16),
            Action: request.Action,
            Result: result,
        };
        if let Some(seat) = self.seats.get_mut(usize::from(request.Seat)) {
            seat.last = Some(event);
        }
        if self.events.len() == MAX_PLAYER_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

impl Encounter {
    pub(super) fn HumanCommand(&mut self, command: TacticalCommand) {
        use PlayerAction::*;
        let action = match command {
            TacticalCommand::Player(request) => {
                self.PlayerCommand(request);
                return;
            }
            TacticalCommand::TogglePause if self.paused => Resume,
            TacticalCommand::TogglePause => Pause,
            TacticalCommand::SelectParty(slot) => Possess {
                character: slot.into(),
            },
            TacticalCommand::Inspect(character) if usize::from(character) < PARTY => {
                Possess { character }
            }
            TacticalCommand::Inspect(target) => Target { target },
            TacticalCommand::ReleaseParty => Release,
            TacticalCommand::MoveTo(position) => Move { position },
            TacticalCommand::Attack(target) => Attack { target },
            TacticalCommand::Ability(slot) if self.players.Human() == Some(slot.into()) => {
                let Some(target) = self.players.seats[0].target else {
                    self.notice = "Inspect an enemy before queuing an ability.";
                    return;
                };
                Ability { target }
            }
            TacticalCommand::Ability(_) => {
                self.notice = "Possess that adventurer first.";
                return;
            }
            TacticalCommand::Clear => Clear,
            TacticalCommand::Hold => Hold,
            TacticalCommand::SetPartyAutonomy(_) => return,
        };
        self.PlayerCommand(PlayerRequest {
            Seat: HUMAN_SEAT,
            Receipt: 0,
            Observed: self.players.revision,
            Action: action,
        });
    }

    pub(super) fn PlayerCommand(&mut self, request: PlayerRequest) {
        let before = self
            .players
            .seats
            .get(usize::from(request.Seat))
            .and_then(|seat| seat.character);
        let result = self
            .ApplyPlayer(request)
            .unwrap_or_else(PlayerResult::Rejected);
        let after = self
            .players
            .seats
            .get(usize::from(request.Seat))
            .and_then(|seat| seat.character);
        self.players
            .Event(self.frame, request, after.or(before), result);
        if request.Seat == HUMAN_SEAT || matches!(result, PlayerResult::Queued) {
            self.notice = match (result, request.Action) {
                (PlayerResult::Queued, _) => "Order queued.",
                (PlayerResult::Accepted, PlayerAction::Possess { .. }) => {
                    "Adventurer under player control."
                }
                (PlayerResult::Accepted, PlayerAction::Clear) => "Orders cleared.",
                (PlayerResult::Accepted, PlayerAction::Release) => {
                    "Adventurer released to autonomy."
                }
                (PlayerResult::Rejected(PlayerError::Occupied), _) => {
                    "Another player controls that adventurer."
                }
                (PlayerResult::Rejected(PlayerError::NoCharacter), _) => {
                    "Possess an adventurer to issue orders."
                }
                (PlayerResult::Rejected(PlayerError::InvalidCharacter), _) => {
                    "That adventurer is unavailable."
                }
                (PlayerResult::Rejected(PlayerError::InvalidTarget), _) => {
                    "That target is unavailable."
                }
                (PlayerResult::Rejected(PlayerError::InvalidPosition), _) => {
                    "That tile is blocked."
                }
                (PlayerResult::Rejected(PlayerError::Cooldown), _) => "Ability is on cooldown.",
                (PlayerResult::Rejected(PlayerError::QueueFull), _) => {
                    "Order queue full. Clear it with C."
                }
                _ => self.notice,
            };
        }
    }

    fn ApplyPlayer(&mut self, request: PlayerRequest) -> Result<PlayerResult, PlayerError> {
        use PlayerAction::*;
        let slot = usize::from(request.Seat);
        let Some(&seat) = self.players.seats.get(slot) else {
            return Err(PlayerError::Unavailable);
        };
        if !self.autonomy {
            return Err(PlayerError::Unavailable);
        }
        if matches!(request.Action, Leave) && slot != 0 {
            self.Transfer(slot, None);
            self.players.seats[slot] = Seat::default();
            return Ok(PlayerResult::Accepted);
        }
        if self.state != GameState::Playing {
            return Err(PlayerError::NotPlayable);
        }
        if request.Observed > self.players.revision
            || self.players.revision - request.Observed > PLAYER_REVISION_AGE
        {
            return Err(PlayerError::Stale);
        }
        if matches!(request.Action, Claim) && slot != 0 {
            if seat.claimed {
                return Err(PlayerError::AlreadyClaimed);
            }
            self.players.seats[slot].claimed = true;
            return Ok(PlayerResult::Accepted);
        }
        if !seat.claimed {
            return Err(PlayerError::Unclaimed);
        }
        match request.Action {
            Possess { character } => {
                let index = usize::from(character);
                if index >= PARTY || self.actors[index].health == 0 {
                    return Err(PlayerError::InvalidCharacter);
                }
                if self
                    .players
                    .Owner(index)
                    .is_some_and(|id| id != request.Seat)
                {
                    return Err(PlayerError::Occupied);
                }
                self.Transfer(slot, Some(index));
            }
            Release => self.Transfer(slot, None),
            Pause => self.paused = true,
            Resume => self.paused = false,
            Target { target } => {
                if !self.LiveEnemy(target.into()) {
                    return Err(PlayerError::InvalidTarget);
                }
                self.players.seats[slot].target = Some(target);
                if slot == 0 {
                    self.target = Some(target.into());
                }
            }
            Claim | Leave => return Err(PlayerError::Unavailable),
            action => {
                let index = seat.character.ok_or(PlayerError::NoCharacter)?;
                if self.actors[index].health == 0 {
                    return Err(PlayerError::InvalidCharacter);
                }
                if matches!(action, Clear) {
                    self.CancelOrders(index);
                    return Ok(PlayerResult::Accepted);
                }
                if self.actors[index].queue.len() == LIMIT {
                    return Err(PlayerError::QueueFull);
                }
                let order = match action {
                    Move { position } if position.into_iter().all(f32::is_finite) => {
                        let cell = position.map(|axis| axis.round() as i32);
                        if !Walkable(cell) {
                            return Err(PlayerError::InvalidPosition);
                        }
                        Order::Move(cell)
                    }
                    Move { .. } => return Err(PlayerError::InvalidPosition),
                    Attack { target } | Ability { target } => {
                        if !self.LiveEnemy(target.into()) {
                            return Err(PlayerError::InvalidTarget);
                        }
                        if matches!(action, Ability { .. }) && self.actors[index].cooldown != 0 {
                            return Err(PlayerError::Cooldown);
                        }
                        self.players.seats[slot].target = Some(target);
                        if slot == 0 {
                            self.target = Some(target.into());
                        }
                        if matches!(action, Attack { .. }) {
                            Order::Attack(target.into())
                        } else {
                            Order::UseAbility(target.into())
                        }
                    }
                    Hold => Order::Hold,
                    _ => unreachable!("non-orders handled before queueing"),
                };
                self.actors[index].queue.push_back(QueuedOrder {
                    order,
                    player: Some(request),
                    executed: false,
                });
                return Ok(PlayerResult::Queued);
            }
        }
        Ok(PlayerResult::Accepted)
    }

    pub(super) fn Transfer(&mut self, seat: usize, next: Option<usize>) {
        let previous = self.players.seats[seat].character;
        if previous == next {
            return;
        }
        if let Some(previous) = previous {
            self.resume[previous] = true;
            self.CancelOrders(previous);
        }
        self.players.seats[seat].character = next;
        if seat == 0
            && let Some(next) = next
        {
            self.selected = next;
        }
    }

    pub(super) fn CancelOrders(&mut self, index: usize) {
        for entry in self.actors[index].queue.drain(..) {
            if let Some(request) = entry.player {
                self.players
                    .Event(self.frame, request, Some(index), PlayerResult::Canceled);
            }
        }
    }

    pub(super) fn PlayerView(&self) -> PlayerView {
        let battle = Battle::Read(&self.actors, self.players.Human(), self.previous_proposals);
        PlayerView {
            Revision: self.players.revision,
            Seats: self
                .players
                .seats
                .iter()
                .enumerate()
                .map(|(index, seat)| PlayerSeat {
                    Id: index as u8,
                    Kind: if index == 0 {
                        PlayerKind::Human
                    } else {
                        PlayerKind::Llm
                    },
                    Claimed: seat.claimed,
                    Character: seat.character.map(|id| id as u16),
                    Target: seat.target,
                    Last: seat.last,
                    Orders: seat.character.map_or_else(Vec::new, |id| {
                        self.actors[id]
                            .queue
                            .iter()
                            .map(|entry| entry.order.View())
                            .collect()
                    }),
                })
                .collect(),
            Events: self.players.events.iter().copied().collect(),
            Obstacles: PIERS.to_vec(),
            Battle: self
                .actors
                .iter()
                .enumerate()
                .map(|(index, actor)| {
                    let target = actor.target.map(|id| id as u16);
                    let advantage = index >= PARTY && actor.target.is_some_and(|id| id != 1);
                    BattleActor {
                        Id: index as u16,
                        Target: target,
                        Facing: actor.facing,
                        Stunned: actor.stun > 0,
                        Slowed: actor.slow > 0,
                        Blocking: actor.block > 0,
                        HeavyThreat: index < PARTY && battle.Heavy(index),
                        Advantage: advantage,
                        Ability: (index < PARTY).then_some(index as u8),
                        Range: STATS[index].range as u8,
                    }
                })
                .collect(),
        }
    }
}
