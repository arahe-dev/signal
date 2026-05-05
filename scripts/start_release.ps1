param(
    [int]$Port,
    [string]$Token,
    [string]$DbPath,
    [string]$PublicBaseUrl,
    [string]$VapidSubject,
    [switch]$NoTailscaleServe,
    [switch]$SkipTailscaleInstallPrompt,
    [switch]$StopExisting,
    [switch]$RunDoctor
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
    vapid_subject = "mailto:you@example.com"
    tailscale_serve = $true
}

if (Test-Path $configPath) {
    $json = Get-Content $configPath -Raw | ConvertFrom-Json
    foreach ($name in @("host", "port", "db_path", "token", "require_token_for_read", "enable_web_push", "public_base_url", "vapid_subject", "tailscale_serve")) {
        if ($null -ne $json.$name) {
            $config[$name] = $json.$name
        }
    }
}

if ($Port) { $config.port = $Port }
if ($Token) { $config.token = $Token }
if ($DbPath) { $config.db_path = $DbPath }
if ($PublicBaseUrl) { $config.public_base_url = $PublicBaseUrl }
if ($VapidSubject) { $config.vapid_subject = $VapidSubject }
if ($NoTailscaleServe) { $config.tailscale_serve = $false }

Set-Location $distRoot

function Test-VapidSubject {
    param([string]$Subject)
    if ([string]::IsNullOrWhiteSpace($Subject)) { return $false }
    if ($Subject -match '^mailto:[^@\s]+@[^@\s]+\.[^@\s]+$') { return $true }
    if ($Subject -match '^https://[^/\s]+') { return $true }
    return $false
}

function Ensure-TailscaleCli {
    param([switch]$SkipPrompt)

    $tailscale = Get-Command tailscale -ErrorAction SilentlyContinue
    if ($tailscale) {
        return $tailscale.Source
    }

    Write-Host ""
    Write-Host "Tailscale CLI was not found." -ForegroundColor Yellow
    Write-Host "Signal can still run locally, but phone pairing/push over your private Tailnet needs Tailscale Serve."
    if ($SkipPrompt) {
        Write-Host "Skipping Tailscale install prompt because -SkipTailscaleInstallPrompt was provided."
        return $null
    }

    $answer = Read-Host "Install Tailscale now with winget? [Y/n]"
    if ($answer -match '^(n|no)$') {
        Write-Host "Skipping Tailscale install. Install later from https://tailscale.com/download/windows"
        return $null
    }

    $winget = Get-Command winget -ErrorAction SilentlyContinue
    if ($winget) {
        Write-Host "Installing Tailscale with winget..."
        winget install --id Tailscale.Tailscale --exact --source winget
        $tailscale = Get-Command tailscale -ErrorAction SilentlyContinue
        if ($tailscale) {
            Write-Host "Tailscale CLI installed: $($tailscale.Source)" -ForegroundColor Green
            return $tailscale.Source
        }
        Write-Host "Tailscale installer finished, but tailscale.exe is not on PATH yet. Open a new terminal after login/setup." -ForegroundColor Yellow
        return $null
    }

    Write-Host "winget is not available. Opening the Tailscale Windows download page."
    Start-Process "https://tailscale.com/download/windows"
    return $null
}

if (-not (Test-VapidSubject ([string]$config.vapid_subject))) {
    Write-Host "WARNING: vapid_subject should be a real mailto: email or https: contact URL." -ForegroundColor Yellow
    Write-Host "Current value: $($config.vapid_subject)"
    Write-Host "You can update it from the dashboard Settings card after the daemon starts."
}

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
$localServer = "http://127.0.0.1:$($config.port)"
$phoneUrl = "$($config.public_base_url.TrimEnd('/'))/app?token=$($config.token)"

if ($config.tailscale_serve) {
    $tailscale = Ensure-TailscaleCli -SkipPrompt:$SkipTailscaleInstallPrompt
    if ($tailscale) {
        Write-Host "Refreshing Tailscale Serve: https=443 -> http://127.0.0.1:$($config.port)"
        tailscale serve --bg --https=443 "http://127.0.0.1:$($config.port)"
        tailscale serve status
    } else {
        Write-Host "Tailscale Serve refresh skipped. The local dashboard still works at $localDashboard"
    }
}

Write-Host ""
Write-Host "Dashboard: $localDashboard"
Write-Host "Phone:     $phoneUrl"
Write-Host "Push test: $($config.public_base_url.TrimEnd('/'))/dashboard?token=$($config.token)"
Write-Host ""
Write-Host "Doctor:"
Write-Host "  .\signal-cli.exe --server $localServer --token $($config.token) doctor"
Write-Host "Doctor with public URL:"
Write-Host "  .\signal-cli.exe --server $localServer --token $($config.token) doctor --public-url $($config.public_base_url) --check-public"
Write-Host "Push test:"
Write-Host "  .\signal-cli.exe --server $localServer --token $($config.token) doctor --check-push"
Write-Host ""

$daemonArgs = @(
    "--host=$($config.host)",
    "--port=$($config.port)",
    "--db-path=$($config.db_path)",
    "--token=$($config.token)",
    "--vapid-file=.\signal_vapid.json",
    "--vapid-subject=$($config.vapid_subject)",
    "--public-base-url=$($config.public_base_url)"
)

if ($config.require_token_for_read) { $daemonArgs += "--require-token-for-read" }
if ($config.enable_web_push) { $daemonArgs += "--enable-web-push" }

if ($RunDoctor) {
    $out = Join-Path $env:TEMP "signal-release-daemon.out.log"
    $err = Join-Path $env:TEMP "signal-release-daemon.err.log"
    Remove-Item -LiteralPath $out,$err -Force -ErrorAction SilentlyContinue
    $process = Start-Process -FilePath $exe -ArgumentList $daemonArgs -WorkingDirectory $distRoot -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
    for ($i = 0; $i -lt 20; $i++) {
        try {
            Invoke-RestMethod -Uri "$localServer/health" | Out-Null
            break
        } catch {
            Start-Sleep -Milliseconds 500
        }
    }
    Write-Host ""
    Write-Host "Running doctor..."
    & (Join-Path $distRoot "signal-cli.exe") --server $localServer --token $config.token doctor
    Write-Host ""
    Write-Host "Daemon process id: $($process.Id)"
    Write-Host "Logs: $out"
    Write-Host "Errors: $err"
} else {
    & $exe @daemonArgs
}
