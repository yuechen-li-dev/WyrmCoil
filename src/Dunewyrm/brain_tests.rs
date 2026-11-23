use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Root,
    A,
    B,
    Child,
    Reaction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Main,
    Other,
    Empty,
}

#[derive(Clone, Copy, Default)]
enum Action {
    #[default]
    Decide,
    Push,
    Succeed,
    Fail,
    Pop,
    RootPop,
    Goto,
    Wait,
}

#[derive(Default)]
struct Facts {
    scores: [Option<f32>; 2],
    slot: Option<Slot>,
    action: Action,
    leaf_normal: bool,
    leaf_interrupt: bool,
    root_interrupt: bool,
}

#[derive(Default)]
struct Memory {
    steps: Vec<(State, u8)>,
    entered: Vec<State>,
    exited: Vec<State>,
    returned: Vec<ChildReturn<State>>,
}

#[derive(Clone, Copy, Default)]
struct Script {
    states: Option<&'static [State]>,
    options: Option<&'static [State]>,
    tie: bool,
}

const STATES: &[State] = &[
    State::Root,
    State::A,
    State::B,
    State::Child,
    State::Reaction,
];
const SLOTS: &[Slot] = &[Slot::Main, Slot::Other, Slot::Empty];
const ROOT_RULES: &[Transition<Script>] = &[Transition {
    Kind: TransitionKind::Interrupt,
    Target: State::Reaction,
    Push: false,
    DependsOn: 1,
    When: |f, _, _| f.root_interrupt,
    Reason: "Root interrupt",
}];
const LEAF_RULES: &[Transition<Script>] = &[
    Transition {
        Kind: TransitionKind::Normal,
        Target: State::B,
        Push: false,
        DependsOn: 2,
        When: |f, _, _| f.leaf_normal,
        Reason: "Leaf normal",
    },
    Transition {
        Kind: TransitionKind::Interrupt,
        Target: State::Reaction,
        Push: true,
        DependsOn: 4,
        When: |f, _, _| f.leaf_interrupt,
        Reason: "First leaf interrupt",
    },
    Transition {
        Kind: TransitionKind::Interrupt,
        Target: State::B,
        Push: false,
        DependsOn: 4,
        When: |f, _, _| f.leaf_interrupt,
        Reason: "Second leaf interrupt",
    },
];

impl Brain for Script {
    type State = State;
    type Slot = Slot;
    type Frame = u8;
    type Memory = Memory;
    type Observation = Facts;
    type Command = u8;

    fn States(&self) -> &'static [State] {
        self.states.unwrap_or(STATES)
    }
    fn Slots(&self) -> &'static [Slot] {
        SLOTS
    }
    fn Root(&self) -> State {
        State::Root
    }

    fn Decision(&self, slot: Slot) -> Decision<State> {
        Decision {
            Options: self.options.unwrap_or(match slot {
                Slot::Main | Slot::Other => &[State::A, State::B],
                Slot::Empty => &[],
            }),
            Policy: DecisionPolicy {
                Hysteresis: if self.tie { 0.0 } else { 0.10 },
                MinCommitTicks: if self.tie { 0 } else { 2 },
                TieEpsilon: 0.02,
            },
        }
    }

    fn Score(&self, _: Slot, option: State, facts: &Facts, _: &Memory) -> Option<f32> {
        facts.scores[usize::from(option == State::B)]
    }

    fn Enter(&self, state: State, memory: &mut Memory) -> u8 {
        memory.entered.push(state);
        0
    }

    fn Exit(&self, state: State, _: u8, memory: &mut Memory) {
        memory.exited.push(state);
    }

    fn Step(&self, state: State, frame: &mut u8, ctx: Context<'_, Self>) -> Step<State, Slot, u8> {
        *frame += 1;
        ctx.Memory.steps.push((state, *frame));
        if let Some(child) = ctx.Child.take() {
            ctx.Memory.returned.push(child);
        }
        if state == State::Root && matches!(ctx.Observation.action, Action::Decide) {
            return Step::Decide(ctx.Observation.slot.unwrap_or(Slot::Main));
        }
        match (state, ctx.Observation.action) {
            (State::Root | State::A, Action::Push) if *frame == 1 || state == State::A => {
                Step::Push(if state == State::Root {
                    State::A
                } else {
                    State::Child
                })
            }
            (State::Root, Action::RootPop) => Step::Pop,
            (State::Root, _) => Step::Stay,
            (_, Action::Succeed) => Step::Succeed,
            (_, Action::Fail) => Step::Fail,
            (_, Action::Pop) => Step::Pop,
            (_, Action::Goto) => Step::Goto(State::B),
            (_, Action::Wait) => Step::WaitUntil(ctx.Tick + 2),
            _ => Step::Command(*frame),
        }
    }

    fn Transitions(&self, state: State) -> &'static [Transition<Self>] {
        match state {
            State::Root => ROOT_RULES,
            State::A => LEAF_RULES,
            _ => &[],
        }
    }
}

fn ScheduleWith(overlay: bool, normal: u64) -> Schedule {
    Schedule {
        KeepRootFrame: overlay,
        InterruptEvery: 1,
        TransitionEvery: normal,
    }
}

fn New() -> Agent<Script> {
    Agent::New(Script::default(), Memory::default(), ScheduleWith(true, 1)).unwrap()
}

fn Scores(a: f32, b: f32) -> Facts {
    Facts {
        scores: [Some(a), Some(b)],
        ..Facts::default()
    }
}

#[test]
fn utility_recomputes_scores_and_retains_the_live_frame_before_switching() {
    let mut a = New();
    use DecisionReason::*;
    assert_eq!(a.Stack(), [State::Root]);
    assert_eq!(a.Tick(&Scores(0.6, 0.5), 0, false), Ok(None));
    assert_eq!(a.InspectMemory().entered, [State::Root, State::A]);
    assert_eq!(a.InspectMemory().steps, [(State::Root, 1)]);
    assert_eq!(a.Tick(&Scores(0.8, 0.94), 0, false), Ok(Some(1)));
    assert_eq!(a.Reports().next().unwrap().Reason, MinimumCommitment);
    assert_eq!(a.Reports().next().unwrap().Current, Some((State::A, 0.8)));
    assert_eq!(a.Tick(&Scores(0.91, 0.98), 0, false), Ok(Some(2)));
    assert_eq!(a.Reports().next().unwrap().Reason, Hysteresis);
    assert_eq!(a.Tick(&Scores(0.98, 0.92), 0, false), Ok(Some(3)));
    assert_eq!(a.Reports().next().unwrap().Reason, BestIsCurrent);
    assert_eq!(
        a.InspectMemory().steps[5..7],
        [(State::Root, 4), (State::A, 3)]
    );
    assert_eq!(a.Tick(&Scores(0.71, 0.98), 0, false), Ok(None));
    assert_eq!(a.Stack(), [State::Root, State::B]);
    assert_eq!(a.InspectMemory().entered, [State::Root, State::A, State::B]);
    assert_eq!(a.InspectMemory().exited, [State::A]);
}

#[test]
fn utility_ties_invalid_scores_empty_sets_and_slots_are_explicit() {
    let mut a = Agent::New(
        Script {
            tie: true,
            ..Script::default()
        },
        Memory::default(),
        ScheduleWith(true, 1),
    )
    .unwrap();
    a.Tick(&Scores(0.7, 0.7), 0, false).unwrap();
    assert_eq!(a.Reports().next().unwrap().Selected, Some(State::A));
    a.Tick(&Scores(0.7, 0.711), 0, false).unwrap();
    assert_eq!(
        a.Reports().next().unwrap().Reason,
        DecisionReason::PreferCurrentTie
    );
    let other = Facts {
        slot: Some(Slot::Other),
        ..Scores(0.7, 0.711)
    };
    a.Tick(&other, 0, false).unwrap();
    assert_eq!(
        a.Reports()
            .find(|report| report.Slot == Slot::Other)
            .unwrap()
            .Selected,
        Some(State::B)
    );
    a.Tick(&Scores(f32::NAN, f32::INFINITY), 0, false).unwrap();
    let main = a.Reports().next().unwrap();
    assert_eq!(main.Reason, DecisionReason::InvalidUtility);
    assert_eq!(
        main.Candidates,
        [(State::A, Utility::Invalid), (State::B, Utility::Invalid)]
    );
    a.Tick(
        &Facts {
            slot: Some(Slot::Empty),
            ..Facts::default()
        },
        0,
        false,
    )
    .unwrap();
    assert_eq!(
        a.Reports().last().unwrap().Reason,
        DecisionReason::NoValidOption
    );
    assert_eq!(a.Stack(), [State::Root, State::B]);
    a.Tick(&Scores(0.7, 0.711), 0, false).unwrap();
    assert_eq!(a.Reports().next().unwrap().Selected, Some(State::A));
    assert_eq!(
        a.Reports().next().unwrap().Reason,
        DecisionReason::PreferCurrentTie
    );
    assert_eq!(
        a.Stack(),
        [State::Root, State::B],
        "a retained decision leaves the active stack alone"
    );
}

#[test]
fn nested_states_return_typed_results_to_their_live_parent_and_unwind_in_order() {
    for (action, kind) in [
        (Action::Succeed, ReturnKind::Succeeded),
        (Action::Fail, ReturnKind::Failed),
        (Action::Pop, ReturnKind::Returned),
    ] {
        let mut a = New();
        a.Tick(&Scores(0.8, 0.5), 0, false).unwrap();
        a.Tick(
            &Facts {
                action: Action::Push,
                ..Facts::default()
            },
            0,
            false,
        )
        .unwrap();
        assert_eq!(a.Stack(), [State::Root, State::A, State::Child]);
        a.Tick(
            &Facts {
                action,
                ..Facts::default()
            },
            0,
            false,
        )
        .unwrap();
        assert_eq!(a.Stack(), [State::Root, State::A]);
        assert_eq!(a.Tick(&Scores(0.8, 0.5), 0, false), Ok(Some(2)));
        assert_eq!(
            a.InspectMemory().returned,
            [ChildReturn {
                State: State::Child,
                Kind: kind
            }]
        );
        a.Tick(
            &Facts {
                action: Action::Push,
                ..Facts::default()
            },
            0,
            false,
        )
        .unwrap();
        a.Tick(&Scores(0.3, 0.8), 0, false).unwrap();
        assert_eq!(a.Stack(), [State::Root, State::B]);
        assert_eq!(
            a.InspectMemory().exited,
            [State::Child, State::Child, State::A]
        );
    }
}

#[test]
fn scan_priority_is_per_frame_leaf_first_and_respects_dirty_masks_and_cadence() {
    let mut a = Agent::New(Script::default(), Memory::default(), ScheduleWith(true, 3)).unwrap();
    a.Tick(&Scores(0.8, 0.5), 0, false).unwrap();
    let f = Facts {
        leaf_normal: true,
        root_interrupt: true,
        ..Scores(0.8, 0.5)
    };
    for dirty in [2, 0] {
        a.Tick(&f, dirty, false).unwrap();
        assert_eq!(a.Stack(), [State::Root, State::A]);
    }
    a.Tick(&f, 3, false).unwrap();
    assert_eq!(a.Trace().back().unwrap().Reason, "Leaf normal");
    assert_eq!(a.Stack(), [State::Root, State::B]);
    a.Tick(&f, 0, false).unwrap();
    assert_eq!(a.Trace().back().unwrap().Reason, "Root interrupt");
    assert_eq!(a.Stack(), [State::Root, State::Reaction]);

    let mut a = New();
    a.Tick(&Scores(0.8, 0.5), 0, false).unwrap();
    let f = Facts {
        leaf_interrupt: true,
        leaf_normal: true,
        ..Scores(0.8, 0.5)
    };
    a.Tick(&f, 6, false).unwrap();
    assert_eq!(a.Trace().back().unwrap().Reason, "First leaf interrupt");
    assert_eq!(a.Stack(), [State::Root, State::A, State::Reaction]);
}

#[test]
fn goto_wait_root_reentry_and_registration_errors() {
    let mut overlay = New();
    overlay.Tick(&Scores(0.8, 0.5), 0, false).unwrap();
    overlay
        .Tick(
            &Facts {
                action: Action::RootPop,
                ..Facts::default()
            },
            0,
            false,
        )
        .unwrap();
    assert_eq!(overlay.Stack(), [State::Root]);
    assert_eq!(overlay.InspectMemory().entered, [State::Root, State::A]);
    assert_eq!(overlay.InspectMemory().exited, [State::A]);
    overlay.Tick(&Scores(0.8, 0.5), 0, false).unwrap();
    assert_eq!(
        overlay.InspectMemory().returned,
        [ChildReturn {
            State: State::A,
            Kind: ReturnKind::Returned
        }]
    );

    let mut a = Agent::New(Script::default(), Memory::default(), ScheduleWith(false, 1)).unwrap();
    a.Tick(
        &Facts {
            action: Action::Push,
            ..Facts::default()
        },
        0,
        false,
    )
    .unwrap();
    a.Tick(
        &Facts {
            action: Action::Goto,
            ..Facts::default()
        },
        0,
        false,
    )
    .unwrap();
    assert_eq!(a.Stack(), [State::Root, State::B]);
    a.Tick(
        &Facts {
            action: Action::Wait,
            ..Facts::default()
        },
        0,
        false,
    )
    .unwrap();
    assert_eq!(a.Tick(&Scores(0.8, 0.5), 0, false), Ok(None));
    assert_eq!(a.Tick(&Scores(0.8, 0.5), 0, false), Ok(Some(2)));
    a.Tick(
        &Facts {
            action: Action::Pop,
            ..Facts::default()
        },
        0,
        false,
    )
    .unwrap();
    a.Tick(&Scores(0.8, 0.5), 0, false).unwrap();
    assert_eq!(a.Stack(), [State::A]);
    a.Tick(
        &Facts {
            action: Action::Pop,
            ..Facts::default()
        },
        0,
        false,
    )
    .unwrap();
    assert_eq!(a.Stack(), [State::Root]);
    assert_eq!(a.InspectMemory().entered.last(), Some(&State::Root));

    for (script, error) in [
        (
            Script {
                states: Some(&[State::A]),
                ..Script::default()
            },
            "root must be a registered state",
        ),
        (
            Script {
                states: Some(&[State::Root, State::Reaction, State::Root]),
                ..Script::default()
            },
            "duplicate state",
        ),
        (
            Script {
                states: Some(&[State::Root, State::A]),
                ..Script::default()
            },
            "invalid transition target",
        ),
        (
            Script {
                options: Some(&[State::A, State::A]),
                ..Script::default()
            },
            "invalid or duplicate decision option",
        ),
    ] {
        assert_eq!(
            Agent::New(script, Memory::default(), ScheduleWith(true, 1)).err(),
            Some(error)
        );
    }
}

#[test]
fn diagnostic_histories_are_bounded_and_distinguish_world_and_brain_time() {
    let mut a = New();
    for tick in 100..125 {
        let scores = if tick % 6 < 3 {
            Scores(1.0, 0.0)
        } else {
            Scores(0.0, 1.0)
        };
        a.TickAt(&scores, 0, tick == 100, tick).unwrap();
    }
    assert_eq!(a.DecisionHistory().len(), TRACE_LIMIT);
    assert_eq!(a.Trace().len(), TRACE_LIMIT);
    let last = a.Reports().next().unwrap();
    assert_eq!((last.WorldTick, last.Tick), (124, 25));
    assert_eq!(a.DecisionHistory().back().unwrap().WorldTick, 123);
    assert_eq!(
        a.TickAt(&Scores(1.0, 0.0), 0, false, 124),
        Err("world tick must advance")
    );
    assert_eq!(a.Clock(), 25);
}

#[test]
#[ignore = "release-mode CPU timing probe"]
fn profile_branch_replacement_and_trace() {
    use std::{hint::black_box, time::Instant};
    const SAMPLES: u128 = 100_000;
    for trace in [false, true] {
        let mut agent = New();
        agent.Enter(State::A);
        agent.Enter(State::Child);
        let start = Instant::now();
        for _ in 0..SAMPLES {
            let before = trace.then(|| agent.Stack());
            agent.ExitTo(1);
            agent.Enter(State::A);
            agent.Enter(State::Child);
            if let Some(before) = before {
                agent.Record(before, "profile replacement", None);
            }
            black_box(&agent.stack);
            agent.memory.entered.clear();
            agent.memory.exited.clear();
        }
        println!(
            "trace={trace} replace_two_frames={}ns fixture_active_frame={}B",
            start.elapsed().as_nanos() / SAMPLES,
            size_of::<Active<Script>>(),
        );
    }
}
