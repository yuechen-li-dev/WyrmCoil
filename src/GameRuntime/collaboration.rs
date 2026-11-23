//! Session-scoped local bridge. Only the game process can apply player actions.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use wyrmcoil_game::{
    PLAYER_REVISION_AGE, PLAYER_SEATS, PlayerAction, PlayerEvent, PlayerRequest, PlayerResult,
    TacticalCommand, TacticalView,
};

pub mod mcp;

const VERSION: u16 = 1;
const REQUEST_LIMIT: usize = 4_096;
const REPLY_LIMIT: usize = 128 * 1024;
const WIRE_TIMEOUT: Duration = Duration::from_millis(400);
const COMMAND_TIMEOUT: Duration = Duration::from_millis(1500);
const RATE_INTERVAL: Duration = Duration::from_millis(60);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Invitation {
    version: u16,
    session: String,
    port: u16,
    capability: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u16,
    capability: String,
    client: String,
    call: Call,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Call {
    Observe,
    Action { observed: u64, action: PlayerAction },
    Disconnect,
}

#[derive(Serialize, Deserialize)]
struct Snapshot {
    session: String,
    name: String,
    seat: Option<u8>,
    liveness: Vec<LeaseStatus>,
    view: TacticalView,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaseStatus {
    pub Seat: u8,
    pub IdleMs: u64,
    pub ExpiresMs: u64,
}

#[derive(Serialize, Deserialize)]
enum Answer {
    Observed(Snapshot),
    Applied {
        event: PlayerEvent,
        tick: u64,
        revision: u64,
    },
    Rejected {
        code: String,
        correction: Option<Snapshot>,
    },
}

struct Incoming {
    client: String,
    call: Call,
    deadline: Instant,
    answer: mpsc::SyncSender<Answer>,
}

struct Lease {
    client: String,
    activity: Instant,
    mutation: Option<Instant>,
}

struct Pending {
    receipt: u64,
    seat: u8,
    action: PlayerAction,
    answer: mpsc::SyncSender<Answer>,
}

pub struct Bridge {
    name: String,
    invitation: PathBuf,
    session: String,
    inbox: mpsc::Receiver<Incoming>,
    shutdown: Arc<AtomicBool>,
    worker: thread::Thread,
    timeout: Duration,
    leases: [Option<Lease>; PLAYER_SEATS - 1],
    next_receipt: u64,
    pending: Vec<Pending>,
}

impl Bridge {
    /// Called only for a ready tactical preview. The environment variable is an explicit opt-in.
    pub fn FromEnvironment(name: &str) -> Result<Option<Self>, String> {
        let Ok(label) = std::env::var("WYRMCOIL_GAME_MCP") else {
            return Ok(None);
        };
        let seconds = match std::env::var("WYRMCOIL_GAME_MCP_TIMEOUT") {
            Ok(value) => value
                .parse::<u64>()
                .map_err(|_| "invalid game MCP timeout")?,
            Err(_) => 120,
        };
        if !(5..=1800).contains(&seconds) {
            return Err("game MCP timeout must be 5–1800 seconds".into());
        }
        Self::ForLabel(name, &label, Duration::from_secs(seconds)).map(Some)
    }

    pub(super) fn ForLabel(name: &str, label: &str, timeout: Duration) -> Result<Self, String> {
        Self::Start(name, InvitationPath(label)?, timeout)
    }

    fn Start(name: &str, invitation: PathBuf, timeout: Duration) -> Result<Self, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let session = RandomHex::<16>()?;
        let capability = RandomHex::<32>()?;
        let data = Invitation {
            version: VERSION,
            session: session.clone(),
            port,
            capability: capability.clone(),
        };
        let directory = invitation.parent().ok_or("missing invitation directory")?;
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
        // A second editor cannot replace a live invitation with the same label.
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&invitation)
            .map_err(|e| format!("publish game invitation {}: {e}", invitation.display()))?;
        if let Err(error) = file.write_all(&bytes) {
            drop(file);
            let _ = fs::remove_file(&invitation);
            return Err(error.to_string());
        }
        drop(file);
        let (incoming, inbox) = mpsc::sync_channel(2);
        let shutdown = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&shutdown);
        let thread = match thread::Builder::new()
            .name("wyrmcoil-local-player".into())
            .spawn(move || Listen(listener, &capability, &signal, incoming))
        {
            Ok(thread) => thread,
            Err(error) => {
                let _ = fs::remove_file(&invitation);
                return Err(error.to_string());
            }
        };
        Ok(Self {
            name: name.into(),
            invitation,
            session,
            inbox,
            shutdown,
            worker: thread.thread().clone(),
            timeout,
            leases: std::array::from_fn(|_| None),
            next_receipt: 1,
            pending: Vec::new(),
        })
    }

    pub fn Status(&self, now: Instant) -> Vec<LeaseStatus> {
        self.leases
            .iter()
            .enumerate()
            .filter_map(|(index, lease)| {
                let idle = now.saturating_duration_since(lease.as_ref()?.activity);
                Some(LeaseStatus {
                    Seat: index as u8 + 1,
                    IdleMs: idle.as_millis() as u64,
                    ExpiresMs: self.timeout.saturating_sub(idle).as_millis() as u64,
                })
            })
            .collect()
    }

    /// Called on the existing host pump immediately before the next game request.
    pub fn Poll(&mut self, view: &TacticalView, now: Instant) -> Vec<TacticalCommand> {
        let Some(players) = view.Players.as_ref() else {
            return Vec::new();
        };
        for pending in std::mem::take(&mut self.pending) {
            if let Some(event) = players
                .Events
                .iter()
                .find(|event| event.Receipt == pending.receipt)
            {
                if matches!(pending.action, PlayerAction::Claim)
                    && matches!(event.Result, PlayerResult::Rejected(_))
                {
                    self.leases[usize::from(pending.seat) - 1] = None;
                }
                let _ = pending.answer.try_send(Answer::Applied {
                    event: *event,
                    tick: view.Frame,
                    revision: players.Revision,
                });
            } else {
                let _ = pending
                    .answer
                    .try_send(Rejected("missing_authoritative_result"));
            }
        }
        let mut commands = Vec::new();
        for (index, lease) in self.leases.iter_mut().enumerate() {
            if lease
                .as_ref()
                .is_some_and(|lease| now.saturating_duration_since(lease.activity) >= self.timeout)
            {
                *lease = None;
                commands.push(TacticalCommand::Player(PlayerRequest {
                    Seat: index as u8 + 1,
                    Receipt: 0,
                    Observed: players.Revision,
                    Action: PlayerAction::Leave,
                }));
            }
        }
        while let Ok(incoming) = self.inbox.try_recv() {
            if Instant::now() >= incoming.deadline {
                continue;
            }
            if let Some(command) = self.Handle(view, now, incoming) {
                commands.push(command);
            }
        }
        commands
    }

    fn Handle(
        &mut self,
        view: &TacticalView,
        now: Instant,
        incoming: Incoming,
    ) -> Option<TacticalCommand> {
        let players = view.Players.as_ref().unwrap();
        let index = self
            .leases
            .iter()
            .position(|lease| lease.as_ref().is_some_and(|l| l.client == incoming.client));
        let snapshot = |this: &Self| Snapshot {
            session: this.session.clone(),
            name: this.name.clone(),
            seat: index.map(|i| i as u8 + 1),
            liveness: this.Status(now),
            view: view.clone(),
        };
        if let Some(index) = index {
            self.leases[index].as_mut().unwrap().activity = now;
        }
        let (observed, action) = match incoming.call {
            Call::Observe => {
                let _ = incoming.answer.try_send(Answer::Observed(snapshot(self)));
                return None;
            }
            Call::Disconnect => (players.Revision, PlayerAction::Leave),
            Call::Action { observed, action } => (observed, action),
        };
        let error = if !matches!(action, PlayerAction::Leave)
            && (observed > players.Revision
                || players.Revision.saturating_sub(observed) > PLAYER_REVISION_AGE)
        {
            Some(Answer::Rejected {
                code: "stale_observation".into(),
                correction: Some(snapshot(self)),
            })
        } else if matches!(action, PlayerAction::Claim) && index.is_some() {
            Some(Rejected("seat_already_claimed"))
        } else if !matches!(action, PlayerAction::Claim) && index.is_none() {
            Some(Rejected("claim_a_seat_first"))
        } else if !matches!(action, PlayerAction::Leave | PlayerAction::Release)
            && index
                .and_then(|index| self.leases[index].as_ref()?.mutation)
                .is_some_and(|last| now.saturating_duration_since(last) < RATE_INTERVAL)
        {
            Some(Rejected("rate_limited"))
        } else {
            None
        };
        if let Some(error) = error {
            let _ = incoming.answer.try_send(error);
            return None;
        }
        let index = match index.or_else(|| {
            self.leases.iter().enumerate().position(|(i, lease)| {
                lease.is_none() && players.Seats.get(i + 1).is_some_and(|s| !s.Claimed)
            })
        }) {
            Some(index) => index,
            None => {
                let _ = incoming
                    .answer
                    .try_send(Rejected("all_guest_seats_claimed"));
                return None;
            }
        };
        if matches!(action, PlayerAction::Claim) {
            self.leases[index] = Some(Lease {
                client: incoming.client,
                activity: now,
                mutation: None,
            });
        } else if matches!(action, PlayerAction::Leave) {
            self.leases[index] = None;
        } else {
            self.leases[index].as_mut().unwrap().mutation = Some(now);
        }
        let seat = index as u8 + 1;
        let receipt = self.next_receipt;
        self.next_receipt += 1;
        self.pending.push(Pending {
            receipt,
            seat,
            action,
            answer: incoming.answer,
        });
        Some(TacticalCommand::Player(PlayerRequest {
            Seat: seat,
            Receipt: receipt,
            Observed: observed,
            Action: action,
        }))
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.worker.unpark();
        let _ = fs::remove_file(&self.invitation);
    }
}

fn Listen(
    listener: TcpListener,
    capability: &str,
    stop: &AtomicBool,
    incoming: mpsc::SyncSender<Incoming>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let answer = Receive::<Request>(&mut stream, REQUEST_LIMIT, WIRE_TIMEOUT)
                    .map(|req| Dispatch(req, capability, &incoming))
                    .unwrap_or_else(|_| Rejected("invalid_local_packet"));
                let _ = Send(&mut stream, &answer, REPLY_LIMIT);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::park_timeout(Duration::from_millis(5))
            }
            Err(_) => break,
        }
    }
}

fn Dispatch(request: Request, capability: &str, incoming: &mpsc::SyncSender<Incoming>) -> Answer {
    if request.version != VERSION
        || request.capability != capability
        || !IsToken(&request.client, 32)
    {
        return Rejected("unauthorized_session");
    }
    let (answer, response) = mpsc::sync_channel(1);
    if incoming
        .try_send(Incoming {
            client: request.client,
            call: request.call,
            deadline: Instant::now() + COMMAND_TIMEOUT,
            answer,
        })
        .is_err()
    {
        return Rejected("host_busy");
    }
    response
        .recv_timeout(COMMAND_TIMEOUT)
        .unwrap_or_else(|_| Rejected("preview_not_servicing_requests"))
}

fn Rejected(code: &str) -> Answer {
    Answer::Rejected {
        code: code.into(),
        correction: None,
    }
}

fn Send(stream: &mut TcpStream, value: &impl Serialize, limit: usize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() >= limit {
        return Err("local packet too large".into());
    }
    bytes.push(b'\n');
    stream
        .set_write_timeout(Some(WIRE_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream.write_all(&bytes).map_err(|e| e.to_string())
}

fn Receive<T: DeserializeOwned>(
    stream: &mut TcpStream,
    limit: usize,
    timeout: Duration,
) -> Result<T, String> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    let mut chunk = [0; 1_024];
    while bytes.len() < limit {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|e| e.to_string())?;
        let available = chunk.len().min(limit - bytes.len());
        let length = stream
            .read(&mut chunk[..available])
            .map_err(|e| e.to_string())?;
        if length == 0 {
            break;
        }
        if let Some(line) = chunk[..length].iter().position(|byte| *byte == b'\n') {
            bytes.extend_from_slice(&chunk[..line]);
            return serde_json::from_slice(&bytes).map_err(|_| "invalid local JSON".into());
        }
        bytes.extend_from_slice(&chunk[..length]);
    }
    Err("truncated or oversized local packet".into())
}

fn InvitationPath(label: &str) -> Result<PathBuf, String> {
    if !(1..=40).contains(&label.len())
        || !label
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("session label must contain 1–40 ASCII letters, digits or hyphens".into());
    }
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or("LOCALAPPDATA is required for local game invitations")?;
    Ok(PathBuf::from(local)
        .join("WyrmCoil")
        .join("invitations")
        .join(format!("{label}.json")))
}

fn ReadInvitation(path: &std::path::Path) -> Result<Invitation, String> {
    let file = fs::File::open(path).map_err(|_| "no invitation for this label")?;
    let mut data = Vec::new();
    file.take(1_025)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 1_024 {
        return Err("invalid invitation size".into());
    }
    let invite: Invitation = serde_json::from_slice(&data).map_err(|_| "invalid invitation")?;
    if invite.version != VERSION
        || invite.port == 0
        || !IsToken(&invite.session, 32)
        || !IsToken(&invite.capability, 64)
    {
        return Err("unsupported invitation".into());
    }
    Ok(invite)
}

fn IsToken(token: &str, length: usize) -> bool {
    token.len() == length && token.bytes().all(|c| c.is_ascii_hexdigit())
}

fn RandomHex<const N: usize>() -> Result<String, String> {
    use std::fmt::Write;
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    let mut output = String::with_capacity(N * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").unwrap();
    }
    Ok(output)
}

fn Exchange(invite: &Invitation, client: &str, call: Call) -> Result<Answer, String> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, invite.port));
    let mut stream = TcpStream::connect_timeout(&address, WIRE_TIMEOUT)
        .map_err(|_| "local session is closed")?;
    let request = Request {
        version: VERSION,
        capability: invite.capability.clone(),
        client: client.into(),
        call,
    };
    Send(&mut stream, &request, REQUEST_LIMIT)?;
    Receive(&mut stream, REPLY_LIMIT, COMMAND_TIMEOUT + WIRE_TIMEOUT * 2)
}
