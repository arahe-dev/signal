# Phase F Browser Checklist

Use this checklist after starting the daemon and opening `http://127.0.0.1:8791/dashboard?token=dev-token`.

## Routes

- `/dashboard?token=dev-token` loads, shows Setup Health, and stores the admin token.
- `/pair?code=test` returns a visible invalid/expired pairing page, not a 404.
- `/app` loads and shows pairing/token guidance when no token is stored.
- `/diagnostics?token=dev-token` loads without exposing secrets.
- `/message/<id>?token=...` opens a message detail and reply form for a valid message.

## Dashboard Buttons

- Start Pairing shows a full phone pairing URL and QR/fallback link.
- Copy Pairing URL copies or shows a clear browser clipboard error.
- Revoke Device confirms, revokes only that device, and refreshes counts.
- Reset All Devices is visually dangerous, confirms, and preserves messages/replies.
- Send Test Push shows attempted/sent/failed/skipped JSON and explains attempted `0`.
- Clear Stale/Legacy Subscriptions confirms and shows deleted counts.

## PWA Buttons

- Enable Notifications reports the exact failing stage if service worker, permission, VAPID, subscribe, or backend save fails.
- Refresh Inbox reloads active messages and shows empty/revoked/token states clearly.
- Open Message navigates to `/message/<id>` with the active token.
- Send Reply shows success/failure visibly.
- Quick Reply submits the selected reply visibly.

## Expected Empty States

- No active devices: pair a phone.
- No active push subscriptions: open `/app` on the phone and tap Enable Notifications.
- Legacy/unbound subscriptions: clear stale/legacy or re-pair and enable notifications.
- Revoked token: PWA clears old localStorage token and asks to pair again.

