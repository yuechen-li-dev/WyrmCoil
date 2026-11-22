#Requires -Version 7.2
$ErrorActionPreference = 'Stop'
$cargo_output = Join-Path $PSScriptRoot 'target/cargo'
$suffix = if ($IsWindows) { '.exe' } else { '' }
$published = Join-Path $PSScriptRoot "target/wyrmcoil-game$suffix"

cargo build --locked --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') --target-dir $cargo_output --bin guardhouse
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$binary = Join-Path $cargo_output "debug/guardhouse$suffix"
$temporary = "$published.$([Guid]::NewGuid().ToString('N')).tmp"
try {
    [IO.File]::Copy($binary, $temporary)
    if (!$IsWindows) {
        & chmod +x $temporary
        if ($LASTEXITCODE -ne 0) { throw 'Could not make the published game executable.' }
    }
    [IO.File]::Move($temporary, $published, $true)
} finally {
    if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
}
Write-Host "Published $published"
