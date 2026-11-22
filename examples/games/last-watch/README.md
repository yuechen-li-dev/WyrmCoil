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
| 1, 2, 3 | Select Alda, Merrit, or Vey |
| Left click | Select an adventurer or inspect an enemy |
| Right click ground | Queue a move order |
| Right click enemy | Queue an attack and mark the target |
| Q, E, R | Queue Shield Bash, Backstab, or Frost Bolt against the marked enemy |
| C, H | Clear the selected queue or queue a hold order |
| WASD, mouse wheel | Pan and zoom the tactical camera |
| Escape | Pause the encounter |

Focus fire on one guard at a time. Let Alda draw attention, then use
Merrit's Backstab from behind or while the guard is distracted. Vey
can slow a target from range. A trailing attack order keeps attacking.
An attack with another order behind it advances after its next strike.
Hold defends the tile until you clear the queue. Gold marks the relic,
cyan marks your selection and destinations, and orange marks the target.

`src/encounter.rs` owns combat and navigation. `src/scene.rs` owns the
starting room and asset and clip choices. The game executable talks to
WyrmCoil's SDK over the existing framed process boundary. After the
integrated editor is built, ordinary game edits need only a game rebuild
and restart. See [asset credits](ASSET_LICENSES.md) for original downloads.
