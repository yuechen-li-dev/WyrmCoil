# WyrmCoil M3a in Zed

## GPU ownership audit

The pinned Zed revision is `a246423a3b24a08e0d0c8574eab3a9de422747e0`.
On Windows, `gpui_platform` constructs `WindowsPlatform`, and its window uses
`DirectXRenderer`. The Windows platform owns the shared D3D11 device and
immediate context. The window renderer owns its DXGI swap chain, attachments,
viewport, and presentation. D3D11 records this work through the immediate
context. There is no wgpu queue or command encoder in this Windows draw path.
The pinned `gpui_wgpu` renderer owns those facilities on its supported
platforms. Its git wgpu 30 types differ from WyrmCoil's crates.io wgpu 24 types.

| Approach | Fit at this pin |
| --- | --- |
| Use the D3D11 renderer to render a small offscreen surface and sample it with GPUI's existing polychrome sprite pipeline | Keeps one device and uses GPUI's ordering, clipping, and presentation. Windows already has a scene surface batch with an empty compositor implementation. |
| Render directly into the swap chain in the surface batch | Also uses one device. A foreign draw would have to honor GPUI's depth ordering, content mask and blending in the draw itself. It bypasses the requested offscreen boundary. |
| Use the existing image atlas entry point | The public API takes CPU image bytes. A native target could be copied into an atlas slot with a GPUI extension, at the cost of a GPU copy and atlas-slot lifetime management. |

The existing public `paint_surface` method is Apple-only and takes a
CVPixelBuffer. Windows' atlas stores BGRA8-unorm polychrome textures, and the
sprite compositor applies scene clipping and alpha. The Windows renderer
recreates its device resources after device loss, and its resize path
recreates the swap-chain targets. A renderer-owned preview texture can use
the same BGRA8-unorm format and an opaque primitive over a transparent clear.
Its dimensions should come from snapped device pixels and its lifetime
should follow scene use.

## Shader audit

WyrmCoil's native canonical path validates SDSL-V, lowers it to VD-MIR,
and emits HLSL with concrete stage and entry metadata. The Oct fixtures
exercise DXC SPIR-V and structural validation. DXC targets shader-model-6
DXIL or Vulkan SPIR-V. The pinned Windows renderer uses DXBC and builds
its own shaders at `vs_4_1` and `ps_4_1`. Its existing Windows dependency
exposes D3DCompile, so the same generated HLSL can use those profiles to
stay compatible with GPUI's Direct3D feature-level 10.1 minimum. A
resource-free vertex-ID triangle needs no shader bindings or vertex layout.
The preview's snapshot and camera supply its position through the GPUI
surface bounds. [Microsoft DXC](https://github.com/microsoft/DirectXShaderCompiler) · [Shader profiles](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-models)

## Implementation

The preview builds the SDSL-V artifact once per session. The Windows scene
surface carries the emitted source and both entry names. The pinned D3D11
backend uses its existing D3DCompile API to build shader-model-4.1 bytecode
and create the vertex and pixel shaders. The DXC conformance test validates
the same entry points and HLSL through SPIR-V as a separate compiler check.

On first use and after a physical size change, the GPUI renderer binds a
transparent BGRA8 target, sets a device-pixel viewport, and issues `Draw(3, 0)`.
The vertex shader supplies the triangle from `SV_VertexID`. The normal GPUI
polychrome sprite pass then samples that same texture view with the scene's
ordering and clip mask. There is no atlas upload, GPU texture copy, or CPU
readback for this marker. The GPU triangle is static within its texture. Each
scene positions it using the existing player snapshot and camera. The editor
still presents through its existing swap chain and the preview still uses the
M2 simulation scheduler, input, grid, and guard.

The renderer caches the shader objects and sampled target until the surface
leaves the scene, its target dimensions change, or the device is lost. Empty
or fully clipped bounds produce no GPU scene surface. The renderer unbinds the
offscreen target before sampling it and clears the sampled binding after
composition. Pane resize recenters the marker, with a new target when its
snapped size changes. Minimization and focus still follow the M2 scheduler.

## Reproduction

Build from an x64 Visual Studio Developer PowerShell with the Windows SDK and
CMake available. Use the existing M0 build entry point.

```powershell
pwsh -File scripts/build-zed.ps1
cargo test --locked
Push-Location vendor/zed
cargo test --locked -p wyrmcoil_view --lib
Pop-Location
```

On Windows with a D3D11 adapter, run the offscreen resource test from
`vendor/zed`.

```powershell
cargo test --locked -p gpui_windows --lib gpu_viewport_reuses_and_resizes_its_target_then_drops_the_pass -- --ignored
```

For the external compiler check, set `WYRMCOIL_OCT_ROOT` to the pinned Oct
checkout and put `dxc`, `spirv-val`, and `spirv-dis` on `PATH`.

```powershell
cargo test --locked --test sdslv_oct_conformance -- --include-ignored
```

Launch `vendor/zed/target/debug/zed.exe` in a workspace, then run
`wyrmcoil: open preview` from Zed's command palette. A cyan triangle marks the
player. A process launched with `WYRMCOIL_GPUI_MARKER` set uses the M2 square
for comparison. The adapter uses that square on platforms without the Windows
GPU pass, and if SDSL-V compilation fails. Windows backend failures report a
diagnostic in Zed's log.

## Validation

The WyrmCoil baseline revision was `cd45c31dbcd5d7a62c66f884c8c67097aacbd213`
plus the M3a changes. Oct's conformance checkout was
`fa5b4993ca61f79b2ce18d09e9e3bbce88b9b9a0`.

| Check | Result |
| --- | --- |
| WyrmCoil regression suite | 401 passed, 8 ignored in the default run. The Oct/DXC checks ran separately. |
| Oct/DXC qualification | All 7 conformance tests passed with Vulkan SDK 1.4.350.0. The new test verifies both viewport entries through DXC and validates and disassembles the resulting SPIR-V. |
| Zed adapter suite | 19 passed, up from 18 in M2. The new test checks the compiled shader's entry names and D3D semantics. The suite also covers snapshot extraction, viewport placement, input, and scheduler lifecycle. |
| Headless Windows GPU test | Passed on the Direct3D 11 device. The test creates both shaders and the render target, submits a draw, checks that a repeated size reuses the texture, resizes it while reusing the shader, and verifies zero bounds release the cached pass. |
| Integrated Windows x64 build | The locked build script finished in 170.82 s from a warm dependency cache after the GPUI change. It produced `zed.exe` and `cli.exe` under `vendor/zed/target/debug/`. |
| Naming policy | Adapter passed. WyrmCoil reported the 8 existing findings. |
| Pin reproduction | Both patches passed `git apply --cached --check` together against an index from the unmodified Zed pin. Repeated `scripts/build-zed.ps1 -PrepareOnly` passed. |

The final shader-model-4.1 Windows x64 build rendered the cyan SDSL-V
triangle on the RTX 3070. D and Up moved the player to `(1, 1)`.
Maximize and restore repositioned the triangle correctly. Ctrl+W returned to
the editor, reopening rendered the triangle with the player back at
`(0, 0)`, and Alt+F4 exited with code 0. The runtime logged pipeline setup
in 2.33 ms and 2.06 ms on the two opens, with the 16 × 16 offscreen target
creation and draw submission taking 242 µs and 102 µs on the CPU. The Zed log
reported no viewport errors during that run.

The initial Windows smoke run used shader-model-5 DXBC on an RTX 3070.
The triangle was visibly distinct from the guard's square. D and Up moved
the player from `(0, 0)` to `(1, 1)` before the player stopped. The camera
and marker responded together. Escape froze the counters and ignored movement,
and clicking resumed. Maximize and restore repositioned the marker. Switching
to the editor and back preserved the session. Ctrl+P and Ctrl+Shift+P still
opened the file picker and command palette. Ctrl+S saved a scratch edit to
disk. Alt+Tab and minimization suspended ticking and preserved the player
position. Ctrl+W closed the tab, and reopening reset the player to `(0, 0)`.

That run advanced 307 simulation ticks and 307 snapshots in about 5.14 s
(59.8 Hz). Whole-process CPU use over five-second samples averaged 39.3% of
one logical core with the preview active, 0.3% minimized, and 8.1% after
closing the tab. These are spot samples. Presentation cadence, device loss,
and older Windows hardware were outside this validation.

Local logs are under `target/`, including `wyrmcoil-m3a-tests.log`,
`wyrmcoil-m3a-dxc.log`, `zed-m3a-tests.log`, `zed-m3a-gpu-tests.log`,
`zed-m3a-final-build.log`, and the isolated `zed-m3a-smoke/` directory.
The final run's runtime log is `zed-m3a-smoke/user-data/logs/Zed.log`
and its process result is `zed-m3a-smoke/qualification-exit.txt`.

| Artifact | SHA-256 |
| --- | --- |
| `zed.exe` | `44d5c4c7d7981d60e95e38a419761a9a2759527844aff6f9de0d0fe6ac83ec2b` |
| `cli.exe` | `63db277acce5e7edcc6ab85df9caf78a51611cfa2ea66827e8ffaa956115430` |

## Integration cost since M2

| Area | Change |
| --- | --- |
| WyrmCoil adapter | 46 Rust production lines added and 4 removed. 22 SDSL-V lines added. 8 adapter test lines added. |
| WyrmCoil engine and shader compiler | No production modifications. The standalone DXC conformance check adds 23 test lines. |
| Build script | 13 lines added and 9 removed to manage the two separate patches and embed the shader source. |
| Upstream Zed and GPUI | 319 production lines added and 5 removed across 5 files, plus 55 lines for the optional Windows GPU test. The M0 registration patch remains four handwritten lines. |
| Dependencies | No new direct dependencies or new lockfile changes. |

The GPUI platform trait exposes the backend capability. The window submits
the device-pixel bounds with the active clip, and the scene stores the source
and entry names. On Windows, the platform window opts in and the D3D11
renderer creates the offscreen resources and adds surfaces to its existing
sprite draw path. The `gpui_wgpu` backend has no changes.

## M3b handoff

Feed the existing WyrmCoil render extraction into a bounded draw packet with
one dynamic transform or vertex stream. Keep the snapshot and simulation
boundary in the preview. Use a pane-sized offscreen target when a scene pass
needs it and let GPUI retain device, clipping, and presentation ownership.
Qualify SDSL-V features against the backend's shader profile before adding
bindings. Carry forward the resize, visibility, and resource lifetime tests.
