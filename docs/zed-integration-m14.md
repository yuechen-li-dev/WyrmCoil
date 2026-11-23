# M14 — Dominatus-style party AI in The Last Watch

The independent Last Watch game now runs three authored Rust brains through Dunewyrm.
In the Windows Zed preview, Alda, Merrit, and Vey can fight autonomously. Possessing
one adventurer leaves the other two on their own policies. The inspector shows the
live stack, utility scores, commitment, target, and most recent transition and command.

## Run and inspect

```powershell
./examples/games/last-watch/build.ps1
./examples/games/guardhouse/build.ps1
./scripts/build-zed.ps1
cargo run --locked --bin wyrmcoil -- playtest --project examples/games/last-watch --scenario last-watch --output target/m14/last-watch-playtest
cargo run --locked --bin wyrmcoil -- playtest --project examples/games/guardhouse --scenario guardhouse --output target/m14/guardhouse-playtest
```

Open `examples/games/last-watch` in the integrated Zed and use **wyrmcoil: run
game**. The encounter starts paused with Alda possessed. Press **Space** to
start or pause combat, **1–3** or left-click a live adventurer to possess them,
and **Y** to release the party. Right-click to move or attack. **Q/E/R** queues
the possessed adventurer's ability against the inspected target. **C** clears
their queue and **H** holds position. **I** cycles Alda, Merrit, Vey, and closed in
the read-only inspector. Use the command palette to restart after victory or defeat.

Build or edit gameplay independently of Zed after the M14 host has been built.
Reports are JSON evidence from an ordinary Rust playtest. The scenario code can
use Rust functions, assertions, conditionals, and loops. [Last Watch playtest](../examples/games/last-watch/playtest.rs)

## Dunewyrm kernel and Dominatus reference

The M13 engine used `DwSession` for fixed-step, leaf-driven control programs
with an integer frame PC, typed `DwKey` board, staged mailbox, typed actuator
routing, and persisted/replayable traces. Its `Dw::Decide` pushes the selected
target as a child. Its retained-root option protects root lifetime. It had no
independently stepped root that arbitrates a live branch every few ticks and no
ordered per-frame interrupt scan.

The reference audit used local Dominatus commit `7b52df3`, the user architecture
and authoring guides, the orchestration ladder, `HfsmInstance.cs`, the FishTank
root, and the RTS utility benchmark. The implementation follows the observed
order in the reference scheduler.

1. Check scan deadlines and walk the active stack from leaf to root. At each
   frame, evaluate eligible interrupts in declaration order, followed by its
   eligible normal transitions. A leaf normal transition can beat a root
   interrupt. A successful scan consumes the tick and preserves pending dirt
   and scan deadlines. After a scan with no match, advance the participating
   deadlines and clear the dirty mask.
2. Step the persistent root overlay. A stack-changing step consumes the tick.
   A nonstructural decision leaves the live branch and leaf-local data intact
   and lets the leaf execute. A lone root gets one Rust step per agent tick.
3. Step the leaf. `Push`, `Goto`, and the typed `Pop`, `Succeed`, and `Fail`
   outcomes manage resumable frames. The parent takes its child's outcome.
   Unwinding drops frames in LIFO order and calls the authored exit hook.

Each decision slot saves the selected state and its last switch tick. Every
evaluation refreshes the current score. The policy checks identical winner,
minimum commitment, hysteresis, then tie epsilon. Authored order breaks a
tie between otherwise identical candidates. `None` is unavailable and
nonfinite scores are reported as invalid and excluded. If every candidate is
unavailable or invalid, the active stack survives. A switch winds down the
branch above the root, retaining the root when configured.

The new API lives in the same Dunewyrm crate as `DwSession`. Both use the same
LIFO unwinding operation. The engine keeps its `wyrmcoil::Dunewyrm` re-export,
and independent games can depend directly on Dunewyrm without pulling in
graphics or the editor. Existing control programs, clock, and persistence keep
their prior behavior. M14's typed frame API uses associated types for state ID,
decision slot, frame, memory, observation, and command. Authors implement
`Brain` with ordinary Rust enums, initialization, a `Step` match, and optional
static transition rules. `Agent::New` verifies registration and policy data.
The runtime's typed push transition can deliberately preserve the matched
frame. Alda uses it to suspend `AttackCycle` while blocking.
[Dunewyrm authoring API](../src/Dunewyrm/brain.rs)

The typed API uses eligible integer ticks for cadence and commitment. It
records the game tick separately for inspection and replay. Rust frame data
has explicit ownership and is discarded on restart. Typed-frame persistence,
live migration, Dominatus's iterator continuations, and agent messaging were
outside this slice. The existing mailbox and persistence APIs remain available
to `DwSession`.

## Game authority and party policies

The game refreshes combat counters, captures a single immutable battle
observation for all party members, collects independent command proposals,
and applies orders in stable actor order before the enemies act. Both player
queues and autonomous proposals go through the same `ResolveOrder` path.
That path checks liveness, movement, stun, range, visibility, and cooldowns
before applying effects. Each autonomous ability reads its previous command
result from the next observation before reporting completion. Failed commands
leave a typed rejection in the trace. Active `BlockThreat` mitigates the
captain's hit through the existing damage resolver.

| Adventurer | Utility intents | Representative live behavior |
| --- | --- | --- |
| Alda | ProtectParty, Engage, Retreat, Regroup | Responds to a threat on Vey, approaches and attacks, returns from Shield Bash to AttackCycle, and temporarily blocks an incoming heavy hit |
| Merrit | Flank, Assassinate, Engage, Evade, Regroup | Weighs distraction, position, isolation, and health before choosing a target, seeking a flank, or using Backstab |
| Vey | MaintainDistance, CrowdControl, Engage, ProtectSelf, Regroup | Repositions when threatened and uses Frost Bolt against a live candidate with an available cooldown |

The three policies are ordinary Rust functions over a typed combat observation.
Each agent owns a separate stack, target, commitment, timers, and command
history. The act of attacking already exposes target, facing, stun, and slow
in the snapshot. Those facts coordinate this party without a new mailbox.
Utility evaluates every six eligible ticks, with 12 ticks of commitment and
a 0.08 hysteresis margin. Possession freezes that brain's clock, memory, and
stack while world timers continue. Release forces one fresh root evaluation
before the normal scan order resumes. The player's queue waits while its actor
is autonomous. Tactical pause freezes the world and all brains while leaving
possession, queued input, and inspection available.
[Encounter authority](../examples/games/last-watch/src/encounter.rs)
[Rust party policies](../examples/games/last-watch/src/encounter/party.rs)

The host validates bounded party reports on the existing framed protocol.
Older games can omit the additive agent field. An M14 Last Watch executable
expects the updated host. Each agent keeps its latest decision plus at most
eight previous decisions, eight transitions, and eight command results. The
CLI captures up to 32 explicit samples and rewinds through real process
restarts. Its state hash includes actor facts, decisions, commands, and
traces. The report identifies the first divergent request and world tick.
Bit-for-bit replay was verified on this Windows toolchain and executable.

## Qualification

WyrmCoil base `5e227f2bc34aa12f5dd7583d3dbfbb2a14d53a7c`. Pinned Zed
`a246423a3b24a08e0d0c8574eab3a9de422747e0`. Windows x64, Ryzen 7 7700X,
RTX 3070, and Vulkan SDK 1.4.350.0. Root and game tests used Rust 1.95.0
MSVC. The pinned Zed workspace used its own MSVC toolchain.

| Lane | Result |
| --- | --- |
| Windows integrated Zed debug build | Editor and CLI built through the pinned build script |
| Root regression | 432 passed, including 43 Dunewyrm tests |
| Independent game tests | Last Watch 9 and Guardhouse 2 passed |
| Real Windows game process tests | Last Watch and Guardhouse passed |
| Last Watch CLI | 36 passing assertions, 2,477 steps, nine sessions, and five matching replays |
| Guardhouse CLI | Nine passing assertions in 471 steps |
| Zed adapter, D3D11 GPU, Vulkan GPU | 30, 2, and 2 passing tests respectively |
| SDSL-V and shader cache | Eight opt-in conformance cases and six cache tests passed |
| Style | `rustfmt` and `git diff --check` passed. `wyrmfmt` reported the same eight M13 findings |

The CLI captured protection at world tick 56, Vey reevaluating and choosing
distance at tick 57, Merrit flanking at tick 45 in the second scenario,
Shield Bash returning at tick 76, and the attack-cycle block interrupt at
tick 188. The two hybrid scenarios matched all 57 and 667 replay requests.
The manual regression also reproduced victory and defeat. The default hybrid
scenario defeated all three guards and won after the player possessed Alda
and ordered her to the relic.

The Vulkan GUI playthrough released Alda, observed independent combat and
the stack at world tick 485, possessed Vey while paused, then resumed and
ordered Vey to the relic. The run reached victory at tick 1,137. All six
characters rendered with GPU skinning. A separate Vulkan pass possessed Merrit.
In the Vulkan process, switching tabs and saving an ordinary scratch file
worked, closing the preview reaped the game, and reopening produced a fresh,
paused tick-zero session. The Vulkan capture preceded the final correction
to the reusable root overlay's explicit return step.

The rebuilt host then ran the default hybrid game on D3D11 with CPU cached
poses. Sending Alda behind the captain and leaving Vey idle after taking
possession reached defeat at tick 1,705. **Restart Game** restored full health
and tick-zero possession. An earlier D3D11 pass also exercised the inspector
during autonomous combat. The test windows exited and the capture session
was released.

Representative local evidence under `target/m14` includes
[paused Vulkan combat](../target/m14/07-final-vulkan-frozen.jpg),
[Vulkan victory](../target/m14/08-final-vulkan-victory.jpg),
[editor save](../target/m14/09-zed-editor-save.jpg),
[reopen](../target/m14/10-vulkan-reopened.jpg),
[D3D11 combat](../target/m14/05-d3d11-autonomous-combat.jpg),
[D3D11 defeat](../target/m14/11-d3d11-hybrid-defeat.jpg),
[restart](../target/m14/12-d3d11-restart.jpg), and the
[Last Watch replay report](../target/m14/last-watch-playtest/report.json).
GUI evidence covers both playable hybrid outcomes. Specific utility-policy
boundaries, child returns, and interrupts have CLI or focused fixture evidence.

## Cost and comparison with M13

| Measurement | M14 |
| --- | --- |
| Dunewyrm typed runtime and public authoring API | 592 lines of Rust including the public types, plus a small shared unwind function |
| Last Watch agent policy and report projection | 723 production lines in `party.rs` before the optional timing probe |
| New correctness tests | Six kernel tests and four Last Watch tests, plus expanded Rust and real-process playtest coverage |
| Editor adapter | 149 added and 17 removed Rust lines across the tactical UI, marker selection, and viewport status |
| Vendor Zed and GPUI | Five Cargo.lock lines for a local crate. Zero upstream source or GPUI changes |
| Dependencies and code generation | No third-party additions, procedural macros, or generated brain code |
| Three-agent snapshot, candidate scoring, and stepping | 14 ns, 257 ns, and 172 ns per iteration respectively in the release probe. Scoring measures all options every iteration, while stepping uses the real six-tick cadence |
| Frame replacement and transition trace | 4 ns for a synthetic two-frame replace, 58 ns with stack copies and trace recording |
| Trace projection and encoding | 32.6 µs for three saturated inspection reports. The final agent report was 3,114 bytes in the probe |
| Memory | 240 bytes inline per typed party agent plus owned stack and histories. The authored frame payload is 12 bytes. The synthetic core fixture's active frame is 16 bytes |
| CLI replay | Hybrid prefixes took 169 ms and 1,047 ms, including process restarts and observation hashing. The entire 36-assertion run took 3.80 s |
| Vulkan GPU timestamps | Mean room pass 0.130 ms and skin pass 0.083 ms across 1,044 samples from the final run |
| CPU while fighting | In one 2.42 s window, Last Watch used 0.058 and Zed 0.968 logical-core equivalents. Working sets were 6.7 MB and 802 MB. After closing, the game was gone and Zed used 0.031 core equivalents over 2.02 s |

The release CPU probes isolate observation copying, scoring, frame replacement,
and trace projection. They exclude pathfinding, child-process IPC, and GPU
work. The GPU timestamps measure passes and exclude presentation. The desktop
CPU windows are single observations, and no matched M13 CPU sample was taken.
Presentation FPS was unmeasured. M13 had 426 default root tests and 14 Last
Watch CLI assertions. The M14 replay takes a longer path because it exercises
both hybrid and manual scenarios.

The Vulkan qualification process recorded eight tactical shader cache hits over two Vulkan
openings, zero DXC launches, and no Vulkan validation errors. The M13
historical 8.68 s initialization stall remains unresolved. M14 did not run a
new startup-outlier campaign or change the shader compiler, shading artifacts,
memory sharing, or GPU skinning. [M13 baseline](zed-integration-m13.md)

## M15 handoff

Use the M14 Rust playtests as controls while making possession tactically
valuable. The current squad can clear the room by itself, so the first
encounter pass should make the player's timing, target, or positioning
decisions matter. Improve ability and hazard feedback, tune enemy pressure,
and review controls and the party HUD in real playthroughs.

Build standalone distribution on the existing game protocol. Package the
optimized executable, original assets and license records, and a validated
shipping manifest for SPIR-V and optional DXBC. Follow the M13 deployment
contract so an ordinary player does not need Zed or DXC. Qualify a portable
Windows build on a clean machine before broadening the runtime.
