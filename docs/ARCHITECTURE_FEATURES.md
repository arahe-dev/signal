# Signal Architecture Features

This branch keeps the existing daemon, CLI, PWA, pairing, and push flow intact while adding an opt-in coordination layer beside it.

## Event Log

- `GET /api/events?after_seq=<seq>&limit=<n>` returns an append-only event stream.
- `POST /api/events` accepts a CloudEvents-inspired JSON envelope through `CreateEventLogRequest`.
- Existing message, ask, reply, reply-consume, context, and artifact actions now append event-log entries.
- Each event stores `prev_hash` and `event_hash` for a lightweight tamper-evident chain.

## Context Snapshots

- `POST /api/messages/{id}/context` stores a point-in-time repo/worktree snapshot for a ping.
- `GET /api/messages/{id}/context` returns snapshots linked to that ping.
- CLI capture:

```powershell
signal-cli --server http://127.0.0.1:8791 --token dev-token context capture `
  --message-id <message-id> `
  --stage "ping-sent" `
  --repo .
```

## Artifacts

- `POST /api/artifacts/upload` uploads a small base64 artifact, capped at 10 MiB.
- `GET /api/messages/{id}/artifacts` lists artifacts linked to a ping.
- `GET /api/artifacts/{id}/content` serves the stored content.
- CLI upload:

```powershell
signal-cli --server http://127.0.0.1:8791 --token dev-token artifact upload `
  --message-id <message-id> `
  --path .\screenshot.png `
  --kind screenshot
```

## Wake Worker

The daemon never executes shell commands. `signal-worker` is a separate local process that polls for wake pings targeting an agent. It prints the ping by default and can optionally run an explicit command chosen by the local user.

```powershell
signal-worker --server http://127.0.0.1:8791 --token dev-token --agent-id codex --project signal
```

With an explicit command:

```powershell
signal-worker --server http://127.0.0.1:8791 --token dev-token --agent-id codex `
  --command codex --command-arg status
```

## PWA

- Manifest starts at `/app`; auth tokens are not embedded in the manifest or push URLs.
- Push click-through opens `/app?message=<message-id>`.
- The app includes inbox, event log, notification controls, local pairing reset, wake pings, dark mode, message detail, replies, context snapshots, and artifact previews.
- VAPID subject defaults to a real contact address. Override with `--vapid-subject mailto:<real-address>` if this repo is reused.
