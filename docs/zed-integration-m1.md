# WyrmCoil M1 in Zed

## M0 audit

The M0 startup hook still calls `wyrmcoil_view::Init`. The adapter observes new Zed `Workspace` entities and registers `wyrmcoil::OpenPreview`. The action activates an existing preview in that workspace or inserts a new `Preview` entity in the active pane. Zed owns the pane item, its focus handle, and its removal when the tab closes.

GPUI's `paths_bench` example uses animation frame requests and a canvas. M1 uses the underlying `Window::on_next_frame` so the simulation runs in the frame callback and the canvas only paints. The convenience `Context::on_next_frame` captures a strong entity, so the adapter schedules through the window with a weak entity.

The existing `Engine::New()` demo creates player and guard entities. `TickControl()` refreshes the board, advances Dunewyrm, and dispatches acts. `TickSimulation()` integrates the world transforms. `RenderSnapshot()` increments the extraction counter and returns visible render items with world positions, entity IDs, and sprite IDs. The default guard controller already drives the guard in positive X and Y. The player starts at the origin, and both entities start with sprite ID 0.

## Live preview

The preview owns a single `Engine<DemoWorld, InputEvent>`, the latest snapshot, and the fixed-step clock. Each active GPUI frame callback adds monotonic elapsed time to an accumulator with an exact 60 Hz period. Each eligible step calls control before simulation. A callback can consume at most four steps and drops excess elapsed time. A fresh snapshot is extracted after a batch that advanced the engine. Callbacks with too little accumulated time request the next frame without invalidating the scene.

The selected tab in the active pane of the displayed Zed workspace can run while its window is focused and visible. Leaving the tab, losing window activation, or changing window visibility resets timing credit. The next active callback establishes a fresh timestamp. A pending callback checks the current item and window before stepping. It keeps only a weak reference to the preview. The window observers belong to the entity, and closing the tab drops its engine and subscriptions.

The canvas maps snapshot positions to GPUI quads with a shared scale on both axes and positive world Y pointing up. World zero maps to the center of the canvas. The view fits the largest absolute world coordinate plus an 18-unit margin. This removed the visible backward snaps from the initial stepped zoom. Both markers stay within the canvas as the guard walks away, and the guard covers fewer screen pixels per tick as the view zooms out. Resizing recomputes the projection from the canvas bounds. Color comes from the engine's player and guard entity IDs. The status row shows actual entity and sprite IDs, world positions, and the control, simulation, and snapshot counters. Snapshot count measures engine extraction cadence.

## Reproduce

From the WyrmCoil root, use the Windows prerequisites and optional build settings in the [M0 report](zed-integration-m0.md).

```powershell
pwsh -File scripts/build-zed.ps1
Push-Location vendor/zed
cargo test --locked -p wyrmcoil_view --lib
Pop-Location
```

Launch `vendor/zed/target/debug/zed.exe`. In an editor workspace, open the command palette and choose `wyrmcoil: open preview`.

## Windows x64 validation

| Check | Result |
| --- | --- |
| WyrmCoil regression suite | 395 passed, 3 existing skips. |
| Naming policy | The root `wyrmfmt` check reported the eight M0 findings. The adapter passed `wyrmfmt` and Zed's pinned `cargo fmt --check -p wyrmcoil_view`. |
| Headless adapter tests | 8 passed. They cover fractional accumulation, simulation equivalence under different frame partitions, bounded catch-up, pause/resume, real snapshot identities and movement, a fresh session after close, viewport mapping and resize, and forward motion across the former zoom threshold. On a 600 px canvas the regression case now moves about 2.48 px forward at that boundary. The initial stepped projection jumped about 121 px backward. |
| Integrated build | The existing locked M0 script built Zed and its CLI with the M0 Windows toolchain. After the projection fix, the warm script run took 28.65 s. |
| Live scene and resize | The Windows preview showed cyan Player #0 and orange Guard #1, both with sprite ID 0. The player stayed at the origin while the guard's world coordinates and marker advanced. Both markers stayed visible when the window was maximized and restored. After the projection fix, captures at simulation ticks 4, 21, and 52 showed the guard moving up and right through the former zoom threshold. |
| Tab lifecycle and editor | With the preview hidden behind an editor tab for about 49.7 s, the same session resumed without accounting for that interval. The palette reused its tab and Ctrl+W returned to the editor. Reopening after close showed simulation tick 5 and the guard near its spawn position. The project panel opened `beta.txt`, and an edit to `m1-smoke.txt` made while the preview was hidden was verified on disk after Ctrl+S. |
| Window lifecycle | Minimized for about 22.7 s and restored without a catch-up burst. With a second Zed window focused and the preview partly visible, simulation tick 1,155 and snapshot 1,156 stayed constant for 18.16 s. After focus returned the simulation resumed at tick 1,157. Zed exited with code 0 during the lifecycle run with an active preview and again after the final rebuild. |

Two timestamped observations of the final binary spanned 19.727 s. Control and simulation each advanced 1,183 ticks, and snapshot extraction advanced 1,183 frames. That is approximately **60.0 ticks and 60.0 snapshots per second** over this sample. Display presentation cadence was not measured.

Whole-process CPU was sampled over roughly six seconds per state, as a percentage of one logical core. These are Windows development-build samples.

| State | CPU |
| --- | ---: |
| Editor idle before preview | 4.4% |
| Preview active, final binary | 47.7% |
| Preview hidden behind editor tab | 4.1% |
| Window minimized | 3.4% |
| Window unfocused and partly visible | 1.0% |
| Preview closed, final binary | 3.9% |

Active preview CPU cost remains material in the development build. Qualification covered Windows x64. Linux, macOS, display presentation cadence, and longer multi-pane or cross-window stress remain unqualified.

The built editor is `vendor/zed/target/debug/zed.exe` with SHA-256 `be06fe526fb2c8c50caf760abe2cfbfc5baa35af0c6d348b68c686215ff62d54`. Local evidence is in `target/zed-m1-final-build.log`, `target/zed-m1-final-tests.log`, `target/wyrmcoil-m1-regression.log`, `target/wyrmcoil-m1-wyrmfmt.log`, and `target/zed-m1-smoke/`.

## Integration cost

M0 baseline `98d8c7e5420e027c0fc7efbd8aa81e79aea69988`. Pinned Zed `a246423a3b24a08e0d0c8574eab3a9de422747e0`.

| Measure | M0 | M1 |
| --- | ---: | ---: |
| Adapter production Rust | 66 lines | 296 lines |
| Adapter headless tests | 0 lines | 146 lines |
| Integration total, including manifest and build script | 166 lines | 542 lines |
| Upstream handwritten Zed changes | 4 lines in 3 files | 4 lines in the same 3 files |
| Upstream Zed files, including lockfile | 4 | 4 |
| M1 changes to engine source, GPUI internals, upstream Zed, and dependencies | — | 0 |

M1 adds 376 net lines to the adapter, including 230 production lines and 146 test lines. The existing M0 patch, Zed pin, and build path also support M1.

## M2 handoff

Route gameplay input through the existing `Engine::EnqueueInput` boundary. The preview should accept gameplay bindings only while its own surface has focus and Zed has activated the window. Reserve editor shortcuts and the command palette, and leave mouse capture off until it has explicit entry and release behavior. Clear held input on tab switch, blur, workspace switch, or window deactivation. Apply queued input at the next fixed control tick so input handling stays separate from painting and snapshot extraction.
