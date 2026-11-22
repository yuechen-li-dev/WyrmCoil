#Requires -Version 7.2
param(
    [switch]$PrepareOnly
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$zed_revision = 'a246423a3b24a08e0d0c8574eab3a9de422747e0'
$zed_url = 'https://github.com/zed-industries/zed.git'
$repo_root = Split-Path -Parent $PSScriptRoot
$integration_root = Join-Path $repo_root 'integrations/zed'
$zed_root = Join-Path $repo_root 'vendor/zed'

function Invoke-ZedGit {
    & git -C $zed_root -c core.longpaths=true @args
    if ($LASTEXITCODE -ne 0) {
        throw "Zed git $($args[0]) failed (exit $LASTEXITCODE)."
    }
}

if (!(Test-Path -LiteralPath $zed_root)) {
    New-Item -ItemType Directory -Path $zed_root | Out-Null
}
if (!(Test-Path -LiteralPath (Join-Path $zed_root '.git') -PathType Container)) {
    if (Get-ChildItem -LiteralPath $zed_root -Force | Select-Object -First 1) {
        throw "$zed_root is not an empty directory or a managed Zed checkout."
    }
    Invoke-ZedGit init
    Invoke-ZedGit config core.longpaths true
    Invoke-ZedGit config core.autocrlf false
}

$head = & git -C $zed_root rev-parse --verify --quiet HEAD
if ($LASTEXITCODE -eq 1) {
    Invoke-ZedGit fetch --depth 1 $zed_url $zed_revision
    Invoke-ZedGit checkout --detach FETCH_HEAD
    $head = Invoke-ZedGit rev-parse HEAD
} elseif ($LASTEXITCODE -ne 0) {
    throw "Could not inspect the Zed checkout at $zed_root."
}
if ($head -ne $zed_revision) {
    throw "Expected Zed revision $zed_revision, found $head at $zed_root."
}

$diff_args = @('--no-ext-diff', '--no-textconv', '--no-color', '--no-renames', '--full-index',
    '--src-prefix=a/', '--dst-prefix=b/', '--diff-algorithm=myers', '--unified=3')

function Get-ZedPatchWithOverlays([string]$base, [string[]]$overlays, [string[]]$paths) {
    $saved_index = $env:GIT_INDEX_FILE
    $index_path = Join-Path ([IO.Path]::GetTempPath()) "wyrmcoil-zed-$([Guid]::NewGuid()).index"
    try {
        $env:GIT_INDEX_FILE = $index_path
        Invoke-ZedGit read-tree HEAD
        Invoke-ZedGit apply --cached $base
        foreach ($overlay in $overlays) {
            Invoke-ZedGit apply --cached $overlay
        }
        (Invoke-ZedGit diff @diff_args --cached HEAD -- @paths) -join "`n"
    } finally {
        $env:GIT_INDEX_FILE = $saved_index
        if (Test-Path -LiteralPath $index_path) {
            Remove-Item -LiteralPath $index_path
        }
    }
}

function Apply-ZedPatch([string]$name, [string[]]$paths, [string[]]$previous, [string[]]$overlays) {
    $patch_path = Join-Path $integration_root "patches/$name.patch"
    $expected_patch = (Get-Content -LiteralPath $patch_path) -join "`n"
    $current_patch = (Invoke-ZedGit diff @diff_args HEAD -- @paths) -join "`n"
    $overlay_paths = @($overlays | ForEach-Object { Join-Path $integration_root "patches/$_.patch" })
    $applied_count = 0
    if ($current_patch) {
        for ($count = $overlay_paths.Count; $count -gt 0; $count--) {
            $prefix = $overlay_paths[0..($count - 1)]
            if ($current_patch -ceq (Get-ZedPatchWithOverlays $patch_path $prefix $paths)) {
                $expected_patch = $current_patch
                $applied_count = $count
                break
            }
        }
    }
    if (!$current_patch) {
        Invoke-ZedGit apply --check $patch_path
        Invoke-ZedGit apply $patch_path
    } elseif ($current_patch -cne $expected_patch) {
        $previous_path = $null
        foreach ($candidate in $previous) {
            $candidate_path = Join-Path $integration_root "patches/$candidate.patch"
            if ($current_patch -ceq ((Get-Content -LiteralPath $candidate_path) -join "`n")) {
                $previous_path = $candidate_path
                break
            }
        }
        if (!$previous_path) {
            throw "The tracked changes for $name in $zed_root differ from the integration patch."
        }
        Invoke-ZedGit apply --reverse $previous_path
        try {
            Invoke-ZedGit apply --check $patch_path
            Invoke-ZedGit apply $patch_path
        } catch {
            Invoke-ZedGit apply $previous_path
            throw
        }
    }
    for ($index = $applied_count; $index -lt $overlay_paths.Count; $index++) {
        $overlay_path = $overlay_paths[$index]
        Invoke-ZedGit apply --check $overlay_path
        Invoke-ZedGit apply $overlay_path
    }
}
Apply-ZedPatch 'm0' @('.', ':(exclude)crates/gpui', ':(exclude)crates/gpui_windows') @() @('m6v-workspace', 'm7-workspace', 'm8-workspace', 'm9-workspace')
Apply-ZedPatch 'm4' @('crates/gpui', 'crates/gpui_windows') @('m3b', 'm3a') @('m6v-gpu', 'm8-gpu')

$crate_path = 'crates/wyrmcoil_view'
$crate_root = Join-Path $zed_root $crate_path
New-Item -ItemType Directory -Force -Path (Join-Path $crate_root 'src') | Out-Null
foreach ($file in 'Cargo.toml', 'src/wyrmcoil_view.rs', 'src/viewport.sdslv', 'src/room.rs', 'src/room.sdslv', 'src/vulkan.rs', 'src/tactical.rs', 'src/tactical_art.rs', 'src/tactical.sdslv') {
    $source = Join-Path $integration_root "wyrmcoil_view/$file"
    $destination = Join-Path $crate_root $file
    if (!(Test-Path -LiteralPath $destination) -or
        (Get-FileHash -LiteralPath $source).Hash -ne (Get-FileHash -LiteralPath $destination).Hash) {
        Copy-Item -LiteralPath $source -Destination $destination
    }
}

Write-Host "Prepared Zed $zed_revision at $zed_root"
if ($PrepareOnly) {
    return
}

Push-Location -LiteralPath $zed_root
try {
    foreach ($binary in 'zed', 'cli') {
        cargo build --locked --package $binary --bin $binary
        if ($LASTEXITCODE -ne 0) {
            throw "Zed $binary build failed (exit $LASTEXITCODE)."
        }
    }
} finally {
    Pop-Location
}
