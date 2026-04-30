# Phase C Release Checklist

1. `scripts/build_release.ps1` passes.
2. `dist/signal/scripts/start_release.ps1 -StopExisting` starts the daemon.
3. Dashboard loads at `http://127.0.0.1:8791/dashboard?token=dev-token`.
4. Tailscale Serve routes HTTPS traffic to `http://127.0.0.1:8791`.
5. Pair phone from dashboard.
6. Enable notifications from `/app` on the phone.
7. Custom debug push is received on iPhone.
8. `signal-cli ask --json` returns phone reply JSON.
9. Revoke phone blocks old device access.
10. Reset all devices clears paired device/push state without deleting messages/replies.
11. Re-pair works after reset.

## Useful Commands

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
powershell -ExecutionPolicy Bypass -File .\dist\signal\scripts\start_release.ps1 -StopExisting
```

```powershell
.\dist\signal\signal-cli.exe `
  --server http://127.0.0.1:8791 `
  --token dev-token `
  ask `
  --title "Release checklist ask" `
  --body "Reply yes from phone." `
  --source manual `
  --agent-id release-check `
  --project signal `
  --timeout 2m `
  --json
```
