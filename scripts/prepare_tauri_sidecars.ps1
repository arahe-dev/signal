param(
    [string]$TargetTriple
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$binariesDir = Join-Path $repoRoot "src-tauri\binaries"

Set-Location $repoRoot

if (-not $TargetTriple) {
    $TargetTriple = (rustc --print host-tuple).Trim()
}

if (-not $TargetTriple) {
    throw "Unable to determine Rust target triple."
}

Write-Host "Building Signal sidecars for $TargetTriple..."
cargo build --release -p signal-daemon -p signal-cli -p signal-worker

if (-not (Test-Path $binariesDir)) {
    New-Item -ItemType Directory -Path $binariesDir | Out-Null
}

$sidecars = @("signal-daemon", "signal-cli", "signal-worker")
foreach ($name in $sidecars) {
    $source = Join-Path $repoRoot "target\release\$name.exe"
    if (-not (Test-Path $source)) {
        throw "Missing sidecar binary: $source"
    }
    $destination = Join-Path $binariesDir "$name-$TargetTriple.exe"
    Copy-Item -LiteralPath $source -Destination $destination -Force
    Write-Host "Prepared $destination"
}
