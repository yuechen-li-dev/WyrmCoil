# M8 · The Last Watch

The Last Watch is a separate Cargo game with three party members, two
guards and a captain. Alda can interrupt with Shield Bash, Merrit can
exploit a distracted target with Backstab, and Vey can slow targets with
Frost Bolt. Focus fire to survive, then send a party member to the relic.
The [game README](../examples/games/last-watch/README.md) covers the
controls and [asset credits](../examples/games/last-watch/ASSET_LICENSES.md)
record the official downloads, source versions, hashes and used files.

## Runtime and rendering

Protocol version 1 and its existing room messages remain valid for
Guardhouse. A tactical game opts into separate Start reply and Tick
messages on the same length-framed process transport. Commands and
snapshots remain bounded. The guest owns combat, the order queues, the
animation state and the simulation tick. A packet can carry orders
while tactical time is frozen. The adapter skips empty packets while
paused and handles pan and zoom locally. The M7 process, restart,
trusted-project check, and artifact publishing rules still apply.

The game resolves a grid step with a stable breadth-first search.
Obstacles block navigation and ranged sight. Cell reservations prevent
party members from stepping into each other's positions. Guard patrol,
targeting, hit points and cooldowns are stepped at the same fixed tick
as party combat. Animation time comes from the guest's tick counters.
Picking uses the existing perspective camera basis to construct a
viewport ray, then tests actor bounds and the ground plane. GPUI draws
health and status overlays.

The game describes its model, attachment and clip choices in Rust.
`gltf` 1.4.1 reads the original GLTF/GLB assets, including hierarchy,
indices, normals, UVs, palette textures, joints, inverse binds and the
animation channels used by these files. The first viewport load samples
the KayKit palettes to vertex colors and CPU-skins the selected poses at
up to about 10 visual samples per second. LINEAR rotations use
quaternion interpolation. STEP holds the preceding key. Equipment
follows the original named hand sockets. The asset cache is keyed by
project and declared art. Reopen the preview after changing files under
an unchanged asset path.

The visible dungeon and characters are actual imported geometry. Each
live scene instance identifies a base vertex and an index range in the
immutable mesh atlas. A pose update only changes the chosen range.
Adjacent instances sharing a range are batched. The SDSL-V shader
unpacks vertex colors and the existing projection, depth testing and
Windows compositor are reused. Vulkan still publishes to Zed's
D3D11 compositor on the matched device. D3D11 accepts the same draw
data when explicitly selected with `WYRMCOIL_GRAPHICS_BACKEND=d3d11`.
This slice supports opaque palette assets. General UV sampling on the
GPU, alpha materials and interpolated GPU skinning remain future work.

## Reproduce

1. Build the pinned Windows editor through `scripts/build-zed.ps1`.
2. Open `examples/games/last-watch` as a trusted single-folder project.
3. Run **Build Last Watch** or `pwsh -NoProfile -File build.ps1` from
   the game directory. Open the command palette and run
   `wyrmcoil: run game`.
4. Plan orders while paused. Press Space to resume. Stop and restart
   use the existing WyrmCoil commands.

```powershell
cargo test --locked --offline
cargo test --locked --offline --manifest-path examples/games/last-watch/Cargo.toml
pwsh -NoProfile -File examples/games/last-watch/build.ps1
cargo test --locked --offline --test last_watch_execution -- --ignored
```

The Zed revision is `a246423a3b24a08e0d0c8574eab3a9de422747e0`. The
M8 overlays are relative to the M7 prepared workspace, with the M6V
interop patch present. The Zed app and workspace source have no extra
handwritten changes. The two-file GPUI overlay adds 50 lines and
removes 11 for per-instance mesh ranges and the D3D11 range test.
The Zed lockfile overlay adds 52 generated lines for the glTF import
dependency.

## Windows qualification

The pinned Windows x64 Zed application and CLI built successfully.
The native Vulkan playthrough on the NVIDIA GeForce RTX 3070 showed
the imported dungeon, six rigged actors and their equipment. Through
the normal viewport input, I selected units with the keyboard and
mouse, inspected enemies, queued and executed all three abilities,
and defeated the guards. Vey then reached the relic and won at
simulation tick 960. A separate restarted run sent the party into the
captain's lane without attack orders and reached defeat at tick 692.
These are short encounter runs. Five-to-ten-minute player pacing was
unqualified.

Camera zoom and pan worked at tick zero while tactical time remained
paused. The first fight showed all three ability cooldowns and health
changes. Restart restored positions, health and empty queues. I edited
Alda's starting health from 144 to 165 in Zed, saved, rebuilt the game,
and observed 165/165 after restarting within the same editor. A bad
edit returned Cargo exit 101 and preserved the previous artifact by
hash. The task picker found **Build Last Watch**. The qualification
ran its exact build command from the shell. I restored the scratch
project after testing.

In a second Windows run, the explicit D3D11 fallback showed the same
assets and animated patrol, accepted a ground command, and displayed
the moved warrior after minimize, restore and resize. Both sessions
exited with status zero. The Vulkan session verified the Stop command
and `Ctrl+W` tab closure with no child process or staged session left.

| Gate | Result |
| --- | --- |
| Root Cargo suite | 415 passed, 13 optional tests ignored by the default command. |
| Last Watch unit tests | Five passed, including FIFO orders, pause, abilities, navigation, deterministic victory, defeat and restart. |
| Actual Last Watch and Guardhouse subprocesses | Both integration tests passed with independently built game executables. |
| Pinned Zed adapter | 29 passed, including tactical input coordinate round trips and existing lifecycle regressions. |
| Native Windows graphics | Vulkan external-memory depth, explicit draw ranges and SDSL-V compute passed with validation enabled. Both D3D11 viewport tests passed. |

Local evidence is in `target/m8-gui`. It includes `01-vulkan-dungeon.png`,
`02-paused-queued-abilities.png`, `03-three-cooldowns-and-casualty.png`,
`04-vulkan-victory.png`, `07-vulkan-defeat.png`,
`11-vulkan-rebuilt-health.png` and `17-d3d11-resized.png`.
The build and test logs are under `target/m8-*.log`.

## Measured costs

| Measure | Result |
| --- | --- |
| Downloads | 60,752,057 bytes across three official free archives. |
| Used assets | 29 original files totaling 5,777,342 bytes, including 13 unique model/equipment meshes and nine named clips from four animation files. |
| Import and visual animation preparation | 2.58–2.94 s in instrumented debug launches. The atlas held 311 CPU-skinned pose samples with 32,791,696 vertex bytes and 14,072,808 index bytes uploaded once per backend session. Live animation selects cached mesh ranges. Its isolated update cost was unmeasured. |
| Draws and dynamic uploads | The sampled D3D11 patrol submitted 10 indexed draws for 52 instances and uploaded 4,784 instance bytes per changed draw. The corresponding Vulkan instance payload is 4,160 bytes by layout. Vulkan uses the same range grouping. |
| Tick cadence | 366 ticks across approximately 6.203 s in the Vulkan viewport, about 59 Hz against the 60 Hz target. Input-to-display latency and presentation FPS were unmeasured. |
| CPU and working set | A 6.05 s Vulkan patrol sample measured 90.1% of one logical core and 914 MiB for debug Zed. Last Watch used 0.77% of one core and 5.9 MiB. Zed used 6.8% of one core after closing the preview in a separate 5 s sample. |
| Build and startup | The full incremental editor build after the GPUI change took 3m 39s. The final shader rebuild took 26.27 s. The edited game published in 10.87 s in a fresh scratch Cargo target, and a cached project build took about 3 s. Independent game process launch and scene mount took 106.6 ms, with a 95.3 ms restart in the headless process test. First graphical launch adds art preparation and GPU setup. |

## M9 handoff

The native playthrough establishes the game boundary and the palette
asset path. The first graphics bottleneck to tackle is the 46.9 MB
prebaked pose atlas and its cold preparation stall. Keep the same
assets, gameplay snapshots and shaders-as-source workflow while
moving one character's joint palette to the Vulkan vertex shader.
Measure initialization, memory and motion quality against this slice,
then decide whether to extend the renderer change to the other actors.
