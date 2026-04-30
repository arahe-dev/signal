param(
    [string]$Name = "Start Signal"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$distSignal = Join-Path $repoRoot "dist\signal"
$startScript = Join-Path $distSignal "scripts\start_release.ps1"

if (-not (Test-Path $startScript)) {
    throw "Missing $startScript. Run scripts\build_release.ps1 first."
}

$desktop = [Environment]::GetFolderPath("Desktop")
$shortcutPath = Join-Path $desktop "$Name.lnk"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = "powershell.exe"
$shortcut.Arguments = "-ExecutionPolicy Bypass -File `"$startScript`" -RunDoctor"
$shortcut.WorkingDirectory = $distSignal
$shortcut.IconLocation = Join-Path $distSignal "signal-daemon.exe"
$shortcut.Save()

Write-Host "Created shortcut:"
Write-Host "  $shortcutPath"
