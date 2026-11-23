use super::*;
use serde_json::{Value, json};
use std::io::{self, BufRead};

const MCP_LIMIT: usize = 16 * 1024;
const MCP_VERSION: &str = "2025-11-25";

struct Client {
    label: String,
    path: PathBuf,
    id: String,
    lease: Option<Invitation>,
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(invite) = &self.lease {
            let _ = Exchange(invite, &self.id, Call::Disconnect);
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rpc {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCall {
    name: String,
    #[serde(default = "EmptyArguments")]
    arguments: Value,
    #[serde(default, rename = "_meta")]
    _meta: Option<Value>,
}

fn EmptyArguments() -> Value {
    json!({})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterArgs {
    character: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventArgs {
    after: u64,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MutationArgs {
    session: String,
    observed_revision: u64,
    character: Option<u16>,
    command: Option<PlayerAction>,
}

/// MCP uses newline-delimited JSON-RPC over stdio, separately from game frames.
pub fn Serve(label: &str) -> Result<(), String> {
    let mut client = Client {
        label: label.into(),
        path: InvitationPath(label)?,
        id: RandomHex::<16>()?,
        lease: None,
    };
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut version = None;
    let mut line = Vec::with_capacity(1024);
    loop {
        line.clear();
        let count = (&mut input)
            .take(MCP_LIMIT as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(());
        }
        if count > MCP_LIMIT {
            return Err("MCP input exceeds its bound".into());
        }
        let response = match serde_json::from_slice::<Rpc>(&line) {
            Ok(request) => Respond(&mut client, &mut version, request),
            Err(_) => Some(RpcError(Value::Null, -32700, "invalid JSON-RPC packet")),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response).map_err(|e| e.to_string())?;
            output
                .write_all(b"\n")
                .and_then(|()| output.flush())
                .map_err(|e| e.to_string())?;
        }
    }
}

fn Respond(client: &mut Client, version: &mut Option<String>, request: Rpc) -> Option<Value> {
    let id = request.id?;
    if request.jsonrpc != "2.0" || !(id.is_string() || id.is_i64() || id.is_u64()) {
        return Some(RpcError(id, -32600, "invalid JSON-RPC envelope"));
    }
    let result = match request.method.as_str() {
        "initialize" => {
            let requested = request
                .params
                .get("protocolVersion")
                .and_then(Value::as_str);
            let selected = match requested {
                Some("2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05") => {
                    requested.unwrap()
                }
                _ => MCP_VERSION,
            };
            *version = Some(selected.into());
            json!({"protocolVersion":selected,"capabilities":{"tools":{}},
                "serverInfo":{"name":"wyrmcoil-game","version":"1.0.0"},
                "instructions":"Inspect the battle, claim a seat, and use the current session and observation revision for each mutation. Abilities are selected by the possessed character."})
        }
        "ping" => json!({}),
        "tools/list" if version.is_some() => json!({"tools":Tools()}),
        "tools/call" if version.is_some() => {
            let Ok(call) = serde_json::from_value::<ToolCall>(request.params) else {
                return Some(RpcError(id, -32602, "invalid tool call"));
            };
            let result = client.Tool(&call.name, call.arguments);
            let (data, error) = match result {
                Ok(result) => result,
                Err(code) => (json!({"status":"rejected","code":code}), true),
            };
            let mut reply =
                json!({"content":[{"type":"text","text":data.to_string()}],"isError":error});
            if version.as_deref() != Some("2024-11-05") && version.as_deref() != Some("2025-03-26")
            {
                reply["structuredContent"] = data;
            }
            reply
        }
        "tools/list" | "tools/call" => return Some(RpcError(id, -32002, "initialize first")),
        _ => return Some(RpcError(id, -32601, "unknown method")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn RpcError(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

impl Client {
    fn Tool(&mut self, name: &str, arguments: Value) -> Result<(Value, bool), String> {
        let read = matches!(
            name,
            "list_game_sessions"
                | "inspect_game"
                | "observe_battle"
                | "inspect_party"
                | "inspect_character"
                | "inspect_agent"
                | "get_game_events"
        );
        match name {
            "list_game_sessions" | "inspect_game" | "observe_battle" | "inspect_party" => {
                Arguments::<NoArgs>(arguments.clone())?;
            }
            "inspect_character" | "inspect_agent" => {
                Arguments::<CharacterArgs>(arguments.clone())?;
            }
            "get_game_events" => {
                Arguments::<EventArgs>(arguments.clone())?;
            }
            "claim_player_seat"
            | "release_player_seat"
            | "possess_character"
            | "release_character"
            | "issue_player_command"
            | "request_resume" => {}
            _ => return Err("unknown_tool".into()),
        }
        let invite = match ReadInvitation(&self.path) {
            Ok(invite) => invite,
            Err(_) if name == "list_game_sessions" => return Ok((json!({"sessions":[]}), false)),
            Err(error) => return Err(error),
        };
        let mut action = None;
        let call = if read {
            Call::Observe
        } else {
            let args: MutationArgs = Arguments(arguments.clone())?;
            if args.session != invite.session {
                return Err("session_changed_observe_again".into());
            }
            let requested = MatchMutation(name, &args)?;
            action = Some(requested);
            Call::Action {
                observed: args.observed_revision,
                action: requested,
            }
        };
        let answer = match Exchange(&invite, &self.id, call) {
            Ok(answer) => answer,
            Err(_) if name == "list_game_sessions" => return Ok((json!({"sessions":[]}), false)),
            Err(error) => return Err(error),
        };
        match answer {
            Answer::Observed(snapshot) => {
                Ok((ReadTool(name, arguments, &snapshot, &self.label)?, false))
            }
            Answer::Applied {
                event,
                tick,
                revision,
            } => {
                let rejected = matches!(event.Result, PlayerResult::Rejected(_));
                let result = json!({"session":invite.session,"tick":tick,"revision":revision,
                    "seat":event.Seat,"status":if rejected {"rejected"} else if event.Result == PlayerResult::Queued {"queued"} else {"accepted"},
                    "event":event});
                if !rejected && matches!(action, Some(PlayerAction::Claim)) {
                    self.lease = Some(invite);
                }
                if matches!(action, Some(PlayerAction::Leave)) {
                    self.lease = None;
                }
                Ok((result, rejected))
            }
            Answer::Rejected { code, correction } => {
                let mut result = json!({"status":"rejected","code":code});
                if let Some(snapshot) = correction {
                    result["correction"] = Summary(&snapshot, &self.label);
                }
                Ok((result, true))
            }
        }
    }
}

fn Arguments<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|_| "invalid_tool_arguments".into())
}

fn MatchMutation(name: &str, args: &MutationArgs) -> Result<PlayerAction, String> {
    use PlayerAction::*;
    match (name, args.character, args.command) {
        ("claim_player_seat", None, None) => Ok(Claim),
        ("release_player_seat", None, None) => Ok(Leave),
        ("possess_character", Some(character), None) => Ok(Possess { character }),
        ("release_character", None, None) => Ok(Release),
        ("request_resume", None, None) => Ok(Resume),
        (
            "issue_player_command",
            None,
            Some(
                command @ (Move { .. }
                | Attack { .. }
                | Ability { .. }
                | Target { .. }
                | Hold
                | Clear
                | Pause),
            ),
        ) => Ok(command),
        _ => Err("invalid_tool_arguments".into()),
    }
}

fn Summary(snapshot: &Snapshot, label: &str) -> Value {
    let view = &snapshot.view;
    let players = view.Players.as_ref().unwrap();
    json!({"session":snapshot.session,"group":label,"name":snapshot.name,"tick":view.Frame,
        "revision":players.Revision,"seat":snapshot.seat,"paused":view.Paused,"phase":view.State,
        "objective":if view.State == wyrmcoil_game::GameState::Playing {"Defeat the watch and recover the relic."} else {&view.Objective}})
}

fn ReadTool(name: &str, args: Value, snapshot: &Snapshot, label: &str) -> Result<Value, String> {
    let view = &snapshot.view;
    let players = view.Players.as_ref().ok_or("session_has_no_player_view")?;
    let mut result = Summary(snapshot, label);
    match name {
        "list_game_sessions" => return Ok(json!({"sessions":[result]})),
        "inspect_game" | "inspect_party" => {
            result["seats"] = json!(players.Seats);
            result["liveness"] = json!(snapshot.liveness);
            if name == "inspect_party" {
                result["party"] = json!(
                    view.Actors
                        .iter()
                        .filter(|actor| actor.PartySlot.is_some())
                        .collect::<Vec<_>>()
                );
                result["abilities"] = json!(["Shield Bash", "Backstab", "Frost Bolt"]);
            }
        }
        "observe_battle" => {
            result["actors"] = json!(view.Actors);
            result["tactical"] = json!(players.Battle);
            result["obstacles"] = json!(players.Obstacles);
            result["relic"] = json!(view.Relic);
            result["seats"] = json!(players.Seats);
            result["recent_events"] = json!(
                players
                    .Events
                    .iter()
                    .rev()
                    .take(6)
                    .rev()
                    .collect::<Vec<_>>()
            );
        }
        "inspect_character" | "inspect_agent" => {
            let args: CharacterArgs = Arguments(args)?;
            let actor = view
                .Actors
                .iter()
                .find(|actor| actor.Id == args.character)
                .ok_or("unknown_character")?;
            let owner = players
                .Seats
                .iter()
                .find(|seat| seat.Character == Some(actor.Id));
            let agent = view.Agents.iter().find(|agent| agent.Actor == actor.Id);
            result["actor"] = json!(actor);
            result["owner"] = json!(owner);
            result["control"] = json!(agent.map(|agent| agent.Mode));
            result["tactical"] = json!(players.Battle.iter().find(|fact| fact.Id == actor.Id));
            if name == "inspect_agent" {
                let agent = agent.ok_or("character_has_no_dunewyrm_brain")?;
                result["agent"] = json!({"mode":agent.Mode,"clock":agent.Clock,"stack":agent.Stack,
                    "intent":agent.Stack.get(1),"leaf":agent.Stack.last(),"target":agent.Target,
                    "blocking":agent.Blocking,"decision":agent.Decision,"transitions":agent.Transitions,
                    "command":agent.Commands.last()});
            }
        }
        "get_game_events" => {
            let args: EventArgs = Arguments(args)?;
            let limit = args.limit.unwrap_or(12);
            if !(1..=24).contains(&limit) {
                return Err("invalid_event_page_size".into());
            }
            let events: Vec<_> = players
                .Events
                .iter()
                .filter(|event| event.Sequence > args.after)
                .take(limit)
                .collect();
            let oldest = players.Events.first().map(|event| event.Sequence);
            result["events"] = json!(events);
            result["next_after"] = json!(events.last().map_or(args.after, |event| event.Sequence));
            result["history_gap"] =
                json!(oldest.is_some_and(|first| first > args.after.saturating_add(1)));
        }
        _ => unreachable!("validated read tool"),
    }
    Ok(result)
}

fn Schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

fn Tools() -> Vec<Value> {
    let id = json!({"type":"integer","minimum":0,"maximum":65535});
    let revision = json!({"type":"integer","minimum":0});
    let session = json!({"type":"string","minLength":32,"maxLength":32});
    let no_args = Schema(json!({}), &[]);
    let character = Schema(json!({"character":id}), &["character"]);
    let mutation = Schema(
        json!({"session":session,"observed_revision":revision}),
        &["session", "observed_revision"],
    );
    let possess = Schema(
        json!({"session":session,"observed_revision":revision,"character":id}),
        &["session", "observed_revision", "character"],
    );
    let mut actions = Vec::new();
    for action in ["attack", "ability", "target"] {
        actions.push(Schema(
            json!({"action":{"type":"string","enum":[action]},"target":id}),
            &["action", "target"],
        ));
    }
    actions.push(Schema(json!({"action":{"type":"string","enum":["move"]},
        "position":{"type":"array","minItems":2,"maxItems":2,"items":{"type":"number","minimum":-256,"maximum":256}}}),
        &["action","position"]));
    actions.push(Schema(
        json!({"action":{"type":"string","enum":["hold","clear","pause"]}}),
        &["action"],
    ));
    let order = Schema(
        json!({"session":session,"observed_revision":revision,
        "command":{"type":"object","oneOf":actions}}),
        &["session", "observed_revision", "command"],
    );
    let events = Schema(
        json!({"after":revision,"limit":{"type":"integer","minimum":1,"maximum":24}}),
        &["after"],
    );
    [
        ("list_game_sessions", "List the authorized local session for this server.", no_args.clone(), true),
        ("inspect_game", "Read the tick, phase, objective and player seats.", no_args.clone(), true),
        ("observe_battle", "Observe the party, enemies, positions, effects, terrain and recent events.", no_args.clone(), true),
        ("inspect_party", "Inspect the party roster, abilities, queues and possession.", no_args, true),
        ("inspect_character", "Read one character and its current owner, target and effects.", character.clone(), true),
        ("inspect_agent", "Read one adventurer's Dunewyrm stack, decision and transitions.", character, true),
        ("get_game_events", "Read a bounded page after the given event sequence. Continue with next_after.", events, true),
        ("claim_player_seat", "Claim one guest seat for this server instance.", mutation.clone(), false),
        ("release_player_seat", "Release this server's seat and cancel its remaining orders.", mutation.clone(), false),
        ("possess_character", "Possess one unowned living party character by ID.", possess, false),
        ("release_character", "Return this seat's character to Dunewyrm autonomy.", mutation.clone(), false),
        ("issue_player_command", "Issue a world-space Move, Attack, Ability, Target, Hold, Clear or Pause. The ability belongs to the owned character.", order, false),
        ("request_resume", "Resume combat through the shared tactical pause control.", mutation, false),
    ].into_iter().map(|(name, description, input, read)| json!({
        "name":name,"description":description,"inputSchema":input,"annotations":{"readOnlyHint":read,"openWorldHint":false}
    })).collect()
}
