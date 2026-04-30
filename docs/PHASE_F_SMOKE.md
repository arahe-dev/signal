# Phase F Smoke Script

`scripts/smoke_release.ps1` validates local server/API behavior without requiring a physical iPhone.

## Usage

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\smoke_release.ps1 -Port 8791 -Token dev-token
```

JSON output:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\smoke_release.ps1 -Port 8791 -Token dev-token -Json
```

Read-only route/API checks:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\smoke_release.ps1 -Port 8791 -Token dev-token -NoMutate
```

## Checks

- `GET /health`
- `GET /api/diagnostics`
- `GET /pair?code=test`
- `GET /dashboard?token=...`
- `GET /app`
- `POST /api/pair/start`
- `POST /api/pair/complete`
- `GET /api/devices`
- temporary smoke device revoke cleanup
- `GET /api/push/status`
- `GET /diagnostics?token=...`
- `signal-cli doctor --json` when `signal-cli.exe` is available

The script exits `1` only when failures are present. Warnings and skips are reported but do not fail the run.

