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
- Tailscale on PC and phone
- iPhone Safari / Home Screen PWA
- Same Tailscale tailnet on PC and phone

## 5-Minute Windows Quickstart

Desktop wrapper build:

```powershell
npm install
npm run tauri:build
```

Outputs:

```text
target\release\signal-desktop.exe
target\release\bundle\msi\Signal_0.1.0_x64_en-US.msi
target\release\bundle\nsis\Signal_0.1.0_x64-setup.exe
```

See `docs\DESKTOP_TAURI.md` for desktop release details.

Build the developer-preview release:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

Start the release build:

```powershell
powershell -ExecutionPolicy Bypass -File .\dist\signal\scripts\start_release.ps1 -StopExisting
```

If Tailscale is not installed, the start script offers to install it with `winget`. You can skip that prompt with `-NoTailscaleServe` or `-SkipTailscaleInstallPrompt`.

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
2. Confirm Settings shows a real VAPID contact such as `mailto:name@example.com`.
3. Click Start Pairing.
4. Open the pairing link on the phone you want to pair.
5. Complete pairing.
6. Open `/app` on the phone.
7. Tap Enable Notifications.
8. Add the app to Home Screen if needed.

## VAPID Contact

Web Push requires a real contact in the VAPID subject. Signal defaults to `mailto:you@example.com` for local dogfood builds.

Change it from the dashboard Settings card, or set it in `signal.config.json`:

```json
{
  "vapid_subject": "mailto:name@example.com"
}
```

Use either a real `mailto:` email address or an `https:` contact URL.

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
  --public-base-url https://your-device.your-tailnet.ts.net
```

## Push Test

Use the dashboard Push section and set:

- Title: `Signal custom test`
- Body: `This is a custom debug push from the dashboard.`
- URL/path: `/app`

If attempted is `0`, diagnostics will explain whether there are no subscriptions, only revoked/stale subscriptions, or only legacy/unbound subscriptions.

Clear stale/legacy subscriptions from the dashboard when old browser subscriptions make counts confusing. This does not delete devices, messages, or replies.

## Smoke Test

After starting the release daemon, run the local smoke script:

```powershell
powershell -ExecutionPolicy Bypass -File .\dist\signal\scripts\smoke_release.ps1 -Port 8791 -Token dev-token
```

For read-only checks:

```powershell
powershell -ExecutionPolicy Bypass -File .\dist\signal\scripts\smoke_release.ps1 -Port 8791 -Token dev-token -NoMutate
```

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
.\dist\signal\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor --public-url https://your-device.your-tailnet.ts.net --check-public
.\dist\signal\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token doctor --check-push
```

- Dashboard says no active push subscriptions: pair the phone, open `/app`, and tap Enable Notifications.
- Push shows legacy/unbound subscriptions: re-enable notifications from the paired phone, or use Test Push when exactly one active device exists so Signal can claim the legacy subscription.
- iPhone icon or push state looks stale: delete the Home Screen icon, clear Safari website data for the Tailscale host, reopen the phone URL, and add to Home Screen again.
- Port is in use: start with `-StopExisting` or choose `-Port 8792`.
- Tailscale CLI missing: let `start_release.ps1` install it with `winget`, or start with `-NoTailscaleServe` and configure Tailscale Serve manually.
- VAPID subject rejected: open dashboard Settings and save a real `mailto:` email or `https:` contact URL, then retry notifications.

## Dogfood

See `docs/DOGFOOD.md` for practical agent/script patterns.

- Use `send` for nonblocking progress pings.
- Use `ask --timeout --json` only when automation should block for a human reply.
- Use `doctor` first when pairing or push behavior breaks.

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
