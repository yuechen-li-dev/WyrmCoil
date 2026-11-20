# WyrmCoil M4 in Zed

## Audit and conventions

The M3b working tree on WyrmCoil baseline `15bae3c07e3a95831698c2751f4f74e388af753e`
is the starting point. Zed stays pinned at `a246423a3b24a08e0d0c8574eab3a9de422747e0`.
The preview owns simulation, the latest snapshot, and its SDSL-V artifact.
Its 2D camera projects snapshot positions into the pane and sends two
float4 values per instance. The D3D11 pass owns the input layout, shaders,
dynamic instance buffer, and pane-sized BGRA8 color target. GPUI owns the
device and samples the target as a polychrome sprite in its normal scene.
The backend retires the pass when its surface disappears. A pane resize
replaces the target while keeping the layout, shaders, and buffer.

Aurelian's `Camera3D.cs` uses a right-handed Y-up world with -Z forward at
zero yaw, radians, and a first-person eye raised from the feet. Its
`System.Numerics` row-vector matrices compose view then projection. The
Vulkan path flips projection Y, clears forward depth to its far value,
and compares with `Less`. `PrimitiveModels3D.cs` builds outward-facing
triangles, and the Vulkan pipeline declares counterclockwise fronts.
M4 adopts the world axes, angle units, and outward winding. It uses
column-vector transforms stored as explicit columns and evaluates `P * V * M`.
D3D clip Y points up. Depth zero is far and one is near, matching the
pinned backend's existing `Greater` depth state and zero clear. [Aurelian camera source](https://github.com/yuechen-li-dev/copeland/blob/e0f7d61bf90f8d0579b1fcb60f41e7c6bf1bb1a8/src/Aurelian/Aurelian.Runtime/Camera3D.cs)

WyrmCoil already uses Margaret's `Vec3` and `Point3` in its world and ray
code. The room reuses that vector math and adds only the matrix operations
needed to validate and project a perspective camera. A separate snapshot
type keeps the existing planar demo intact. The room uses a shared static
indexed cube. Its top face also supplies the floor plane. The D3D11 pass
needs a per-vertex input slot, indexed instancing, and a matching depth
attachment to support this path.

## Room and rendering

`wyrmcoil: open preview` opens the room. `wyrmcoil: open planar preview`
opens the M3b demo in its own tab. Both use the existing fixed-step clock,
input latch, focus handling, and Zed workspace lifecycle. The room routes
walk and look axes through a single Dunewyrm frame and an immediate act.
World movement uses 1/60 s steps with normalized diagonal speed and
bounded pitch. The guard patrols independently using an integer tick
phase. The preview receives an immutable copy of the eye, guard, floor,
and boxes after each simulation advance. `World::Snapshot` lets each world
declare its snapshot type. The planar demo still returns `RenderSnapshot`.
External `World` implementations now declare that associated type.

W/S walk, A/D strafe, and the arrow keys turn and look. Escape pauses and
clicking resumes. The default vertical field of view is 60 degrees with
0.10 and 70 world-unit clip distances. `RoomCamera` carries the FOV in
radians and rejects invalid or nonfinite projections. The aspect ratio
uses the same snapped device-pixel extent GPUI uses for the target.

The preview sends four model-view-projection columns and a color per object.
The SDSL-V vertex shader applies those columns to each local position. The
pixel shader outputs opaque color with simple face brightness. The single
mesh contains 24 position/brightness vertices and 36 16-bit indices. The
floor uses the six-index top face with zero Y scale. Three colored boxes,
three wall sections, and the moving guard share the full mesh. The renderer
uses two indexed instanced draws for the floor and boxes on an updated
frame. It uploads the 456 bytes of static geometry at mesh creation and
writes 640 bytes of instance data per room redraw.

The D3D11 viewport owns the immutable mesh buffers, reusable instance
buffer, shaders, input layout, depth state, and rasterizer state. It binds a
`D32_FLOAT` depth attachment with the same dimensions as the color target,
clears it to zero, and tests `Greater` with depth writes enabled. Backface
culling uses outward counterclockwise fronts. Only color/depth targets and
views are recreated on resize. GPUI samples the offscreen color view and
composites it in the existing workspace scene. Display uses the same device
and swapchain and has no live pixel readback. [D3D11 rasterizer conventions](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_rasterizer_desc)

## Reproduce

Use the Windows x64 prerequisites from [M0](zed-integration-m0.md).
The pinned script accepts upstream or the exact M3a/M3b patches and stages
the room source and shader in the Zed workspace.

```powershell
pwsh -File scripts/build-zed.ps1
cargo test --locked
Push-Location vendor/zed
cargo test --locked -p wyrmcoil_view --lib
cargo test --locked -p gpui_windows --lib gpu_viewport -- --ignored
Pop-Location
```

For external compiler validation, configure `WYRMCOIL_OCT_ROOT` and put
`dxc`, `spirv-val`, and `spirv-dis` on `PATH`.

```powershell
cargo test --locked --test sdslv_oct_conformance -- --include-ignored
```

Launch `vendor/zed/target/debug/zed.exe` and run `wyrmcoil: open preview`.
Set `ZED_LOG=info,gpui_windows::directx_renderer=trace` at launch for draw,
upload, and target creation logs. The room requires Windows D3D11 and
reports an unsupported backend or a source compile failure in the preview.

## Validation

| Check | Result |
| --- | --- |
| WyrmCoil | 406 passed, 9 ignored in the default run. Five added tests cover conventions, projection and clipping, aspect, invalid inputs, composition, mesh winding, and deterministic control and patrol. |
| Adapter | 23 passed, up from 21. Added tests cover shader entry/layout, shared mesh and snapshot instances, distinct walk/look controls, timing reset, and fresh state. |
| DXC and Oct | All 8 conformance tests passed with SPIR-V validation of both room entries and their stage interfaces. The existing SDSL-V compiler needed no M4 changes. |
| D3D11 tests | Both passed. Pixel readback showed the nearer of two overlapping boxes wins in either submission order. A reversed face was culled. Resizing rebuilt the D32 attachment while reusing the static and dynamic buffers. Zero-size retirement and the planar regression also passed. |
| Pin and formatter | Clean pin, M3a upgrade, M3b upgrade, and repeated preparation passed. The adapter passed `wyrmfmt`. The root check reported the same eight existing findings. |
| Integrated Windows x64 build | Passed in 174.79 s with a warm dependency cache. Both the editor and CLI built with `--locked`. |

### Windows desktop smoke

The RTX 3070 displayed the floor, three walls, three colored boxes, and
orange moving guard in the ordinary preview tab. W moved the eye from
Z 8.80 to 7.52 and enlarged the near cyan box. S moved it back to 8.16.
A and D shifted the eye laterally. Right and Up changed yaw to 0.35 and
pitch to 0.07 radians with the eye fixed. The guard moved and passed
behind a nearer box while the eye stood still.

Escape held both the simulation and the camera fixed through another W
press. Clicking resumed. Maximize and restore rebuilt the color and depth
targets, preserving perspective and static mesh buffers. During a minimized
interval of over six seconds, the displayed tick count advanced only 13
ticks across the minimize and restore transitions. Ctrl+P opened Zed's
file picker from game focus, and Ctrl+S wrote the scratch edit to disk.
The original 2D tab showed both GPU triangles and Up moved its player.
Returning to the 3D tab preserved its camera. Ctrl+W closed the room and
reopening reset the camera to `(0, 1.65, 8.80)`. Alt+F4 exited with code 0.

| Measurement | Result |
| --- | --- |
| Simulation and snapshots | 366 of each in 6.110 s, approximately 59.9 Hz. Presentation FPS was outside the measurement. |
| CPU spot samples | 45.1% of one logical core while active, 0.3% minimized, and 6.5% after closing. Each sample ran about six seconds with renderer trace logging enabled. |
| Draws and uploads | The trace recorded 7,176 room redraws with two calls and one 640-byte instance upload each, excluding GPUI composition. The planar check recorded 321 redraws with one call and 64 bytes each. Each room mesh upload was 384 vertex bytes and 72 index bytes. |
| Resource reuse | The trace recorded three room pipeline/mesh/buffer sets across the initial open, return after tab switch, and fresh reopen. Five depth-enabled targets included two target-only resizes between 1536 × 960 and 2560 × 1257. Initial shader, layout, states and mesh setup took 3.44 ms on the CPU. |
| Diagnostics and coverage | No viewport or DXGI errors in the smoke trace. The host had one active monitor. Mixed-DPI monitor transitions and device-loss recovery remain unqualified. |

Evidence is under `target/` in `zed-m4-final-build.log`, `zed-m4-tests.log`,
`zed-m4-gpu-tests.log`, `wyrmcoil-m4-tests.log`, `wyrmcoil-m4-dxc.log`, and
`zed-m4-smoke/`. The smoke directory holds the GPU trace, CPU samples,
saved scratch file, and recorded exit code. The Oct fixture checkout was
`fa5b4993ca61f79b2ce18d09e9e3bbce88b9b9a0`.

| Artifact under `vendor/zed/target/debug` | SHA-256 |
| --- | --- |
| `zed.exe` | `817f844f5a463bbabe7a0a15d763b84bb1f591125091334121880fb968dde33c` |
| `cli.exe` | `7d38e59f0fa07e0a88967bf25fdc256aeb5350f2dbe4e6b72782c7888d1f00f6` |

## Integration cost since M3b

| Area | Lines added / removed |
| --- | ---: |
| Existing preview adapter production | 200 / 99 |
| New room adapter production / tests | 144 / 0 production, 56 / 0 tests |
| Room demo production / tests | 311 / 0 production, 174 / 0 tests |
| Generic engine snapshot boundary | 3 / 3 |
| SDSL-V room source / compiler changes | 35 shader lines, zero compiler changes |
| Build script | 12 / 5 |
| GPUI scene and window | 52 / 5 |
| GPUI Windows renderer including the GPU tests | 324 / 46 |

The incremental Zed diff touches three files and uses the existing five
patched paths. The room adds no package dependencies or lockfile changes.
The initial viewport makes 13 GPU object creation requests including the
shader pair, layout, three buffers, two state objects, two textures, and
three views. A resize replaces the five size-dependent color/depth objects.
The planar path retains one instanced draw and 64-byte uploads.

## M5 handoff

Use one small declarative room asset for the floor, walls, boxes, and guard
spawn. Validate its sizes and spawn coordinates in a CPU test, then feed
it to the same world and indexed pass. This gives scene authoring its first
real input without changing GPU ownership or introducing a scene graph.
