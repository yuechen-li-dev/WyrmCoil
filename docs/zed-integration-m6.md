# WyrmCoil M6 in Zed

## M5 audit

The M5 baseline is `81671a56c7dafb3c5f65ef6a610889eb42f90754`, with Zed pinned at
`a246423a3b24a08e0d0c8574eab3a9de422747e0`. The training-room task explicitly runs `cargo run
--locked` in the game folder. The authoring executable validates and encodes a
versioned TOML artifact, flushes a temporary sibling, and renames it over
`target/wyrmcoil-scene.toml`. Its identity is the existing versioned FNV-1a key
over the canonical scene payload. A failed build leaves the previous file in
place.

The preview resolves the artifact from one local directory worktree. The M5
reload reads at most one MiB plus one byte, validates the scene and identity,
and compares it with the mounted scene. A changed scene constructs a candidate
engine and snapshot before swapping. An unchanged scene preserves the camera
and simulation clock. The GPU pipeline and viewport belong to the preview
and live across a scene swap.

Zed's `Project::fs()` exposes an existing directory watcher with timed event
batching. This can observe artifacts inside Git-ignored Cargo output. Worktree
scan events are a poor fit because the artifact can be excluded from the
project model. The preview can own one cancellable GPUI task with a weak
entity handle and a directory subscription. A fresh project can observe its
root until `target` appears, then subscribe to the direct parent and reread
once to close the registration gap. M6 can use these APIs without edits to
Zed or GPUI.

## Workflow

1. Open `examples/games/training-room` as the single local Zed folder and run
   `wyrmcoil: open preview`.
2. Edit and save `src/scene.rs` in the editor.
3. Run `task: spawn` → `Build WyrmCoil Scene`. The equivalent shell command
   from that folder is `cargo run --locked`.
4. The open 3D preview mounts the next valid artifact automatically. The status
   line shows the scene name, identity, and read and mount timings. The manual
   `wyrmcoil: reload scene` action remains available.

Saving Rust source does not publish a scene. Cargo remains an explicit author
action. Rust language-server checks run independently of this workflow.

## Watcher and reload behavior

Each eligible room preview owns one GPUI task and one logical registration
with Zed's shared native watcher. The task listens for path batches with a
100 ms delay from the first event. It accepts the artifact path and directory
notifications that can signal creation or a rescan. It ignores source, build
binaries, and temporary artifact filenames. When `target` does not exist,
the task temporarily observes the worktree root and switches to `target` on
creation. Opening or switching the registration probes the artifact once.
Zed coalesces duplicate paths within a batch. Each relevant batch can trigger
one bounded read. If several publications land in the same batch, the preview
validates the file available at the end of that batch.

The task waits for the native event stream between batches. The preview has
no polling loop and stays subscribed while its tab is open, even when another
editor tab is active. Native notifications are required for automatic reload.
Registration failure or a closed event stream shows an error in the preview.
The task holds weak preview and workspace handles. Closing the tab drops the
task and registration. Reopening starts a fresh task and checks the current
artifact. The session is bound to the original worktree and checks eligibility
again before each read. Reopen the preview after changing the worktree.

Both manual and automatic reload use `RoomPreview::Reload`. They take turns
on the GPUI foreground, so a manual action cannot race an automatic mount.
The existing bounded reader verifies the schema, data, and content identity.
An unchanged identity keeps the world, camera, clock, and held state. A new
identity constructs the engine and first snapshot before replacing the active
session and clearing held input. The control-session constructor now exposes
its existing error to the loader. A failed read, invalid scene, missing file,
or failed construction leaves the previous world intact. The preview waits
for a new filesystem event after an error. Identical notices do not trigger
another view update. Scene swaps preserve the renderer and shader resources.

## Validation

| Check | Result |
| --- | --- |
| `cargo test --locked` | 411 passed with nine default skips, matching M5. |
| Zed adapter | 28 passed, up from 24. New cases cover path filtering, 256 duplicate path events yielding one read request, real filesystem probes and rollback, and a failed Dunewyrm session constructor. The M5 replacement test also covers the automatic entry point. |
| Graphics | Eight pinned shader conformance tests and two native D3D11 viewport tests passed. |
| Formatter | The integration passed `wyrmfmt`. The root check reported its eight existing findings. |
| Zed build | The pinned Windows x64 editor and CLI built with `scripts/build-zed.ps1`. The first successful M6 build took 41.85 s with the M5 build setup, and the final incremental build took 29.69 s. |

The adapter tests run from the prepared `vendor/zed` workspace with
`cargo test --locked -p wyrmcoil_view --lib`. They control when probes are
called and use real files without relying on filesystem event timing. The
desktop tests cover delivery and release of native subscriptions.

### Windows desktop evidence

The RTX 3070 smoke test opened a copy of the training-room project before
`target` existed. I edited `src/scene.rs` in Zed to move and recolor a box and
add another. Saving left the built-in room running and the artifact absent.
The build steps ran the Zed task's exact `cargo run --locked` command from
the shell. The Windows desktop automation exercised the editor and preview.
The first build published `662ccc15ea61a11e` and the open preview displayed
the new geometry and colors without a command or restart.

An identical build preserved the paused simulation at tick 1163 with the
camera at Z 8.75. An invalid zero-width declaration returned exit code 1 and
left the artifact byte-identical and the same world mounted. Correcting it
published `87cac557b4270d0a`. The inactive preview mounted it and showed the
wider cyan box at tick zero when reselected. Navigation and pause/resume
worked. An in-place truncated write, an unsupported version, and deletion
all surfaced diagnostics and preserved the paused camera and tick 598.
Three consecutive identical builds finished in 0.367 s and left that tick
intact. Another burst temporarily removed the artifact and atomically
replaced it 48 times in 98.40 ms. The 24 intermediate alternate publications
did not mount, as shown by the retained clock and scene. Raw OS duplicate
event totals were not measured.

The final build repeated native replacement while the preview was open,
after closing it, and after reopening and invoking Open Preview again. Zed
logged two file-event batches with the preview open, one with it closed,
and two after reopening. The baseline subscriber is Zed's own worktree
watcher. The extra subscriber exists only while the 3D preview is open.
The watcher inspector showed the same underlying native recursive roots.
Closing the preview also stopped viewport draws. Reopening loaded the latest
artifact and automatically applied the next publication. The manual reload
action continued to work. Both smoke-test sessions exited with code 0. The
trace included GPUI window-handle diagnostics at shutdown, also seen in M5.

### Live measurements

| Measurement | M6 result |
| --- | --- |
| Watcher | One logical subscription and GPUI task per 3D preview. Windows reuses Zed's existing native root. Zero preview subscriptions after tab closure. |
| Debounce | 100 ms Zed event batch, followed by one read for a relevant batch. |
| Sample artifact | 2,840 bytes each for the two edited rooms. |
| Read and validation | 1.72–2.51 ms observed for the sample builds and reopen. |
| Engine replacement | 0.02–0.23 ms, including the first snapshot. M5 reported about 1.9 ms for the combined read and replacement in its test. |
| GPU resource reuse | Zero pipeline or target recreations during the final in-place replacements. A tab reopen initializes its own pipeline and target. M5's two indexed draws and 640-byte or 720-byte payloads remained in use. |
| Process CPU | Six-second samples with GPU trace logging used 47.5% of one logical core active, 0.8% paused with the watcher attached, and 3.4% after closure. M5 recorded 53.5% active and 3.4% closed. Both the paused and closed M6 samples logged zero viewport draws. These are process samples and do not isolate watcher CPU or display FPS. |

Screenshots, Cargo output, the CPU samples and native logs are under
`target/zed-m6-smoke/`. `03-first-automatic-reload.png` records first
publication, `15-burst-preserved.png` records the retained tick, and
`21-final-automatic-reload.png` and `22-final-reopened-reload.png` show the
final binary. `final-Zed.log` and `lifecycle.txt` record the open/close/reopen
delivery counts. Native notification behavior was qualified on Windows x64.

The binaries are in `vendor/zed/target/debug`.

| Binary | SHA-256 |
| --- | --- |
| `zed.exe` | `6d8ed8248664c8b5680fb457c3d93efdb0ac4db24aa3eda44ff1b91987049b70` |
| `cli.exe` | `8d55bcca125682709a009a2f425664f79125b0c1c3c2a424e48f85daaefdfd3` |

## Integration cost since M5

| Area | Lines added / removed |
| --- | ---: |
| Preview and room adapter production | 164 / 42 |
| Fallible room and engine construction | 20 / 5 |
| Adapter tests | 119 / 1 |
| Vendor patch, Zed/GPUI source, lockfile, artifact schema, renderer, dependencies | 0 / 0 |

Four Rust files and this report changed. M6 continues to use the M5 artifact
reader, authoring executable, and Zed task definition.

## M7 handoff

Add source locations to scene validation diagnostics so an author can jump
from a scoped object error in the build terminal to the Rust declaration.
Capture the location in the authoring builder and report it during scene
compilation. Keep the compiled artifact schema and preview reload path steady.
