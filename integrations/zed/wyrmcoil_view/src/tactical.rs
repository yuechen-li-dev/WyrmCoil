use gpui::{
    Bounds, Div, GpuViewportMeshInstance, ParentElement, Pixels, Point, Styled, Window, div, fill,
    point, px, rgb, size,
};
use wyrmcoil::Demo::room::{RoomCamera, RoomMatrix, Vec3};
use wyrmcoil::GameRuntime::collaboration::LeaseStatus;
use wyrmcoil::GameRuntime::{
    ActorMotion, AgentInspection, ControlMode, GameState, MAX_TACTICAL_COMMANDS, TacticalActor,
    TacticalCommand, TacticalOrder, TacticalView,
};

pub(super) struct TacticalPreview {
    pub view: TacticalView,
    pub bounds: Option<Bounds<Pixels>>,
    pub camera: RoomCamera,
    pub inspection: Option<usize>,
    commands: Vec<TacticalCommand>,
}

impl TacticalPreview {
    pub fn New(view: TacticalView, camera: RoomCamera) -> Self {
        Self {
            view,
            bounds: None,
            camera,
            inspection: None,
            commands: Vec::new(),
        }
    }

    pub fn Pan(&mut self, strafe: i32, forward: i32) -> bool {
        let before = self.camera.Eye;
        self.camera.Eye.x = (self.camera.Eye.x + strafe as f32 * 0.10).clamp(-8.5, 8.5);
        self.camera.Eye.z = (self.camera.Eye.z - forward as f32 * 0.10).clamp(1.0, 18.0);
        self.camera.Eye != before
    }

    pub fn Zoom(&mut self, amount: f32) {
        let forward = self.camera.Forward();
        let before = self.camera.Eye.y;
        self.camera.Eye.y = (before + forward.y * amount).clamp(6.0, 22.0);
        self.camera.Eye.z += (self.camera.Eye.y - before) * forward.z / forward.y;
    }

    pub fn TakeCommands(&mut self) -> Vec<TacticalCommand> {
        std::mem::take(&mut self.commands)
    }

    pub fn Cancel(&mut self) {
        self.commands.clear();
    }

    pub fn Queue(&mut self, command: TacticalCommand) {
        if self.commands.len() < MAX_TACTICAL_COMMANDS {
            self.commands.push(command);
        }
    }

    pub fn Key(&mut self, key: &str, repeat: bool) -> bool {
        if key == "i" && !self.view.Agents.is_empty() {
            if !repeat {
                self.inspection = match self.inspection {
                    None => Some(0),
                    Some(index) if index + 1 < self.view.Agents.len() => Some(index + 1),
                    Some(_) => None,
                };
            }
            return true;
        }
        let command = match key {
            "space" | " " => TacticalCommand::TogglePause,
            "escape" if !self.view.Paused => TacticalCommand::TogglePause,
            "escape" => return true,
            "1" => TacticalCommand::SelectParty(0),
            "2" => TacticalCommand::SelectParty(1),
            "3" => TacticalCommand::SelectParty(2),
            "q" => TacticalCommand::Ability(0),
            "e" => TacticalCommand::Ability(1),
            "r" => TacticalCommand::Ability(2),
            "c" => TacticalCommand::Clear,
            "h" => TacticalCommand::Hold,
            "y" if !self.view.Agents.is_empty() => TacticalCommand::ReleaseParty,
            _ => return false,
        };
        if !repeat {
            self.Queue(command);
        }
        true
    }

    pub fn Click(&mut self, point: Point<Pixels>, order: bool) -> bool {
        let Some(bounds) = self.bounds else {
            return false;
        };
        let Some(ray) = ScreenRay(self.camera, bounds, point) else {
            return false;
        };
        let picked = self
            .view
            .Actors
            .iter()
            .filter(|actor| actor.Health > 0)
            .filter_map(|actor| ActorHit(self.camera.Eye, ray, actor).map(|hit| (hit, actor)))
            .min_by(|(a, _), (b, _)| a.total_cmp(b));
        if let Some((_, actor)) = picked {
            let id = actor.Id;
            if order && actor.PartySlot.is_none() {
                self.Queue(TacticalCommand::Attack(id));
            } else if !order {
                self.Queue(TacticalCommand::Inspect(id));
            }
        } else if order && ray.y < -0.0001 {
            let target = self.camera.Eye + ray * (-self.camera.Eye.y / ray.y);
            self.Queue(TacticalCommand::MoveTo([target.x, target.z]));
        }
        true
    }
}

fn ScreenRay(camera: RoomCamera, bounds: Bounds<Pixels>, at: Point<Pixels>) -> Option<Vec3> {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if !bounds.contains(&at) || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let [u, v] = [
        f32::from(at.x - bounds.origin.x) * 2.0 / width - 1.0,
        1.0 - f32::from(at.y - bounds.origin.y) * 2.0 / height,
    ];
    let right = camera.Right();
    let forward = camera.Forward();
    let up = right.Cross(forward);
    let s = (camera.VerticalFov * 0.5).tan();
    let aspect = width / height;
    Some(forward + right * (u * s * aspect) + up * (v * s))
}

fn ActorHit(eye: Vec3, ray: Vec3, actor: &TacticalActor) -> Option<f32> {
    let [x, z] = actor.Position;
    let offset = eye - Vec3::New(x, 0.72, z);
    let aa = ray.Dot(ray);
    let bb = offset.Dot(ray);
    let cc = offset.Dot(offset) - 0.76 * 0.76;
    let discriminant = bb * bb - aa * cc;
    if discriminant < 0.0 {
        return None;
    }
    let distance = (-bb - discriminant.sqrt()) / aa;
    (distance >= 0.0).then_some(distance)
}

pub(super) fn PlayerSelection(view: &TacticalView) -> Option<u16> {
    if view.Agents.is_empty() {
        Some(view.Selected)
    } else {
        view.Agents
            .iter()
            .find(|agent| agent.Mode == ControlMode::PlayerControlled)
            .map(|agent| agent.Actor)
    }
}

pub(super) fn Status(
    view: &TacticalView,
    controls: &'static str,
    inspection: Option<usize>,
    local_players: Option<&[LeaseStatus]>,
) -> Div {
    let state = match (view.State, view.Paused) {
        (GameState::Won, _) => "Victory · restart from the command palette",
        (GameState::Lost, _) => "Defeat · restart from the command palette",
        (GameState::Playing, true) => "Tactical pause",
        (GameState::Playing, false) => "Real time",
    };
    let mut party = div().flex().gap_3();
    for actor in view.Actors.iter().filter(|actor| actor.PartySlot.is_some()) {
        let slot = actor.PartySlot.unwrap_or_default() as usize;
        let abilities = ["Q Shield Bash", "E Backstab", "R Frost Bolt"];
        let ability = abilities.get(slot).copied().unwrap_or("Ability");
        let cooldown = if actor.Cooldown > 0 {
            format!("{ability} · {:.1}s", f32::from(actor.Cooldown) / 60.0)
        } else {
            format!("{ability} · ready")
        };
        let health = f32::from(actor.Health) / f32::from(actor.MaxHealth);
        let mode = view
            .Agents
            .iter()
            .find(|agent| agent.Actor == actor.Id)
            .map(|agent| agent.Mode);
        let possessed = mode == Some(ControlMode::PlayerControlled);
        party = party.child(
            div()
                .w(px(180.0))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(
                    if possessed || (mode.is_none() && actor.Id == view.Selected) {
                        0x375369
                    } else {
                        0x263247
                    },
                ))
                .child(format!(
                    "{}  {}  {}/{}",
                    slot + 1,
                    actor.Name,
                    actor.Health,
                    actor.MaxHealth
                ))
                .child(
                    div()
                        .h(px(3.0))
                        .bg(rgb(0x151b26))
                        .child(div().h_full().w(px(164.0 * health)).bg(rgb(0x7acf90))),
                )
                .child(match mode {
                    Some(ControlMode::Autonomous) => format!("{cooldown} · AI"),
                    Some(ControlMode::PlayerControlled) => format!("{cooldown} · Player"),
                    Some(ControlMode::LlmControlled) => format!("{cooldown} · LLM"),
                    None => cooldown,
                }),
        );
    }
    let mut orders = String::new();
    for order in &view.Orders {
        use std::fmt::Write;
        if !orders.is_empty() {
            orders.push_str(" → ");
        }
        let name = |id| {
            view.Actors
                .iter()
                .find(|actor| actor.Id == id)
                .map(|actor| actor.Name.as_str())
                .unwrap_or("unknown")
        };
        match order {
            TacticalOrder::MoveTo([x, z]) => {
                let _ = write!(orders, "Move {x:.0},{z:.0}");
            }
            TacticalOrder::Attack(id) => {
                let _ = write!(orders, "Attack {}", name(*id));
            }
            TacticalOrder::Ability(id) => {
                let _ = write!(orders, "Ability → {}", name(*id));
            }
            TacticalOrder::Hold => orders.push_str("Hold"),
        }
    }
    if orders.is_empty() {
        orders.push_str("No orders");
    }
    let target = view
        .Target
        .and_then(|id| view.Actors.iter().find(|actor| actor.Id == id));
    let target = target.map_or(String::new(), |actor| {
        format!(
            "  ·  Target {} {}/{}",
            actor.Name, actor.Health, actor.MaxHealth
        )
    });
    let guards = view
        .Actors
        .iter()
        .filter(|actor| actor.PartySlot.is_none() && actor.Health > 0)
        .count();
    let mut status = div()
        .flex_none()
        .flex()
        .flex_col()
        .gap_1()
        .text_sm()
        .px_3()
        .py_2()
        .child(format!(
            "{state}  ·  {}  ·  {guards} guards left  ·  tick {}",
            view.Objective, view.Frame
        ))
        .child(controls)
        .child(party)
        .child(div().truncate().child(format!("{orders}{target}")));
    if !view.Agents.is_empty() {
        status = status.child(
            div()
                .truncate()
                .child("Y Release party  ·  I Cycle inspector"),
        );
    }
    if let Some(local_players) = local_players
        && let Some(players) = &view.Players
    {
        let mut summary = String::from("Local MCP");
        for seat in players.Seats.iter().skip(1) {
            use std::fmt::Write;
            let label = if seat.Claimed {
                "connected"
            } else if seat.Last.is_some() {
                "disconnected"
            } else {
                "available"
            };
            let _ = write!(summary, "  ·  LLM {} {label}", seat.Id);
            if let Some(lease) = local_players.iter().find(|lease| lease.Seat == seat.Id) {
                let _ = write!(summary, " (idle {}s)", lease.IdleMs / 1_000);
            }
        }
        status = status.child(div().truncate().child(summary));
    }
    if let Some(agent) = inspection.and_then(|index| view.Agents.get(index)) {
        status = status.child(AgentStatus(view, agent));
    }
    status
}

fn AgentStatus(view: &TacticalView, agent: &AgentInspection) -> Div {
    let name = |id| {
        view.Actors
            .iter()
            .find(|actor| actor.Id == id)
            .map_or("—", |actor| actor.Name.as_str())
    };
    let owner = view
        .Players
        .as_ref()
        .and_then(|p| p.Seats.iter().find(|s| s.Character == Some(agent.Actor)));
    let mode = owner.map_or_else(
        || {
            if agent.Mode == ControlMode::Autonomous {
                "Autonomous".into()
            } else {
                "Player".into()
            }
        },
        |seat| {
            if seat.Id == 0 {
                "Human · seat 0 · brain suspended".into()
            } else {
                format!("LLM · seat {} · brain suspended", seat.Id)
            }
        },
    );
    let mut panel = div()
        .rounded_md()
        .bg(rgb(0x202b3d))
        .px_2()
        .py_1()
        .child(div().truncate().child(format!(
            "{} · {mode} · brain tick {} · target {}{}",
            name(agent.Actor),
            agent.Clock,
            agent.Target.map_or("—", name),
            if agent.Blocking { " · blocking" } else { "" },
        )))
        .child(div().truncate().child(agent.Stack.join(" → ")));
    if let Some(event) = owner.and_then(|seat| seat.Last) {
        panel = panel.child(div().truncate().child(format!(
            "Player {:?} · {:?} at tick {}",
            event.Action, event.Result, event.Tick
        )));
    }
    if let Some(decision) = &agent.Decision {
        let candidates = decision
            .Candidates
            .iter()
            .map(|(option, score)| match score {
                Some(score) => format!("{option} {score:.2}"),
                None => format!("{option} —"),
            })
            .collect::<Vec<_>>()
            .join("  ·  ");
        let current = decision
            .Current
            .as_ref()
            .map_or_else(|| "—".into(), |(_, score)| format!("{score:.2}"));
        panel = panel
            .child(div().truncate().child(candidates))
            .child(div().truncate().child(format!(
                "{} · {} · current {current} · margin {:.2} · commitment {} at world tick {}",
                decision.Selected.as_deref().unwrap_or("—"),
                decision.Reason,
                decision.Hysteresis,
                decision.CommitRemaining,
                decision.WorldTick,
            )));
    } else {
        panel = panel
            .child(div().truncate().child("No utility evaluation yet"))
            .child(div().truncate().child("—"));
    }
    let transition = agent
        .Transitions
        .last()
        .map_or_else(|| "—".into(), |t| format!("{} @{}", t.Reason, t.WorldTick));
    let command = agent.Commands.last().map_or_else(
        || "—".into(),
        |c| format!("{:?} {:?} @{}", c.Order, c.Outcome, c.Tick),
    );
    panel.child(
        div()
            .truncate()
            .child(format!("Transition {transition}  ·  Command {command}")),
    )
}

pub(super) fn AppendInstances(
    view: &TacticalView,
    projection: RoomMatrix,
    instances: &mut Vec<GpuViewportMeshInstance>,
) {
    let mut push = |position, scale, yaw, color| {
        instances.push(GpuViewportMeshInstance {
            clip_from_model: projection
                .Multiply(RoomMatrix::Placement(position, scale, yaw))
                .0,
            color,
            indices: [0; 3],
        })
    };
    let selected = PlayerSelection(view);
    for actor in &view.Actors {
        let [x, z] = actor.Position;
        if actor.Health == 0 {
            continue;
        }
        let color = match actor.PartySlot {
            Some(0) => [0.22, 0.65, 0.95, 1.0],
            Some(1) => [0.53, 0.84, 0.38, 1.0],
            Some(_) => [0.71, 0.46, 0.96, 1.0],
            None => [0.94, 0.30, 0.29, 1.0],
        };
        let offset = if actor.Motion == ActorMotion::Walk {
            (f32::from(actor.MotionTicks) * 0.34).sin().abs() * 0.08
        } else {
            0.0
        };
        push([x, 0.62 + offset, z], [0.54, 1.22, 0.46], -actor.Yaw, color);
        if selected == Some(actor.Id) || view.Target == Some(actor.Id) {
            let marker = if selected == Some(actor.Id) {
                [0.23, 0.78, 0.96, 1.0]
            } else {
                [0.98, 0.57, 0.26, 1.0]
            };
            push([x, 0.048, z], [1.05, 0.07, 1.05], 0.0, marker);
        }
    }
    for order in &view.Orders {
        if let TacticalOrder::MoveTo([x, z]) = *order {
            push(
                [x, 0.075, z],
                [0.30, 0.10, 0.30],
                0.78,
                [0.30, 0.86, 1.0, 1.0],
            );
        }
    }
}

pub(super) fn PaintHealth(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    projection: RoomMatrix,
    view: &TacticalView,
) {
    for actor in &view.Actors {
        if actor.Health == 0 {
            continue;
        }
        let [x, z] = actor.Position;
        let [cx, cy, cz, cw] = projection.Transform([x, 1.60, z, 1.0]);
        if cw <= 0.0 || cz < 0.0 || cz > cw || cx.abs() > cw || cy.abs() > cw {
            continue;
        }
        let x = bounds.origin.x + bounds.size.width * ((cx / cw + 1.0) * 0.5) - px(20.0);
        let y = bounds.origin.y + bounds.size.height * ((1.0 - cy / cw) * 0.5);
        if !bounds.contains(&point(x, y)) || !bounds.contains(&point(x + px(40.0), y + px(4.0))) {
            continue;
        }
        let health = f32::from(actor.Health) / f32::from(actor.MaxHealth);
        let color = if actor.PartySlot.is_some() {
            0x86c9ff
        } else {
            0xfd7770
        };
        window.paint_quad(fill(
            Bounds::new(point(x, y), size(px(40.0), px(4.0))),
            rgb(0x151b26),
        ));
        window.paint_quad(fill(
            Bounds::new(point(x, y), size(px(40.0 * health), px(4.0))),
            rgb(color),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_actor_and_ground_round_trip_through_viewport_picking() {
        let camera = RoomCamera {
            Eye: Vec3::New(0.0, 13.0, 9.5),
            Yaw: 0.0,
            Pitch: -0.94,
            VerticalFov: 0.88,
            Near: 0.1,
            Far: 90.0,
        };
        let bounds = Bounds::new(point(px(20.0), px(80.0)), size(px(900.0), px(600.0)));
        let projection = camera.ViewProjection(1.5).unwrap();
        let pixel = |position| {
            let [x, y, _, w] = projection.Transform(position);
            point(
                bounds.origin.x + bounds.size.width * ((x / w + 1.0) * 0.5),
                bounds.origin.y + bounds.size.height * ((1.0 - y / w) * 0.5),
            )
        };
        let ground = pixel([2.0, 0.0, -3.0, 1.0]);
        let ray = ScreenRay(camera, bounds, ground).unwrap();
        let hit = camera.Eye + ray * (-camera.Eye.y / ray.y);
        assert!((hit.x - 2.0).abs() < 0.01 && (hit.z + 3.0).abs() < 0.01);
        assert!(ScreenRay(camera, bounds, point(px(3.0), px(4.0))).is_none());

        let actor = TacticalActor {
            Id: 3,
            Name: "Guard".into(),
            PartySlot: None,
            Position: [2.0, -3.0],
            Yaw: 0.0,
            Health: 10,
            MaxHealth: 10,
            Cooldown: 0,
            Motion: ActorMotion::Idle,
            MotionTicks: 0,
        };
        let mut preview = TacticalPreview::New(
            TacticalView {
                Players: None,
                Agents: Vec::new(),
                Frame: 0,
                Paused: true,
                State: GameState::Playing,
                Objective: String::new(),
                Selected: 3,
                Target: None,
                Actors: vec![actor],
                Orders: Vec::new(),
                Relic: [0.0; 2],
            },
            camera,
        );
        preview.bounds = Some(bounds);
        assert!(preview.Click(pixel([2.0, 0.72, -3.0, 1.0]), true));
        assert_eq!(preview.TakeCommands(), vec![TacticalCommand::Attack(3)]);
        preview.Key("space", false);
        preview.Key("space", true);
        assert_eq!(preview.TakeCommands(), vec![TacticalCommand::TogglePause]);
        preview.Key("q", false);
        preview.Cancel();
        assert!(preview.TakeCommands().is_empty());
    }
}
