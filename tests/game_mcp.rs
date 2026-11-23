#![cfg(windows)]
#![allow(non_snake_case)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use wyrmcoil::GameRuntime::{GameSession, TacticalCommand};

struct Mcp {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    id: u32,
}

impl Mcp {
    fn Start(label: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wyrmcoil"))
            .args(["game-mcp", "--session", label])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut client = Self {
            child,
            input,
            output,
            id: 0,
        };
        let start = client.Rpc(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},
            "clientInfo":{"name":"wyrmcoil-integration-test","version":"1"}}),
        );
        assert_eq!(start["protocolVersion"], "2025-11-25");
        client.Send(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}));
        client
    }

    fn Send(&mut self, request: Value) {
        writeln!(self.input.as_mut().unwrap(), "{request}").unwrap();
    }

    fn Rpc(&mut self, method: &str, args: Value) -> Value {
        self.id += 1;
        self.Send(json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":args}));
        let mut line = String::new();
        assert_ne!(
            self.output.read_line(&mut line).unwrap(),
            0,
            "server closed"
        );
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], self.id);
        assert!(reply["error"].is_null(), "{reply}");
        reply["result"].clone()
    }

    fn Call(&mut self, tool: &str, args: Value) -> Value {
        self.Rpc("tools/call", json!({"name":tool,"arguments":args}))["structuredContent"].clone()
    }

    fn Mutate(&mut self, tool: &str, view: &Value, mut extra: Value) -> Value {
        extra["session"] = view["session"].clone();
        extra["observed_revision"] = view["revision"].clone();
        self.Call(tool, extra)
    }

    fn Disconnect(mut self) {
        self.input = None;
        assert!(self.child.wait().unwrap().success());
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        if self.input.take().is_some() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

enum Drive {
    Time(Duration, mpsc::Sender<()>),
    Tick(Vec<TacticalCommand>, usize, mpsc::Sender<()>),
    Close,
}

struct Host {
    drive: mpsc::Sender<Drive>,
    task: Option<thread::JoinHandle<()>>,
}

impl Host {
    fn Start(root: &Path, label: &str) -> Self {
        let mut game = GameSession::Start(root).unwrap();
        assert!(game.Poll(Duration::from_secs(3)).unwrap());
        game.OpenLocalPlayers(label, Duration::from_secs(30))
            .unwrap();
        assert!(
            game.OpenLocalPlayers(label, Duration::from_secs(30))
                .is_err()
        );
        let (drive, requests) = mpsc::channel();
        let task = thread::spawn(move || {
            let mut now = Instant::now();
            loop {
                match requests.recv_timeout(Duration::from_millis(2)) {
                    Ok(Drive::Time(elapsed, done)) => {
                        now += elapsed;
                        let commands = game.PollLocalPlayers(now);
                        if !commands.is_empty() {
                            Tick(&mut game, commands);
                        }
                        let _ = done.send(());
                    }
                    Ok(Drive::Tick(commands, idle, done)) => {
                        Tick(&mut game, commands);
                        for _ in 0..idle {
                            Tick(&mut game, Vec::new());
                        }
                        let _ = done.send(());
                    }
                    Ok(Drive::Close) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let commands = game.PollLocalPlayers(now);
                if !commands.is_empty() {
                    Tick(&mut game, commands);
                }
            }
            game.Close();
        });
        Self {
            drive,
            task: Some(task),
        }
    }

    fn Time(&self, elapsed: Duration) {
        let (done, receive) = mpsc::channel();
        self.drive.send(Drive::Time(elapsed, done)).unwrap();
        receive.recv_timeout(Duration::from_secs(3)).unwrap();
    }

    fn Tick(&self, commands: Vec<TacticalCommand>, idle: usize) {
        let (done, receive) = mpsc::channel();
        self.drive.send(Drive::Tick(commands, idle, done)).unwrap();
        receive.recv_timeout(Duration::from_secs(3)).unwrap();
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.drive.send(Drive::Close);
        self.task.take().unwrap().join().unwrap();
    }
}

fn Tick(game: &mut GameSession, commands: Vec<TacticalCommand>) {
    game.BeginTacticalTick(commands).unwrap();
    assert!(game.Poll(Duration::from_millis(500)).unwrap());
}

#[test]
#[ignore = "requires the independently published Last Watch artifact from build.ps1"]
fn real_mcp_clients_share_the_authoritative_last_watch_preview_host() {
    use TacticalCommand::*;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/games/last-watch");
    let label = format!("m15-test-{}", std::process::id());
    let started = Instant::now();
    let mut one = Mcp::Start(&label);
    eprintln!("MCP spawn and initialize {:?}", started.elapsed());
    assert_eq!(
        one.Rpc("tools/list", json!({}))["tools"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
    assert_eq!(
        one.Call("list_game_sessions", json!({}))["sessions"],
        json!([])
    );

    let host = Host::Start(&root, &label);
    let path = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
        .join("WyrmCoil/invitations")
        .join(format!("{label}.json"));
    let invitation: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut forged =
        TcpStream::connect(("127.0.0.1", invitation["port"].as_u64().unwrap() as u16)).unwrap();
    forged
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    writeln!(forged, "{}", json!({"version":1,"capability":"wrong","client":"00000000000000000000000000000000","call":"observe"})).unwrap();
    let mut rejection = String::new();
    BufReader::new(forged).read_line(&mut rejection).unwrap();
    assert!(rejection.contains("unauthorized_session"));

    let started = Instant::now();
    let view = one.Call("observe_battle", json!({}));
    eprintln!(
        "MCP observe round trip {:?}, structured result {} bytes",
        started.elapsed(),
        view.to_string().len()
    );
    assert_eq!(view["tick"], 0);
    assert_eq!(view["actors"].as_array().unwrap().len(), 6);
    assert_eq!(
        one.Mutate("possess_character", &view, json!({"character":1}))["code"],
        "claim_a_seat_first"
    );
    let started = Instant::now();
    assert_eq!(one.Mutate("claim_player_seat", &view, json!({}))["seat"], 1);
    eprintln!("MCP claim accepted by game in {:?}", started.elapsed());
    let mut two = Mcp::Start(&label);
    assert_eq!(two.Mutate("claim_player_seat", &view, json!({}))["seat"], 2);
    let mut three = Mcp::Start(&label);
    assert_eq!(
        three.Mutate("claim_player_seat", &view, json!({}))["code"],
        "all_guest_seats_claimed"
    );
    assert_eq!(
        one.Mutate("possess_character", &view, json!({"character":1}))["status"],
        "accepted"
    );
    assert_eq!(
        two.Mutate("possess_character", &view, json!({"character":1}))["event"]["Result"]["Rejected"],
        "Occupied"
    );
    assert_eq!(
        one.Mutate(
            "issue_player_command",
            &view,
            json!({"command":{"action":"hold"}})
        )["code"],
        "rate_limited"
    );
    host.Time(Duration::from_millis(75));
    assert_eq!(
        two.Mutate("possess_character", &view, json!({"character":2}))["status"],
        "accepted"
    );
    assert_eq!(
        one.Mutate(
            "issue_player_command",
            &view,
            json!({"command":{"action":"attack","target":222}})
        )["event"]["Result"]["Rejected"],
        "InvalidTarget"
    );
    host.Time(Duration::from_millis(75));
    let mut stale = view.clone();
    stale["revision"] = json!(9_999);
    let reply = one.Mutate(
        "issue_player_command",
        &stale,
        json!({"command":{"action":"hold"}}),
    );
    assert_eq!(reply["code"], "stale_observation");
    assert_eq!(reply["correction"]["session"], view["session"]);
    let movement = one.Mutate(
        "issue_player_command",
        &view,
        json!({"command":{"action":"move","position":[-2,4]}}),
    );
    assert_eq!(movement["status"], "queued");
    host.Tick(vec![MoveTo([0., 4.]), TogglePause], 20);
    let battle = one.Call("observe_battle", json!({}));
    assert_eq!(battle["actors"][0]["Position"], json!([0.0, 4.0]));
    assert_eq!(battle["actors"][1]["Position"], json!([-2.0, 4.0]));
    let events = one.Call("get_game_events", json!({"after":0,"limit":24}));
    let receipt = &movement["event"]["Receipt"];
    assert!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["Receipt"] == *receipt && e["Result"] == "Completed")
    );
    assert_eq!(
        one.Call("inspect_agent", json!({"character":1}))["agent"]["clock"],
        0
    );
    assert_eq!(
        one.Mutate("release_character", &battle, json!({}))["status"],
        "accepted"
    );
    let resumed = one.Call("inspect_agent", json!({"character":1}));
    assert_eq!(resumed["agent"]["mode"], "Autonomous");
    assert_eq!(resumed["agent"]["clock"], 1);

    one.Disconnect();
    assert_eq!(
        two.Call("inspect_party", json!({}))["seats"][1]["Claimed"],
        false
    );
    host.Time(Duration::from_secs(31));
    let expired = two.Call("inspect_party", json!({}));
    assert_eq!(expired["seats"][2]["Claimed"], false);
    assert_eq!(expired["seats"][2]["Character"], Value::Null);
    drop(host);
    assert!(!path.exists());
    assert_eq!(
        three.Call("list_game_sessions", json!({}))["sessions"],
        json!([])
    );
    let restarted = Host::Start(&root, &label);
    let fresh = three.Call("observe_battle", json!({}));
    assert_ne!(fresh["session"], view["session"]);
    assert_eq!(fresh["tick"], 0);
    assert_eq!(
        three.Mutate("claim_player_seat", &view, json!({}))["code"],
        "session_changed_observe_again"
    );
    drop(restarted);
    assert!(!path.exists());
}
