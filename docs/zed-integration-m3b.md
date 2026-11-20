# WyrmCoil M3b in Zed

## M3a audit

Baseline WyrmCoil is `15bae3c07e3a95831698c2751f4f74e388af753e` and Zed is pinned at
`a246423a3b24a08e0d0c8574eab3a9de422747e0`. The preview compiles
`integrations/zed/wyrmcoil_view/src/viewport.sdslv` through the canonical
parser, validation, VdMir, and HLSL emitter when it creates a session. Its
artifact supplies one vertex entry and one pixel entry. A Windows scene
surface carries that source and those entries into GPUI's D3D11 renderer.
The renderer uses D3DCompile with `vs_4_1` and `ps_4_1`, the same profiles
used by GPUI's own shaders on this pin.

M3a owns no vertex buffer or input layout. The vertex shader uses
`SV_VertexID` for three local positions. The renderer retains both shader
objects and a sampled BGRA8 texture per scene surface. The render-target
view lasts for the initial offscreen draw, and the shader-resource view
retains the texture until the surface disappears or the device is lost.
The player position changes through the sprite compositor's surface bounds.
The guard is a GPUI quad. The window submits its normal scene and clip mask,
and the D3D11 compositor samples the target directly with the existing
polychrome sprite pipeline. The tab owns the session and shader source.
Removing its surface retires the renderer's resources on the next draw.

`RenderSnapshot` already carries the exact visible and alive entries with
entity IDs, positions, and sprite IDs. `ExtractSpriteVertices` drops entity
identity and `BuildVisiblePrimitiveDemoBatch` expands a world-scale quad
into six vertices per item. The upload and command planners consume that
expanded vertex layout. The preview needs entity identity for color and a
screen-space marker size. Reusing `RenderSnapshot` and the existing preview
projection avoids expanding vertices or creating another render plan.

M3b uses a pane-sized transparent surface, a shared procedural triangle,
and one compact stream of projected transforms and colors. D3D11 can draw
the entire ordered stream with `DrawInstanced`. The CPU projects the world
through the existing camera, and the shader applies the per-instance
transform. A distant guard keeps the existing muted edge indicator. An
SDK FXC probe against `vs_4_1` rejected `[[vk::location(0)]]` with X3000.
These annotations need to be emitted only for SPIR-V so the same HLSL can
carry the vertex inputs for D3D11.

## Implementation

The preview sorts the snapshot copy by entity ID, with sprite ID as a
secondary key. Higher IDs paint later. Both demo entities use sprite 0 and
share the same triangle. The engine's entity IDs select cyan for the player
and orange for the guard. Snapshot extraction still determines visibility
and the status line still shows the original world position and sprite ID.

The M2 camera and its dead zone still determine projection. The CPU maps
marker centers through that camera into pane-relative clip space and sends
the normalized center and half-size in `TEXCOORD0`. `TEXCOORD1` carries RGBA.
The vertex shader expands three vertex IDs around each center. The pixel
shader returns that instance's opaque color. When the guard leaves the
viewport, the same draw renders its muted edge indicator in the direction
of its actual position. The pane controls aspect and scale. GPUI snaps the
offscreen bounds to device pixels and applies its existing scene clip.

The D3D11 pass creates a vertex shader, a pixel shader, an input layout, and
one dynamic vertex buffer for instances. The shared mesh lives in the shader.
The target is one BGRA8 texture with a render view and a sampled view. A new
or resized target is redrawn, as is a target whose instances changed. A
stable target reuses its contents. The renderer submits one `DrawInstanced`
with three vertices per instance, then GPUI samples the same texture through
its existing sprite compositor. The draw uses opaque painter's order over a
transparent clear. The runtime path has no texture readback or atlas copy.

Two instances fit inline in the GPUI scene and the cached upload data.
The normal two-object draw writes 64 bytes to the same dynamic buffer.
Growth allocates only when the visible count exceeds capacity. A pane resize
replaces only the texture and its two views. The pass unbinds its shaders,
vertex buffer, and render view after the draw, and the compositor unbinds
its sampled view after use. An empty surface, tab closure, or device reset
retires the cached pass. The M2 preview's fixed-step simulation and input
remain on their existing lifecycle.

The only shader compiler change is guarding Vulkan location attributes with
DXC's `__spirv__` preprocessor symbol. Direct3D still reads `TEXCOORD` from
the same emitted HLSL. The vertex ID and per-instance attributes use
separate input streams as supported by the current validator.
[DXC SPIR-V macro documentation](https://github.com/microsoft/DirectXShaderCompiler/blob/main/docs/SPIR-V.rst#macros-for-spir-v)

## Reproduction

Use the Windows x64 build prerequisites from [M0](zed-integration-m0.md).
The script accepts a clean Zed pin, the exact M3a patch, or the current M3b
patch. It upgrades the M3a patch while rejecting unrelated edits to the
managed Zed files.

```powershell
pwsh -File scripts/build-zed.ps1
cargo test --locked
Push-Location vendor/zed
cargo test --locked -p wyrmcoil_view --lib
cargo test --locked -p gpui_windows --lib gpu_viewport_reuses_and_resizes_its_target_then_drops_the_pass -- --ignored
Pop-Location
```

Run `zed.exe` from `vendor/zed/target/debug` and choose
`wyrmcoil: open preview`. Set `RUST_LOG` to
`info,gpui_windows::directx_renderer=trace` before launch to log instance
uploads and draws. `WYRMCOIL_GPUI_MARKER` still selects the original square
markers for diagnostic comparison.

The optional Oct and SPIR-V validation uses `WYRMCOIL_OCT_ROOT`, `dxc`,
`spirv-val`, and `spirv-dis` as described in [M3a](zed-integration-m3a.md).

## Validation

| Check | Result |
| --- | --- |
| WyrmCoil suite | 401 passed, 8 ignored in the default lane. |
| Adapter suite | 21 passed, up from 19. New cases cover real snapshot projection, stable entity ordering, empty and one-object snapshots, hidden and dead objects, zero bounds, and the distant guard. Existing M2 input, timing, and camera cases passed. |
| Windows D3D11 test | Passed on the real device. Test-only readback confirmed two independently colored triangles, transparent background, painter's order on overlap, and replacement of old positions after an update. Buffer, layout, and shader handles survived resize. Zero bounds discarded the pass. |
| Shader compatibility | Seven Oct/DXC conformance tests passed. The instanced vertex and pixel shaders compile to validated SPIR-V. An emitter regression checks the conditional Vulkan location qualifier. |
| Naming policy | The adapter passed `wyrmfmt`. The root check reported the eight existing findings. |
| Reproducibility | A separate pinned checkout prepared successfully from upstream, upgraded from the exact M3a GPU patch, and accepted the resulting patch a second time. |
| Integrated build | The locked Windows x64 build produced `zed.exe` and `cli.exe` in 184.56 s with a warm dependency cache. |

### Windows x64 GUI

Zed ran on an RTX 3070 with Direct3D 11.1. The preview showed a cyan player
at `(0, 0)` and a bright orange guard at `(5, 5)` near session start.
D and Up moved the player to `(1, 1)` while the guard advanced on its own
and became a muted edge marker. Repeated D taps moved the player to
`(23, 1)` while the grid scrolled about 60 logical pixels at the same zoom.
Escape froze the simulation and an A press had no effect. A click resumed.

Both triangles remained visible through maximize and restore. The buffer
and shader objects were reused during those resizes, as confirmed by the
resource trace. Minimize and Alt+Tab suspended the simulation and resumed
without a catch-up burst or player movement. Switching tabs preserved the
engine session. Ctrl+S saved a scratch edit verified on disk. Ctrl+W
returned to the editor, reopening started a fresh session at `(0, 0)`,
and Alt+F4 exited with code 0 with the preview open.

| Measurement | Result |
| --- | --- |
| Simulation and snapshot sample | 366 ticks and 366 snapshots in 6.106 s, approximately 59.9 Hz each. Display presentation FPS was outside this measurement. |
| Offscreen submissions | 3,301 recorded draws across the smoke run. Every draw used two instances in one `DrawInstanced` call, with 64 instance bytes uploaded. This count excludes GPUI composition and cached frames. |
| GPU object creation | Three sets of shader objects, layouts and buffers, covering the first open, return from the editor tab, and reopening after close. Five targets total, including the two target-only allocations during maximize and restore. |
| Initial pipeline setup | 2.79 ms on the CPU, including D3DCompile and shader and layout creation. |
| Whole-process CPU | Six-second samples measured 44.9% of one logical core active, 6.5% minimized, and 3.4% after closing. The smoke run had renderer trace logging enabled. |
| GPU diagnostics | Zed reported no viewport or DXGI failures in this run. Device loss and mixed-DPI monitor moves were outside the scope of the smoke run. |

The executable artifacts are in `vendor/zed/target/debug`.

| Artifact | SHA-256 |
| --- | --- |
| `zed.exe` | `2d08b0b74c3df093f14950b7e88aea9fffd806c7c09592c9f88603cb657189c3` |
| `cli.exe` | `87fe8f7f0480fa33cabfd6e9d85e6ddd73b58fe38ae29dd9cfe11b78a70e15fd` |

Evidence lives under `target/` in `zed-m3b-final-build.log`,
`zed-m3b-tests.log`, `zed-m3b-gpu-tests.log`, `wyrmcoil-m3b-tests.log`,
`wyrmcoil-m3b-dxc.log`, and `zed-m3b-smoke/`. The smoke directory holds
`user-data/logs/Zed.log`, `cpu.jsonl`, `m3b-smoke.txt`, and `exit.txt`.
The Oct fixtures came from `fa5b4993ca61f79b2ce18d09e9e3bbce88b9b9a0`.

## Integration cost since M3a

| Area | Lines added / removed |
| --- | ---: |
| Adapter Rust production | 64 / 16 |
| Adapter Rust tests | 71 / 3 |
| SDSL-V viewport shader | 17 / 6 |
| WyrmCoil world, simulation, and renderer contracts | 0 / 0 |
| Shader emitter production | 5 / 3 |
| Shader conformance and emitter tests | 10 / 1 |
| Zed patch preparation | 14 / 3 |
| GPUI core production | 23 / 5 |
| Windows D3D11 backend production | 132 / 33 |
| Windows GPU test | 92 / 7 |

Four pinned upstream files changed since M3a. The complete renderer patch
still touches five upstream files. It adds 436 production lines and removes
five against upstream, excluding the 140 test lines. The integrated source
total is 1,062 production lines when counting the entire preview adapter,
the SDSL-V shader, and the net GPUI patch. M3a had 886 lines by that metric.
There are no new package dependencies or lockfile changes.

Seven viewport GPU objects are created on first use, up from five in M3a. M3b owns
two shader objects, one input layout, one buffer, one texture, and two
views. The target texture and views account for three new objects after a
resize. The instance packet fits inline for the demo's two objects, which
the adapter test checks. The test does not measure allocations in the
existing Zed renderer or the preview UI.

Native presentation is specific to Windows D3D11. The existing GPUI square
path covers platforms without this surface implementation.

## M4 handoff

Keep `RenderSnapshot` and the Zed pane ownership boundary. Start with one
world-space mesh, a camera transform, and an explicit depth attachment in
the existing offscreen pass. Qualify a single authored scene object before
adding more authoring controls or material features.
