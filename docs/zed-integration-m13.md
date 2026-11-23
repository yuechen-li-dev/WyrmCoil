# M13. Persistent SPIR-V and startup qualification

## Shader ownership

Zed’s Windows compositor owns presentation and the D3D11 device. WyrmCoil’s
background producer owns Vulkan device initialization and shader artifacts.
The Vulkan renderer consumes one compiled vertex and one fragment artifact
for the room or tactical mesh. Last Watch adds a separate vertex and
fragment pair for skinned meshes. A producer retains those bytes while it
is alive. The M12 in-memory cache helped when returning to an existing
preview. Closing the preview or restarting Zed discarded it.

M13 extends the existing SDSL-V → VdMir → HLSL → DXC path. It validates
and lowers source on each new shader preparation. A hit skips DXC process
creation, including the former `--version` probe. A miss uses DXC's normal
SPIR-V validation. The authored SDSL-V and generated HLSL also feed the
Shader Model 4.1 D3D11 fallback.
[DXC SPIR-V mapping and validation](https://github.com/microsoft/DirectXShaderCompiler/blob/main/docs/SPIR-V.rst).

The key is SHA-256 over the cache and compiler schemas, SDSL-V source digest,
the exact emitted HLSL, DXC executable and sibling compiler DLL contents,
stage, entry point, profile, and all output-affecting command options.
Fingerprinting happens once for a producer batch. Changed shader modules get
new keys while other modules keep theirs. An edit anywhere in one authored
module invalidates its entry points, including a stage whose emitted
binary would happen to be identical. Unsupported compiler layouts bypass
persistent reuse. The qualified layout is the Windows SDK's paired
`dxc.exe` and `dxcompiler.dll`.

WyrmCoil's SDSL-V front end rejects unresolved imports before emission.
The cached HLSL path accepts the emitter's local `__spirv__` guards.
HLSL with other directives goes through the uncached compiler path. A
future module resolver must continue passing fully materialized shader
input downstream and cover any external dependencies in the key.

## Cache storage and failure behavior

The Windows default is
`%LOCALAPPDATA%/WyrmCoil/Cache/shaders/spirv-v1`.
`WYRMCOIL_SHADER_CACHE` selects a different parent directory. Each entry
contains an 80-byte versioned header and the raw, little-endian SPIR-V
payload. The header holds the key, payload SHA-256, and observed previous
compile duration. Reuse checks the key, digest, size, SPIR-V header,
instruction framing, target version, entry name and execution model.
Independent conformance and cache tests also run `spirv-val`.

One OS file lock coordinates publication and clearing across processes.
The compiler checks again under the lock before compiling. A complete
entry is published with a sibling temporary file and atomic rename. Bad
or incomplete entries are replaced after a successful build. Compiler
errors retain source and entry labels. After ten seconds of lock contention
the current request can compile without publishing. The cache is bounded
to 128 records of up to 512 KiB each and evicts oldest publications.
The CLI clears only cache records and its own metadata.

The optional cache trace reports individual hits, misses, rejected entries,
and prior compile time available for each hit. The CLI reports the current
record count and bytes alongside cumulative hits, misses, rejections and
the sum of prior compile times for hits. That sum estimates work skipped.
First-frame latency is measured separately.

## Editor scheduling and presentation

Game staging, asset preparation, SDSL-V processing, DXC, and Vulkan
device and pipeline creation run on the preview's existing GPUI workers.
M13 also prepares the Windows fallback bytecode on those workers. GPUI
receives the SM4.1 bytecode and creates the D3D11 shader objects on the
renderer thread. Preview closure drops pending tasks. Work already inside
a synchronous library or compiler call may finish before the worker
retires its result. The D3D11 device, Win32 handles, and interop handoff
remain on their existing owner thread.

`WYRMCOIL_VULKAN_STARTUP=1` retains the M12 stages and adds game discovery,
executable staging/spawn, observed handshake, scene decode, CPU art,
SDSL-V parse/validate/lower/emission, compiler fingerprint, cache lookup,
DXC spawn and child execution, and SPIR-V structural checking. Process and
scope IDs distinguish overlapping API calls. The GPUI trace timestamps
first accepted DXGI Present for both renderers. Launch to that timestamp
includes worker scheduling, compositor handoff and the DXGI call.
It measures frame delivery to DXGI. Physical display timing requires
additional telemetry. Child execution wall time includes the child's
initialization, compilation, validation and exit. Vulkan GPU draw ticks
have their own timing path.

## M12 reference

A fresh M12 release-fast editor process opened Last Watch on the
RTX 3070, driver 610.88, with validation off and a warm OS file cache.
The Vulkan producer took 6.6945 s. The room shader scope was 761.6 ms,
the skin shader scope was 5,657.9 ms, and instance creation was 187.2 ms.
The original trace grouped child-process startup and execution. The same
preview showed all six GPU actors. M12's earlier release producer sample
was 1.871 s with about 1.49 s attributed to its shader scopes. Its
precompiled hardware probes covered 74–181 ms across 32 launches.

The historical 8.68 s event remains unexplained. The 6.69 s reproduction
shows a separate slow shader-tool path. The raw reference is
`target/m13/m12-baseline-stderr.log`.

## Startup measurements

The GUI runs used the same RTX 3070, scene, game executable, and warm OS
file cache. Closing the tab discarded the mounted art and Vulkan producer.
Cold shader-cache runs started with an empty versioned cache. The debug
series ran while the release editor was building. The table retains
observed outliers. Ranges are minimum to maximum, and times are in
milliseconds. These sample sizes do not support a meaningful P95.

| Debug Vulkan scenario, validation off | Runs | DXC per launch | Hits / misses per launch | Producer median [range] | Command to first DXGI frame median [range] |
| --- | ---: | ---: | ---: | ---: | ---: |
| Empty shader cache | 2 | 4 | 0 / 4 | 473 [409–536] | 886 [822–950] |
| Warm cache, reopened preview | 5 | 0 | 4 / 0 | 191 [177–203] | 558 [532–10,983] |
| Warm cache, fresh editor process | 1 | 0 | 4 / 0 | 458 | 919 |

Cold and warm reopen medians were 138 and 126 ms for game executable
staging and launch, 29 and 26 ms for the observed handshake, and
148 and 141 ms for CPU art. Handshake measurements include the delay
until the editor poll. Debug cache hits took roughly 13–15 ms total in
lookup and validation plus about 15 ms to fingerprint DXC. Hashing the
same SDK files in an unoptimized SHA-256 build took about 490 ms.
The debug profile now optimizes only the existing SHA-256 implementation
for this work, in addition to M12's PNG dependency settings.

| Release-fast scenario | Runs | DXC per launch | Hits / misses per launch | Producer median [range] | Command to first DXGI frame median [range] |
| --- | ---: | ---: | ---: | ---: | ---: |
| Empty cache, validation off | 3 | 4 | 0 / 4 | 293 [289–478] | 557 [534–793] |
| Warm cache, reopened preview, validation off | 3 | 0 | 4 / 0 | 148 [147–165] | 422 [414–5,410] |
| Warm cache, fresh process, validation off | 1 | 0 | 4 / 0 | 274 | 527 |
| Warm cache, fresh process, validation on | 1 | 0 | 4 / 0 | 345 | 612 |
| Warm cache, reopened preview, validation on | 1 | 0 | 4 / 0 | 184 | 469 |
| Explicit D3D11, fresh process | 1 | 0 | 0 / 0 | — | 223 |

The release benchmarks ran after the builds completed. A fresh release
process with validation off delivered its first frame at 793 ms with an
empty cache and at 527 ms with a warm cache. DXC cold-launch medians
were 22.9 ms to spawn and 103.0 ms for child execution, summed across
four stages. In warmed release reopens the compiler fingerprint was
about 13 ms. Four cache lookups, including structural inspection, took
about 11 ms. The four published records occupied 7,428 bytes.

The following validation-off release medians include the outlier. Several
scopes are nested or overlap on workers.

| Stage | Empty cache, ms | Warm reopen, ms |
| --- | ---: | ---: |
| Game discovery, executable staging and spawn | 117.25 | 114.55 |
| Observed protocol handshake | 14.52 | 8.64 |
| Scene decode | 1.28 | 2.07 |
| CPU art preparation | 55.84 | 54.99 |
| Vulkan instance | 29.11 | 29.14 |
| Vulkan logical device | 35.39 | 31.64 |
| Shader module creation, both pairs | 1.66 | 1.72 |
| Graphics pipeline creation, both passes | 1.90 | 1.92 |
| Room and skin buffers and uploads | 2.42 | 2.50 |
| Shared color image attachment | 9.08 | 9.27 |
| First queue submit API call | 0.91 | 0.91 |

The raw trace also covers parse, validation, VdMir, HLSL emission,
individual interop calls, fence and handoff. The release warm validation
case explicitly loaded the Khronos layer and reported no Vulkan
validation errors. During a six-second release patrol sample the editor
used 25.7% of one logical core, a 791 MiB working set and 346 MiB of
private bytes. After closing the preview it used 1.6% of one logical
core, a 720 MiB working set and 279 MiB of private bytes. Single runtime
samples describe the observed session. Display cadence was unmeasured.

One warm reopen delivered its first frame in 10.983 s. The game
executable's spawn call accounted for 10.487 s and the Vulkan producer
for 203 ms. The latter hit all four shader entries. A separate
corruption-repair run had a 10.777 s game spawn call and a 223 ms Vulkan
producer. A warm release reopen took 5.410 s, including a 5.069 s game
process spawn and a 148 ms Vulkan producer. These outliers occurred before
shader or Vulkan startup. The traces identify the affected call but leave
its OS-level cause open.

Sixteen fresh, precompiled native room probes exercised graphics,
Win32 interop and real vector addition. Eight with validation off took
266–319 ms to build the room (median 295 ms), and eight with validation
requested took 279–332 ms (median 306 ms). The parallel release build
was still active. Device instance creation ran at medians of 248 and
253 ms. The room pipeline itself was 0.35 ms without the layer and
0.58 ms with it. The compute proof pipeline medians were 0.15 and
0.20 ms. M13 keeps the existing null Vulkan pipeline cache. The measured
pipeline work and the need to bind any future pipeline cache to device
and driver identity favor deferring persistence.
[Vulkan pipeline cache compatibility](https://docs.vulkan.org/refpages/latest/refpages/source/VkPipelineCacheHeaderVersionOne.html).

The debug GUI showed all six GPU-skinned actors. A warm run targeted a
guard and queued Shield Bash, Backstab and Frost Bolt during tactical
pause. After resume, all three cooldowns advanced. The battle reached
defeat and restart restored tick zero. The editor accepted its save
shortcut after the preview closed. Truncating only the cached skin
fragment artifact caused one rejection, one DXC invocation and three
hits. The rebuilt preview displayed all six actors again. Forcing a
missing DXC path exercised automatic D3D11 fallback, including its
diagnostic, all six CPU-skinned models and live patrol motion.

For the shader-edit check, a separate debug build halved RGB in the
skin fragment shader. The Vulkan preview showed six darker actors
and unchanged scenery. The two tactical scenery entries hit the
cache. Only the two entries from the changed skin module invoked
DXC. The canonical shader and debug binary were restored and
verified by file hash.

## Regression evidence

| Lane | Result |
| --- | --- |
| Root Cargo suite | 426 passed in each of debug and release |
| Cache tests | 6 passed with real DXC and independent `spirv-val` checks |
| Pinned Oct and SDSL-V conformance | 8 opt-in cases passed, including graphics and compute |
| Vulkan GPU tests | 2 passed, plus 16 repeated Vulkan room/compute probes |
| D3D11 GPU tests | 2 passed, including depth, resize and release |
| Zed adapter | 30 passed, including cancellation, scene reload and fallback contracts |
| Independent game tests | Guardhouse 2, Last Watch 5 |
| Real game-executable integration | Both opt-in integration tests passed |
| Actual process playtests | Guardhouse 9 assertions and Last Watch 14 in both profiles, including victory and defeat |
| Style checks | `rustfmt` and diff check passed. `wyrmfmt` preserved eight existing findings |

Raw evidence is under `target/m13`. The debug captures show the cold
and warm rosters, three active cooldowns, defeat, corruption recovery,
automatic D3D11 fallback, and the changed skin shader. Release captures
show warm Vulkan Last Watch with validation, Vulkan Guardhouse, and
explicit D3D11 Last Watch. The dedicated editor and game processes
were gone at the final check. Use
`startup-metrics.json`, the per-process stderr logs, and
`compositor.log` to audit the first-frame measurements.

## Development and deployment

- WyrmCoil working tree based on `00f2cef0325f3b77dc361a80e5ca4d5cdc659e95`.
- Zed pin `a246423a3b24a08e0d0c8574eab3a9de422747e0`.
- Root qualification uses Rust 1.95.0 MSVC. The pinned Zed toolchain uses
  Rust 1.98.1 MSVC and the x64 Windows SDK. Release-fast uses upstream’s
  optimized profile with debug symbols disabled in qualification.
- Both pinned Zed profiles built the editor and CLI. M13's GPUI patch
  adds 47 lines and removes 19. The lockfile overlay adds two dependency
  references. The engine uses the `sha2` crate already pinned in Zed.
- Vulkan authoring targets Vulkan 1.3. In Zed it also requires the selected
  Vulkan and DXGI devices to share a LUID, Win32 external memory and a
  keyed mutex. Windows D3D11 fallback uses SM4.1 bytecode and CPU poses.
- DXC qualification uses SDK 1.4.350.0, build `fe2615732`. The SHA-256
  prefixes of the executable and compiler DLL are `2f94ff0e5575` and
  `6e990d20e533`. The same SDK supplies `spirv-val` and the validation layer.
- Last Watch is an independently built Rust game using the existing
  WyrmCoil process and scene protocol. The game executable and project
  assets travel together. The source GLB, glTF, buffers and textures
  retain their original paths and [asset credits](../examples/games/last-watch/ASSET_LICENSES.md).

```powershell
$sdk = 'C:/VulkanSDK/1.4.350.0/Bin'
$env:WYRMCOIL_DXC = "$sdk/dxc.exe"
$env:Path = "$sdk;$env:Path"
$env:WYRMCOIL_SHADER_CACHE_TRACE = '1'
$env:WYRMCOIL_VULKAN_STARTUP = '1'

./examples/games/last-watch/build.ps1
./scripts/build-zed.ps1 -Profile release-fast
cargo run --locked --bin wyrmcoil -- shader-cache stats
cargo run --locked --bin wyrmcoil -- shader-cache clear
cargo test --locked --lib Engine::shader::sdslv::spirv_cache -- --include-ignored

$env:WYRMCOIL_OCT_ROOT = (Resolve-Path target/oct-m5-fixtures).Path
cargo test --locked --test sdslv_oct_conformance -- --ignored
cargo run --locked --bin wyrmcoil -- playtest --project examples/games/last-watch --scenario last-watch --output target/playtests/last-watch
```

Open the game project in Zed and use `wyrmcoil: run game` after building.
The authoring cache still reads DXC's files on startup to identify the
toolchain. A shipped build can exclude DXC by publishing validated SPIR-V
with the game. At publish time, compile and validate each declared stage,
then record its source and generated-artifact digests, entry, stage,
target and toolchain in a release manifest. The standalone host should
verify that manifest and its artifact payloads directly at startup. It
can ship precompiled DXBC if it offers D3D11. The development cache remains
regenerable. Shipping the cache directory alone leaves the authoring
compiler-identity dependency in place.

## M14 handoff

Use the M13 gameplay and renderer lanes as regression controls. The first
player-facing work is encounter readability and balance, including ability
feedback and timing, plus sound and music. Qualify those changes with real
playthroughs. For distribution, build a standalone host over the existing
game protocol and package the optimized game executable, source assets,
licenses and precompiled shaders. Check launch, shutdown and fallback from
a clean machine without Zed or DXC installed. The M13 editor integration
remains the authoring workflow.
