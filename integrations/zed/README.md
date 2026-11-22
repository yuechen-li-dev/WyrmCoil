# WyrmCoil Preview for Zed

Run the build script from the WyrmCoil checkout.

```powershell
pwsh -File scripts/build-zed.ps1
```

The script uses PowerShell 7.2+, Git, rustup, and the prerequisites for your platform. On Windows, install the MSVC x64 toolchain, matching Spectre-mitigated libraries, Windows SDK, and CMake. Make CMake available on `PATH`. [Pinned Zed development guide](https://github.com/zed-industries/zed/tree/a246423a3b24a08e0d0c8574eab3a9de422747e0/docs/src/development)

Use `-PrepareOnly` to fetch and patch Zed without running Cargo. The script stages the crate in `vendor/zed/crates/wyrmcoil_view` so it can inherit Zed's workspace settings. Edit the canonical crate here in `integrations/zed/wyrmcoil_view` and rerun the script to stage changes.

With the default Cargo target directory, launch `vendor/zed/target/debug/zed.exe` on Windows or `vendor/zed/target/debug/zed` on Linux and macOS. The build also puts Zed's development CLI next to the editor. From an open workspace, open Zed's command palette and search for `wyrmcoil: open preview`. The command opens or focuses the **WyrmCoil Preview** tab.

The revision and build settings are in the [M0 report](../../docs/zed-integration-m0.md). On Windows, `wyrmcoil: open preview` opens the 3D room. Use WASD to walk, the arrow keys to look, and Escape to pause. `wyrmcoil: open planar preview` opens the original M3b demo. `WYRMCOIL_GPUI_MARKER` selects its square markers for comparison. [M4 room integration report](../../docs/zed-integration-m4.md)

The Windows 3D preview uses Vulkan 1.3 with DXC when the editor's adapter supports shared BGRA8 textures. Put DXC on `PATH`, install it with the Vulkan SDK, or set `WYRMCOIL_DXC` to its full path before launching Zed. Set `WYRMCOIL_GRAPHICS_BACKEND=d3d11` to select the existing D3D11 renderer explicitly. The preview shows the active backend and any fallback reason. [M6V integration report](../../docs/zed-integration-m6v.md)

For an independent game, open the [Guardhouse project](../../examples/games/guardhouse/README.md) on its own. Run **Build Guardhouse Game**, then use `wyrmcoil: run game`, `wyrmcoil: stop game`, and `wyrmcoil: restart game` in the command palette. These actions use the same 3D viewport. [M7 execution report](../../docs/zed-integration-m7.md)
