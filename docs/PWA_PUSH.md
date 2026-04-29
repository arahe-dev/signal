# PWA Web Push

Signal uses standards-based Web Push for the iPhone Home Screen PWA. The push payload is generic and does not include message content:

```json
{
  "title": "Signal",
  "body": "New Signal message. Tap to open inbox.",
  "url": "/app"
}
```

The daemon must be reachable over HTTPS, for example through Tailscale Serve. iOS requires the site to be installed as a Home Screen app before push subscriptions work.

Use a VAPID subject that is a real `mailto:` or `https:` contact URL. Example:

```powershell
cargo run -p signal-daemon -- `
  --host 127.0.0.1 `
  --port 8790 `
  --db-path .\signal_demo_8790.db `
  --token dev-token `
  --require-token-for-read `
  --enable-web-push `
  --vapid-file .\signal_vapid.json `
  --vapid-subject mailto:signal@example.local `
  --public-base-url https://ari-legion.taild0cc8e.ts.net
```

If `/api/push/test` returns `subscription_vapid_key_mismatch_resubscribe_required`, the PWA subscription was created with a different VAPID key. Delete the Home Screen PWA, clear website data for the Tailscale URL, reinstall the PWA, and tap Enable Notifications again.

If Apple returns HTTP 403, inspect `/api/push/test` diagnostics. Confirm `vapid_private_matches_public` is `true`, `derived_audience` is `https://web.push.apple.com`, and the subscription VAPID key matches the current key.
