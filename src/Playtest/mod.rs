//! Run compiled games with typed input and fixed-step replay.
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::Demo::room::RoomInput;
use crate::GameRuntime::{
    GameSession, GameState, Observation, PROTOCOL, TacticalCommand, TacticalView,
};

pub const REPORT_VERSION: u32 = 1;
const MAX_REQUESTS: usize = 20_000;
const MAX_ASSERTIONS: usize = 256;
const MAX_RUN: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Facts {
    Room {
        tick: u64,
        observation: Observation,
        player: [f32; 3],
        guard: [f32; 3],
        camera_angles: [f32; 2],
        boxes: Vec<String>,
    },
    Tactical {
        view: TacticalView,
    },
}

impl Facts {
    pub fn Tick(&self) -> u64 {
        match self {
            Self::Room { tick, .. } => *tick,
            Self::Tactical { view } => view.Frame,
        }
    }

    pub fn State(&self) -> GameState {
        match self {
            Self::Room { observation, .. } => observation.State,
            Self::Tactical { view } => view.State,
        }
    }

    pub fn Tactical(&self) -> Result<&TacticalView, String> {
        match self {
            Self::Room { .. } => Err("this game exposes room facts".into()),
            Self::Tactical { view } => Ok(view),
        }
    }
}

#[derive(Clone)]
pub enum Input {
    Idle,
    Navigate(RoomInput),
    Tactical(Vec<TacticalCommand>),
}

#[derive(Serialize)]
struct Assertion {
    step: usize,
    tick: u64,
    source: String,
    label: String,
    passed: bool,
    detail: String,
}

#[derive(Serialize)]
struct Divergence {
    request: usize,
    tick: u64,
    expected: String,
    actual: String,
}

#[derive(Serialize)]
struct Replay {
    requests: usize,
    matched: usize,
    elapsed_ms: u128,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    protocol_version: u32,
    executable: PathBuf,
    executable_identity: Option<String>,
    scenario: String,
    game: Option<String>,
    sessions_started: usize,
    executed_steps: usize,
    final_tick: Option<u64>,
    observation_identity: Option<String>,
    final_observation: Option<Facts>,
    assertions: Vec<Assertion>,
    replays: Vec<Replay>,
    first_divergence: Option<Divergence>,
    first_failure: Option<String>,
    elapsed_ms: u128,
    exit_status: i32,
    artifacts: Vec<String>,
}

pub struct Runner {
    project: PathBuf,
    started: Instant,
    session: Option<GameSession>,
    facts: Option<Facts>,
    initial_identity: u64,
    history: Vec<(Input, u64)>,
    requests: usize,
    report: Report,
}

impl Runner {
    fn Start(&mut self) -> Result<(), String> {
        self.CheckBudget()?;
        if self.report.sessions_started >= 64 {
            return Err("playtest session limit reached".into());
        }
        if let Some(session) = self.session.take() {
            session.Close();
        }
        let session = GameSession::StartExecutable(&self.project, &self.report.executable)?;
        self.session = Some(session);
        let session = self.session.as_mut().unwrap();
        Wait(session)?;
        let identity = FormatIdentity(ExecutableIdentity(session.StagedExecutable())?);
        if let Some(expected) = &self.report.executable_identity
            && expected != &identity
        {
            return Err("game executable changed during playtest".into());
        }
        self.report.executable_identity = Some(identity);
        self.report.game = Some(session.Name().to_owned());
        self.report.sessions_started += 1;
        self.facts = Some(Observe(session)?);
        Ok(())
    }

    pub fn Restart(&mut self) -> Result<(), String> {
        self.Start()?;
        self.initial_identity = FactsIdentity(self.Observe())?;
        self.history.clear();
        Ok(())
    }

    fn CheckBudget(&self) -> Result<(), String> {
        if self.requests >= MAX_REQUESTS || self.started.elapsed() >= MAX_RUN {
            return Err("playtest request or wall-time limit reached".into());
        }
        Ok(())
    }

    pub fn Observe(&self) -> &Facts {
        self.facts.as_ref().expect("successful playtest start")
    }

    fn Deliver(&mut self, input: &Input) -> Result<u64, String> {
        self.CheckBudget()?;
        let tick = self.Observe().Tick();
        let session = self.session.as_mut().ok_or("no playtest session")?;
        let result = match input {
            Input::Idle if session.Tactical().is_some() => session.BeginTacticalTick(Vec::new()),
            Input::Idle => session.BeginTick(RoomInput::default()),
            Input::Navigate(input) if session.Tactical().is_none() => session.BeginTick(*input),
            Input::Navigate(_) => Err("navigation input is only accepted by room games".into()),
            Input::Tactical(commands)
                if commands.len() <= crate::GameRuntime::MAX_TACTICAL_COMMANDS =>
            {
                session.BeginTacticalTick(commands.clone())
            }
            Input::Tactical(_) => Err("too many tactical commands for one request".into()),
        };
        self.requests += 1;
        result.and_then(|()| Wait(session)).map_err(|error| {
            format!(
                "request {} after simulation tick {tick}: {error}",
                self.requests
            )
        })?;
        self.facts = Some(Observe(session)?);
        FactsIdentity(self.Observe())
    }

    /// Deliver one input exchange. A tactical game may keep its clock frozen.
    pub fn Step(&mut self, input: Input) -> Result<(), String> {
        let identity = self.Deliver(&input)?;
        self.history.push((input, identity));
        self.report.executed_steps += 1;
        Ok(())
    }

    /// Advance exactly this many simulation ticks or fail on the first frozen tick.
    pub fn Advance(&mut self, ticks: usize, input: Input) -> Result<(), String> {
        for _ in 0..ticks {
            let previous = self.Observe().Tick();
            self.Step(input.clone())?;
            if self.Observe().Tick() != previous + 1 {
                return Err(format!("simulation did not advance from tick {previous}"));
            }
        }
        Ok(())
    }

    pub fn Checkpoint(&self) -> usize {
        self.history.len()
    }

    /// Restart and replay a prefix of the current session. Later inputs are discarded.
    pub fn Rewind(&mut self, requests: usize) -> Result<(), String> {
        if requests > self.history.len() {
            return Err(format!("checkpoint {requests} is outside this session"));
        }
        self.history.truncate(requests);
        let history = std::mem::take(&mut self.history);
        let started = Instant::now();
        self.Start()?;
        let mut matched = 0;
        let result = (|| {
            self.CompareReplay(0, self.initial_identity, FactsIdentity(self.Observe())?)?;
            for (index, (input, expected)) in history.iter().enumerate() {
                let actual = self
                    .Deliver(input)
                    .map_err(|error| format!("replay request {}: {error}", index + 1))?;
                self.CompareReplay(index + 1, *expected, actual)?;
                matched += 1;
            }
            Ok(())
        })();
        self.report.replays.push(Replay {
            requests,
            matched,
            elapsed_ms: started.elapsed().as_millis(),
        });
        self.history = history;
        result
    }

    fn CompareReplay(&mut self, request: usize, expected: u64, actual: u64) -> Result<(), String> {
        if expected == actual {
            return Ok(());
        }
        let tick = self.Observe().Tick();
        self.report.first_divergence = Some(Divergence {
            request,
            tick,
            expected: FormatIdentity(expected),
            actual: FormatIdentity(actual),
        });
        Err(format!(
            "replay diverged at request {request}, simulation tick {tick}"
        ))
    }

    #[track_caller]
    pub fn Check(&mut self, label: &str, passed: bool) -> Result<(), String> {
        self.Assert(label, passed, String::new())
    }

    #[track_caller]
    pub fn Equal<T: std::fmt::Debug + PartialEq>(
        &mut self,
        label: &str,
        actual: T,
        expected: T,
    ) -> Result<(), String> {
        let passed = actual == expected;
        self.Assert(
            label,
            passed,
            format!("expected {expected:?}, observed {actual:?}"),
        )
    }

    #[track_caller]
    pub fn Near(
        &mut self,
        label: &str,
        actual: f32,
        expected: f32,
        tolerance: f32,
    ) -> Result<(), String> {
        let passed = actual.is_finite()
            && expected.is_finite()
            && tolerance.is_finite()
            && tolerance >= 0.0
            && (actual - expected).abs() <= tolerance;
        self.Assert(
            label,
            passed,
            format!("expected {expected} ± {tolerance}, observed {actual}"),
        )
    }

    #[track_caller]
    fn Assert(&mut self, label: &str, passed: bool, detail: String) -> Result<(), String> {
        if self.report.assertions.len() >= MAX_ASSERTIONS {
            return Err("playtest assertion limit reached".into());
        }
        let source = std::panic::Location::caller().to_string();
        self.report.assertions.push(Assertion {
            step: self.report.executed_steps,
            tick: self.Observe().Tick(),
            source: source.clone(),
            label: Bounded(label),
            passed,
            detail: Bounded(&detail),
        });
        if passed {
            Ok(())
        } else {
            Err(format!(
                "{source}: {label} at tick {}. {detail}",
                self.Observe().Tick()
            ))
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            session.Close();
        }
    }
}

/// Execute a Rust scenario, write its report, and return an error for a failing CLI invocation.
pub fn Run(
    project: &Path,
    executable: &Path,
    output: &Path,
    identity: &str,
    scenario: impl FnOnce(&mut Runner) -> Result<(), String>,
) -> Result<(), String> {
    fs::create_dir_all(output).map_err(|error| format!("{}: {error}", output.display()))?;
    let destination = output.join("report.json");
    let mut runner = Runner {
        project: project.to_owned(),
        started: Instant::now(),
        session: None,
        facts: None,
        initial_identity: 0,
        history: Vec::new(),
        requests: 0,
        report: Report {
            schema_version: REPORT_VERSION,
            protocol_version: PROTOCOL,
            executable: executable.to_owned(),
            executable_identity: None,
            scenario: Bounded(identity),
            game: None,
            sessions_started: 0,
            executed_steps: 0,
            final_tick: None,
            observation_identity: None,
            final_observation: None,
            assertions: Vec::new(),
            replays: Vec::new(),
            first_divergence: None,
            first_failure: None,
            elapsed_ms: 0,
            exit_status: 0,
            artifacts: vec!["report.json".into()],
        },
    };
    let mut result = runner.Restart().and_then(|()| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scenario(&mut runner)))
            .unwrap_or_else(|panic| {
                let text = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or(panic.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");
                Err(format!("Rust playtest panicked: {}", Bounded(text)))
            })
    });
    if result.is_ok()
        && let Some(failed) = runner
            .report
            .assertions
            .iter()
            .find(|assertion| !assertion.passed)
    {
        result = Err(format!("{}: {}", failed.source, failed.label));
    }
    if let Some(session) = runner.session.take() {
        session.Close();
    }
    if let Some(facts) = runner.facts.take() {
        runner.report.final_tick = Some(facts.Tick());
        runner.report.observation_identity = Some(FormatIdentity(FactsIdentity(&facts)?));
        runner.report.final_observation = Some(facts);
    }
    runner.report.elapsed_ms = runner.started.elapsed().as_millis();
    runner.report.exit_status = i32::from(result.is_err());
    runner.report.first_failure = result.as_ref().err().map(|error| Bounded(error));
    let bytes = serde_json::to_vec_pretty(&runner.report).map_err(|error| error.to_string())?;
    fs::write(&destination, bytes)
        .map_err(|error| format!("{}: {error}", destination.display()))?;
    let passed = runner
        .report
        .assertions
        .iter()
        .filter(|assertion| assertion.passed)
        .count();
    eprintln!(
        "{} {} · {} steps · {} passed assertions · {}",
        if result.is_ok() { "PASS" } else { "FAIL" },
        identity,
        runner.report.executed_steps,
        passed,
        destination.display()
    );
    result
}

fn Bounded(value: &str) -> String {
    value.chars().take(1024).collect()
}

fn Observe(session: &GameSession) -> Result<Facts, String> {
    if let Some(view) = session.Tactical() {
        return Ok(Facts::Tactical { view: view.clone() });
    }
    let snapshot = session.ObserveRoom().ok_or("missing room snapshot")?;
    let eye = snapshot.Camera.Eye;
    let guard = snapshot.Guard.Position;
    Ok(Facts::Room {
        tick: session.Clock().ok_or("missing room clock")?.SimulationTick,
        observation: session
            .Observation()
            .ok_or("missing game observation")?
            .clone(),
        player: [eye.x, eye.y, eye.z],
        guard: [guard.x, guard.y, guard.z],
        camera_angles: [snapshot.Camera.Yaw, snapshot.Camera.Pitch],
        boxes: snapshot
            .Boxes
            .iter()
            .map(|part| part.Id.to_string())
            .collect(),
    })
}

fn Wait(session: &mut GameSession) -> Result<(), String> {
    while session.Busy() {
        session.Poll(Duration::from_millis(50))?;
    }
    Ok(())
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;

fn HashBytes(mut state: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        state = (state ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    state
}

fn FormatIdentity(hash: u64) -> String {
    format!("fnv1a64:{hash:016x}")
}

pub fn ContentIdentity(bytes: &[u8]) -> String {
    FormatIdentity(HashBytes(FNV_OFFSET, bytes))
}

fn FactsIdentity(facts: &Facts) -> Result<u64, String> {
    let bytes = serde_json::to_vec(facts).map_err(|error| error.to_string())?;
    Ok(HashBytes(FNV_OFFSET, &bytes))
}

fn ExecutableIdentity(path: &Path) -> Result<u64, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut chunk = [0u8; 16 * 1024];
    let mut hash = FNV_OFFSET;
    loop {
        let size = file.read(&mut chunk).map_err(|error| error.to_string())?;
        if size == 0 {
            return Ok(hash);
        }
        hash = HashBytes(hash, &chunk[..size]);
    }
}
