# M15 — Local LLM players in The Last Watch

The Last Watch now has one human seat and two guest seats. The human
controls the game from the existing tactical preview. Guest controllers
use a typed MCP server. All three seats use the game's validated order
queue, and unowned characters use their existing Dunewyrm brains.

## Run a local session

1. Build the independently published Last Watch executable and the
   `wyrmcoil` CLI. Build the pinned editor with `scripts/build-zed.ps1`.
2. Open Last Watch as a trusted local Zed workspace. Merge the
   [sample context server](../examples/games/last-watch/.zed/settings.game-mcp.example.json)
   into that workspace's `.zed/settings.json`. Use `wyrmcoil` on the editor's
   PATH, or point the workspace command at your newly built executable.
3. Launch the integrated editor with `WYRMCOIL_GAME_MCP=last-watch`.
   Choose **wyrmcoil: run game**. The HUD shows the local MCP seats when
   the invitation publishes. The game starts paused with Alda under
   human control.
4. Start a native Zed Agent or an ACP external-agent thread. Use the
   provider's normal authentication if needed. In the Agent Panel, ask
   the agent to observe the battle, claim its own seat, possess Merrit,
   and pick a move from the observations. Keep Alda focused on a guard
   so Merrit has an opportunity. A capable agent can observe the move,
   queue Backstab against a living distracted target, check the receipt
   and effects, then release Merrit and the seat.
5. Press **I** in the game to inspect the current owner, last player
   result, and suspended or active brain. Human input retains the
   existing click and keyboard controls. Use the standard Zed tool
   permissions and approve mutating calls individually as desired.

```powershell
./examples/games/last-watch/build.ps1
cargo build --locked --bin wyrmcoil
./scripts/build-zed.ps1
$env:Path = "$(Resolve-Path target/debug);$env:Path"
$env:WYRMCOIL_GAME_MCP = 'last-watch'
& ./vendor/zed/target/debug/zed.exe (Resolve-Path examples/games/last-watch)
```

Set the environment before launching the editor process that owns the
preview. A running editor retains its original environment. Each separate
MCP server process gets one guest identity. Configure a second named
server instance with the same label if two native threads need separate
guest seats. Give each thread its own server name so both do not use the
same MCP process. ACP agent sessions can start their own forwarded
instance. A client that ignores forwarded servers can configure the same
`wyrmcoil game-mcp --session last-watch` command in its native MCP settings.
Read-only tools can inspect the selected session without claiming a
seat. Gameplay tools require a claimed seat and the current session
identity and observation revision. [Zed MCP](https://zed.dev/docs/ai/mcp) ·
[Zed External Agents](https://zed.dev/docs/ai/external-agents)

## Control authority and lifecycle

The M14 preview sent human gestures as tactical commands over the M7
child-process frame protocol. The game mapped them onto one private
possession slot and a per-character FIFO. M15 replaces the slot with
three explicit seats. Human gestures translate to the same
`PlayerAction` accepted for a guest. The game process owns seat changes,
target and cooldown checks, queue admission, and execution. The existing
movement, line-of-sight and damage functions apply both kinds of orders.
Zed supplies ordered input and displays the resulting observations.
[Authoritative controllers](../examples/games/last-watch/src/encounter/players.rs)

Human inputs precede the bounded guest batch in a preview tick.
Claims choose a free guest seat in index order. A contested character
keeps its current owner, and the losing request returns `Occupied`.
Possession suspends only that brain. Release or disconnect cancels the
former controller's pending orders, preserves any movement already
in flight, and marks the same brain for fresh arbitration at its next
eligible tick. The other characters continue running normally. Defeated
characters lose possession. M14's manual-party mode remains a regression
route and cannot take over a game with claimed guest seats.

Paused combat accepts ownership and queued orders while world time
stays frozen. Any authorized seat can request tactical pause or resume.
The game recognizes duplicate pause/resume requests without flipping the
state. The visible preview runs at the normal cadence while an agent
reasons. Selecting a different workspace item or making the window
inactive retains Zed's existing preview suspension behavior. During
that suspension, a bridge call can return
`preview_not_servicing_requests`. Reopen or refocus the preview and
observe again.

An observation up to 360 revisions old can still submit a command.
The game validates the current owner, living target, cooldown, and
legal move before queueing, and checks execution constraints at action
time. An obsolete or future revision returns a fresh summary to recover
from. A replaced session requires a new observation, even when the
invitation label is reused. The event ring distinguishes acceptance,
queueing, execution, completion, rejection and cancellation. Events
are paginated by sequence, with an explicit gap if history aged out.
[SDK actions and observations](../src/Game/player.rs)

## Local transport and authorization

The standalone stdio process speaks MCP JSON-RPC to Zed or the ACP agent.
A separate, versioned request goes to the local session bridge, and the
original framed game protocol carries the authoritative action. On Windows
the bridge uses a standard-library IPv4 loopback listener on an ephemeral
port. That gives a bounded local transport without Win32 named-pipe
handle and overlapped-I/O code. The preview creates the listener only
with explicit launch opt-in. A single session-owned worker handles local
wire I/O through a bounded inbox. The existing preview pump consumes
ready commands without waiting for the worker, client, or inference.
[Bridge](../src/GameRuntime/collaboration.rs) · [MCP tools](../src/GameRuntime/collaboration/mcp.rs)

Discovery is scoped to the selected label. The per-user
`%LOCALAPPDATA%\WyrmCoil\invitations` directory contains one ephemeral
invitation with the random session ID, port and separate 256-bit bearer
capability. Labels accept only ASCII letters, digits and hyphens.
The capability stays in local IPC and is absent from tool output. Local
processes running as the same Windows user are in the invitation's trust
boundary. Keep the invitation out of shared project files and source
control. A second live preview cannot overwrite the same label. After
an abnormal exit, an orphaned invitation cannot reconnect to the old
endpoint. Start with a new label or remove the stale local file after
confirming that its host has exited.

Each guest lease defaults to 120 seconds and can be set with
`WYRMCOIL_GAME_MCP_TIMEOUT` from 5 to 1,800 seconds. Stdio EOF sends a
disconnect. If a process disappears without that cleanup, expiry
causes a typed leave event before the next eligible simulation work.
The monotonic timer operates in the host. Replay records the resulting
typed commands and their order, so the game stays independent of wall
time. Ordinary guest mutations have a 60 ms minimum interval per
seat. The bridge bounds requests, replies, wait times, and events.
Closing the preview or restarting revokes the invitation and retires
its bridge. Native Zed can keep its configured MCP process alive with
an empty session list until the next preview.

## Zed collaboration and online feasibility

The compatibility authority was Zed
`a246423a3b24a08e0d0c8574eab3a9de422747e0`. Its
`project/src/context_server_store.rs` resolves context servers across
visible worktrees and sets the local process's working directory.
`context_server/src/protocol.rs` negotiates MCP
`2025-11-25`. The ACP v1 implementation in
`agent_servers/src/acp/v1.rs` converts project MCP configurations into
stdio server definitions for new, loaded and resumed sessions. The ACP v2
path in `agent_servers/src/acp/v2.rs` also forwards project configuration.
Agents can choose which capabilities they consume.

Channels can already supply a meeting place, presence, voice chat,
shared notes, and shared code. An agreed human-readable label can relate
a game to a channel without binding their permissions. The pinned
extension WIT exposes the MCP command seam. The inspected channel and
call APIs are Zed's internal collaboration implementation and its
project/document protocol. The supported extension surface exposes no
channel membership or arbitrary game-data transport. A remote collaborator's
loopback server points at their own machine. Channel membership alone
confers no game seat or invitation. [Zed collaboration](https://zed.dev/docs/collaboration/overview) ·
[Zed Channels](https://zed.dev/docs/collaboration/channels)

The two-client loopback test uses one real WyrmCoil host. Both clients
claim distinct seats, and an attempted double possession is rejected.
This qualifies the ownership model and separate transport boundary for
M16. Online play needs its own authenticated game endpoint. Use a
versioned handshake with a session ID and authorized player identity.
Assign seats on the host, bind commands to the seat and session
generation, de-duplicate request IDs, publish revisions and bounded
event deltas, and use an expiring reconnect grant. On disconnect,
apply the same leave or release command already exercised locally.
The M7 game child and its tactical resolver remain the simulation
authority. The transport authenticates and orders messages. It can
replicate observations without replicating private world state or
requiring rollback. Keep invitations, presence and social sharing
independent of host admission.

## Windows evidence

- The pinned Windows x64 debug editor and CLI built. M15 changes
  WyrmCoil's own adapter and leaves upstream Zed and GPUI source alone.
  The reproducible vendor
  overlay has one Cargo.lock dependency line for `getrandom` 0.3.4,
  already pinned in both lockfiles.
- Zed's native `context_server` started the workspace stdio process,
  negotiated MCP 2025-11-25 and requested `tools/list`. This was
  verified from its actual logs. The native model turn and the graphical
  Agent Panel launch of the ACP thread were outside the observed run.
- An authenticated Codex ACP 2.9.0 agent played in the live pinned Zed
  Vulkan preview. A separate non-UI ACP qualification client sent the
  same MCP server definition that Zed's project forwarding constructs.
  The tool trace contains live observations and the model's decisions.
  It chose a move to `[-4,3]`, which completed at tick 53, selected a
  distracted captain after observing the changed fight, and landed
  Backstab at tick 1,520. It checked the completion and cooldown,
  released Merrit, then saw her autonomous `Root → Flank` at tick
  2,099. It released its guest seat before ending the turn. The human
  started Alda's attack from Zed and later recovered the relic at
  tick 3,085. Vey remained autonomous.
- Vulkan showed six GPU-skinned actors on the RTX 3070. The warm launch
  recorded four tactical shader cache hits and zero DXC spawns.
  Validation logged zero Vulkan errors. Closing the preview reaped the
  game and removed the invitation. The dedicated editor exited cleanly.
- The live D3D11 preview displayed all six actors with CPU-cached poses.
  A separate MCP client possessed Vey, completed a move to `[2,4]`,
  paused combat, and released her. Closing removed the invitation.
  Reopening started at tick zero with Alda under human control and both
  guest seats available. Editor shutdown reaped the game and the MCP
  process.
- Real stdio MCP integration exercised 13 tools at discovery, forged
  capability rejection, two guest seats, conflict, rate limiting, stale
  revisions, independent human and guest movement, executed receipts,
  autonomous resumption, EOF release, controlled lease expiry, restart
  generations and cleanup.

Evidence is local under `target/m15`. See
[mixed ownership](../target/m15/01-vulkan-mixed-party.jpg),
[move completion](../target/m15/02-acp-merrit-move-completed.jpg),
[Backstab completion](../target/m15/03-acp-backstab-completed.jpg),
[brain resumption](../target/m15/04-acp-released-to-dunewyrm.jpg),
[Vulkan victory](../target/m15/05-vulkan-cooperative-victory.jpg),
[ACP turn trace](../target/m15/real-acp-play.jsonl),
[D3D11 preview](../target/m15/06-d3d11-mcp-control.jpg), and the
[D3D11 bridge trace](../target/m15/d3d-bridge-proof.json).

## Measurements and reproduction

```powershell
cargo test --locked
cargo test --locked --test game_mcp -- --ignored --nocapture
cargo run --locked --bin wyrmcoil -- playtest --scenario last-watch --project examples/games/last-watch --output target/m15/last-watch-playtest
cargo run --locked --bin wyrmcoil -- playtest --scenario guardhouse --project examples/games/guardhouse --output target/m15/guardhouse-playtest
```

The root suite passed 432 tests. The Zed adapter passed 30 tests,
shader conformance passed 13, and the Vulkan and D3D11 hardware suites
passed two each. Rust formatting and whitespace checks passed.
`wyrmfmt` reported the same eight pre-existing findings as M14.
The real-process Guardhouse and Last Watch execution tests passed
independently. The Last Watch CLI run
passed 54 assertions across 2,536 steps, 11 sessions and six replays
in 4.50 s. Guardhouse passed nine assertions across 471 steps, five
sessions and three replays in 0.79 s. Neither run reported divergence.
[Last Watch report](../target/m15/last-watch-playtest/report.json) ·
[Guardhouse report](../target/m15/guardhouse-playtest/report.json)

One Windows debug integration run measured 205 ms to spawn and initialize
the MCP server, 32 ms for the first observation, and 47 ms from seat claim
call to authoritative reply. The initial structured battle result was
2,460 bytes. In the live D3D11 run, battle observations took 7.35–14.33 ms,
claim took 34.44 ms, possession took 34.04 ms and release took 25.13 ms.
These timings cover local process and pipe costs. Model inference and
approval time were unmeasured. The Vulkan timestamp log measured
0.128 ms per room pass and 0.081 ms per skin pass over 2,966 samples.
There was one active game session, with two simultaneous guest leases
demonstrated by the integration test and one guest in the model run.
Presentation FPS was unmeasured. A 2.34 s D3D11 combat sample measured
0.020 logical core equivalents and 6.49 MB working set for the game,
0.502 core equivalents and 314.47 MB for Zed, and 0.000 core equivalents
and 6.34 MB for the idle native MCP process.
[Process sample](../target/m15/d3d-combat-cpu.json)

The bridge and MCP implementation add 949 source lines. The SDK player
types add 110 lines, the encounter ownership module adds 362, and the
room and tactical preview adapter gain 72 net lines. The stdio
integration test adds 328 lines and the Rust playtest gains 134 net
lines. The only new direct dependency is the already pinned
`getrandom`, used to generate invitation credentials and session IDs.

M15 changes the size of tactical snapshots, so replay cost includes a
larger observation and the extra mixed-player sequence. A matched
simulation-only CPU comparison and model-inference latency were
unmeasured. The historical M13 startup outlier remains unresolved.

## M16 handoff

Use the existing two-guest Rust replay and real MCP test as offline
controls. Add one remote guest over an independently authenticated
transport with host-assigned seats, action sequence acknowledgments,
disconnect and rejoin exercised first. Share a game label in a Zed
Channel for convenience. Preserve a separate game invitation and audit
the trust rules before broadening access outside the host.
