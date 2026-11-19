# WyrmCoil M0 in Zed

## Compatibility audit

| Area | Finding |
| --- | --- |
| Revisions | WyrmCoil baseline `3e57fe9b176879122485d611d87bd91245fccf7e`. Zed upstream `a246423a3b24a08e0d0c8574eab3a9de422747e0`, inspected at its upstream HEAD on November 19, 2026. |
| Cargo | Zed uses an explicit member list, workspace dependency aliases, resolver 2, and `crates/zed` as its default member. Both projects use Rust 2024. Zed pins stable Rust 1.98.1, installed by rustup. WyrmCoil has no toolchain pin and its baseline tests ran with Rust 1.95.0. |
| Application startup | `crates/zed/src/main.rs` initializes features inside the GPUI application callback, including `image_viewer::init` immediately after the editor. Zed owns window creation and application startup. |
| Workspace item | `image_viewer` registers a project-backed image item and its serializer. Zed's in-tree `theme_preview` shows how a transient pane item can use `Item`, `Focusable` and `Render`, with a command installed through `App::observe_new` and `Workspace::register_action`. M0 uses that transient pattern. |
| Path dependency | The integration crate will be staged into `vendor/zed/crates/wyrmcoil_view` to inherit Zed's workspace dependencies. Its engine path points back to WyrmCoil's root package. Cargo path dependencies can belong to a separate workspace and retain their own workspace inheritance, including WyrmCoil's Margaret path dependencies. Zed resolves its own lockfile. |
| Graphics | WyrmCoil resolves crates.io `wgpu 24.0.5` and `winit 0.30.13`. Zed pins git `wgpu 30.0.0` at `cddb2f5d80dc0009b2ef6767635140cacf90a007` for its wgpu backend. The inspected Windows GPUI source uses DirectX. M0 crosses the application/view boundary with no GPU objects. |
| Platform build | Use Cargo from the pinned Zed root so rustup and upstream `.cargo/config.toml` select the toolchain and native Windows flags, including the static CRT. Windows uses x64 MSVC, the Windows SDK and CMake per the upstream development guide. Linux and macOS can use the same PowerShell 7 checkout and Cargo steps with their respective Zed prerequisites. |
| Aurelian reference | Aurelian's dependency policy keeps editor and backend plumbing outside the engine's runtime contracts. The Zed adapter follows that ownership boundary. |

Sources: [Zed workspace](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/Cargo.toml), [Windows build guide](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/docs/src/development/windows.md), [Aurelian dependency policy](https://github.com/yuechen-li-dev/copeland/blob/e0f7d61bf90f8d0579b1fcb60f41e7c6bf1bb1a8/docs/Aurelian/architecture/dependency-policy.md).

## Architecture and reproduction

WyrmCoil owns the canonical adapter in `integrations/zed/wyrmcoil_view`. The build script shallow-fetches the pinned upstream commit into the ignored `vendor/zed` directory, applies one checked patch, and copies the adapter into the Zed workspace. It accepts an already prepared checkout with the same patch and rejects a different revision or tracked diff. The patch includes the integrated Cargo lockfile so the application build can use `--locked`.

The adapter registers `wyrmcoil::OpenPreview` for each new Zed workspace. The command opens an inert pane item or focuses the existing preview in that workspace. The tab label uses the engine's existing `wyrmcoil::ProjectName()` API. Zed owns focus, pane placement, and tab closure.

From the WyrmCoil root, with PowerShell 7.2+, Git, rustup, and the pinned platform prerequisites installed:

```powershell
pwsh -File scripts/build-zed.ps1 -PrepareOnly
pwsh -File scripts/build-zed.ps1
```

On Windows, make CMake available on `PATH` and use MSVC x64 with the Windows SDK. The qualification host used Visual Studio 2026 Community and SDK 10.0.26100.0. The qualification build used `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_DEV_INCREMENTAL=false`, and six Cargo jobs to reduce resource use. It kept Zed's own target flags and default application features. `ZED_RC_TOOLKIT_PATH` pointed to the SDK's x64 tools and Cargo used Git CLI fetching with long paths enabled.

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

The Zed app adds one direct dependency, `wyrmcoil_view`. The adapter depends directly on Zed's existing `gpui` and `workspace` members and on the existing local `wyrmcoil` package. WyrmCoil's own workspace manifest and engine dependencies remain intact. The integrated lockfile preserves every original Zed package identity and adds 52 identities across targets, including the adapter and two WyrmCoil packages. Several dependency entries gain version qualifiers because Cargo now resolves multiple versions. The lockfile contains both wgpu generations. The GPUI renderer and WyrmCoil's rendering types stay on their respective sides of the adapter.

## Validation

| Check | Result |
| --- | --- |
| Checkout and patch | Fresh local WyrmCoil clone at the baseline commit with the M0 files overlaid. The script fetched the exact Zed SHA as a shallow detached checkout, applied the expected four-file patch, and reproduced the same diff on a second run. An intentionally changed Zed HEAD was rejected. |
| WyrmCoil regression suite | `cargo test` passed on Windows with 395 passes and three existing skips. |
| WyrmCoil formatter policy | `cargo run --bin wyrmfmt -- check --lang rust src tests examples` exited 1 on the baseline. The eight findings are four WGSL entry-point names embedded in Rust and four fixture names inside `src/wyrmfmt.rs`. M0 leaves those sources intact. |
| Adapter formatting and workspace membership | `cargo fmt --check -p wyrmcoil_view` passed. Cargo metadata identified the adapter as a Zed workspace member. The clean clone resolved the locked dependency tree offline and found its own WyrmCoil checkout. |

## M1 handoff

Use the preview item's lifetime to own a read-only WyrmCoil session or snapshot handle. Establish how it is acquired, refreshed, and released across tab close and workspace teardown while keeping simulation ownership with the engine. Qualify that data boundary before considering a GPU presentation path.
