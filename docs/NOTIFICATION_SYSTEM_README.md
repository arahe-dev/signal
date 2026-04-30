Signal sends Web Push notifications from the local daemon to paired browser/PWA devices.
The iPhone receives notifications through Safari/Home Screen Web Push over the private Tailscale URL.
Each push subscription is stored in SQLite and can be bound to a paired device token.
Revoked devices and stale subscriptions are skipped during push sends.
Notification taps deep-link back to the Signal PWA or exact message page.
