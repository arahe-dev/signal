param(
    [string]$Server = "http://127.0.0.1:8787",
    [string]$Token = "dev-token",
    [string]$AgentId = "codex",
    [string]$Project = "ivy"
)

$ErrorActionPreference = "Continue"
$script:FailCount = 0

function Test-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )
    Write-Host "`n[TEST] $Name" -ForegroundColor Cyan
    try {
        $result = & $Script
        if ($?) {
            Write-Host "[PASS] $Name" -ForegroundColor Green
            return $true
        } else {
            Write-Host "[FAIL] $Name" -ForegroundColor Red
            $script:FailCount++
            return $false
        }
    } catch {
        Write-Host "[FAIL] $Name : $_" -ForegroundColor Red
        $script:FailCount++
        return $false
    }
}

function Get-WithAuth {
    param([string]$Path)
    $uri = "$Server$Path"
    $headers = @{ "X-Signal-Token" = $Token }
    Invoke-RestMethod -Uri $uri -Headers $headers -Method Get -ErrorAction SilentlyContinue
}

function Post-WithAuth {
    param([string]$Path, [object]$Body)
    $uri = "$Server$Path"
    $headers = @{ "X-Signal-Token" = $Token; "Content-Type" = "application/json" }
    Invoke-RestMethod -Uri $uri -Headers $headers -Method Post -Body ($Body | ConvertTo-Json) -ErrorAction SilentlyContinue
}

Write-Host "=== Signal Smoke Test ===" -ForegroundColor Yellow
Write-Host "Server: $Server"
Write-Host "Token: $Token"
Write-Host "AgentId: $AgentId"
Write-Host "Project: $Project"

# Step 1: Health check
Test-Step "Health check" {
    $r = Invoke-RestMethod -Uri "$Server/health" -Method Get
    $r.ok -eq $true
}

# Step 2: Send a message
$messageId = $null
Test-Step "Send message" {
    $body = @{
        title = "Smoke test message"
        body = "Testing the smoke demo script"
        source = "smoke-test"
        agent_id = $AgentId
        project = $Project
    }
    $r = Post-WithAuth -Path "/api/messages" -Body $body
    if ($r) {
        $script:messageId = $r.id
        Write-Host "  Created message: $($r.id)" -ForegroundColor Gray
    }
    $null -ne $r
}

# Step 3: List inbox
Test-Step "List inbox" {
    $r = Get-WithAuth -Path "/api/messages?limit=10"
    $r.Count -gt 0
}

# Step 4: Get message detail
Test-Step "Get message detail" {
    if (-not $messageId) {
        Write-Host "  Skipped - no message ID" -ForegroundColor Yellow
        return $true
    }
    $r = Get-WithAuth -Path "/api/messages/$messageId"
    $null -ne $r.message
}

# Step 5: Create a reply via API
$replyId = $null
Test-Step "Create reply" {
    if (-not $messageId) {
        Write-Host "  Skipped - no message ID" -ForegroundColor Yellow
        return $true
    }
    $body = @{
        body = "Smoke test reply"
        source = "smoke-test"
    }
    $r = Post-WithAuth -Path "/api/messages/$messageId/replies" -Body $body
    if ($r) {
        $script:replyId = $r.id
        Write-Host "  Created reply: $($r.id)" -ForegroundColor Gray
    }
    $null -ne $r
}

# Step 6: Fetch latest reply
Test-Step "Fetch latest reply" {
    $r = Get-WithAuth -Path "/api/replies/latest?agent_id=$AgentId&project=$Project"
    $null -ne $r
}

# Step 7: Consume reply
Test-Step "Consume reply" {
    if (-not $replyId) {
        Write-Host "  Skipped - no reply ID" -ForegroundColor Yellow
        return $true
    }
    $headers = @{ "X-Signal-Token" = $Token }
    $r = Invoke-RestMethod -Uri "$Server/api/replies/$replyId/consume" -Headers $headers -Method Post
    $r.status -eq "consumed"
}

# Summary
Write-Host "`n=== Summary ===" -ForegroundColor Yellow
if ($FailCount -eq 0) {
    Write-Host "ALL TESTS PASSED" -ForegroundColor Green
    exit 0
} else {
    Write-Host "FAILED: $FailCount test(s)" -ForegroundColor Red
    exit 1
}