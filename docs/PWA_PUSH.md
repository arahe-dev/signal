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
  --public-base-url https://your-device.your-tailnet.ts.net
```

The daemon exposes the active VAPID public key at `/api/push/vapid-public-key`. The PWA uses that key when creating browser subscriptions, so after changing VAPID keys you must delete the old Home Screen app, clear Safari website data, reinstall the PWA, and tap Enable Notifications again.

If `/api/push/test` returns `subscription_vapid_key_mismatch_resubscribe_required`, the PWA subscription was created with a different VAPID key. Delete the Home Screen PWA, clear website data for the Tailscale URL, reinstall the PWA, and tap Enable Notifications again.

If Apple returns HTTP 403, inspect `/api/push/test` diagnostics. Confirm `vapid_private_matches_public` is `true`, `derived_audience` is `https://web.push.apple.com`, and the subscription VAPID key matches the current key.

## Custom Icon

The daemon serves custom icons for the iPhone Home Screen PWA:

- `/apple-touch-icon.png` - 180x180 PNG for Home Screen icon
- `/apple-touch-icon-precomposed.png` - 180x180 PNG fallback for older iOS Web Clip behavior
- `/apple-touch-icon-180x180.png` - 180x180 PNG root fallback
- `/icon-192.png` - 192x192 PNG for install prompt
- `/icon-512.png` - 512x512 PNG for install prompt

Apple touch icons are served with `Cache-Control: no-cache, no-store, must-revalidate` while icon behavior is being verified, because iOS caches Web Clip icons aggressively.

### To refresh the iPhone Home Screen icon:

1. Delete the old Signal Home Screen icon.
2. Open Settings -> Safari -> Advanced -> Website Data.
3. Delete data for `your-device.your-tailnet.ts.net` if present.
4. Reopen: `https://your-device.your-tailnet.ts.net/app?token=dev-token`
5. Share -> Add to Home Screen.
6. If icon still does not update, restart Safari/iPhone and retry.

Icons are served by the local Rust daemon through Tailscale Serve - no public website is required.
