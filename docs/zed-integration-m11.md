# M11. Multi-character Vulkan skinning

## M10 audit

The Last Watch supplies six animated actors and five equipped model
definitions. All five assets have a 23-joint skin with the same named
parent topology under `Rig_Medium`. Every model uses a distinct joint
order, rest TRS and inverse-bind payload. Keep these model-specific
bindings. The guards use the same Ranger mesh, skeleton and equipment
configuration, with independent actor clocks and palette ranges.

| Actor | GLB skin 0 | Body primitives | Body vertices | Slot 2 | Inverse bind SHA-256 prefix | Equipped sockets | M10 Vulkan |
| --- | --- | ---: | ---: | --- | --- | --- | --- |
| Alda | Knight | 9 | 5,328 | spine | `2592fc8c57c7` | Sword right, shield left | GPU, 64 poses in CPU fallback |
| Merrit | Rogue | 7 | 6,422 | upperleg.l | `752c971fb14d` | Dagger each hand | CPU, 69 poses |
| Vey | Mage | 8 | 5,421 | upperleg.r | `0c93e0cb9802` | Staff right | CPU, 74 poses |
| Gate Guard | Ranger | 8 | 7,734 | upperleg.l | `47ea921c8f4f` | Sword right | CPU, shared 52-pose Ranger atlas |
| Hall Guard | Ranger | 8 | 7,734 | upperleg.l | `47ea921c8f4f` | Sword right | CPU, shared 52-pose Ranger atlas |
| Captain | Barbarian | 7 | 6,050 | spine | `c0eebc3638ac` | Sword right | CPU, 52 poses |

All actors use the authored Idle_A, Walking_A, Hit_A and Death_A.
The melee basic attack is Melee_1H_Attack_Slice_Diagonal. Alda uses
Melee_Block_Attack for Shield Bash, Merrit uses Melee_Dualwield_Attack_Stab
for Backstab, and Vey uses Ranged_Magic_Shoot and Ranged_Magic_Raise.
Enemies reuse their basic attack clip in the ability slot. External
animation tracks bind by node name, with per-model rest state and
inverse binds. The actual socket names are `handslot.r` and `handslot.l`.

The measured M10 run used 247 CPU animation poses for the remaining
characters, 38.63 MB of static GPU geometry payload including Alda,
and a 1,600-byte palette. The repeated M10 run at 14:53:44 recorded
1.92 ms source I/O, 14.17 ms parsing, 1.97 ms buffer loading and
validation, 510.67 ms images, 26.22 ms mesh extraction, 0.18 ms
skin/inverse-bind loading, and 2.66 ms clip-track extraction. Model and
palette preparation took 5.33 ms, pose evaluation 41.49 ms,
CPU deformation 1,461.90 ms, and atlas appends 18.89 ms. Fresh art
preparation totaled 2.10 s. GPU session creation took 340.54 ms,
including 70.75 ms for the room shader artifacts and 4.90 ms for the
regular vertex/index upload. This is an existing measured run from
`target/m10-gui/render-evidence.log`. The five embedded character
texture payloads have distinct hashes. Pose generation is the first
optimization target.

## Implementation

Default Vulkan skinning uses all six actors. Five equipped bind meshes
share one vertex buffer and one index buffer. Mesh identity includes
the equipment configuration. The two Rangers share one mesh range and
one set of bound clips. They use distinct transforms and joint-palette
ranges. Other models share decoded external animation data and bind
the track names to their own nodes at game mount. This preserves each
GLB's skin ordering and inverse binds without retargeting.

The adapter owns animation scratch and one CPU palette array. Each actor
records its last evaluated clip and tick. Sampling fills the actor's
assigned slice and reuses the same scratch arrays across the roster.
The skin layout is column-major glTF world transform × inverse bind for
body joints, with a rigid animated node matrix for each equipped item.
Each matrix is four float4 columns in a 64-byte storage-buffer slot.
The shader adds the instance's unsigned base slot to each vertex joint
index. It applies the existing four weights, model-to-clip transform
and vertex colors through SDSL-V → VdMir → HLSL → DXC SPIR-V. The M10
color pipeline has no normal input or normal output.

One dedicated vertex buffer stores 88-byte skin instance records. Each
instance references a bounded, nonoverlapping range verified against
its mesh's joint count and the device storage-buffer limit. Five
indexed draw batches cover the roster, with one instanced draw for the
Rangers. In the initial room, source counts give five regular draws
(floor, walls, pillars, relic and selection marker) plus five skin
draws. An unpaused frame can add or remove markers with game state.

The existing D3D11/Vulkan keyed-mutex handoff and per-room fence remain
the scheduling authority. Each room permits one in-flight submission.
When its fence is busy the submission returns before touching any
buffer. The next available submission copies the current instance
records and at most one contiguous dirty span of the persistent palette.
Identical palettes skip the mapped write. Resize retains the four skin
buffers, descriptor and pipeline, and only replaces viewport images.
Session retirement follows the existing room shutdown fence. Live
rendering transfers no pixels through host memory.

Set `WYRMCOIL_SKINNING=cpu` for Vulkan with the original CPU atlas.
`alda` and `party` select the one- and three-actor comparison modes.
Explicit D3D11 uses CPU skinning for the entire roster. Bind errors log
the actor and leave that actor on the CPU path. Vulkan device and
compositor errors retain M10's automatic D3D11 fallback. The preview
reports the active backend and GPU/CPU counts.

## Windows qualification

Windows x64, NVIDIA GeForce RTX 3070 (driver 610.88), Vulkan SDK
1.4.350.0 and Zed pin `a246423a3b24a08e0d0c8574eab3a9de422747e0`.
M11 was built from the working tree on M10 commit `305b246`.
`scripts/build-zed.ps1` built the incremental pinned debug application
in 96 s. The initial Vulkan status and screenshots show the matched
DXGI adapter in use and **6 GPU / 0 CPU**.

In the GUI, the party queued all three abilities while frozen at tick
zero, then moved and attacked the Gate Guard after resuming. Frost Bolt
reported a slow, Shield Bash reported an interrupt, and all three
ability cooldowns were active. The guard died and the enemy attacks
later killed two party members. Screenshots show moving body poses and
equipment, frozen combat state, death poses and a reset encounter on
restart. Closing the preview retired its Vulkan session. Reopening
showed six GPU actors at the initial pose. The explicit Vulkan CPU
mode patrolled correctly. D3D11 rendered all six CPU-skinned actors and
finished a queued move. Guardhouse opened on D3D11. Returning to its
source tab, saving a scratch comment, undoing and saving again worked.
All dedicated test windows and game children exited. The screenshot
set and filtered log are in `target/m11-gui`. The GUI run covered
combat and restart. The deterministic CLI separately covered complete
victory and defeat. Brief hit reactions were verified by the numerical
probe rather than isolated in GUI captures.

With the Khronos layer requested, the GPU test and desktop logs
contained no validation errors. The real Vulkan test submitted the
SDSL-V vertex-skinning shader and read back positions through a
test-only float target. Each model used its equipped bind mesh and a
nonuniform world transform, with a nonzero palette base. The CPU atlas
reference and shader used identical bind, idle, walk, attack, ability,
hit and death samples. Melee guards reuse their attack for the ability
slot. Target tolerance was 0.00002 world units per component.

| Skin | Poses | Equipped vertices per pose | Max component error | Mean component error |
| --- | ---: | ---: | ---: | ---: |
| Knight | 19 | 5,969 | 4.77e-7 | 2.183e-9 |
| Rogue | 19 | 6,836 | 4.77e-7 | 2.098e-9 |
| Mage | 19 | 5,919 | 7.15e-7 | 1.884e-9 |
| Ranger | 19 | 8,092 | 4.77e-7 | 1.908e-9 |
| Barbarian | 19 | 6,408 | 4.77e-7 | 2.191e-9 |

The synthetic Vulkan coverage draws 1, 3, 6 and 12 copies of one mesh
with independent palettes in a single batch. It checks one- and
four-joint blending, repeated poses, overlapping/out-of-range palette
slots, invalid joint data, storage capacity and a 64-byte partial
update. A second hardware test exercised Vulkan/DXGI interop, room
resizing, depth and compute. The production path retains a single
in-flight slot, so no second writable palette buffer is needed.

| Regression suite | Result |
| --- | --- |
| Root Cargo tests | 418 passed in the default suite |
| Oct/DXC shader conformance | 8 opt-in tests passed |
| Vulkan hardware and D3D11 viewport tests | 2 Vulkan and 2 D3D11 passed |
| Pinned Zed adapter | 29 passed |
| Last Watch and Guardhouse Cargo tests | 5 and 2 passed |
| Real game-process tests | 2 passed |
| CLI playtests with replay | Guardhouse 9 assertions, Last Watch 14 |
| Naming policy | Eight existing findings, none in changed files |

To reproduce on Windows from the repository root, put the SDK's
`Bin` directory on `PATH` and use the pinned Zed build script. Launch
the resulting Zed executable with the Last Watch project open, then
run `wyrmcoil: run game`.

```powershell
$env:WYRMCOIL_DXC = 'C:/VulkanSDK/1.4.350.0/Bin/dxc.exe'
$env:VK_INSTANCE_LAYERS = 'VK_LAYER_KHRONOS_validation'
$env:WYRMCOIL_GPU_TIMING = '1'
./scripts/build-zed.ps1
./examples/games/last-watch/build.ps1
cargo test --lib Engine::render::backend::vulkan::tests -- --ignored --nocapture
cargo run --bin wyrmcoil -- playtest --project examples/games/last-watch --scenario last-watch --output target/m11-playtest/last-watch
```

## Performance

The matched debug comparison uses fresh art preparation with warm OS
file caches, the same KayKit assets and the same Zed build. The
one-GPU M11 control reproduced M10's 2.07 s. The CPU-only M11 control
reproduced the M9-path debug result of 2.39 s. M9 separately reported a
2.68 s cold art run. Values below are individual session measurements.
Static bytes count requested Vulkan vertex/index buffer payloads.

| Metric | CPU only | 1 GPU | 3 GPU | 6 GPU |
| --- | ---: | ---: | ---: | ---: |
| Source I/O | 2.08 ms | 2.04 ms | 1.83 ms | 1.93 ms |
| glTF parsing and structural checks | 13.97 ms | 14.11 ms | 13.86 ms | 14.11 ms |
| Buffer load and validation | 1.97 ms | 1.96 ms | 1.94 ms | 1.97 ms |
| Image load and decode | 505.14 ms | 505.34 ms | 504.42 ms | 508.69 ms |
| Mesh extraction | 26.41 ms | 26.35 ms | 26.47 ms | 26.35 ms |
| Skin and inverse-bind decode | 0.17 ms | 0.17 ms | 0.18 ms | 0.16 ms |
| Track decode | 2.60 ms | 2.67 ms | 2.56 ms | 2.65 ms |
| Model/skin preparation and clip bindings | 5.28 ms | 5.90 ms | 7.40 ms | 8.69 ms |
| Prep pose evaluation | 49.18 ms | 39.03 ms | 16.81 ms | 0.008 ms |
| Geometry deformation/preparation | 1,748.24 ms | 1,442.96 ms | 708.75 ms | 1.47 ms |
| Atlas append | 23.86 ms | 19.71 ms | 7.88 ms | 0.044 ms |
| Total fresh art prep | 2.39 s | 2.07 s | 1.30 s | 0.57 s |
| Fresh game mount, before first submit | 2.49 s | 2.17 s | 1.40 s | 0.85 s |
| Initial Vulkan session creation | 260 ms | 340 ms | 328 ms | 201 ms |
| Vulkan static mesh allocation and upload | 3.13 ms | 4.67 ms | 1.77 ms | 0.29 ms |
| Skin pass prep and upload | — | 1.87 ms | 2.70 ms | 3.96 ms |
| Cached character poses | 311 | 247 | 104 | 0 |
| Static vertex/index bytes | 46,864,504 | 38,633,924 | 18,352,172 | 1,897,534 |
| Palette storage bytes | — | 1,600 | 4,736 | 9,344 |
| Skin instance bytes per submitted frame | — | 88 | 264 | 528 |
| Indexed skin draw batches per frame | — | 1 | 3 | 5 |

Full GPU skinning reduced static geometry by 36,736,390 bytes
(95.1%) versus M10 and 44,966,970 bytes (96.0%) versus the CPU path.
The full scene binds 146 matrices, including eight equipment slots.
The regular viewport still uploads game markers and props in its own
instance buffer. The initial room has 3,680 regular instance bytes,
528 skin instance bytes and up to 9,344 changing palette bytes, for
at most 13,552 bytes in that frame. Four persistent skin VkBuffers
each have a backing allocation. Other skin resources per session are
one descriptor pool, layout and set, one pipeline layout and one
pipeline. Shader modules are transient during pipeline initialization.
Capacity is set at mount. Changes in which mounted actors are visible
reuse the same buffers.

| Live hardware observations | CPU only | 1 GPU | 3 GPU | 6 GPU |
| --- | ---: | ---: | ---: | ---: |
| CPU evaluation per sampled actor | — | 136 µs | 116 µs | ≈122 µs |
| Mean mapped palette write including bookkeeping | — | 4.6 µs | 4.7 µs | 5.4 µs |
| Mean bytes per palette write | — | 1,538 | 4,674 | 9,044 |
| Palette writes / skin submissions | — | 735 / 735 | 902 / 902 | 2,002 / 2,006 |
| Mean GPU skin batch scope | — | 13 µs | 45 µs | 74 µs |
| Mean whole room GPU scope | 60 µs | 73 µs | 97 µs | 128 µs |

The Vulkan timestamps include drawing and raster work. The one- and
three-actor observations were patrol samples. The six-actor scope
included combat and pauses. Submission intervals also include editor
focus and pause changes. The simulation still uses the 60 Hz fixed
clock. Presentation FPS was unmeasured.

For a controlled CPU-side scaling comparison, the same equipped
Knight mesh and walking clip were sampled independently for each actor
over 360 updates. The mapped-copy timer measured real Vulkan storage
uploads without GPU rendering. The independent 1–12 actor draw probe
qualified shared-mesh addressing separately.

| Identical mesh instances | Palette capacity | Mean evaluation per update | Mean mapped copy per update | Bytes transferred over 360 updates |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1,600 B | 145 µs | 0.91 µs | 554,816 |
| 3 | 4,800 B | 436 µs | 1.15 µs | 1,706,816 |
| 6 | 9,600 B | 874 µs | 1.39 µs | 3,434,816 |
| 12 | 19,200 B | 1.73 ms | 1.98 µs | 6,890,816 |

The first full-GPU art run took 819 ms with 199 ms spent in source and
buffer I/O. The matched warm-file-cache run took 572 ms. The in-preview
warm restart took 103 ms with an art cache hit and no mesh reupload.
A separate three-character run temporarily stopped responding during
an 8.68 s Vulkan session initialization. It recovered and ran normally.
The logged shader and buffer stages accounted for less than 80 ms of
that outlier, so the delay remains unattributed.

Six-second Zed process samples showed 103% of one logical core in
full-GPU combat, 105% on Vulkan CPU patrol, and 6.8% after closing the
full-GPU preview. The full-GPU sample showed 890 MiB working set and
359 MiB private bytes versus 491 MiB and 513 MiB for CPU patrol.
The sessions differ in activity and working-set history. A controlled
whole-editor CPU or resident-memory comparison remains open. GPU
allocation padding and VRAM residency were unmeasured.

## Change cost and M12 handoff

Compared with M10, the production diff adds 578 and removes 241 lines.
The adapter Rust diff is +257/−127, Vulkan +193/−71, shared asset and
pose evaluation +121/−37, and the shader +7/−6. Existing hardware
and shader tests add 256 and remove 126 lines. The shader compiler,
source asset decoder, gameplay, wire format, upstream Zed, GPUI and
D3D11 have zero changes. There are no new dependencies. All six
default Vulkan actors migrated. The explicit CPU paths still render
all six through the original atlas.

M12 should measure texture decoding in release builds and attribute
the remaining Vulkan initialization outlier. Image loading and decode
account for 509 ms of the 572 ms matched art run. Profile the shared
decoder and its color-sampling callers before changing cache behavior.
In active gameplay, CPU pose evaluation remains approximately linear
in actor count. Keep that cost distinct from cold art preparation when
choosing the next optimization.
