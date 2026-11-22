# WyrmCoil M6V in Zed

The Windows x64 Zed Preview rendered the real WyrmCoil room on an RTX
3070 through Vulkan and native SDSL-V SPIR-V. Zed retained its D3D11
editor renderer. The shared texture stayed GPU-resident through
composition. The headless Windows qualification also executed native
SDSL-V vector addition on the room's own Vulkan device and queue.

## Architecture audit

The baseline is WyrmCoil `183064b` and Zed
`a246423a3b24a08e0d0c8574eab3a9de422747e0`. The Windows GPUI platform creates a
DXGI adapter, D3D11 device and immediate context. The M4 viewport already prepares
one offscreen target before traversing GPUI surface batches and samples it with
the regular polychrome sprite pipeline. The 3D preview supplies immutable
cube vertices and indices, and a packet of clip transforms and colors derived
from the real room snapshot. The world emits the same portable snapshots for
both backends.

Earlier Windows GPUI did reuse the Linux Blade/Vulkan renderer. Zed switched
Windows to D3D11 for driver compatibility. The pinned Linux implementation now
selects `gpui_wgpu` through its Linux platform and its Wayland/X11 surface
configuration. The Windows selection is a separate `WindowsPlatform`. There
is no Windows feature flag at this pin that plugs the Linux renderer into the
D3D11 surface compositor. [Zed Windows development report](https://zed.dev/blog/windows-progress-report)

| Route | Integration work |
| --- | --- |
| GPUI's Linux wgpu route | It owns an editor device, swapchain-facing surface, pipelines, and atlas. Using it for the Windows compositor requires a Windows platform port and changes the editor backend. |
| WyrmCoil wgpu | The render abstractions are useful for existing examples. Win32 external-memory import and keyed submit would still require native HAL and Vulkan access, in addition to wgpu's ownership and shader validation. |
| Direct Vulkan | `ash 0.38` already resolves through wgpu in both lockfiles. It exposes the Win32 handles and submission structures and can consume native DXC SPIR-V. Use this path for the game and keep the D3D11 editor and fallback. |

The native backend selects the adapter by DXGI LUID and checks image usage
and memory types before a dedicated Win32 import. Each Vulkan command buffer
transfers ownership from the external queue family, renders into BGRA8 with
a D32 depth attachment, and returns the color image to external ownership
in `GENERAL` layout. A keyed submit acquires key 0 and releases key 1.
The D3D11 compositor polls the Vulkan fence, probes key 1 with a zero
timeout, enqueues a GPU copy into GPUI's private viewport texture, and
returns key 0. A nonblocking D3D11 event query gates the next producer
submission. The existing GPUI sprite batch samples the private texture at
the viewport's normal scene order and clipping bounds. This path uses one
GPU copy per completed game frame and keeps all pixel transfer on the device.
[Vulkan Win32 import](https://docs.vulkan.org/refpages/latest/refpages/source/VkExternalMemoryHandleTypeFlagBits.html)
[Vulkan keyed submit](https://docs.vulkan.org/refpages/latest/refpages/source/VkWin32KeyedMutexAcquireReleaseInfoKHR.html)
[DXGI keyed acquisition](https://learn.microsoft.com/en-us/windows/desktop/api/dxgi/nf-dxgi-idxgikeyedmutex-acquiresync)

The installed SDK 1.4.350 reports an RTX 3070 on NVIDIA driver 610.88,
Vulkan device API 1.4.341, subgroup size 32, and a storage-buffer range
limit of 4,294,967,295 bytes. It reports Win32 external memory, keyed mutex,
external semaphore, and timeline semaphore support. The first DXGI adapter
matched Vulkan's valid LUID `00000000:0000c863` on this boot. Radeon did not
match. A native probe found importable, dedicated-only RGBA8 and BGRA8 UNORM
for color attachment, transfer destination and sampled usage. It allocated
and bound the RGBA8 import, submitted a Vulkan clear with a keyed handoff,
and read `255,64,127,255` through D3D11 in the test.

### Concept Vulkan reference

I inspected `Concept/libraries/Vulkan` at `703d3a8`. Its owned `Context`
and borrowed `DeviceView` informed the compute proof. The room retains
ownership of the Vulkan device and queue while the test borrows them for
a single submission. Concept's
`QueueHandoff` owns the producer and consumer submissions and retires
them before releasing the semaphore. That implementation hands work
between queues on one Vulkan device. The Windows compositor still
requires its keyed-mutex bridge to hand the image between APIs.

The most useful performance idea is to timestamp the GPU work and read
the queries after the existing fence signals. Concept's
`ElapsedNanoseconds` accounts for the queue's timestamp precision and
device period. Applying that pattern to the Vulkan render scope, and
measuring the D3D11 copy and host redraw separately, would show where
to spend the next optimization. The CPU samples alone cannot attribute
the difference. Concept also treats an unexpected fence error as
distinct from GPU completion or terminal device loss. The viewport's
recovery path needs that distinction before device-loss qualification.

## Implementation and workflow

`wyrmcoil: open preview` now selects Vulkan for the Windows 3D room.
The same authored SDSL-V produces both graphics artifacts. WyrmCoil parses and
validates it, lowers it to VdMir, and emits HLSL. DXC targets Vulkan 1.3 and
produces SPIR-V for `Room_VS` and `Room_PS`. The D3D11 fallback consumes
the same HLSL through D3DCompile and shader model 4.1. The 2D preview
continues to use D3D11. The 3D preview status shows the matched Vulkan
device or the reason D3D11 fallback activated.

The room producer holds compiled SPIR-V artifacts for the life of the
preview. The surface pass owns the Vulkan device, queue, reusable mesh,
instance buffer, graphics pipeline, commands and fence. GPU allocations
stay within the Windows renderer. Scene-artifact replacement updates the
existing mesh instance packet. Resize replaces the GPUI destination,
shared image and Vulkan depth attachment after pending work retires.
When its surface disappears, GPUI drops the pass, and Vulkan retires the
last submission before destroying its resources. The D3D11 runtime
retains its queued copy resources. Tab switching can recreate a surface
pass and reuse the preview's compiled SPIR-V. The preview's fixed-step
simulation and M6 artifact watcher operate independently of GPU progress.

Use the pinned build script with the same MSVC environment as M6. DXC is
discovered using `WYRMCOIL_DXC`, then the Windows Vulkan SDK environment
(`VULKAN_SDK` or `VK_SDK_PATH`), then `PATH`. To exercise the explicit
D3D11 fallback, set `WYRMCOIL_GRAPHICS_BACKEND=d3d11` before starting Zed.
An eligible one-folder local game project can publish scene artifacts
with the same explicit Cargo build as M6.

```powershell
pwsh -NoProfile -File scripts/build-zed.ps1
$env:WYRMCOIL_DXC = 'C:\VulkanSDK\1.4.350.0\Bin\dxc.exe'
& vendor/zed/target/debug/zed.exe examples/games/training-room
```

## Native compute proof

The compiler now accepts one standalone canonical compute entry per
module. Its Oct-style thread input supplies `DispatchId`, `GroupId`,
`GroupThreadId` and `GroupIndex`. Explicit `[numthreads(x, y, z)]` and
`[binding(n)]` become typed VdMir execution metadata and storage
bindings. Read-only and writable storage arrays admit `f32`, `i32`
and `u32` elements. Validation rejects invalid workgroup sizes, duplicate
bindings, writes to read-only storage and noninteger indices. The
existing HLSL emitter selects `StructuredBuffer` or `RWStructuredBuffer`
and assigns binding numbers in Vulkan set 0. DXC emits a `cs_6_0`
SPIR-V entry. Templates, workgroup variables and a general compute
scheduler remain outside this slice.

`spirv-val` accepted the SPIR-V for both the WyrmCoil vector-add
fixture and the pinned Oct `ComputeNoRegression` fixture. Disassembly
confirmed the compute entry, global invocation ID, local size 8×1×1,
and expected descriptor bindings. The Windows GPU witness used the
same `VulkanRoom` device and queue for graphics, a four-workgroup
`VectorAdd_CS` dispatch, then graphics again. All 32 outputs matched the
CPU reference exactly. The overlap pixel and full image remained equal
with reversed box order after compute. Khronos validation was active
and reported no errors. Readback is confined to this test. The native
log is in `target/m6v-compute-gpu.log`.

## Automated evidence

- The root test suite passed 413 tests with 11 default skips. The Zed
  adapter passed 28 tests. Both native D3D11 tests passed. `wyrmfmt`
  accepted the adapter and reported the same eight existing findings in
  the root check.
- The pinned prepare script applied to a pristine checkout, ran twice
  without drift and upgraded an M6-prepared checkout using two short
  overlay patches.
- The native RTX 3070 integration test compiled the real room to SPIR-V,
  rejected a mismatched LUID and zero extent, drew the real snapshot into
  shared textures at 160×128 and 200×144, and reused the pipeline across
  resize. It staged the test output through D3D11 and found 12 colors,
  including expected BGRA `49,146,224,255` at the overlap. Reversing
  cube submission order produced byte-identical pixels. The Vulkan
  validation layer emitted no reported errors during this run.
- The Oct-pinned shader lane passed 11 cases, including graphics and
  compute SPIR-V validation.

```powershell
cargo test --locked
cargo run --locked --bin wyrmfmt -- check --lang rust src tests examples
$env:Path = "C:\VulkanSDK\1.4.350.0\Bin;$env:Path"
$env:VK_INSTANCE_LAYERS = 'VK_LAYER_KHRONOS_validation'
cargo test --locked --lib room_renders_and_dispatches_compute_on_the_same_vulkan_device -- --ignored --nocapture
# Point WYRMCOIL_OCT_ROOT at the pin in tests/sdslv_oct_conformance.rs.
cargo test --locked --test sdslv_oct_conformance -- --include-ignored
Push-Location vendor/zed
cargo test --locked -p wyrmcoil_view --lib
cargo test --locked -p gpui_windows --lib gpu_viewport -- --ignored
Pop-Location
```

## Windows desktop evidence

The Vulkan view displayed the room, depth occlusion and moving guard.
Forward and yaw input changed the camera, Escape paused it, and clicking
resumed play. Pane resize recreated the target at 1,064×898 from
1,296×898. Minimizing stopped compositor copies and restoration resumed
them. The project tree, command palette, text editing and Ctrl+S worked.
Ctrl+W released the room and reopening restored the camera with a fresh
Vulkan session. The 2D preview opened separately. The first and final
validation-enabled Vulkan runs both exited with code 0.

I changed a box's width, location and color in Zed and ran the existing
scene-build command explicitly. The paused room automatically mounted
artifact `66d0f866856e3066` and showed the larger purple box. The final
build repeated the reload in reverse to `1ebe01898af685a1`. The native log
recorded no pipeline or target recreation at publication. The two
replacements took 2.04 ms and 1.68 ms to read and validate, and 0.90 ms
and 0.29 ms to mount, respectively.

The same scene also rendered with explicit D3D11 selection. Pointing
`WYRMCOIL_DXC` to a missing executable triggered a visible
`D3D11 fallback` diagnostic and preserved the room. A deliberately
mismatched LUID was rejected in the native test. Physical device loss and
other GPU/driver combinations remain unqualified. The loader trace
confirmed that the validation layer was active in the instrumented desktop
run, with no reported Vulkan validation errors.

The screenshots, loader output and CPU samples are in
`target/zed-m6v-smoke/`. See `05-vulkan-reloaded-paused.png`,
`11-final-vulkan-reloaded.png`, `12-final-vulkan-reopened.png` and
`14-d3d11-automatic-fallback.png`. The renderer trace is in
`final-vulkan-Zed.log` and `perf-Zed.log`. These captures cover the
graphics qualification build. The subsequent compute compiler build was
qualified headlessly, including the room's Vulkan/D3D11 interop regression.

## Measurements and cost

| Measurement | M6V result |
| --- | --- |
| Windows x64 build | The first pinned editor and CLI build took 3m06s and 15.64s in Cargo. The final graphics editor took 2m51s, and the CLI took 1m32s including contention with the regression test build. The incremental compute compiler rebuild took 31.19s for the editor and 1.27s for the CLI. |
| Vulkan initialization | 275.82 ms without validation layers, including 91.28 ms for the shader artifacts. The layer-enabled first open and reopen measured 323.17 ms and 191.48 ms. Returning to a prior tab can recreate the device and pipeline while retaining SPIR-V. |
| Simulation and snapshot cadence | 369 ticks and 369 snapshots in 6.136 s, or about 60.1 Hz each, from the final room counters. |
| Compositor cadence | 60 completed Vulkan frames in each of five consecutive full seconds without the validation layer. With the layer active the corresponding sample ranged from 49 to 60. Display presentation timing and GPU timestamps were unmeasured. |
| Process CPU | Six-second samples in the dev build with trace enabled used 84.7% of one logical core in active Vulkan and 46.7% in active D3D11, both without Vulkan validation. With validation active the Vulkan session used 103.6% active, 3.1% paused and 12.0% after closure. The paused and closed samples had no Vulkan copies. These samples include ordinary editor work. |
| GPU utilization | One device-wide `nvidia-smi` sample during active Vulkan reported 40% GPU and 12% memory utilization. That sample includes other GPU clients. |
| Draw and transfer | The eight-instance room uses two indexed draws, 456 static geometry bytes and 640 instance bytes per changed frame, as in M6. Vulkan adds one BGRA8 GPU copy per completed frame, or 3,821,888 bytes at the sampled 1,064×898 extent. Fence and event polling avoids waits in the live loop. Isolated copy and synchronization timings were unmeasured. |
| GPU resources | One reusable device, queue, graphics pipeline, command buffer and fence per surface pass. The Vulkan side has three buffers and, per extent, one imported image and one depth image. D3D11 retains its private viewport texture, the shared texture and one completion query. M6 used its private offscreen target with D3D11 depth. |
| Code | 847 lines for the native backend including its test module declaration and +6/−5 to expose it. The adapter added 187 Rust lines and removed 4. Compute added 444 compiler lines and removed 23. The native GPU proof added 237 lines to the graphics witness test. |
| Pinned Zed delta | +286/−1 across GPUI scene and window hooks and the Windows viewport pass, plus two generated lockfile entries. Zed application and workspace source remain at their M6 patch. The overlay keeps the M0 and M4 patches intact. |
| Dependencies | Windows runtime `ash 0.38`, adapter `log`, and root Windows test binding `windows 0.58`. All already resolved in their respective lockfiles. The Windows D3D11 compositor and SDSL-V fallback remain in use. |

The compute-qualified binaries are in `vendor/zed/target/debug`. The
editor used in the desktop captures had SHA-256
`a5ba801f0c64ecf00f3f75cc7126dd68b62db69060c3bac77bedfe4b76eab92e`.

| Binary | SHA-256 |
| --- | --- |
| `zed.exe` | `e83bdf764c0836c1cfe503054f01e18bdc8fb59cbe536b7d61c274fdeece7252` |
| `cli.exe` | `5780356db153a332d36dbfa2360f1db2176665fcf5db9592adf6820fe8b059dc` |

## M7 handoff

Keep scene artifacts as the validated initial-world input. Define the
smallest independent Rust game-project execution contract around an
explicit trusted build, versioned inputs and outputs, and a deterministic
tick boundary. Preserve the current editor and renderer lifetimes across
a game-project restart. The matched Vulkan device already has an
executable storage-compute witness for future GPU work.
