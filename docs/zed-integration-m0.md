# WyrmCoil M0 in Zed

## Compatibility audit

| Area | Finding |
| --- | --- |
| Revisions | WyrmCoil baseline `3e57fe9b176879122485d611d87bd91245fccf7e`. Zed upstream `a246423a3b24a08e0d0c8574eab3a9de422747e0`, inspected at its upstream HEAD on November 19, 2026. |
| Cargo | Zed uses an explicit member list, workspace dependency aliases, resolver 2, and `crates/zed` as its default member. Both projects use Rust 2024. Zed pins stable Rust 1.98.1, installed by rustup. WyrmCoil has no toolchain pin and its baseline tests ran with Rust 1.95.0. |
| Application startup | `crates/zed/src/main.rs` initializes features inside the GPUI application callback, including `image_viewer::init` immediately after the editor. Zed owns window creation and application startup. |
| Workspace item | `image_viewer` registers a project-backed image item and its serializer. Zed's in-tree `theme_preview` shows how a transient pane item can use `Item`, `Focusable` and `Render`, with a command installed through `App::observe_new` and `Workspace::register_action`. M0 uses that transient pattern. |
| Path dependency | The script stages the integration crate into `vendor/zed/crates/wyrmcoil_view` to inherit Zed's workspace dependencies. Its engine path points back to WyrmCoil's root package. Cargo path dependencies can belong to a separate workspace and retain their own workspace inheritance, including WyrmCoil's Margaret path dependencies. Zed resolves its own lockfile. |
| Graphics | WyrmCoil resolves crates.io `wgpu 24.0.5` and `winit 0.30.13`. Zed pins git `wgpu 30.0.0` at `cddb2f5d80dc0009b2ef6767635140cacf90a007` for its wgpu backend. The inspected Windows GPUI source uses DirectX. M0 crosses the application/view boundary with no GPU objects. |
| Platform build | Use Cargo from the pinned Zed root so rustup and upstream `.cargo/config.toml` select the toolchain and native Windows flags, including the static CRT. Windows uses x64 MSVC, the Windows SDK and CMake per the upstream development guide. Linux and macOS can use the same PowerShell 7 checkout and Cargo steps with their respective Zed prerequisites. |
| Aurelian reference | Aurelian's dependency policy keeps editor and backend plumbing outside the engine's runtime contracts. The Zed adapter follows that ownership boundary. |

[Zed workspace](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/Cargo.toml) · [Windows build guide](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/docs/src/development/windows.md) · [Aurelian dependency policy](https://github.com/yuechen-li-dev/copeland/blob/e0f7d61bf90f8d0579b1fcb60f41e7c6bf1bb1a8/docs/Aurelian/architecture/dependency-policy.md)

## Architecture and reproduction

WyrmCoil owns the canonical adapter in `integrations/zed/wyrmcoil_view`. The build script shallow-fetches the pinned upstream commit into the ignored `vendor/zed` directory, applies one checked patch, and copies the adapter into the Zed workspace. It accepts an already prepared checkout with the same patch and rejects a different revision or tracked diff. The patch includes the integrated Cargo lockfile so the application build can use `--locked`.

The script manages the upstream checkout directly. WyrmCoil can track the pin, the adapter, and the patch together without putting Zed's source or a downstream Zed history in the repository.

The adapter registers `wyrmcoil::OpenPreview` for each new Zed workspace. The command opens an inert pane item or focuses the existing preview in that workspace. The tab label uses the engine's existing `wyrmcoil::ProjectName()` API. Zed owns focus, pane placement, and tab closure.

The adapter declares GPL-3.0-or-later, matching the Zed application crate. The engine package declares MIT.

The WyrmCoil build inputs are committed at `1976153c29e8bdd637fcfe6491437f5432536811`, based on `3e57fe9b176879122485d611d87bd91245fccf7e`. The pinned Zed revision is `a246423a3b24a08e0d0c8574eab3a9de422747e0`.

Run the script from the WyrmCoil root with PowerShell 7.2+, Git, rustup, and the pinned platform prerequisites installed.

```powershell
pwsh -File scripts/build-zed.ps1 -PrepareOnly
pwsh -File scripts/build-zed.ps1
```

On Windows, make CMake available on `PATH` and use MSVC x64 with the Windows SDK and matching Spectre-mitigated libraries. The qualification host used Visual Studio 2026 Community, MSVC 14.51.36231, and SDK 10.0.26100.0. The build used Zed's target flags and default application features. Use the following Cargo settings to match the qualification build's resource use.

```powershell
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_DEV_INCREMENTAL = 'false'
$env:CARGO_BUILD_JOBS = '6'
pwsh -File scripts/build-zed.ps1
```

`ZED_RC_TOOLKIT_PATH` pointed to the SDK's x64 tools and Cargo used Git CLI fetching with long paths enabled. The host's VS installation lacked the Spectre component and the quiet installer required elevation. The retry used the matching `Microsoft.VC.14.51.CRT.x64.Desktop.spectre.base.vsix` from the Visual Studio catalog. Its verified SHA-256 is `d806ef641124a6ace9cf5c7f8eaf3b83730c8d79dd67177b55accd572361029d`. The libraries were staged alongside a copy of the installed x64 compiler tools under the ignored `target/zed-msvc` directory and selected in the developer shell for this build.

The default artifact path is `vendor/zed/target/debug/zed.exe` on Windows and `vendor/zed/target/debug/zed` on Linux and macOS. The script also builds the sibling `cli.exe` or `cli` needed by Zed's development setup for features such as SSH askpass. In an open workspace, search the command palette for `wyrmcoil: open preview`.

## Integration costs

| Measure | Count |
| --- | ---: |
| Adapter Rust | 66 lines |
| Adapter manifest | 18 lines |
| PowerShell build script | 82 lines |
| Total new integration code and manifest | 166 lines |
| Upstream application source | 1 added line in 1 file |
| Upstream application and workspace manifests | 3 added lines in 2 files |
| Generated upstream Cargo lockfile delta | 783 added, 90 removed |
| Total changed upstream files and lines | 4 files, 787 added and 90 removed |

The Zed app adds one direct dependency, `wyrmcoil_view`. The adapter depends directly on Zed's existing `gpui` and `workspace` members and on the existing local `wyrmcoil` package. The integrated lockfile preserves every original Zed package identity and adds 52 identities across targets, including the adapter and two WyrmCoil packages. Many dependency entries gain version qualifiers. Cargo also unifies some compatible edges, including `windows-sys` references from several existing packages to the already-locked 0.61.2. The lockfile contains both wgpu generations. The Windows application dependency graph includes WyrmCoil's wgpu 24.0.5, while GPUI uses DirectX on that target.

## Validation

| Check | Result |
| --- | --- |
| Checkout and patch | Fresh local clone at the WyrmCoil integration commit. Git checked the patch out with LF on Windows. The script fetched the exact Zed SHA as a shallow detached checkout and accepted the prepared workspace on a second run. Separate probes confirmed rejection of a different Zed HEAD and an extra tracked edit. |
| WyrmCoil regression suite | `cargo test` passed on Windows with 395 passes and three existing skips. |
| WyrmCoil formatter policy | `cargo run --bin wyrmfmt -- check --lang rust src tests examples` exited 1 before and after integration. Both runs reported the same four WGSL entry-point names embedded in Rust and four fixture names inside `src/wyrmfmt.rs`. |
| Adapter formatting and workspace membership | `cargo fmt --check -p wyrmcoil_view` passed. Cargo metadata identified the adapter as a Zed workspace member. The clean clone resolved the locked dependency tree offline and found its own WyrmCoil checkout. |
| Windows application build | The locked script built the editor and its CLI. The successful retry took 2m 00s with intermediate artifacts available. |
| Windows clean rebuild | Ran the script from a fresh clone at the integration commit with an empty Cargo target directory, four Cargo jobs, and populated package caches. The script fetched and patched Zed, built the editor in 10m 55s, and built the CLI in 1m 38s. Total script time was 12m 40s. The clean-build editor returned the same preview action record and the CLI identified the same Zed revision. |
| Linked action registry | `zed.exe --dump-all-actions` returned exactly one `wyrmcoil::OpenPreview` entry, with palette name `wyrmcoil: open preview`. `cli.exe --version` identified the pinned Zed commit. |
| Windows GUI smoke | Launched the built editor with isolated user data and a two-file project. Opened `alpha.txt` from the project panel, typed and saved a line, and checked the saved bytes. Found and opened the preview through the command palette. Opened `beta.txt` from the project panel with the preview open. Running the command again focused the same preview. Ctrl+W closed the preview and returned focus to `beta.txt`. |
| Platform coverage | Windows x64 MSVC build and desktop smoke. Linux and macOS build and launch remain unqualified. |

The GUI-qualified development artifacts are in `vendor/zed/target/debug`.

| Artifact | Size in bytes | SHA-256 |
| --- | ---: | --- |
| `zed.exe` | 504,177,664 | `4a6400723cd717dcb7580e1d25bd991f60cb229234509ca9b3f4f8c4bde78082` |
| `cli.exe` | 8,021,504 | `517807eede738ccca4b821deceaccb366ad3da18618480a8f1652aacbf57b265` |

The clean rebuild used `target/zed-m0-cold-build` for its outputs. Local evidence includes `target/zed-build-final.log`, `target/zed-build-cold.log`, `target/zed-m0-action.json`, `target/zed-m0-cold-action.json`, `target/zed-m0-smoke/project/alpha.txt`, `target/wyrmcoil-m0-tests.log`, and `target/wyrmcoil-m0-wyrmfmt.log`.

## M1 handoff

Use the preview item's lifetime to own a read-only WyrmCoil session or snapshot handle. Establish how it is acquired, refreshed, and released across tab close and workspace teardown while keeping simulation ownership with the engine. Qualify that data boundary before considering a GPU presentation path.
