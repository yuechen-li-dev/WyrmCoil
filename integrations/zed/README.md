# WyrmCoil Preview for Zed

From the WyrmCoil checkout, run:

```powershell
pwsh -File scripts/build-zed.ps1
```

The script uses PowerShell 7.2+, Git, rustup, and the platform prerequisites in the pinned [Zed development guide](https://github.com/zed-industries/zed/tree/a246423a3b24a08e0d0c8574eab3a9de422747e0/docs/src/development). On Windows, install the MSVC x64 toolchain, SDK, and CMake, then make CMake available on `PATH`.

Use `-PrepareOnly` to fetch and patch Zed without running Cargo. The script stages the crate in `vendor/zed/crates/wyrmcoil_view` so it can inherit Zed's workspace settings. Edit the canonical crate here in `integrations/zed/wyrmcoil_view` and rerun the script to stage changes.

With the default Cargo target directory, launch `vendor/zed/target/debug/zed.exe` on Windows or `vendor/zed/target/debug/zed` on Linux and macOS. The build also puts Zed's development CLI next to the editor. From an open workspace, open Zed's command palette and search for `wyrmcoil: open preview`. The command opens or focuses the **WyrmCoil Preview** tab.

See [the M0 report](../../docs/zed-integration-m0.md) for the revision, build settings, and qualification evidence.
