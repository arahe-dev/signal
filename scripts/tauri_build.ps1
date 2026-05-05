$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

. (Join-Path $PSScriptRoot "set_rust_remap.ps1") -RepoRoot $repoRoot

powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\prepare_tauri_sidecars.ps1
tauri build
