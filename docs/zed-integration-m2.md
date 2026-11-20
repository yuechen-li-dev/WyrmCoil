# WyrmCoil M2 in Zed

## Input audit

GPUI dispatches keyboard events along the focused element path. Zed's keybindings resolve before unhandled key-down events reach the view's bubble listener. A preview focus handle and local key context can therefore receive movement keys while retaining editor shortcuts. GPUI exposes key-up events, a repeat flag on key-down events, and focus and modifier observers.

The M1 preview already owns the active-pane check, window activation and visibility subscriptions, and fixed-step suspension. M2 adds input cancellation to those boundaries and to preview blur. The command palette and other panels have their own focus handles.

The demo's existing horizontal move and stop events are consumed in the Player frame, which runs periodically. The engine enqueues typed events before the control tick and Dunewyrm promotes staged mailbox messages at the tick boundary. A demo-only movement message can use that path and take effect on the next control tick in any of the three demo frames. The existing velocity store and simulation integration already supply movement.

## Controls and ownership

Open `wyrmcoil: open preview` in Zed's command palette. Zed focuses the selected pane item using its normal tab behavior. Click the preview to focus it or resume play. Use **WASD** or the **arrow keys**. **Escape** pauses the preview and **Ctrl+W** closes its tab.

The local GPUI listeners track the eight movement keys independently, so releasing one alias leaves its other alias held. OS repeats are consumed in game focus without creating a new press. A press and release between ticks latches a one-tick tap. Opposite directions cancel. Modifier chords pass through GPUI's existing routing and clear movement. Space follows the editor's key routing. On Windows the default workspace Escape binding dispatches `Unfollow`; the preview pauses on this action while allowing it to continue to the workspace. Escape in the picker or command palette stays with that UI.

At each fixed step, the adapter emits a typed movement event when the latched axes change. The control tick promotes its mailbox message and a demo actuator updates the existing player velocity. Cardinal speed is one world unit per simulation tick and diagonals use the same magnitude. Demo-specific board slots keep player movement independent of the guard's velocity command when both run on the same tick.

Blur drops held keys and pending taps. Tab deactivation, visibility changes, window activation changes, and manual pause also reset the fixed-step clock. The next eligible tick sends neutral movement when the previous command was nonzero, before simulation can integrate the old velocity. Resume ignores autorepeat until a new press is received. A closed tab drops the session and its focus subscriptions.

## Visible movement

During the live test the player moved 24 world units, which appeared as roughly 2.5 screen pixels once the distant guard drove the M1 auto-fit scale. The canvas now uses a steady scale of 80 world units along its shorter side. The camera follows when the player leaves an 18-unit dead zone, and the world grid scrolls with the camera to show sustained movement. A distant guard uses a muted orange edge marker pointing toward its real snapshot position. The preview continues to paint GPUI quads and exposes both entities' world positions.

## Reproduce

Use the same Windows prerequisites and build options as M1. [M1 build and validation report](zed-integration-m1.md)

```powershell
pwsh -File scripts/build-zed.ps1
Push-Location vendor/zed
cargo test --locked -p wyrmcoil_view --lib
Pop-Location
```

Launch `vendor/zed/target/debug/zed.exe` and open the preview command in a workspace.

## Windows x64 validation

| Check | Result |
| --- | --- |
| Base WyrmCoil suite | 395 passed and 3 existing skips. The root naming check reported the same 8 baseline findings. |
| Adapter suite | 18 passed, up from 8 in M1. The added tests cover key transitions and aliases, repeated key-down, a tap between ticks, held movement and release through the real demo, opposing directions and diagonals, interruption and fresh sessions, deterministic replay, shortcut filtering, and camera follow. Adapter `wyrmfmt` and `cargo fmt --check -p wyrmcoil_view` passed. |
| Integrated build | The locked M0 script built the editor and CLI in 30.24 seconds on a warm Windows x64 build. |
| Live input | Tapped all eight movement keys and observed the matching position changes. A simultaneous W+D tap advanced both axes by about 0.7 units. After the camera fix, eight quick D taps moved the player 8 world units, or about 96 screen pixels, even with the guard already 181 units away. The player's position then stayed at `(8, 0)` while the guard advanced. |
| Focus and shortcuts | Ctrl+P opened Zed's file finder, W went into that picker, and Escape dismissed it without pausing the game. Ctrl+Shift+P retained the command palette and reused the existing tab. The project panel's Up arrow selection moved without moving the player. With the editor tab selected, literal `wasd` was saved to disk with Ctrl+S and the player stayed at `(8, 0)`. |
| Lifecycle | Escape froze the scene and a movement key had no effect until clicking to resume. The final build showed no catch-up or stale player movement across about 20.5 seconds minimized and 24.1 seconds with a second Zed window focused. Ctrl+W returned to the editor. Opening the preview again started the player at `(0, 0)`. The final build exited with code 0 with the preview open. |

The final binary advanced 1,598 simulation ticks and 1,598 snapshots over a 26.642-second active sample, approximately **60.0 Hz** for each. Display presentation timing was outside this sample. Six-second whole-process CPU samples were 46.4% of one logical core while active, 1.3% minimized, 5.2% with the second workspace focused, and 6.5% after closing the preview.

The Windows automation exercised taps and simultaneous chords. In a follow-up physical-keyboard check on the rebuilt preview, the user confirmed that holding D moved the player and releasing D stopped movement. Headless tests also cover sustained movement and cancellation with a missing key-up through the real demo engine. Linux and macOS input behavior remain unqualified.

The built editor is `vendor/zed/target/debug/zed.exe` with SHA-256 `4f91d8ded6e4b6e8c74c94085b08bbe70bb438b802ea654dd55addec64220b72`. Local evidence includes `target/zed-m2-final-build.log`, `target/zed-m2-final-tests.log`, `target/wyrmcoil-m2-regression.log`, `target/wyrmcoil-m2-wyrmfmt.log`, and the CPU and exit records in `target/zed-m2-smoke/`.

## Integration cost

The M2 baseline is M1 commit `8aa10eb3fc5695822cf6469641d9bca918923117`. Zed remains pinned at `a246423a3b24a08e0d0c8574eab3a9de422747e0`.

| Measure | M0 | M1 | M2 |
| --- | ---: | ---: | ---: |
| Adapter production Rust | 66 lines | 296 lines | 508 lines |
| Adapter test Rust | 0 lines | 146 lines | 398 lines |
| Integration total, including manifest and build script | 166 lines | 542 lines | 1,006 lines |
| Upstream handwritten Zed patch | 4 lines | 4 lines | 4 lines |
| Upstream files touched, including lockfile | 4 | 4 | 4 |

M2 adds 212 net production lines and 252 net test lines to the adapter. The demo's `world.rs` adds 50 lines and removes 2. M2 changes zero generic engine lines, zero GPUI lines, and zero upstream Zed lines, and adds no dependency.

## M3 handoff

Prototype a native GPU viewport behind the existing preview boundary. Keep GPUI ownership of the workspace, input focus, bounds, and composition. Feed engine snapshots into the GPU presentation path and identify the supported GPUI texture or surface API before proposing any upstream patch. Qualify resize, DPI changes, visibility transitions, and GPU resource lifetime on Windows.
