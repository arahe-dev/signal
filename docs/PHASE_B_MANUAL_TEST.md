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
https://ari-legion.taild0cc8e.ts.net/pair?code=pair_<full_code>
```

6. Pair device. The pair page should store `signal_device_token` and redirect to `/app`.

7. Open `/app` without a query token. It should load using localStorage.

8. Tap Enable Notifications. Expected stages:

- service worker register succeeds
- permission is granted
- VAPID key fetch returns `length=65`, `firstByte=4`
- subscription saves successfully

9. Run ask:

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

10. Tap the notification. It should open the exact message.

11. Reply from phone. CLI should print JSON with `status: "replied"`.

12. Revoke device from dashboard.

13. Confirm old phone token cannot access:

- `/app` shows Pair Again / revoked state on API calls
- old token receives `device_revoked`
- push subscriptions for that device are skipped

14. Pair again with a new code.
