# M9. Shared glTF import and Rust playtests

The Last Watch loads its original KayKit glTF and GLB files through
`wyrmcoil::Engine::asset::gltf::AssetLibrary`. Guardhouse and The Last Watch
both have Rust scenarios that launch their published game executables and
drive the same version 1 process protocol used by the Zed preview.

## Run a playtest

From either game directory, publish the game with its existing build task
or `pwsh -NoProfile -File build.ps1`. Then run **Playtest Guardhouse** or
**Playtest Last Watch** from Zed's task picker. Run, stop, and restart in
the graphical preview still use the existing `wyrmcoil:` commands.

The equivalent commands from the repository root work without Zed.

```powershell
pwsh -NoProfile -File examples/games/guardhouse/build.ps1
pwsh -NoProfile -File examples/games/last-watch/build.ps1
cargo run --locked --bin wyrmcoil -- playtest --project examples/games/guardhouse --scenario guardhouse --output target/playtests/guardhouse
cargo run --locked --bin wyrmcoil -- playtest --project examples/games/last-watch --scenario last-watch --output target/playtests/last-watch
cargo run --locked --bin wyrmcoil -- asset inspect characters/Knight.glb --root examples/games/last-watch/assets --poses 20
```

`--project` supplies the guest working directory. By default the runner
loads the published executable at the project's `target/wyrmcoil-game.exe`
on Windows. Pass `--game` to name another executable. The host stages a
private copy per session and verifies that its content identity matches
across restarts. Build and run are explicit actions for trusted local code.

## Author a scenario in Rust

The two examples keep their scenarios in each project's `playtest.rs`.
For another game, use a Cargo test or a small Cargo binary depending on
`wyrmcoil`, and call `Playtest::Run(project, executable, output, identity,
Scenario)`. Give the scenario a versioned identity and use
`ContentIdentity(include_bytes!(...))` to record its source revision.
The example scenarios use regular loops and branches.

```rust
use wyrmcoil::GameRuntime::TacticalCommand;
use wyrmcoil::Playtest::{Input, Runner};

pub fn Scenario(run: &mut Runner) -> Result<(), String> {
    run.Check("encounter starts paused", run.Observe().Tactical()?.Paused)?;
    run.Step(Input::Tactical(vec![
        TacticalCommand::SelectParty(1),
        TacticalCommand::MoveTo([-2.0, 4.0]),
    ]))?;
    let checkpoint = run.Checkpoint();
    run.Step(Input::Tactical(vec![TacticalCommand::TogglePause]))?;
    run.Advance(13, Input::Idle)?;
    run.Equal("clock after unpausing", run.Observe().Tick(), 14)?;
    run.Rewind(checkpoint)?;
    run.Check("original paused queue is back", run.Observe().Tactical()?.Paused)
}
```

`Step` sends one input exchange, which also works while a tactical game
is paused. `Advance` requires the simulation tick to advance by exactly
one on each step. `Check`, `Equal` and `Near` report the caller's Rust
location, tick and assertion details. `Near` requires an explicit
tolerance. Room facts expose tick, result, remaining objectives, camera
and guard position, and visible object IDs. Tactical facts use the
existing typed observations for orders, actor positions, health, motion,
cooldowns and outcome.

`Rewind` starts a fresh process, runs the recorded input prefix and
compares the initial observation and every subsequent observation digest.
The report identifies the first divergent request and simulation tick.
Requests and restarts are bounded. Each reply uses the host's existing
game pipe timeout. The runner reaps the direct game process before
returning. Use a scratch project or an isolated save directory for games
with external effects. Replaying input does not undo filesystem or
network effects. Rust scenarios are trusted test binaries and need their
own bounds on loops that do not call the runner.

For interactive control, replace `--scenario …` with `--stdio`. The
session accepts commands such as `observe`, `select 1`, `move -2 4`,
`pause`, `resume`, `advance 13`, `assert tick 14`, `restart`, `replay`
and `quit`. This uses the same runner and emits a JSON observation per
command. The CLI exits nonzero on failure. Its version 1 `report.json`
includes the scenario and binary identities, protocol version, launches,
original input exchanges, assertion locations, final facts, replay
comparisons and elapsed time. Observation identities and assertion
values are stable across verified reruns. Timing and invocation paths
reflect each invocation.

To investigate a failing test, open the source location from the terminal
or report, inspect the observation and the tick in the failure, then
rebuild the game and run the same scenario. Use the graphical preview to
check camera, input focus, animation and composition.

## Asset ownership

M8 combined glTF parsing, source loading, pose evaluation and viewport
color packing in the render module. M9 moves source resolution, bounded
reads, decode and validation into the shared importer. `ModelAsset` owns
nodes, mesh lanes, material references, decoded images, joint influences
and inverse binds. `ClipSet` owns selected animation tracks. In-file tracks
address source nodes by index. External clips resolve unique named nodes.
`ModelAsset::Pose` evaluates local transforms into world matrices in
glTF coordinates. Its output is owned independently of the model.

Cache keys use canonical project-relative model/clip identity and
canonical external image paths. Loads share immutable model, clip and
image data within an `AssetLibrary`. A new library reads the current files.
`PaletteMesh` consumes that data for the existing viewport and applies
the M8 palette sampling, CPU skinning and named attachments. The Zed
adapter still owns prop placement, requested character and equipment
choices, sampled pose ranges and GPU atlas lifetimes. Both render
backends receive the same atlas. A graphical session reuses its atlas
across ticks and unchanged-art game restarts.

The supported subset is glTF 2.0/GLB with local or embedded buffers,
opaque indexed and unindexed triangles, base-color PNG/JPEG,
four skin influences, and linear or step translation, rotation and
scale tracks. The loader rejects unsupported extensions, non-triangle
primitives, sparse accessors, morphs, alpha material modes and unsupported
texture channels. It verifies buffer ranges, indices, transforms, skin
weights/joints, animation keys and project path containment. Errors name
the asset and import stage. The viewport samples base color into vertex
colors with nearest/clamp behavior, uses an upward shading normal when
the source has no normals, and does not shade metallic/roughness. These
are M8 presentation choices. The CPU model keeps source positions,
available normals, UVs, texture pixels and scalar material factors.

## Comparison with Aurelian

Aurelian.Playtesting drives a target through the same game host, records
facts as observations, and restores checkpoints by starting from a clean
session and replaying input. WyrmCoil uses the existing framed
`GameSession` request/reply path as that target and compares typed facts
per exchange. The Rust scenario is compiled using Cargo. JSON is the
evidence format. The machine-readable protocol and existing game SDK
remain at version 1. Native screenshots stay in the separate Windows GPU
and editor qualification path because the current interop compositor
requires the Windows renderer and its D3D11 device.

## Windows x64 qualification

Tested from WyrmCoil baseline `a0f8031` plus the M9 working changes, using
Zed `a246423a3b24a08e0d0c8574eab3a9de422747e0` on an RTX 3070.

| Gate | Result |
| --- | --- |
| Root Cargo tests | 417 passed, 13 optional tests ignored by the default command. |
| Game unit tests | Two Guardhouse and five Last Watch tests passed. |
| Real game process tests | Both passed. They exercised version rejection, failed-build preservation, timeout, deliberate crash, staged-process cleanup, and CLI reports. |
| Rust scenarios | Guardhouse won at tick 323 and lost at 148. Last Watch won at 1,014 and lost at 692. Both restarted, matched every checkpoint-replay observation and repeated independently with matching assertion values and final digests. |
| Failed assertion | Stdin requested tick 99 after two ticks. The CLI returned exit 1 and reported expected 99, observed 2, with the assertion source location and JSON report. |
| Integrated build | The pinned Windows editor and its companion CLI built through `scripts/build-zed.ps1`. The final incremental editor build took 67 s and the companion CLI build 3.88 s. Both used the dev profile. |
| Zed adapter and GPU tests | 29 adapter tests passed. Two D3D11 viewport tests and the validation-layer Vulkan depth, interop and compute test passed. |
| Naming check | The eight existing shader-fixture and formatter-fixture findings remained. |

Native smoke tests used the dedicated Zed profile and scratch project
copies. Both Build and Playtest tasks appeared in each task picker. The
exact playtest task commands ran from PowerShell in the two project
directories. The Windows computer-use skill bars launching terminal
commands through UI automation, so execution inside the integrated
terminal was unqualified.

The Vulkan preview showed the dungeon, three adventurers, three guards
and their equipment. A right-click queued a move while paused at tick
zero. After Space, the warrior moved, the queue cleared and the guards
patrolled. Guardhouse rendered and simulated in a separate Zed window.
A fresh explicit D3D11 run showed the same opening dungeon and actor
equipment. The previews closed, Zed returned to the Rust source tab and
accepted the save shortcut, and both editor sessions were quit. The
D3D11 exit code was 0. The Vulkan test windows closed after Quit, with
no numeric exit code collected. This was a graphical smoke test. The
win and loss sequences in this report are headless playtest evidence.

Evidence lives under `target/m9-gui` (task pickers, Vulkan dungeon,
executed order, Guardhouse and D3D11 dungeon), `target/m9-playtest`
(reports and independent repeats), and `target/m9-*.log`.

## Measurements and change cost

One cold Vulkan Last Watch open and a subsequent fresh D3D11 session
produced the following instrumented debug timings. The D3D11 pass
benefited from the OS file cache. These are individual samples.

| Phase | Vulkan | D3D11 |
| --- | ---: | ---: |
| Main source file reads | 100.73 ms | 2.07 ms |
| glTF parse and structure validation | 16.86 ms | 13.99 ms |
| Buffer reads, decoding and range validation | 38.30 ms | 1.93 ms |
| Image loading and texture decoding | 533.14 ms | 505.98 ms |
| Geometry extraction | 27.36 ms | 26.30 ms |
| Skin loading | 0.17 ms | 0.16 ms |
| Animation track decoding | 2.77 ms | 2.59 ms |
| Palette preparation | 5.58 ms | 5.26 ms |
| Pose evaluation | 56.11 ms | 53.44 ms |
| CPU skinning and transformed vertex generation | 1,849.82 ms | 1,751.80 ms |
| Pose atlas append | 30.94 ms | 21.65 ms |
| Total art preparation | 2.68 s | 2.40 s |
| Static mesh GPU buffer allocation and host upload | 22.05 ms | Unmeasured |

Both runs prepared 13 mesh/equipment roles, nine clips and 311 cached
poses from the existing 29 source files. The packed atlas remained
46,864,504 bytes, matching M8: 32,791,696 vertex bytes and 14,072,808
index bytes. Whole-process and total GPU residency were unmeasured.
Headless inspection of Knight measured 118.21 ms cold and 0.507 ms for
a warm model/clip lookup including path resolution. Its estimated
4,562,272-byte CPU payload includes 4,194,304 bytes of decoded RGBA.

Guardhouse's CLI run took 689 ms. Its 561 replayed exchanges across
three sessions took 374 ms including relaunch. Last Watch took 1,388 ms.
Its 1,760 replayed exchanges took 769 ms including relaunch. These
times include the normal host/guest pipe and checks. Presentation
FPS, fresh viewport cadence and input-to-display latency were unmeasured.

The shared asset module has 1,328 production lines and 184 test lines
including relocated tests, replacing the 616-line M8 importer. The
runner and CLI have 709 production lines. The example Rust scenarios
add 196 lines. The host changes are 65 added and 28 removed lines,
with zero protocol or game SDK changes. The viewport adapter changes
are 28 added and 36 removed lines, plus 21 added and three removed
Vulkan timing lines in the engine. There are zero upstream Zed or GPUI
source changes. The Zed workspace lock overlay adds three dependency
entries for `image`, `serde_json` and `urlencoding`, already pinned
transitively in M8. The original assets and provenance are unchanged.

## M10 handoff

Move one character's skinning to the existing SDSL-V Vulkan vertex
pipeline. Compare its world-space vertex results with the CPU reference
and measure cold preparation, per-pose upload and actual GPU residency.
The current vertex input carries position and packed color. GPU skinning
needs joint and weight lanes plus a per-pose palette binding, so M9
keeps the working cached-pose renderer. Keep the D3D11 and headless
playtest paths as regression controls.
