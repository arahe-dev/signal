# Phase B Manual Test

1. Start daemon:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\start_signal.ps1 -Port 8791 -Release
```

2. Start or reset Tailscale Serve:

```powershell
tailscale serve reset
tailscale serve --bg --https=443 http://127.0.0.1:8791
tailscale serve status
```

3. Open dashboard:

```text
http://127.0.0.1:8791/dashboard?token=dev-token
```

4. Click Start Pairing.

5. Scan QR or open the full pairing URL on iPhone:

```text
https://your-device.your-tailnet.ts.net/pair?code=pair_<full_code>
```

6. Pair device. The pair page should store `signal_device_token` and redirect to `/app`.

7. Open `/app` without a query token. It should load using localStorage.

8. Tap Enable Notifications. Expected stages:

- service worker register succeeds
- permission is granted
- VAPID key fetch returns `length=65`, `firstByte=4`
- subscription saves successfully

9. Dashboard push management checks:

- Counts distinguish active devices, revoked devices, active push subscriptions, revoked/stale push subscriptions, and legacy/unbound push subscriptions.
- Send Test Push with title `Signal custom test` and body `This is a custom debug push from the dashboard.`
- The JSON result should show attempted/sent/failed/skipped counts.
- Click Reset all devices and confirm. Messages and replies should remain, devices should become revoked, and push subscriptions should become revoked.
- Pair the phone again and enable notifications again before continuing.

10. Run ask:

```powershell
cargo run -p signal-cli -- `
  --server http://127.0.0.1:8791 `
  --token dev-token `
  ask `
  --title "Phase B manual ask" `
  --body "Reply yes from phone." `
  --source manual `
  --agent-id test `
  --project signal `
  --timeout 2m `
  --json
```

11. Tap the notification. It should open the exact message.

12. Reply from phone. CLI should print JSON with `status: "replied"`.

13. Revoke device from dashboard.

14. Confirm old phone token cannot access:

- `/app` shows Pair Again / revoked state on API calls
- old token receives `device_revoked`
- push subscriptions for that device are skipped

15. Send Test Push again. Expected result:

- `attempted` is `0` if no active device-bound subscriptions exist
- revoked/stale subscriptions are counted as skipped
- the endpoint returns JSON instead of crashing or treating no-active as a hard error

16. Pair again with a new code.
