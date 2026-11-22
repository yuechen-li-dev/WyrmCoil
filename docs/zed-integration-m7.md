# WyrmCoil M7 in Zed

## Execution boundary

Zed owns the preview item, focus, keyboard latching and the frame
callback. WyrmCoil owns the fixed-step clock, Dunewyrm control tick,
world simulation and immutable snapshot. The room currently instantiates
its world from a validated scene artifact. The independent training-room
executable compiles an authored scene and exits. Its artifact contains
placement, camera, bounds and patrol data. Gameplay code currently has
no way to execute after the artifact is mounted.

The Windows M6V graphics qualification exercised native SPIR-V on the
RTX 3070 and composed a GPU-resident copy through Zed's D3D11 renderer.
The subsequent compute build ran a storage-buffer dispatch between room
draws on the same device in a headless GPU test. Guardhouse can use the
existing room snapshots and both graphics backends directly.

Aurelian's Game Shell puts fixed updates and game state in ordinary
authored code, and its scene composition keeps initial declarations
separate from runtime state. Its TinyFarm playtest harness uses the same
gameplay authority for native and recorded inputs. M7 follows those
ownership choices with a smaller initial game contract.

| Candidate | Fit for M7 |
| --- | --- |
| Windows DLL | Runs Rust compiled independently with cheap calls. Requires a separate C ABI, shared-memory ownership rules and safe unloading. A native crash can take down the editor. |
| Executable | Uses a normal Cargo binary and copies only scene, tick and action data. The child can be restarted independently. The host already owns rendering, so GPU objects stay in the editor. Requires bounded IPC and child cleanup. |
| WASM | Provides explicit imports and guest memory, with more isolation options. Requires a runtime dependency, target tooling and a host capability API. |

M7 uses the executable path with a versioned, length-delimited pipe
protocol, one fixed tick in flight, and a finite response deadline.
Guardhouse owns its scene source and gameplay state. WyrmCoil validates
the initial scene, applies the game's supported mutations and takes
the snapshot using the existing room world. The project publishes a
successful Cargo build atomically, and the host starts a session copy
so a rebuild can proceed while the current code runs. Zed's existing
local-project trust must permit execution, and a Run or Restart action
is an explicit request to start that code.

## Run Guardhouse

1. Build the integrated editor with `pwsh -File scripts/build-zed.ps1`
   using the Windows toolchain in the [integration guide](../integrations/zed/README.md).
2. Open `examples/games/guardhouse` as its own trusted local folder in
   Zed. Run **Build Guardhouse Game** from the task picker. The terminal
   equivalent from that folder is `pwsh -NoProfile -File build.ps1`.
3. Open the command palette and run `wyrmcoil: run game`. Use WASD to
   move and the arrow keys to look. Collect three gold keys and touch
   the green exit while avoiding the red guard. Escape pauses, and
   clicking the preview resumes.
4. `wyrmcoil: stop game` ends the child process and leaves its last
   snapshot visible. `wyrmcoil: restart game` loads the latest published
   executable and starts with a fresh world, clock and score. `Ctrl+W`
   closes the preview and stops the game. Use `wyrmcoil: reload scene`
   to return the 3D preview to the M6 scene-artifact workflow.

The example uses the public `wyrmcoil-game` SDK. Its `Game::Start`
supplies the authored scene and initial observation. `Game::FixedUpdate`
receives the one-based tick number, latched axes and pre-simulation
player and guard positions. It returns game status, objective progress
and authored box IDs to hide. Guardhouse owns the collect, detect and
exit rules. The host applies validated actions to `RoomWorld` after
Dunewyrm's control tick and before `TickSimulation`. Existing renderers
consume the resulting immutable `RoomSnapshot`. The game protocol
contains versioned, length-prefixed JSON frames and the existing scene
artifact encoding. The child uses stdout for the protocol and stderr
for diagnostics.

The host allows one request in flight, up to 2 MiB per frame. A worker
does the blocking pipe I/O. The editor may wait up to 1 ms for a new tick
reply, then resets fixed-step timing if it is still waiting. Initialization
and tick response deadlines are 3 s and 500 ms respectively, checked when
the preview advances. Pause and tab suspension stop issuing ticks and cancel
held keys. A tick already issued can complete in the child, with its
response applied when the preview resumes. Win or loss freezes the last
snapshot until an explicit action. Scene watcher events cannot replace a
running or stopped game session.

The game build publishes `target/wyrmcoil-game.exe` by adjacent atomic
replacement after Cargo succeeds. Run and Restart stage a private copy so
Windows file locking does not block a rebuild. Failed initialization and
protocol errors leave the last visible snapshot and display the error.
Stop and preview closure close the request channel. A reaper grants the
child 150 ms to exit, then kills and waits for a stalled child and removes
its staged files. This boundary runs trusted local code with the user's
permissions. It manages the direct child. Cleanup during forced editor
termination and arbitrary descendant process handling are unqualified.

## Qualification

Windows x64, NVIDIA RTX 3070, driver 610.88. WyrmCoil qualification used
working-tree M7 changes on `183064bf47e288fcba945a253a65c931a171d156`,
with the in-progress M6V changes present. Zed is pinned to
`a246423a3b24a08e0d0c8574eab3a9de422747e0`. Both a pristine detached
checkout and an existing M6V checkout prepared successfully and were
idempotent on repeat. The integrated Windows editor and CLI built using
the established MSVC toolchain. The exercised editor was
`vendor/zed/target/debug/zed.exe`.

| Check | Result |
| --- | --- |
| Root `cargo test --locked --offline` | 413 passed, 12 ignored. Includes the shader conformance tests. |
| Guardhouse `cargo test --locked` | Two gameplay tests passed. |
| `cargo test --locked --offline --test guardhouse_execution -- --ignored --nocapture` | One end-to-end test passed. It built and ran the actual executable, replayed a 323-tick win twice identically, tested loss and restart, replaced the executable while an older copy ran, and checked last-good build retention, invalid initialization and protocol, crash and hung-child cleanup. |
| Prepared Zed `cargo test --locked --offline -p wyrmcoil_view` | All 28 adapter tests passed, including scene watcher and preview regressions. |
| M6V graphics and compute | Actual Windows Vulkan test passed with validation enabled. It covered same-device room rendering, interop, resize, depth ordering and SDSL-V vector addition. All 11 shader conformance tests passed with the Oct fixtures available. |
| Native D3D11 | Both GPU viewport tests passed, including resize and order-independent depth. |
| Original training room | Its independent Cargo manifest checked successfully. |
| Naming policy | Same eight root findings in existing shader snippets and `wyrmfmt` fixtures. Adapter naming check passed. |

The Windows playthrough opened a scratch copy of Guardhouse as a regular
Cargo project. Vulkan was selected in the actual Zed viewport. Keyboard
navigation collected the keys, escaped and won. A fresh run lost to the
guard. Stop preserved the final picture. Escape, click-to-resume, editor
tab and command-palette focus, resize, minimize and restore, `Ctrl+W`,
and reopening the game were exercised. The five full seconds sampled while
minimized logged zero Vulkan compositor copies.

The Zed task picker listed **Build Guardhouse Game**. Qualification ran
the task's exact build command from the shell. An edit to the guard's
detection radius was saved in Zed, rebuilt, and run in the same editor
process with the same editor executable hash. The new game lost at spawn
on tick one. A deliberate compiler error returned exit code 101, retained
the previous game artifact hash, and allowed the prior build to run again.
The scratch source was restored and rebuilt. A separate real Zed session
explicitly selected D3D11 and verified that the same game rendered, walked,
and looked correctly. Both editor processes exited with status zero.

Local evidence is under `target/m7-gui`, including `04-won.png`,
`06-lost.png`, `10-rebuilt-lost-at-spawn.png`, `17-native-build-task.png`,
`20-d3d11-movement.png`, process counters and build hashes. Test and
build logs are under `target/m7-*.log`.

## Cost and performance

| Measure | M7 result |
| --- | --- |
| SDK and host | 181 SDK Rust lines and 363 host Rust lines, including the transport. |
| Guardhouse | 151 production Rust lines, 32 build/task lines and 63 unit-test lines. The separate process integration test adds 290 lines. Cargo manifests, generated locks and docs are excluded from these counts. |
| Preview adapter | +205/−34 relative to the local M6V baseline, including the dependency declaration. Most changes are game session lifecycle, command routing and HUD state. |
| Engine and demo | The generic kernel is unchanged. The game adds a 25-line transactional box-removal operation to `RoomWorld` and a host module export. Scene and shader representations are unchanged. |
| Pinned Zed | Zero additional handwritten Zed or GPUI lines. The M7 overlay adds 19 generated lockfile lines. The build script can upgrade a checkout already prepared for M6V. |
| Dependencies | The SDK uses the existing scene crate and Serde, plus `serde_json`. Pinned Zed already ships `serde_json`. The adapter uses Zed's existing `project` crate for permission checks. No new graphics dependencies. |
| Build and launch | Initial game build and publish took 8.2–9.3 s in separate local debug builds. Representative gameplay rebuilds took 1.0–1.6 s. Process launch, scene validation and mount took 81–95 ms in the headless integration, including an 87 ms restart. Integrated incremental Zed and CLI builds took 34.62 s and 1.39 s. |
| Host step | Two 323-tick replay runs took 24.9 and 25.6 ms each, or 77–79 µs per combined control, IPC, simulation and snapshot step. This measures headless throughput. Separate IPC and input-to-display latency were unmeasured. |
| Live cadence | The Vulkan GUI advanced 366 simulation ticks and 366 snapshots in 6.103 s, about 60.0 Hz for each, comparable to M6V's 60.1 Hz room sample. Display FPS was unmeasured. |
| CPU and memory | Six-second debug samples showed 96.3% of one logical core in active Vulkan Zed and 0.26% in the game process. Working sets were about 755 MiB for Zed and 5.7 MiB for the game. After closing the preview, Zed used 8.1% of one core with no game child or staged session remaining. The earlier M6V debug sample showed 84.7% active Vulkan at a different viewport size. These whole-process samples cannot attribute the difference to M7. |
| GPU work | The existing two indexed draw batches, static shared geometry, and device-resident Vulkan-to-D3D11 copy are reused. The room has ten instances before collection at 80 bytes each in the existing instance format. Each collected key removes one instance. Run, Stop and Restart retain the viewport shader object. Per-restart native allocation counts were unmeasured. |

## M8 handoff

Exercise the SDK with one additional game-owned interaction that changes
an existing scene object, such as opening Guardhouse's locked exit when
all keys are collected. Keep the state and rule in the game and add a
narrow typed host action only if the existing box operation cannot express
the visible change. Extend the deterministic real-process replay to
exercise that interaction before adding more world APIs.
