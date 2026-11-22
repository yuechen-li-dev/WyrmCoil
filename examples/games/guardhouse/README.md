# Guardhouse

Collect the three gold keys, stay out of reach of the red guard, and
reach the green exit. WASD walks, the arrow keys look, and Escape
pauses. Click the viewport to resume.

Open this folder on its own in the integrated Zed editor. In a trusted
local project, run the **Build Guardhouse Game** task, then use
`wyrmcoil: run game` from the command palette. `wyrmcoil: stop game`
freezes the last view and stops the executable. `wyrmcoil: restart game`
starts from the latest published build with a fresh room and score.

The equivalent terminal build uses ordinary Cargo.

```powershell
pwsh -NoProfile -File build.ps1
cargo test --locked
```

Edit `src/game.rs` for proximity, collection and ending rules, and
`src/scene.rs` for initial placement. Build and run again to see the new
rules. The task publishes the executable only after Cargo succeeds.
A running preview uses its own copy until you run or restart again.

To use this as a template, keep a dependency on `wyrmcoil-game`, supply a
type implementing `Game`, and call `Serve` from the binary's entry
point. `Start` returns a scene document and an initial observation.
`FixedUpdate` receives a one-based tick number, latched navigation input,
and player and guard positions. Return the current observation and IDs
of any authored boxes to hide. The host applies actions, updates the
world, and draws the result. Send executable diagnostics to stderr.

The example's relative SDK path points at this checkout. Update it and
the Cargo binary name in `build.ps1` if moving or renaming the project.
