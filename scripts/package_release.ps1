param(
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$distRoot = Join-Path $repoRoot "dist"
$distSignal = Join-Path $distRoot "signal"
$zipPath = Join-Path $distRoot "signal-preview-windows.zip"
$hashPath = "$zipPath.sha256"

Set-Location $repoRoot

if (-not $NoBuild) {
    powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "build_release.ps1")
}

if (-not (Test-Path $distSignal)) {
    throw "Missing $distSignal. Run scripts\build_release.ps1 first or omit -NoBuild."
}

Remove-Item -LiteralPath $zipPath,$hashPath -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $distSignal "*") -DestinationPath $zipPath -Force

$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $zipPath).Hash
"$hash  signal-preview-windows.zip" | Set-Content -LiteralPath $hashPath -Encoding ASCII

Write-Host ""
Write-Host "Release package:"
Write-Host "  $zipPath"
Write-Host "SHA256:"
Write-Host "  $hash"
Write-Host "Checksum file:"
Write-Host "  $hashPath"
