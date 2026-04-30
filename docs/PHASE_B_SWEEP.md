# Phase B Sweep Checklist

| Route | Expected auth | Expected success | Expected failure | Tested |
| --- | --- | --- | --- | --- |
| `GET /health` | none | `{"ok":true}` | n/a | yes local |
| `GET /dashboard` | admin query token | dashboard loads | `admin_required` or `unauthorized` | yes local |
| `GET /app` | none for shell, token in JS | PWA shell loads | JS shows pairing required | yes local |
| `GET /pair` | none | valid/missing/invalid code pages render | invalid code shows clear page | yes local |
| `GET /message/{id}` | admin/device query token | message detail and reply form | `device_revoked`/auth error | partial local |
| `POST /api/pair/start` | admin token | full `pair_` code, URL, QR SVG | admin required | yes local |
| `POST /api/pair/complete` | valid pair code | device token returned once | invalid/used/expired code rejected | yes local |
| `GET /api/devices` | admin token | device list | admin required | yes local |
| `POST /api/devices/{id}/revoke` | admin token | device revoked and push subs revoked | admin required | yes local |
| `POST /api/devices/reset-all` | admin token | active devices revoked, push subs disabled, unused pair codes cleared | admin required | yes local |
| `GET /api/messages` | admin/device token | messages JSON | revoked device rejected | yes local |
| `GET /api/messages/{id}` | admin/device token | message JSON | revoked device rejected | compile covered |
| `POST /api/messages` | admin/device token | message stored and push attempted | revoked device rejected | yes local |
| `POST /api/ask` | admin/device token | ask stored and push attempted | revoked device rejected | yes local |
| `GET /api/ask/{id}/wait` | admin/device token | replied/timeout JSON | revoked device rejected | yes local |
| `POST /api/messages/{id}/replies` | admin/device token | reply stored and message replied | revoked device rejected | yes local |
| `GET /api/push/status` | admin/device token | push status JSON | revoked device rejected | yes local |
| `GET /api/push/vapid-public-key` | admin/device token | `publicKey`, `length=65`, `firstByte=4` | structured VAPID/auth error | yes local |
| `POST /api/push/subscribe` | admin/device token | subscription stored with device id when device auth | structured auth/save error | yes local |
| `POST /api/push/test` | admin/device token | custom push summary with attempted/sent/failed/skipped counts | structured push/auth error | yes local |
| `GET /manifest.webmanifest` | none | manifest JSON | n/a | yes local |
| `GET /service-worker.js` | none | JS served | n/a | yes local |
| icons | none | PNGs served | n/a | yes local |

## Buttons

| Surface | Button | Expected behavior | Tested |
| --- | --- | --- | --- |
| `/dashboard` | Start Pairing | calls `/api/pair/start`, renders URL and QR | yes local |
| `/dashboard` | Copy Pairing URL | copies full URL | code inspected |
| `/dashboard` | Revoke Device | calls revoke endpoint and refreshes | yes local |
| `/dashboard` | Reset all devices | confirms, calls reset endpoint, shows summary, reloads counts | yes local |
| `/dashboard` | Send Test Push | sends custom title/body/url to `/api/push/test` and shows JSON result | yes local |
| `/dashboard` | Clear Stale Subscriptions | hidden because no endpoint exists | yes local |
| `/pair` | Pair Device | calls `/api/pair/complete`, stores token | yes local |
| `/app` | Enable Notifications | staged diagnostics, VAPID validation before subscribe | code inspected/local VAPID |
| `/app` | Refresh Inbox | reloads messages and push status | yes local |
| `/message/{id}` | Quick Reply | fills textarea | code inspected |
| `/message/{id}` | Send Reply | posts form with admin/device token | yes local |
