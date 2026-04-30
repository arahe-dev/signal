param(
    [int]$Port = 8791,
    [string]$Token = "dev-token",
    [string]$PublicBaseUrl = "",
    [switch]$NoMutate,
    [switch]$Json
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Resolve-Path (Join-Path $ScriptDir "..")
$Server = "http://127.0.0.1:$Port"
$Headers = @{ "X-Signal-Token" = $Token }
$Checks = New-Object System.Collections.Generic.List[object]
$CreatedDeviceId = $null

function Add-Check {
    param(
        [string]$Name,
        [string]$Status,
        [string]$Message,
        [object]$Details = $null
    )
    $Checks.Add([pscustomobject]@{
        name = $Name
        status = $Status
        message = $Message
        details = $Details
    }) | Out-Null
}

function Invoke-Json {
    param(
        [string]$Uri,
        [string]$Method = "GET",
        [object]$Body = $null
    )
    $params = @{
        Uri = $Uri
        Method = $Method
        Headers = $Headers
    }
    if ($null -ne $Body) {
        $params.ContentType = "application/json"
        $params.Body = ($Body | ConvertTo-Json -Depth 10)
    }
    Invoke-RestMethod @params
}

function Find-Cli {
    $candidates = @(
        (Join-Path $Root "signal-cli.exe"),
        (Join-Path $Root "target\release\signal-cli.exe")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }
    return $null
}

try {
    $health = Invoke-RestMethod -Uri "$Server/health" -Method GET
    Add-Check "health" "pass" "daemon reachable" $health
} catch {
    Add-Check "health" "fail" $_.Exception.Message
}

try {
    $diagnostics = Invoke-Json "$Server/api/diagnostics"
    Add-Check "diagnostics" "pass" "diagnostics returned" @{
        active_devices = $diagnostics.active_devices
        active_subscriptions = $diagnostics.active_subscriptions
        legacy_unbound_subscriptions = $diagnostics.legacy_unbound_subscriptions
    }
} catch {
    Add-Check "diagnostics" "fail" $_.Exception.Message
}

try {
    $pairPage = Invoke-WebRequest -Uri "$Server/pair?code=test" -UseBasicParsing
    if ($pairPage.StatusCode -ge 200 -and $pairPage.StatusCode -lt 500) {
        Add-Check "pair_route" "pass" "/pair?code=test returned HTTP $($pairPage.StatusCode)"
    } else {
        Add-Check "pair_route" "fail" "/pair?code=test returned HTTP $($pairPage.StatusCode)"
    }
} catch {
    Add-Check "pair_route" "fail" $_.Exception.Message
}

try {
    $dashboard = Invoke-WebRequest -Uri "$Server/dashboard?token=$([uri]::EscapeDataString($Token))" -UseBasicParsing
    $ok = $dashboard.Content.Contains("Start Pairing") -and $dashboard.Content.Contains("Setup Health")
    Add-Check "dashboard_route" ($(if ($ok) { "pass" } else { "fail" })) "dashboard route loaded" @{ contains_expected_text = $ok }
} catch {
    Add-Check "dashboard_route" "fail" $_.Exception.Message
}

try {
    $app = Invoke-WebRequest -Uri "$Server/app" -UseBasicParsing
    Add-Check "app_route" "pass" "/app returned HTTP $($app.StatusCode)"
} catch {
    Add-Check "app_route" "fail" $_.Exception.Message
}

if (-not $NoMutate) {
    try {
        $pairStart = Invoke-Json "$Server/api/pair/start" "POST" @{ device_name = "Smoke Test Device" }
        $pairUrl = [string]$pairStart.pair_url
        $pairCode = [string]$pairStart.pairing_code
        $shapeOk = $pairCode.StartsWith("pair_") -and $pairUrl.Contains("/pair?code=$pairCode")
        if ($PublicBaseUrl) {
            $shapeOk = $shapeOk -and $pairUrl.StartsWith($PublicBaseUrl.TrimEnd("/"))
        }
        Add-Check "pair_start" ($(if ($shapeOk) { "pass" } else { "fail" })) "pair code and URL generated" @{
            code_prefix = $pairStart.code_prefix
            pair_url = $pairUrl
        }

        $pairComplete = Invoke-Json "$Server/api/pair/complete" "POST" @{
            pairing_code = $pairCode
            device_name = "Smoke Test Device"
            device_kind = "smoke"
        }
        $CreatedDeviceId = [string]$pairComplete.device_id
        $pairedOk = $CreatedDeviceId.Length -gt 0 -and ([string]$pairComplete.device_token).StartsWith("sig_dev_")
        Add-Check "pair_complete" ($(if ($pairedOk) { "pass" } else { "fail" })) "API-only pairing completed" @{
            device_id = $CreatedDeviceId
            device_name = $pairComplete.device_name
        }
    } catch {
        Add-Check "pair_mutation" "fail" $_.Exception.Message
    }
} else {
    Add-Check "pair_mutation" "skip" "NoMutate set; skipped pair/start and pair/complete"
}

try {
    $devices = Invoke-Json "$Server/api/devices"
    Add-Check "devices_list" "pass" "device list returned" @{ count = $devices.devices.Count }
} catch {
    Add-Check "devices_list" "fail" $_.Exception.Message
}

if ($CreatedDeviceId) {
    try {
        $revoke = Invoke-Json "$Server/api/devices/$CreatedDeviceId/revoke" "POST"
        Add-Check "smoke_device_cleanup" "pass" "temporary smoke device revoked" $revoke
    } catch {
        Add-Check "smoke_device_cleanup" "warn" "temporary smoke device was not revoked: $($_.Exception.Message)"
    }
}

try {
    $pushStatus = Invoke-Json "$Server/api/push/status"
    Add-Check "push_status" "pass" "push status returned" $pushStatus
} catch {
    Add-Check "push_status" "fail" $_.Exception.Message
}

try {
    $diagPage = Invoke-WebRequest -Uri "$Server/diagnostics?token=$([uri]::EscapeDataString($Token))" -UseBasicParsing
    Add-Check "diagnostics_page" "pass" "/diagnostics returned HTTP $($diagPage.StatusCode)"
} catch {
    Add-Check "diagnostics_page" "fail" $_.Exception.Message
}

$cli = Find-Cli
if ($cli) {
    try {
        $doctorOutput = & $cli --server $Server --token $Token doctor --json 2>&1
        if ($LASTEXITCODE -eq 0) {
            Add-Check "doctor" "pass" "doctor exited 0"
        } else {
            Add-Check "doctor" "fail" "doctor exited $LASTEXITCODE" ($doctorOutput -join "`n")
        }
    } catch {
        Add-Check "doctor" "fail" $_.Exception.Message
    }
} else {
    Add-Check "doctor" "skip" "signal-cli.exe not found; build release first"
}

$passes = @($Checks | Where-Object { $_.status -eq "pass" }).Count
$warnings = @($Checks | Where-Object { $_.status -eq "warn" }).Count
$failures = @($Checks | Where-Object { $_.status -eq "fail" }).Count
$skips = @($Checks | Where-Object { $_.status -eq "skip" }).Count
$result = [pscustomobject]@{
    ok = ($failures -eq 0)
    server = $Server
    checks = $Checks
    summary = [pscustomobject]@{
        passes = $passes
        warnings = $warnings
        failures = $failures
        skips = $skips
    }
}

if ($Json) {
    $result | ConvertTo-Json -Depth 12
} else {
    Write-Host "Signal Release Smoke"
    Write-Host "===================="
    Write-Host "Server: $Server"
    foreach ($check in $Checks) {
        $label = $check.status.ToUpperInvariant()
        Write-Host ("[{0}] {1}: {2}" -f $label, $check.name, $check.message)
    }
    Write-Host ("Summary: {0} pass, {1} warn, {2} fail, {3} skip" -f $passes, $warnings, $failures, $skips)
}

if ($failures -gt 0) {
    exit 1
}
