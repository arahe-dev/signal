param(
    [switch]$SkipFmtCheck
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$distRoot = Join-Path $repoRoot "dist"
$distSignal = Join-Path $distRoot "signal"

Set-Location $repoRoot

if (-not $SkipFmtCheck) {
    Write-Host "Checking formatting..."
    cargo fmt --check
}

Write-Host "Running tests..."
cargo test

Write-Host "Building release binaries..."
cargo build --release

if (Test-Path $distSignal) {
    Remove-Item -LiteralPath $distSignal -Recurse -Force
}
New-Item -ItemType Directory -Path $distSignal | Out-Null
New-Item -ItemType Directory -Path (Join-Path $distSignal "docs") | Out-Null
New-Item -ItemType Directory -Path (Join-Path $distSignal "scripts") | Out-Null

Copy-Item -LiteralPath (Join-Path $repoRoot "target\release\signal-daemon.exe") -Destination $distSignal
Copy-Item -LiteralPath (Join-Path $repoRoot "target\release\signal-cli.exe") -Destination $distSignal
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination $distSignal
Copy-Item -LiteralPath (Join-Path $repoRoot "signal.config.example.json") -Destination $distSignal
Copy-Item -LiteralPath (Join-Path $repoRoot "docs\PHASE_B_MANUAL_TEST.md") -Destination (Join-Path $distSignal "docs")
Copy-Item -LiteralPath (Join-Path $repoRoot "docs\PHASE_C_RELEASE_CHECKLIST.md") -Destination (Join-Path $distSignal "docs")
Copy-Item -LiteralPath (Join-Path $repoRoot "scripts\start_release.ps1") -Destination (Join-Path $distSignal "scripts")

Write-Host ""
Write-Host "Release dist created:"
Write-Host "  $distSignal"
