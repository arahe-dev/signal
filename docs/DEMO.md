# Signal Demo Walkthrough

## Prerequisites

- Rust 1.70+ installed
- Tailscale installed (for phone testing)

## Step 1: Start the Daemon (Local/No Auth)

Run the daemon locally without authentication:

```bash
cargo run -p signal-daemon -- --host 127.0.0.1 --port 8787 --db-path ./signal_demo.db
```

You should see:
```
Starting Signal daemon on 127.0.0.1:8787
Database path: ./signal_demo.db
Server listening on http://127.0.0.1:8787
```

## Step 2: Send a Message

In a new terminal, send a test message:

```bash
cargo run -p signal-cli -- send \
  --title "Codex blocked" \
  --body "Runbook eval failed. Should I rerun only that test?" \
  --source codex \
  --agent-id codex \
  --project ivy \
  --server http://127.0.0.1:8787
```

Expected output:
```
Sending message...
✓ Message created: <uuid>
  Title: Codex blocked
  Status: new
```

## Step 3: Open Inbox Locally

Open your browser to:
```
http://127.0.0.1:8787
```

You should see the inbox with your message card.

## Step 4: Start Daemon with Token for Tailscale

Start the daemon with authentication for phone access:

```bash
cargo run -p signal-daemon -- --host 0.0.0.0 --port 8787 --db-path ./signal_demo.db --token dev-token
```

The daemon will show:
```
Warning: listening on all interfaces. Use only on trusted/private networks such as Tailscale.
Starting Signal daemon on 0.0.0.0:8787
Database path: ./signal_demo.db
Token authentication enabled
Server listening on http://0.0.0.0:8787
```

## Step 5: Open Inbox on Phone (Tailscale)

1. Find your laptop's Tailscale IP:
   ```bash
   tailscale ip -4
   ```

2. On your phone, open:
   ```
   http://<your-tailscale-ip>:8787?token=dev-token
   ```

You should see the same inbox interface on your phone.

## Step 6: Submit Reply from Phone

1. Tap on the message card to open detail view
2. The token is preserved in the URL (`?token=dev-token`)
3. Type a reply in the form - the hidden token field is auto-filled
4. Submit the reply

The reply is now stored in SQLite.

## Step 7: Fetch Reply from CLI

```bash
cargo run -p signal-cli -- latest-reply \
  --server http://127.0.0.1:8787 \
  --token dev-token
```

Output:
```
Latest pending reply:
  ID: <uuid>
  Message ID: <uuid>
  Body: <your reply>
  From: phone
  Status: pending
  Created: <timestamp>
```

## Step 8: Consume the Reply

```bash
cargo run -p signal-cli -- latest-reply \
  --server http://127.0.0.1:8787 \
  --token dev-token \
  --consume true
```

Output:
```
Latest pending reply:
  ID: <uuid>
  ...
Consuming reply...
✓ Reply consumed: <uuid>
  New status: consumed
```

## Demo Complete

You have successfully:
1. ✅ Sent a message from CLI (agent)
2. ✅ Viewed message in browser (local)
3. ✅ Viewed message on phone (Tailscale) with token auth
4. ✅ Submitted reply from phone browser with token validation
5. ✅ Fetched reply via CLI with token
6. ✅ Consumed reply to mark as processed

The data persists in `signal_demo.db` and survives daemon restarts.

## Security Notes

- The demo uses token auth which is adequate for private Tailnet dogfood
- Tailscale provides transport encryption between devices
- The app auth is separate from network transport - both are needed
- This is NOT safe for public internet exposure - no HTTPS, basic token auth
- For production, add device pairing and HTTPS