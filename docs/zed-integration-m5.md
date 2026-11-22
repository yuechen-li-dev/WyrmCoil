# WyrmCoil M5 in Zed

## M4 audit

The baseline is WyrmCoil `640aa3285eb5cb6638ed178d8d06da829f1e66f7` with Zed pinned at
`a246423a3b24a08e0d0c8574eab3a9de422747e0`. `Demo/room.rs` hardcodes the
floor, three walls, three boxes, all colors, camera spawn and lens, walk
bounds, and guard spawn, span, period, and phase. It fills the snapshots
from those constants on every extraction. The adapter assumes exactly six
static boxes and uses a fixed eight-instance packet. The cube mesh,
projection, depth attachment, SDSL-V shader, input latching, and fixed-step
Dunewyrm navigation already work. The authored scene supplies initial data
for those pieces. Guard phase, queued input, and camera movement stay in
the running world. The walk bounds remain axis-aligned and there is no
box collision.

## Design references

Aurelian compiles an immutable document to scoped placements before mounting
mutable agents. The root name is metadata, and instance paths distinguish
copies of the same fragment. M5 uses that separation and explicit canonical
paths for its smaller floor/box/player/guard vocabulary. Its owned fragment
trees cannot encode reference cycles. [Aurelian scene composition](https://github.com/yuechen-li-dev/copeland/blob/e0f7d61bf90f8d0579b1fcb60f41e7c6bf1bb1a8/docs/Aurelian/scene-composition.md)

Machina's `UiDocument` names rows and parents, then resolves frames into
viewport-dependent rectangles. Its `Row.Absolute` and `UI.At` illustrate
explicit local placement. WyrmCoil resolves scene transforms in 3D world
units. GPUI's flex layout owns the preview pane and status row. A future
HUD could use a separate contract for anchors and parent-relative pixels.
[Machina authoring guide](https://github.com/yuechen-li-dev/copeland/blob/e0f7d61bf90f8d0579b1fcb60f41e7c6bf1bb1a8/docs/Machina.UI/authoring-guide.md)

## Author, build, and reload

1. Build and launch Zed using `pwsh -File scripts/build-zed.ps1` and
   `vendor/zed/target/debug/zed.exe`. The Windows tool prerequisites are in
   [M0](zed-integration-m0.md).
2. Open `examples/games/training-room` as the workspace folder. Edit and save
   `src/scene.rs` in a regular Rust tab. Open `wyrmcoil: open preview` to see the
   room. `wyrmcoil: open planar preview` still opens the original demo.
3. Run `task: spawn` → `Build WyrmCoil Scene` in a trusted local workspace. Its
   terminal reports Rust compiler errors, scene diagnostics, or the resulting
   scene name and content ID. From the game folder, the equivalent command is
   `cargo run --locked`. From the repository root, use
   `cargo run --locked --manifest-path examples/games/training-room/Cargo.toml`.
4. Run `wyrmcoil: reload scene`. The preview reports the mounted scene and ID,
   or the reload error. It continues drawing the current room on failure.

The game folder owns `target/wyrmcoil-scene.toml`, including when Cargo is
configured to store its build outputs elsewhere. The adapter finds it in the
window's one local folder. Open the game folder separately for multi-root or
remote workspaces. Opening a fresh room preview reads an existing artifact if
present. Otherwise it starts the built-in training room. The adapter only reads
data. Building the scene runs the project's ordinary Rust program and is an
explicit action for a trusted project. A save alone does not build or reload it.

## Scene and render ownership

`Scene::World` takes owned declarations. Builders consume and return a node,
so the document holds a completed tree with private fields. `Group` composes a
parent-relative transform. `Fragment` holds owned declarations, and `Instance`
mounts a copy under a named scope. For example, two copies of a checkpoint can
resolve to `left.crate` and `right.crate` with independent world transforms.
Names are case-sensitive. The compiler validates names, placement, dimensions,
colors, camera and patrol settings, and the room contract of exactly one floor,
player and guard. Resolved boxes are sorted by scoped identity. Translation,
Y-axis rotation in radians, and positive local scale compose as M4 column
matrices. Nested nonuniform scales can yield a sheared model, which the renderer
receives intact. Player yaw follows the resolved forward axis, and guard patrol
stays on world X around the mounted body origin.

`src/Scene` is a small standalone library target built with `serde` and `toml`.
The engine also compiles its module source directly using its existing
dependencies. The standalone authoring executable avoids pulling in wgpu,
Zed or GPUI. Sharing the source keeps the validators and schema identical and
avoids a new dependency edge and lockfile change in Zed. Both entry points use
the same training-room builder. The built-in copy gives a fresh installation a
room before it has produced an artifact.

The compiler produces immutable `CompiledScene` data. Schema v1 is explicit
UTF-8 TOML with a one-MiB read limit. Canonical ordering makes equivalent
declaration orders produce the same bytes. A versioned FNV-1a content key
detects accidental changes. Decoding checks the schema, resolved
values, identity paths, and content key before the demo can construct a world.
The builder validates and encodes before creating a temporary file alongside
the destination. It flushes the file, then replaces the destination with a
rename. Failed Rust builds and scene validation preserve the last valid file.

A changed artifact mounts a fresh world and clears held input. An identical
artifact keeps the current clock and camera. A read or validation failure keeps
the current engine and snapshot. Static boxes share an immutable slice between
the world and snapshots. The renderer consumes the model matrices with the
same indexed cube mesh, SDSL-V shaders, depth test, and GPUI offscreen pass as
M4. It draws the floor and all opaque cubes in two indexed calls. Each instance
is 80 bytes, including the guard. An added box increases the instance upload by
80 bytes and may grow the existing backend buffer. The preview retains its
shader handle across reloads. A fixed-height scene status keeps the offscreen
target extent stable as counters and diagnostics change.

## Validation

| Check | Result |
| --- | --- |
| `cargo test --locked` | 411 passed, up from 406. The nine default skips remain. New tests cover scoped placement, reused fragments, canonical artifact bytes, invalid author input, corrupted artifacts, atomic replacement, and the real room runtime. |
| Zed adapter | 24 passed, up from 23. The new test exercises failed reload, shader handle reuse, changed projection data, and identical-artifact continuity. |
| Shader conformance | All eight passed with DXC, SPIRV-Tools, and Oct fixtures at `fa5b4993ca61f79b2ce18d09e9e3bbce88b9b9a0`. |
| D3D11 | Both hardware viewport tests passed, including depth occlusion in both submission orders and target resize and retirement. |
| `wyrmfmt` | The adapter passed. The root check reported the same eight existing findings in shader snippets and formatter test data. |
| Zed build | `scripts/build-zed.ps1` built the Windows x64 editor and CLI with `--locked`. The first successful M5 build took 229.56 s with the cached, debug-info-free configuration from M0. The final rebuild took 43.99 s. |

To rerun the Zed tests, prepare the integration and run these from `vendor/zed`
in the same Visual Studio developer shell used for the build.

```powershell
cargo test --locked -p wyrmcoil_view --lib
cargo test --locked -p gpui_windows --lib gpu_viewport -- --ignored
```

### Windows desktop smoke

On the RTX 3070, the scratch game opened the built-in layout with artifact ID
`1ebe01898af685a1`. I edited and saved its `scene.rs` in Zed, moved and
recolored the cyan box, and added a lime box. I verified that `Build WyrmCoil
Scene` appeared in the Zed task picker, and invoked its exact `cargo run
--locked` command from the shell. The GUI automation could inspect the task
picker and exercise editor actions. The build process and its diagnostic output
were verified from the shell.

`wyrmcoil: reload scene` mounted `b130e13102b2261f` without restarting Zed.
The baseline artifact was 2,612 bytes and byte-identical across two builds. The
edited artifact was 2,840 bytes. The edited and corrected author builds took
0.82 s and 0.70 s, respectively. Read, validate, instantiate, and snapshot took
1.91 ms and 1.88 ms as reported by the preview, before GPU presentation.

A zero-width lime box exited with code 1 and a `box-lime: invalid dimensions`
diagnostic. The artifact hash stayed `60f1237ab13afceef07a958927eadcd1f0c51de85dd09f4eb611f3c720886da8`.
Reload showed `Scene already current` with the camera and paused tick 588
intact. The corrected geometry mounted as `2c0c9240a9cd308e` and reset to tick
zero. I observed the nearby box occlude farther geometry, and the guard emerge
as its patrol advanced. W and Right moved the camera, Escape paused, and a click
resumed. I verified editor focus and saving, maximize/restore, Ctrl+W, a fresh
reopen from the corrected artifact, and the playable planar preview. Zed exited
with code 0. The trace recorded GPUI window-handle diagnostics during shutdown,
including the active-status message also seen in M4.

The final binary repeated the eight-to-nine-instance reload in 1.92 ms. I then
supplied a malformed artifact. The preview displayed a one-line `missing field
identity` diagnostic and continued advancing its camera-independent guard and
simulation clock. Rebuilding the valid artifact cleared the error without
resetting the world. Across all three reloads, the trace recorded one pipeline
initialization at 5.09 ms and one 1,296 × 929 depth-enabled target, both at the
initial open. The final process also exited with code 0.

| Live measurement | Evidence |
| --- | --- |
| Simulation and snapshots | Both advanced 366 over a 6.105 s sample, about 60.0 Hz. Display presentation FPS was unmeasured. |
| Zed process CPU | 53.5% of one logical core active and 3.4% after closing the room, over six-second spot samples with trace logging. The closed sample logged zero viewport draws. |
| Draw and upload | The room stayed at two indexed draws. The original eight instances uploaded 640 bytes per changed frame. The nine-instance room uploaded 720 bytes. The planar view still used one draw and 64 bytes. |
| Device resources | The existing 384-byte vertex and 72-byte index buffers stayed static. Growing from eight to nine instances required one larger dynamic buffer by the existing capacity rule. The final in-place reload retained the same shader pipeline and offscreen target in the trace. |

Screenshots, author build output, and renderer traces are under the local
`target/zed-m5-smoke/` directory. In particular, `02-zed-task.png` records task
discovery, `06-failed-build-retained-scene.png` shows rollback, and
`15-final-reload.png` and `16-final-corrupt-retained-scene.png` show the final
build. Mixed-DPI, remote workspaces, and device-loss recovery were outside this
qualification.

The Windows binaries are in `vendor/zed/target/debug`.

| Binary | SHA-256 |
| --- | --- |
| `zed.exe` | `a3ea8fdf8611032b111e2cb5bf2cd13c9c16330225157590f8a082718914b5d9` |
| `cli.exe` | `8d55bcca125682709a009a2f425664f79125b0c1c3c2a424e48f85daaefdfd3` |

## Integration cost since M4

| Area | Lines added / removed |
| --- | ---: |
| Scene authoring API, sample room, and author executable | 207 / 0 |
| Scene compilation and artifact encoding/publishing | 471 / 0 |
| Existing room demo production | 76 / 56 |
| Engine module exports | 1 / 0 |
| Preview adapter production | 140 / 36 |
| Scene and adapter tests | 273 / 2 |
| Shader and compiler, GPUI, vendor patches, and Zed manifests and lockfile | 0 / 0 |

The authoring target uses the same two direct library dependencies as this
module already has in the engine. The Zed pin, M0 and M4 patches, and engine
dependency graph stay at the M4 baseline. M5 adds no patched upstream paths.

## M6 handoff

The next authoring step is watching the **compiled artifact** in the current
local worktree, then handing each validated change to the existing reload
operation. Keep compilation explicit and ignore incomplete or repeated writes.
That would shorten the edit-build-preview loop without running author code on
save or introducing state migration.
