# The Last Watch

Alda, Merrit and Vey are the last adventurers to reach the old watch
post. Defeat its two guards and the captain, then send a survivor to
the golden relic chest on the north side.

Open this directory as its own trusted local project in the integrated
Zed editor. Run **Build Last Watch** in Zed's task picker or run
`pwsh -NoProfile -File build.ps1` from this directory. Then choose
`wyrmcoil: run game` in the command palette. The game starts paused.
Use `wyrmcoil: stop game` or `wyrmcoil: restart game` to stop or reset
the encounter. Failed builds preserve the last published executable.

| Control | Action |
| --- | --- |
| Space | Toggle tactical pause |
| 1, 2, 3 | Possess Alda, Merrit, or Vey |
| Left click | Possess an adventurer or inspect an enemy |
| Y | Release possession so all three adventurers use their own policies |
| I | Cycle the read-only agent inspector |
| Right click ground | Queue a move order |
| Right click enemy | Queue an attack and mark the target |
| Q, E, R | Queue the possessed adventurer's ability against the marked enemy |
| C, H | Clear the possessed adventurer's queue or queue a hold order |
| WASD, mouse wheel | Pan and zoom the tactical camera |
| Escape | Pause the encounter |

You start in control of Alda. The other two adventurers use their own Rust
policies. Focus fire on one guard at a time. Let Alda draw attention, then use
Merrit's Backstab from behind or while the guard is distracted. Vey
can slow a target from range. A trailing attack order keeps attacking.
An attack with another order behind it advances after its next strike.
Hold defends the tile until you clear the queue. Gold marks the relic,
cyan marks your selection and destinations, and orange marks the target.

`src/encounter.rs` owns combat and navigation. `src/encounter/party.rs` holds
the party's Dunewyrm policies. `src/scene.rs` owns the
starting room and asset and clip choices. The game executable talks to
WyrmCoil's SDK over the existing framed process boundary. After the
integrated editor is built, ordinary game edits need only a game rebuild
and restart. See [asset credits](ASSET_LICENSES.md) for original downloads.

After building, run **Playtest Last Watch** from the Zed task picker.
The Rust scenario in `playtest.rs` checks possession, autonomous decisions,
interrupts, the paused order queue, victory, defeat and deterministic replay
through the game process. Its report includes the captured agent histories.
The task writes `target/playtests/report.json`. The
[M9 playtest guide](../../../docs/zed-integration-m9.md) also covers
running the scenario from the shell and inspecting the KayKit assets.
[M14 agent runtime and qualification](../../../docs/zed-integration-m14.md)

For local co-op with an agent, launch the integrated editor with
`WYRMCOIL_GAME_MCP=last-watch` and configure the workspace's MCP server.
The host reserves seat 0 for human input, initially controlling Alda,
and lets guests possess
unowned adventurers. [M15 setup and qualification](../../../docs/zed-integration-m15.md)
