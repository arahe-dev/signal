param(
    [int]$Port,
    [string]$Token,
    [string]$DbPath,
    [string]$PublicBaseUrl,
    [string]$VapidSubject,
    [switch]$NoTailscaleServe,
    [switch]$SkipTailscaleInstallPrompt,
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

# Load config from signal.config.json if it exists
$configFile = Join-Path $repoRoot "signal.config.json"
$config = @{
    Port = 8790
    Token = "dev-token"
    DbPath = ".\signal_demo_8790.db"
    PublicBaseUrl = "https://your-device.your-tailnet.ts.net"
    VapidSubject = "mailto:you@example.com"
}

if (Test-Path $configFile) {
    Write-Host "Loading config from $configFile"
    $configJson = Get-Content $configFile -Raw | ConvertFrom-Json
    
    if ($configJson.port) { $config.Port = $configJson.port }
    if ($configJson.admin_token) { $config.Token = $configJson.admin_token }
    if ($configJson.token) { $config.Token = $configJson.token }
    if ($configJson.db_path) { $config.DbPath = $configJson.db_path }
    if ($configJson.public_base_url) { $config.PublicBaseUrl = $configJson.public_base_url }
    if ($configJson.vapid_subject) { $config.VapidSubject = $configJson.vapid_subject }
}

# Override with command-line parameters if provided
if ($Port) { $config.Port = $Port }
if ($Token) { $config.Token = $Token }
if ($DbPath) { $config.DbPath = $DbPath }
if ($PublicBaseUrl) { $config.PublicBaseUrl = $PublicBaseUrl }
if ($VapidSubject) { $config.VapidSubject = $VapidSubject }

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

# Check if port is available
Write-Host "Checking port $($config.Port) availability..."
$portCheck = Test-NetConnection -ComputerName 127.0.0.1 -Port $config.Port -WarningAction SilentlyContinue
if ($portCheck.TcpTestSucceeded) {
    Write-Host "ERROR: Port $($config.Port) is already in use" -ForegroundColor Red
    Write-Host "To use a different port, run: .\scripts\start_signal.ps1 -Port 8791" -ForegroundColor Yellow
    exit 1
}
Write-Host "Port $($config.Port) is available" -ForegroundColor Green

$localUrl = "http://127.0.0.1:$($config.Port)/app?token=$($config.Token)"
$phoneUrl = "$($config.PublicBaseUrl)/app?token=$($config.Token)"

if (-not $NoTailscaleServe) {
    $tailscale = Ensure-TailscaleCli -SkipPrompt:$SkipTailscaleInstallPrompt
    if ($tailscale) {
        Write-Host "Refreshing Tailscale Serve: https=443 -> http://127.0.0.1:$($config.Port)"
        tailscale serve --bg --https=443 "http://127.0.0.1:$($config.Port)"
        tailscale serve status
    } else {
        Write-Host "Tailscale Serve refresh skipped."
    }
}

Write-Host ""
Write-Host "Configuration:"
Write-Host "  Port: $($config.Port)"
Write-Host "  Token: $($config.Token.Substring(0, [Math]::Min(8, $config.Token.Length)))..."
Write-Host "  DB: $($config.DbPath)"
Write-Host ""
Write-Host "Local: $localUrl"
Write-Host "Phone: $phoneUrl"
Write-Host ""
Write-Host "Push status check:"
Write-Host "Invoke-RestMethod -Uri `"$($config.PublicBaseUrl)/api/push/status`" -Headers @{ `"X-Signal-Token`" = `"$($config.Token)`" } | ConvertTo-Json -Depth 10"
Write-Host ""
Write-Host "Ask command example:"
Write-Host "cargo run -p signal-cli -- --server http://127.0.0.1:$($config.Port) --token $($config.Token) ask --title `"Test ask`" --body `"Reply yes from phone`" --source manual --agent-id test --project signal --timeout 2m --json"
Write-Host ""
Write-Host "Device pairing (Phase B):"
Write-Host "cargo run -p signal-cli -- --server http://127.0.0.1:$($config.Port) --token $($config.Token) pair start --name iPhone"
Write-Host "cargo run -p signal-cli -- --server http://127.0.0.1:$($config.Port) --token $($config.Token) devices list"
Write-Host ""
Write-Host "QR placeholder: open this URL on iPhone: $phoneUrl"
Write-Host ""

$daemonArgs = @(
    "--host", "127.0.0.1",
    "--port", "$($config.Port)",
    "--db-path", $config.DbPath,
    "--token", $config.Token,
    "--require-token-for-read",
    "--enable-web-push",
    "--vapid-file", ".\signal_vapid.json",
    "--vapid-subject", $config.VapidSubject,
    "--public-base-url", $config.PublicBaseUrl
)

if ($Release) {
    $exe = Join-Path $repoRoot "target\release\signal-daemon.exe"
    if (-not (Test-Path $exe)) {
        cargo build --release
    }
    & $exe @daemonArgs
} else {
    cargo run -p signal-daemon -- @daemonArgs
}
