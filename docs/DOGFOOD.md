# Signal Dogfood Guide

Signal is a local-first push + reply protocol for agents, scripts, automations, and devices.

Use `send` for progress notifications. Use `ask` only when the automation must block for a human reply. Use `doctor` first when setup or push behavior breaks.

## Nonblocking Progress Ping

```powershell
cargo run -p signal-cli -- `
  --server http://127.0.0.1:8791 `
  --token dev-token `
  send `
  --title "Codex done" `
  --body "The requested task completed." `
  --source codex `
  --agent-id codex `
  --project signal
```

## Blocking Ask

```powershell
cargo run -p signal-cli -- `
  --server http://127.0.0.1:8791 `
  --token dev-token `
  ask `
  --title "Need decision" `
  --body "Should I rerun the full suite or only failed tests?" `
  --source codex `
  --agent-id codex `
  --project signal `
  --timeout 10m `
  --reply-option "full suite" `
  --reply-option "failed only" `
  --json
```

## Local Model Or Script

Call `signal-cli.exe` from any local runner. Treat JSON stdout from `ask --json` as the protocol contract.

```powershell
$reply = .\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token ask --title "Model blocked" --body "Continue?" --source qwen --agent-id qwen --project local --timeout 5m --json | ConvertFrom-Json
if ($reply.status -eq "replied") {
  Write-Host "Human replied: $($reply.reply)"
}
```

## Benchmark Runner Failure

```powershell
if ($LASTEXITCODE -ne 0) {
  .\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token ask --title "Benchmark failed" --body "Retry or stop?" --source bench --agent-id runner --project signal --timeout 10m --reply-option retry --reply-option stop --json
}
```

## Diagnose First

```powershell
.\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor --check-push
```

