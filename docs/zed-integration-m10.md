# M10. Vulkan vertex skinning

The Last Watch's Alda uses a Vulkan vertex shader to deform her imported
KayKit Knight mesh. Her sword and shield follow the original glTF
attachment nodes. The other characters keep their M9 cached poses.

## Run

Build the pinned Windows editor with `scripts/build-zed.ps1`. Publish
The Last Watch with `examples/games/last-watch/build.ps1`, open that game
directory in the integrated editor, and call `wyrmcoil: run game`.
The preview reports the backend and whether Alda uses Vulkan vertex
skinning. Use `WYRMCOIL_SKINNING=cpu` when launching Zed to compare the
M9 pose atlas on the same Vulkan backend. Use
`WYRMCOIL_GRAPHICS_BACKEND=d3d11` for the existing D3D11 CPU path.
Use `WYRMCOIL_GPU_TIMING=1` to record optional Vulkan timestamp queries.

## What was ported

Copeland revision `e0f7d61bf90f8d0579b1fcb60f41e7c6bf1bb1a8` supplies
`src/Aurelian/Aurelian.Shaders/Assets/Deformation/HumanoidSkinning.v.ts`.
Its compute entry reads joint indices and weights from the original
vertices, fetches an ordered palette, blends the influences and applies
the world transform after deformation. Aurelian's anatomical rig uses
six influences, dual quaternions, correction vectors and a millimetre
to metre conversion. The SDSL-V port keeps the indexed blend and
world-after-deformation ordering. KayKit's glTF contract supplies four
linear matrix influences and already uses the viewport's Y-up units.
The Vulkan vertex shader is the first consumer of deformed positions.

## CPU reference and Vulkan boundary

The importer resolves glTF joint indices in each skin's declared order,
with inverse binds in the same order. Its animation evaluator samples
translation, rotation and scale, including LINEAR and STEP tracks.
Parent world transforms multiply local transforms. Each palette entry
is the joint's world transform multiplied by its inverse bind matrix.
With column vectors, the CPU reference computes:

```text
P[j] = WorldJoint[j] * InverseBind[j]
deformed = sum(weight[i] * (P[joint[i]] * vec4(bind_xyz, 1)).xyz)
clip = Projection * View * Model * vec4(deformed, 1)
```

Rigid primitives and equipment use a single unit-weight slot with their
animated node transform. The Vulkan path retains the mesh's original
joint indices relative to each skin and adds the skin's palette base
for the assembled draw. It preserves the reference's treatment of
positive and zero individual weights. Import and Vulkan mesh admission
reject all-zero or malformed weights, out-of-range joints, and a
palette larger than the device's storage-buffer range.

M9 evaluated the same matrices for each cached 10 Hz pose. It deformed
the entire vertex set, appended new ranges and selected a range from
`ActorMotion` and `MotionTicks`. M10 uses the same clip selection and
fixed tick origin, then evaluates the current palette directly at the
simulation tick. Idle and walk wrap. One-shots clamp to the end.
Pause freezes the tick, and restart selects the initial pose. Vulkan
gets one immutable bind mesh for Alda and her equipment. The material
palette already bakes lighting into vertex colors, so the viewport
has no runtime normals to transform.

The 48-byte vertex contains a float4 position/color, uint4 joints and
float4 weights. The palette is a vertex-visible storage buffer at set
0, binding 0. Each matrix occupies four 16-byte columns. The Vulkan
room reuses the existing camera/instance data, depth target, graphics
queue, command buffer, fence and shared-image compositor. An unchanged
palette skips upload. The existing fence must retire before palette
storage can be rewritten. A producer revision tells the compositor
when only the palette or Alda's transform changed. Vulkan creation or
submission failures report the reason and rebuild the complete CPU
art on D3D11 at the next preview update. Gameplay state stays in the
existing process.

The compiler now reuses its storage binding lowering for standalone
graphics shaders with inline `readonly array<float4>` bindings.
Canonical VD-MIR also resolves calls to shader helpers consistently
with their emitted names. DXC and SPIRV-Tools validate the same
SDSL-V source the viewport uses.

## Reproduce the GPU reference check

```powershell
$env:PATH = 'C:\VulkanSDK\1.4.350.0\Bin;' + $env:PATH
$env:WYRMCOIL_DXC = 'C:\VulkanSDK\1.4.350.0\Bin\dxc.exe'
$env:VK_INSTANCE_LAYERS = 'VK_LAYER_KHRONOS_validation'
cargo test --locked --offline --lib kaykit_skin_executes -- --ignored --nocapture
cargo test --locked --offline --test sdslv_oct_conformance tactical_skin_validates -- --ignored
```

The test replaces only the vertex shader's raster position and diagnostic
output. The live shader's palette fetch, weighted blend and model
transform write world-space coordinates into a floating-point target.
Test-only readback compares each position to `PaletteMesh::AppendPose`
at the same time and model transform. It covers every vertex from the
Knight and both attachments, bind pose, idle, walking, normal attack,
Shield Bash, hit and death at three times per clip. The tolerance is
0.00002 world units per component. Single- and four-joint fixtures
check sparse weights, identity and translations. Nonuniform instance
scale, rotation and translation are exercised for the imported mesh.
There is no runtime pixel or vertex readback.

## Qualification

Windows x64 with an NVIDIA GeForce RTX 3070, Zed pinned at
`a246423a3b24a08e0d0c8574eab3a9de422747e0` and WyrmCoil based on
`e4eed7e915e03d39a0602ae6a0f5ca83007e639f`. The Vulkan validation layer
was enabled for the hardware and live GPU tests. The recorded
qualification had no Vulkan validation errors.

| Check | Result |
| --- | --- |
| Vulkan position readback | 19 KayKit poses × 5,969 vertices. Maximum component error 0.000000477 world units. The single- and four-joint fixtures also passed. |
| Windows integrated build | The final incremental build took 27.97 s for Zed and 3.22 s for its companion CLI in the dev profile. |
| Engine and adapter tests | 418 root tests, 29 adapter tests and eight optional shader conformance tests passed. |
| Real graphics tests | Two Vulkan tests and two D3D11 tests passed, including the existing room depth and interop regression. |
| Game regressions | Guardhouse and Last Watch unit tests, real process tests and deterministic CLI playtests passed. |
| Naming policy | The eight existing fixture findings remained. |

In the live GPU preview, Alda idled and walked to the targeted guard.
Shield Bash reduced the guard from 94 to 73 HP, started its cooldown
and reported an interrupt. A queued basic attack reduced the guard
to 5 HP while Alda took damage. With her order cleared, Alda reached
zero HP and held the death pose with both attachments still visible.
The same tick and pose persisted while paused. Restart restored her
144 HP, initial pose and empty queue. Closing and reopening the preview
retired and recreated the Vulkan session. The Windows test observed
Guardhouse on Vulkan and a completed movement order on explicit
D3D11 with CPU-skinned Alda. A deliberately missing DXC exercised the
automatic D3D11 fallback and recovered the complete CPU art. A physical
device loss was not induced.

Screenshots and timing evidence are under `target/m10-gui`. The
hardware readback samples qualify the brief hit deformation directly.
The GUI captures show combat damage and aftermath, with no isolated
hit-reaction frame. The GUI also returned to the Rust source tab and
accepted the save shortcut after closing the preview.

## Performance

The matched debug samples used the same build, assets, room and GPU
with warm OS file caches and fresh art preparation. They are individual
observations. The first Vulkan GPU-skin open took 3.37 s for art
preparation during concurrent qualification work. The M9 report's
2.68 s cold sample is historical and had different cache and load
conditions.

| Measurement | CPU cached poses on Vulkan | Alda skinned on Vulkan |
| --- | ---: | ---: |
| Main source reads | 1.95 ms | 1.98 ms |
| glTF parse and structural validation | 14.30 ms | 14.35 ms |
| Buffer reads and validation | 2.12 ms | 2.07 ms |
| Image loading and texture decode | 506.71 ms | 510.88 ms |
| Geometry extraction | 26.14 ms | 26.31 ms |
| Skin loading | 0.18 ms | 0.16 ms |
| Clip track decode | 2.61 ms | 2.62 ms |
| CPU pose matrix evaluation during prep | 52.20 ms | 41.05 ms |
| CPU vertex deformation and preparation | 1,741.52 ms | 1,438.64 ms |
| Pose atlas append | 23.83 ms | 19.73 ms |
| Total art prep | 2.39 s | 2.07 s |
| Cached poses | 311 | 247 |
| Static vertex and index buffer payload | 46,864,504 bytes | 38,633,924 bytes |
| Dynamic joint palette | – | 1,600 bytes per changed submitted pose |
| Mean Vulkan room GPU time | 75.8 µs | 80.6 µs |
| Mean Alda GPU skin draw time, including raster | – | 4.9 µs |
| Simulation cadence | ~60 ticks/s | ~60 ticks/s |
| Zed CPU, six-second patrol, percent of one logical core | 96.5% | 98.9% |
| Zed working set at end of that sample | 490 MiB | 889 MiB |
| Zed private bytes at end of that sample | 533 MiB | 543 MiB |

The M9 skinning timer includes weighted transform and rigid attachment
work, intermediate vertex allocation, and filling that allocation.
Matrix composition and the atlas copy are timed separately. Moving
Alda's 64 samples off the atlas removed about 303 ms from CPU skinning
in the matched sample, with CPU pose evaluation still present at runtime.
The GPU patrol produced 1,252 current-palette evaluations averaging
202 µs and 1,243 uploads averaging 7.7 µs, inclusive of the host upload
and previous-palette update. The GPU timestamps bracket the room and
the indexed skin draw. They include their raster work and exclude final
desktop presentation. Presentation FPS was unmeasured.

Alda's GPU mesh uses 286,512 vertex bytes and 38,172 index bytes.
Its 25 palette entries comprise 23 joints and two attachments.
Compared with M9, the total static vertex/index buffer size fell by
8,230,580 bytes (17.6%). This is the requested Vulkan buffer payload.
Driver allocation padding and physical VRAM residency were unmeasured.
The GPU process's working set varied markedly by run. A separate fresh
GPU sample was about 509 MiB and 533 MiB private. The sampled whole-Zed
numbers do not establish a process-memory or CPU-usage win.

The final fresh GPU game mount took 2.23 s, including 2.10 s of art
preparation and excluding the compositor's first submit. An earlier
foreground GPU restart had already drawn a reset room at the first
capture, 211 ms after invocation including the capture overhead.
The final GPU restart logged an art cache hit after a window-focus
interruption delayed the observed mount to 16.02 s. The recovered
D3D11 CPU art restarted in 113.17 ms with a cache hit. These were
individual warm restarts with no new pose bake or Vulkan mesh upload.

## Change cost and M11 handoff

The renderer adds one Vulkan vertex/index pair, one reusable palette
buffer, one vertex-visible descriptor, and one indexed draw for Alda.
M10 adds 1,175 production lines, including 488 in the Vulkan backend
and timing instrumentation, 183 in the compiler, 112 in asset
preparation, and 70 in SDSL-V. Qualification adds 501 test lines.
The main GPUI overlay is 17 added and five removed lines across two
files. It changes external draw invalidation to track the producer's
revision. D3D11's CPU render path, the Zed app, gameplay and the game
wire format have no changes. There are no new dependencies.

Qualify a second distinct rig with the same mesh/palette contract before
expanding beyond Alda. Then use the live measurements to compare
removing more cached poses with their per-character palette evaluation
and upload costs. Keep the current CPU oracles and the D3D11 fallback.
