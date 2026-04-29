param(
    [int]$Port = 8790,
    [string]$Token = "dev-token",
    [string]$DbPath = ".\signal_demo_8790.db",
    [string]$PublicBaseUrl = "https://your-device.your-tailnet.ts.net",
    [switch]$NoTailscaleServe,
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$localUrl = "http://127.0.0.1:$Port/app?token=$Token"
$phoneUrl = "$PublicBaseUrl/app?token=$Token"

if (-not $NoTailscaleServe) {
    $tailscale = Get-Command tailscale -ErrorAction SilentlyContinue
    if ($tailscale) {
        Write-Host "Refreshing Tailscale Serve: https=443 -> http://127.0.0.1:$Port"
        tailscale serve --bg --https=443 "http://127.0.0.1:$Port"
        tailscale serve status
    } else {
        Write-Host "Tailscale CLI not found; skipping Tailscale Serve refresh."
    }
}

Write-Host ""
Write-Host "Local: $localUrl"
Write-Host "Phone: $phoneUrl"
Write-Host ""
Write-Host "Push status check:"
Write-Host "Invoke-RestMethod -Uri `"$PublicBaseUrl/api/push/status`" -Headers @{ `"X-Signal-Token`" = `"$Token`" } | ConvertTo-Json -Depth 10"
Write-Host ""
Write-Host "Ask command example:"
Write-Host "cargo run -p signal-cli -- --server http://127.0.0.1:$Port --token $Token ask --title `"Test ask`" --body `"Reply yes from phone`" --source manual --agent-id test --project signal --timeout 2m --json"
Write-Host ""
Write-Host "QR placeholder: open this URL on iPhone: $phoneUrl"
Write-Host ""

$daemonArgs = @(
    "--host", "127.0.0.1",
    "--port", "$Port",
    "--db-path", $DbPath,
    "--token", $Token,
    "--require-token-for-read",
    "--enable-web-push",
    "--vapid-file", ".\signal_vapid.json",
    "--vapid-subject", "mailto:you@example.com",
    "--public-base-url", $PublicBaseUrl
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
