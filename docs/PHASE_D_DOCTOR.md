# Phase D Doctor

`signal-cli doctor` checks the local daemon, auth, diagnostics, VAPID, device/push state, optional public URL routing, and optional push delivery.

## Basic Use

```powershell
.\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor
```

JSON:

```powershell
.\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor --json
```

Public Tailscale check:

```powershell
.\signal-cli.exe `
  --server http://127.0.0.1:8791 `
  --token dev-token `
  doctor `
  --public-url https://ari-legion.taild0cc8e.ts.net `
  --check-public
```

Push check:

```powershell
.\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor --check-push
```

## What It Checks

- Local `/health`
- `/api/diagnostics` with token auth
- VAPID public key shape
- VAPID private/public match when diagnostics provides it
- Active/revoked devices
- Active/revoked/stale/legacy push subscriptions
- Optional public `/health` and `/api/diagnostics`
- Optional `/api/push/test`

## Common Failures

Connection refused:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\start_release.ps1 -StopExisting
```

Public URL unreachable:

```powershell
tailscale serve reset
tailscale serve --bg --https=443 http://127.0.0.1:8791
```

Token rejected:

- Use the dashboard/dev token.
- If using a device token, pair the device again if it was revoked.

No active paired devices:

- Open dashboard.
- Start Pairing.
- Open the pairing link on the phone.

No active device-bound push subscriptions:

- Open `/app` on the paired phone.
- Tap Enable Notifications.
- If Safari state is stale, delete the Home Screen app and clear Safari website data for the Tailscale host.

Legacy/unbound subscriptions exist:

- Re-enable notifications from the currently paired phone.
- Or reset all devices from the dashboard if state is stale.

`BadJwtToken` or VAPID mismatch:

- Confirm `signal_vapid.json` is stable.
- If VAPID changed, re-enable notifications on the phone so the browser subscription matches the current key.

Ask/reply timeout:

- Confirm phone can open `/app`.
- Confirm notification tap opens the exact message.
- Confirm the ask timeout is long enough.
