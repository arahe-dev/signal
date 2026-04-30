# Signal

Signal is a local-first push + reply protocol for agents, scripts, automations, and devices.

It lets a local program ask a human for input, wake an iPhone through Web Push, receive a reply, and continue from structured JSON.

## What It Is Not

- Not a notes app.
- Not a cloud service.
- Not Codex-specific.
- Not dependent on ntfy, Telegram, email, AWS, Oracle, or a VPS.
- Not a native mobile app.

Signal uses Tailscale Serve for private HTTPS and browser/Apple Web Push for iPhone notifications.

## Requirements

- Windows PC
- Rust toolchain
- Tailscale on PC and iPhone
- iPhone Safari / Home Screen PWA
- Same Tailscale tailnet on PC and phone

## 5-Minute Windows Quickstart

Build the developer-preview release:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

Start the release build:

```powershell
powershell -ExecutionPolicy Bypass -File .\dist\signal\scripts\start_release.ps1 -StopExisting
```

Open the dashboard:

```text
http://127.0.0.1:8791/dashboard?token=dev-token
```

Open diagnostics:

```text
http://127.0.0.1:8791/diagnostics?token=dev-token
```

## Pair iPhone

1. Open the dashboard.
2. Click Start Pairing.
3. Open the pairing link on the phone you want to pair.
4. Complete pairing.
5. Open `/app` on the phone.
6. Tap Enable Notifications.
7. Add the app to Home Screen if needed.

## Send First Ask

```powershell
.\dist\signal\signal-cli.exe `
  --server http://127.0.0.1:8791 `
  --token dev-token `
  ask `
  --title "Test ask" `
  --body "Reply yes from phone" `
  --source manual `
  --agent-id test `
  --project signal `
  --timeout 2m `
  --json
```

Expected output after replying from phone:

```json
{
  "status": "replied",
  "message_id": "...",
  "reply_id": "...",
  "reply": "yes",
  "elapsed_seconds": 42,
  "timed_out": false
}
```

## Developer Start

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\start_signal.ps1 -Port 8791 -Release
```

Or manual:

```powershell
cargo run -p signal-daemon -- `
  --host 127.0.0.1 `
  --port 8791 `
  --db-path .\signal_demo.db `
  --token dev-token `
  --require-token-for-read `
  --enable-web-push `
  --public-base-url https://ari-legion.taild0cc8e.ts.net
```

## Push Test

Use the dashboard Push section and set:

- Title: `Signal custom test`
- Body: `This is a custom debug push from the dashboard.`
- URL/path: `/app`

If attempted is `0`, diagnostics will explain whether there are no subscriptions, only revoked/stale subscriptions, or only legacy/unbound subscriptions.

## Revoke And Reset

List devices:

```powershell
.\dist\signal\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token devices list
```

Reset devices and push subscriptions:

```powershell
.\dist\signal\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token devices reset-all
```

The dashboard also has a dangerous Reset all devices button. It preserves messages and replies.

## Troubleshooting

Run doctor first:

```powershell
.\dist\signal\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor
```

Run public URL and push checks:

```powershell
.\dist\signal\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor --public-url https://ari-legion.taild0cc8e.ts.net --check-public
.\dist\signal\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor --check-push
```

- Dashboard says no active push subscriptions: pair the phone, open `/app`, and tap Enable Notifications.
- Push shows legacy/unbound subscriptions: re-enable notifications from the paired phone, or use Test Push when exactly one active device exists so Signal can claim the legacy subscription.
- iPhone icon or push state looks stale: delete the Home Screen icon, clear Safari website data for the Tailscale host, reopen the phone URL, and add to Home Screen again.
- Port is in use: start with `-StopExisting` or choose `-Port 8792`.
- Tailscale CLI missing: start release with `-NoTailscaleServe` and configure Tailscale Serve manually.

## Security Notes

- Keep the daemon bound to `127.0.0.1`.
- Expose through private Tailscale Serve, not the public internet.
- Do not commit `signal.config.json`, `signal_vapid.json`, SQLite DBs, `target/`, or `dist/`.
- `dev-token` is for developer-preview dogfood only.

## Verification

```powershell
cargo fmt
cargo check
cargo test
cargo build --release
```
