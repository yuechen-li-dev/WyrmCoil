# M12. Texture preparation and Vulkan startup

## Asset path and cache

The Last Watch uses its original KayKit glTF and GLB files. The importer
resolves external paths within the project, bounds file reads, and validates
the buffer views. An embedded image borrows its encoded slice from the GLB
buffer. Data URIs decode to owned bytes. External images are resolved and read
before cache lookup, so a new reference reads current file contents.

The decoder accepts PNG and JPEG. It uses a declared MIME type or detects the
format from the file signature, applies image size and allocation limits, and
produces immutable RGBA8 pixels. The shipped images are noninterlaced 1024 ×
1024 8-bit RGBA PNGs with no explicit ICC, sRGB or gamma chunk. The viewport
uses their existing RGB byte values for glTF base color. It samples a
nearest texel using the primitive UV, combines the sample with material
and vertex factors and existing normal-based shading, then packs RGB into
the vertex lane. Each shader receives those packed colors. Vulkan and
D3D11 upload colored vertex/index buffers for this path, so the measured
Last Watch import has no material texture upload. RGBA8 decode still
preserves source alpha. The tactical palette path uses opaque materials.

| Encoded source | Bytes | SHA-256 prefix | Consumers in the art manifest |
| --- | ---: | --- | --- |
| Knight | 12,914 | `028db4d1cd8b` | Knight, sword, shield |
| Rogue | 15,026 | `2e2fc741e20e` | Rogue, dagger |
| Mage | 14,740 | `9ef9672e93b2` | Mage, staff |
| Ranger | 13,248 | `639cd0ba5106` | Ranger, shared by both guards |
| Barbarian | 14,424 | `f11434d3e5e4` | Barbarian |
| Dungeon | 17,047 | `ceb331d6ed53` | Floor, wall, pillar, chest |

The five character images are distinct. The three equipment PNG files
match their corresponding embedded images byte for byte. The M11
loader reused four external paths but handled embedded images separately.
M12 uses the actual encoded bytes and effective decoder format as the
identity for a cache entry. The conversion contract is RGBA8. The
cache checks byte equality before returning an `Arc` to immutable pixels,
including across GLB/external references. This preserves exact colors
even when several sources look similar. Source assets retain their
original hashes.

The cache belongs to one asset import. Its retained payload is capped
at 64 MiB and 128 entries. Eviction releases the cache reference and
live models retain their own references. A source error reports the
model path and image index. Fresh preview preparation rereads sources.
The in-preview Restart command deliberately continues using its mounted
art when the game art manifest is the same. Close and reopen the
preview to start a fresh import of edited source assets.

For the matched room, 13 image references comprise 194,134 encoded
bytes. Six distinct decoded images occupy 25,165,824 bytes and the
three removed duplicate decodes used another 12 MiB of RGBA8 data
in the old path. The six unique encoded payloads total 87,399 bytes.
After hash inspection and optimized release profiling,
there was little justification for a persistent decoded-image cache.
It would replace compressed PNG reads with roughly 24 MiB of derived
payload for this project. The art and shader caches are in-memory.

## Scheduling and ownership

The preview uses GPUI's existing background executor for staging and
starting the game process and preparing an owned art bundle. The
preview polls task completion without waiting and keeps gameplay paused
until the required art and Vulkan compositor frame are ready. A pose is
still evaluated on the CPU using the simulation clock. The preparation
task checks cancellation between models, clips and CPU poses. Stop,
restart, and preview disposal drop the outstanding task and only the
current request can publish. An in-progress synchronous system call
can finish before its dropped task releases resources.

The Windows compositor reads the DXGI LUID on its owner thread. The
producer creates the matching standalone Vulkan device, graphics
pipelines and static uploads on the background executor. GPUI adopts
the finished session on its renderer thread. D3D11 texture creation,
NT handle creation, Vulkan import, the keyed-mutex handoff, depth
attachments, queue submission and composition remain on the owner
thread. The worker receives no HWND, D3D11 context or shared texture
handle. Art errors stop the failed game request. The existing D3D11
fallback rebuilds the CPU art on the worker when the Vulkan producer
fails.

DXC outputs for both the room and skin pass are reused within each
Vulkan producer. The shader modules and graphics pipelines are owned
by their device/session. The workload still uses a null explicit
Vulkan pipeline cache. A persisted device-specific cache would be a
separate choice and was not needed for these timings.

## Diagnostic startup measurements

Set `WYRMCOIL_VULKAN_STARTUP=1` to record timed API scopes and their
thread IDs. The engine emits `wyrmcoil_vk` records to stderr and GPUI
emits the D3D11 records to the Zed log. Each API call records a begin
and end. The first readiness observation, compositing handoff and
successful DXGI Present include time since the preceding milestone.
Present success measures submission to DXGI. Physical display timing
requires separate telemetry. `WYRMCOIL_GPU_TIMING=1` enables the existing
Vulkan GPU timestamp queries for actual draw execution.

Sixteen fresh native test processes ran the same indexed-room probe
on the RTX 3070 with driver 610.88, with precompiled shader artifacts
when the timed room build began. Each process first checked a wrong
LUID and then built a correct room, rendered at two sizes, verified
depth and ran the compute proof on its same device. Driver cache
conditions were left at their existing defaults. Times are wall
milliseconds for API/host scopes. The fence column is submit to
successful observation.

| Validation | Attempts | Full room build min / median / max | Instance median | Logical device median | Submit-to-fence median |
| --- | ---: | ---: | ---: | ---: | ---: |
| Off | 8 | 73.75 / 85.03 / 105.37 | 37.03 | 37.90 | 0.71 |
| Requested | 8 | 87.03 / 97.07 / 104.91 | 45.09 | 43.08 | 1.43 |

| Additional stage | Off median ms | Requested median ms |
| --- | ---: | ---: |
| Loader | 1.19 | 1.49 |
| Adapter enumeration | 4.85 | 5.15 |
| LUID match and property check | 0.057 | 0.076 |
| Capabilities | 0.14 | 0.13 |
| Queue retrieval | 0.028 | 0.059 |
| Room buffers and initial upload | 0.29 | 0.53 |
| Command pool and command buffer | 0.11 | 0.18 |
| Fence creation | 0.038 | 0.058 |
| Shader modules | 0.043 | 0.41 |
| Graphics pipeline | 0.39 | 0.65 |
| Compute proof pipeline | 0.13 | 0.18 |
| External color memory import | 0.080 | 0.126 |
| Depth resource | 0.77 | 0.77 |
| First queue call | 0.106 | 0.248 |

The M11 8.68 s Vulkan startup stall remains unresolved. Its old timer
covered producer construction, before Win32 shared-texture import and
first presentation. In the sixteen native repeats, the rare slow
graphics pipeline was 22.42 ms and the rare slow depth resource was
32.89 ms. These smaller outliers occurred with validation off. The
complete traces are in `target/m12/vulkan-repetitions.log`.

Sixteen additional fresh processes built the matching device before
testing the invalid LUID. This avoids priming Vulkan instance creation
in the same process. First-device startup was measurably slower.
The median instance call rose from 37–45 ms to 96–99 ms. The samples
do not establish the cause of the longer M11 event.

| First device | Attempts | Room build min / median / max | Instance median | Logical device median |
| --- | ---: | ---: | ---: | ---: |
| Validation off | 8 | 116.46 / 135.31 / 181.12 ms | 99.48 ms | 30.59 ms |
| Validation requested | 8 | 124.60 / 133.67 / 149.01 ms | 96.36 ms | 29.88 ms |

The unprimed traces are in `target/m12/vulkan-first-device.log`.

The live handoff keeps timeout-zero keyed-mutex acquisition, nonblocking
Vulkan fence checks and D3D11 `DONOTFLUSH` copy query polling. Vulkan
has one in-flight submission per room. Teardown retains the existing
wait for that submission before destroying GPU resources. The
diagnostics distinguish API call time from the time until the next
editor frame observes completion.

## Matched asset profiling

The benchmark exercises 13 authored models and the CPU color-preparation
pass. Animation clip decoding, pose generation and graphics device
initialization are outside this headless scope. The baseline importer
comes from `305b246`, whose image path is also the M11 image path. Both
versions used byte-identical assets and separate build artifacts. Each
process ran six fresh imports. The first column for each profile
includes its first process sample. The median covers the following
five fresh imports with warmer OS state. The OS cache was left intact
throughout. The desktop was also building Zed during the suite.

| Profile and importer | First-process sample | Next five median (range) | Image stage median | Warm model lookups median |
| --- | ---: | ---: | ---: | ---: |
| Debug baseline | 1,187 ms | 1,155 ms (1,138–1,162) | 1,079 ms | 2.84 ms |
| Debug M12 | 166 ms | 133 ms (125–141) | 37.0 ms | 3.52 ms |
| Release baseline | 55.1 ms | 55.7 ms (55.0–57.1) | 38.3 ms | 3.43 ms |
| Release M12 | 48.8 ms | 49.9 ms (43.1–52.8) | 29.8 ms | 4.59 ms |

In the M12 rows, the debug/release medians were 32.9/26.1 ms for
decode including decoder allocations, 1.13/1.18 ms for external image
reads, 0.002/<0.001 ms for embedded image slicing, 0.001/0.001 ms for
RGBA8 conversion, and 0.084/0.053 ms for retaining and copying cache
keys. The RGBA8 source conversion transfers the decoded allocation.
Format detection and cache lookup were each below 0.05 ms.
Main-source reads took 2.84/2.41 ms, glTF parsing 5.62/1.51 ms,
buffer handling 3.68/4.66 ms, mesh extraction 62.64/3.28 ms,
skin/inverse binds 0.25/0.03 ms, and the fused UV sample, shade and
color-pack pass 9.76/2.05 ms. Warm model timings include path resolution
and lookup of existing CPU model records.

After finishing the Zed builds and closing the qualification windows,
the same four isolated binaries ran serially again with the OS cache
left intact. Each row uses a new process followed by five more fresh
imports in that process. Raw samples are in `target/m12/idle-*.log`.

| Profile and importer | First-process sample | Next five median (range) | Image stage median | Warm model lookups median |
| --- | ---: | ---: | ---: | ---: |
| Debug baseline | 781.2 ms | 783.5 ms (781.4–785.5) | 727.7 ms | 1.87 ms |
| Debug M12 | 77.7 ms | 76.7 ms (76.0–78.6) | 23.4 ms | 1.91 ms |
| Release baseline | 38.7 ms | 36.9 ms (36.5–37.8) | 27.9 ms | 1.77 ms |
| Release M12 | 29.2 ms | 28.9 ms (28.5–29.6) | 19.7 ms | 1.84 ms |

Under those conditions, the median debug mesh extraction took 36.9 ms
and the fused palette-color pass took 5.57 ms. The corresponding
release values were 2.02 ms and 0.83 ms.

Before changing the debug profile, the release CLI decoded the five
embedded character PNGs in about 15 ms. A scoped debug experiment
setting PNG and fdeflate to opt-level 3 took about 22 ms for the same
five images while other crates retained the debug profile. These
measurements drove the choice to optimize the existing decoder crates.
The release improvement comes from eliminating the three duplicate
decodes. No alternate decoder, reduced-resolution assets or derived
disk cache was used.

## Desktop qualification and runtime

The final debug Zed run showed the startup state, accepted a switch
to the source tab and the save shortcut, and presented all six
Vulkan-skinned actors after returning. A warm restart showed tick
zero and reused the mounted art. Preview close retired the GPU
session. During the forced missing-DXC test, the preview reported the
exact shader tool error, prepared the CPU art and ran all six models
on D3D11. This exercised producer failure and fallback on the live
path. Separate deterministic tests exercised request cancellation
and rejected a stale result. No physical device-loss test was run.
The matching M11 and M12 debug screenshots have identical captured
RGB pixels in the 550 × 456 crop containing the party and both
ordinary guards. The compared files are `target/m11-gui/01-roster.jpg`
and `target/m12/05-final-debug-roster.jpg`.

Release Zed loaded all six Vulkan-skinned actors and their equipment,
queued a movement order at tick zero, and executed it after resume.
Tactical pause stopped the game again at tick 149 with the order
consumed. Guardhouse ran in its own preview on Vulkan. The authored
training-room preview recognized its compiled artifact and rendered
on Vulkan. The adapter regression suite also exercised changed,
missing, truncated and recovered scene artifacts. A separate release
session explicitly selected D3D11, loaded the six actors using cached
CPU poses and resumed combat. The dedicated Zed processes and game
children were gone after quitting. Desktop captures are
`target/m12/08-release-roster.jpg`, `target/m12/09-release-paused.jpg`,
`target/m12/10-release-guardhouse.jpg`,
`target/m12/11-release-training-room.jpg` and
`target/m12/12-release-d3d11-roster.jpg`.

| Debug desktop observation | Measured |
| --- | ---: |
| Fresh full-GPU art preparation, three observed loads on final build | 108–131 ms |
| M11 matched debug full-GPU art reference | 572 ms |
| One Vulkan mount before first frame | 397 ms |
| Same producer after tab switch, cached SPIR-V, new Vulkan session | 160 ms |
| Mounted-art restart, same render session | 136 ms |
| D3D11 automatic recovery, CPU art preparation | 2.09–2.66 s |
| D3D11 automatic recovery, full mount before first frame | 2.43–3.00 s |

Debug desktop runs share the machine with the Windows build. Earlier
M12 debug work logged a 289 ms art load and a 9.10 s warm game mount.
The latter showed an art-cache hit and no new Vulkan producer build.
The final build also stages and spawns the game on the worker.
Its debug trace timed that step at 105 ms for a fresh mount and
101 ms for the timed warm restart.

One final debug Vulkan tactical session, with the validation layer
off and the DXC outputs uncached, built in 858 ms. Timestamped
subscopes included 380 ms in `vkCreateInstance`, 46 ms in
`vkCreateDevice`, 183 ms in room DXC and 122 ms in skin DXC.
The worker used thread 21. Interop and presentation were on thread 1.
The D3D11 shared texture and NT handle calls took 0.37 ms and
0.038 ms, respectively. The Vulkan allocation import call took
0.81 ms, within the 11.57 ms shared-image attachment scope.
From submit return to first successful fence observation took
11.63 ms, and to compositor handoff took 12.30 ms. The corresponding
keyed-mutex calls took 0.10 ms each. DXGI accepted the frame 0.25 ms
after the handoff, with 0.095 ms spent in the API call. The first
D3D11 copy query was observed complete 10.93 ms after the handoff.
The turn's first debug run with the validation layer requested had
a 778 ms tactical producer build and 402 ms instance creation.
An earlier built-in producer started concurrently in that run.
The final game-loading view avoids building that extra room while
a new game is staging.

The release Last Watch run measured 36.82 ms for fresh art preparation
and 136.01 ms to mount game state and art. The Vulkan producer completed
in 1.871 s on its worker. Room and skin DXC took 763 and 725 ms,
the instance took 243 ms, and the logical device took 37 ms. Guardhouse
and the training room built their Vulkan sessions in 813 and 851 ms,
respectively. Their room DXC steps took 670 and 732 ms. For release
Last Watch, the first fence observation, compositor handoff and DXGI
acceptance were 7.05 ms, 7.49 ms and 7.65 ms after submit returned.
The keyed-mutex acquire and release API calls took 0.086 and 0.185 ms.
No validation errors appeared in the release Vulkan trace with the
validation layer requested. The separate release D3D11 run prepared
CPU art in 81.77 ms and mounted the game in 189.54 ms. These are
individual desktop observations. The matched import comparison above
uses repeated samples.

The desktop compositor trace is in `target/m12/desktop-evidence.log`.
The Vulkan producer API records for the release run are in
`target/m12/gui-release-stderr.log`.

The M11 full-GPU draw data remains 1,897,534 static vertex/index
bytes, plus the 9,344-byte maximum palette. M12 adds a transient
CPU cache during import and avoids the duplicate decoded payload
described above. GPU memory residency was unmeasured. The final
debug render scope averaged about 134 µs on the GPU across 1,739
timestamped draws, including skinning and raster work. A six-second
patrol sample used 101% of one logical core, 840 MiB working set and
332 MiB private bytes for Zed. After closing the preview the measured
values were 9.1%, 779 MiB and 267 MiB. The CPU work and working
set samples are observational. The simulation still uses its 60 Hz
clock. No display FPS measurement was collected.

| Regression lane | Result |
| --- | --- |
| Root Cargo suite | 421 passing tests in both debug and release |
| Zed adapter | 30 passed, including the request cancellation and replacement test |
| Vulkan hardware | 2 passed in each profile, plus 32 startup probe repetitions |
| D3D11 hardware viewport | 2 passed |
| Shader conformance | 8 opt-in tests passed |
| Last Watch and Guardhouse | 5 and 2 Cargo unit tests in both profiles, plus both real-process tests |
| CLI playtests and replay | Last Watch 14 assertions, Guardhouse 9, with both debug and release game executables |
| Naming policy | Eight existing findings, with none added |

The optimized game playtest reports are under
`target/m12/playtest-last-watch-release/` and
`target/m12/playtest-guardhouse-release/`.

The Windows GPUI overlay touches two files, with 129 additions and
25 removals. The build uses existing project dependencies. Both the
application and CLI linked in debug and optimized `release-fast`.
The release build used the existing pinned WebRTC release payload
via `LK_CUSTOM_WEBRTC` after the first offline attempt could not fetch
it. The WPR dependency's binding generator also needed write
permission for the Cargo source cache.

## Reproduction

```powershell
$env:WYRMCOIL_DXC = 'C:/VulkanSDK/1.4.350.0/Bin/dxc.exe'
$env:WYRMCOIL_VULKAN_STARTUP = '1'
$env:WYRMCOIL_GPU_TIMING = '1'
$env:VK_INSTANCE_LAYERS = 'VK_LAYER_KHRONOS_validation'
# Add the Vulkan SDK Bin folder to PATH for the hardware tests.
cargo test --locked --test m12_asset_profile -- --ignored --nocapture
cargo test --locked --release --test m12_asset_profile -- --ignored --nocapture
cargo test --locked --lib Engine::render::backend::vulkan::tests -- --ignored --nocapture
./scripts/build-zed.ps1
./scripts/build-zed.ps1 -Profile release-fast
```

The pinned Zed revision is `a246423a3b24a08e0d0c8574eab3a9de422747e0`.
Root release uses standard Cargo opt-level 3. The Zed qualification
release profile is upstream `release-fast` (opt-level 3, no LTO),
with debug symbols disabled for the build. Both debug builds optimize
the existing PNG and fdeflate crates at level 3. Other debug settings
remain at the normal profile values. Existing checkout build
prerequisites apply.

## M13 handoff

Keep the startup trace available for the next reported stall. The
first-device probes show that `vkCreateInstance` varies with process
state. The release desktop run measured the largest delay in uncached
DXC calls. Repeat fresh desktop previews with and without existing
shader artifacts under otherwise idle conditions before deciding
whether to persist SPIR-V across application restarts. The room
pipeline API call itself was usually below one millisecond in the
native probe. On the asset side, debug mesh extraction is the next
measured cost to examine. The matched release image stage is about
20 ms for the full set without a parallel build.
