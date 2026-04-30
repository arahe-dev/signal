param(
    [int]$Port,
    [string]$Token,
    [string]$DbPath,
    [string]$PublicBaseUrl,
    [switch]$NoTailscaleServe,
    [switch]$StopExisting
)

$ErrorActionPreference = "Stop"

$scriptDir = $PSScriptRoot
$distRoot = Split-Path -Parent $scriptDir
$exe = Join-Path $distRoot "signal-daemon.exe"
$configPath = Join-Path $distRoot "signal.config.json"
$examplePath = Join-Path $distRoot "signal.config.example.json"

if (-not (Test-Path $exe)) {
    throw "signal-daemon.exe not found at $exe. Run scripts\build_release.ps1 first."
}

if (-not (Test-Path $configPath)) {
    if (Test-Path $examplePath) {
        Copy-Item -LiteralPath $examplePath -Destination $configPath
        Write-Host "Created $configPath from signal.config.example.json"
        Write-Host "Edit token/public_base_url if needed, then rerun this script."
    } else {
        Write-Host "Missing signal.config.json and signal.config.example.json."
        Write-Host "Create signal.config.json next to signal-daemon.exe."
    }
}

$config = @{
    host = "127.0.0.1"
    port = 8791
    db_path = ".\signal_demo.db"
    token = "dev-token"
    require_token_for_read = $true
    enable_web_push = $true
    public_base_url = "https://your-device.your-tailnet.ts.net"
    tailscale_serve = $true
}

if (Test-Path $configPath) {
    $json = Get-Content $configPath -Raw | ConvertFrom-Json
    foreach ($name in @("host", "port", "db_path", "token", "require_token_for_read", "enable_web_push", "public_base_url", "tailscale_serve")) {
        if ($null -ne $json.$name) {
            $config[$name] = $json.$name
        }
    }
}

if ($Port) { $config.port = $Port }
if ($Token) { $config.token = $Token }
if ($DbPath) { $config.db_path = $DbPath }
if ($PublicBaseUrl) { $config.public_base_url = $PublicBaseUrl }
if ($NoTailscaleServe) { $config.tailscale_serve = $false }

Set-Location $distRoot

if (-not [System.IO.Path]::IsPathRooted([string]$config.db_path)) {
    $config.db_path = Join-Path $distRoot ([string]$config.db_path)
}
$dbParent = Split-Path -Parent $config.db_path
if ($dbParent -and -not (Test-Path $dbParent)) {
    New-Item -ItemType Directory -Path $dbParent | Out-Null
}

if ($StopExisting) {
    $connections = Get-NetTCPConnection -LocalPort $config.port -ErrorAction SilentlyContinue
    foreach ($connection in $connections) {
        if ($connection.OwningProcess) {
            Stop-Process -Id $connection.OwningProcess -Force -ErrorAction SilentlyContinue
        }
    }
}

$localDashboard = "http://127.0.0.1:$($config.port)/dashboard?token=$($config.token)"
$phoneUrl = "$($config.public_base_url.TrimEnd('/'))/app?token=$($config.token)"

if ($config.tailscale_serve) {
    $tailscale = Get-Command tailscale -ErrorAction SilentlyContinue
    if ($tailscale) {
        Write-Host "Refreshing Tailscale Serve: https=443 -> http://127.0.0.1:$($config.port)"
        tailscale serve --bg --https=443 "http://127.0.0.1:$($config.port)"
        tailscale serve status
    } else {
        Write-Host "Tailscale CLI not found; skipping Tailscale Serve refresh."
    }
}

Write-Host ""
Write-Host "Dashboard: $localDashboard"
Write-Host "Phone:     $phoneUrl"
Write-Host "Push test: $($config.public_base_url.TrimEnd('/'))/dashboard?token=$($config.token)"
Write-Host ""

$daemonArgs = @(
    "--host=$($config.host)",
    "--port=$($config.port)",
    "--db-path=$($config.db_path)",
    "--token=$($config.token)",
    "--vapid-file=.\signal_vapid.json",
    "--vapid-subject=mailto:you@example.com",
    "--public-base-url=$($config.public_base_url)"
)

if ($config.require_token_for_read) { $daemonArgs += "--require-token-for-read" }
if ($config.enable_web_push) { $daemonArgs += "--enable-web-push" }

& $exe @daemonArgs
