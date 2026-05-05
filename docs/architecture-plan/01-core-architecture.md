# Core Architecture

## Purpose

Add a small local coordination kernel under the current Signal daemon. The kernel records durable events, projects them into fast read models, evaluates grants, and routes local coordination work for humans, devices, agents, and services.

The current daemon, CLI, phone PWA, Web Push, device pairing, and message/reply APIs stay intact. The kernel is introduced beside them, then existing write paths are adapted to append events and update projections transactionally.

## Components

### Existing Surfaces

- `signal-daemon`: Axum HTTP server, dashboard, PWA endpoints, pairing, diagnostics, and push operations.
- `signal-cli`: local command surface for send, ask, replies, devices, doctor, and smoke workflows.
- `signal-core`: current models, storage, event helpers, permissions, token hashing, and SQLite access.
- Phone PWA and Web Push: notification and reply UX. Push payloads stay generic and do not need message content.

### New Kernel Modules

- Envelope: validates and normalizes event metadata.
- Event store: append-only SQLite writer and event reader.
- Projector: applies ordered events into read models and maintains checkpoints.
- Grants: issues, revokes, evaluates, and records usage of opaque tokens.
- Router: maps route subjects to local subscribers, claims, leases, tasks, and handoffs.
- Compatibility adapters: preserve current tables and APIs while events become the source of truth.

SQLite remains the local persistence layer. WAL mode can be enabled after testing to improve concurrent readers while the daemon writes ([SQLite WAL](https://www.sqlite.org/wal.html)).

## Event Envelope

Use a CloudEvents-inspired envelope, not a requirement that every local row be a strict CloudEvent. The useful parts are stable metadata, source plus id uniqueness, type names, subjects, timestamps, and extension fields ([CloudEvents spec](https://github.com/cloudevents/spec/blob/main/cloudevents/spec.md)).

Example:

```json
{
  "specversion": "1.0",
  "id": "018f6b3b-8a3a-75b4-9d7f-59643f4d5f01",
  "type": "signal.message.created",
  "source": "service:signal-daemon",
  "subject": "thread:7f4/message:2bd",
  "time": "2026-05-05T07:10:00Z",
  "datacontenttype": "application/json",
  "dataschema": "signal://schemas/events/message-created.v1",
  "actor": "agent:codex",
  "visibility": "actionable",
  "correlation_id": "ask-018f6b3b",
  "causation_id": null,
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
  "span_id": "00f067aa0ba902b7",
  "idempotency_key": "message:create:agent:codex:ask-018f6b3b",
  "data": {
    "message_id": "2bd",
    "thread_id": "7f4",
    "title": "Need approval",
    "body": "Reply yes or no",
    "reply_mode": "text"
  }
}
```

Envelope rules:

- `id` is unique per `source`; duplicate `source` plus `id` means the same event.
- `type` is a registry value such as `signal.reply.created`.
- `source` is the producer that wrote or emitted the event.
- `actor` is the principal whose authority allowed the action.
- `subject` is the routing/read-model target.
- `visibility` gates who can read payloads and projections.
- `idempotency_key` deduplicates retried commands.
- `correlation_id` groups a workflow; `causation_id` points to the event or command that caused this event.
- `trace_id` and `span_id` follow W3C/OpenTelemetry shapes for cross-component debugging ([W3C Trace Context](https://www.w3.org/TR/trace-context/), [OpenTelemetry trace context fields](https://opentelemetry.io/docs/specs/otel/compatibility/logging_trace_context/)).

If a strict CloudEvents binding is later needed, snake_case extension fields can be mapped to binding-safe extension names at the boundary.

## V1 Event Types

Initial registry:

| Type | Purpose |
| --- | --- |
| `signal.message.created` | A message or ask was created. |
| `signal.message.status_changed` | Message status changed, including pending, replied, timeout, consumed, archived, failed. |
| `signal.reply.created` | A human/device/agent reply was created. |
| `signal.reply.consumed` | A reply was consumed by an authorized agent or service. |
| `signal.thread.read` | A principal marked a thread read. |
| `signal.thread.done` | A principal marked a thread complete/done. |
| `signal.device.paired` | A device was paired. |
| `signal.device.revoked` | A device was revoked. |
| `signal.grant.issued` | A capability grant was issued. |
| `signal.grant.revoked` | A capability grant was revoked. |
| `signal.auth.denied` | A command or read was denied by auth/grant checks. |

Later registry groups:

- `signal.task.created`, `signal.task.claimed`, `signal.task.completed`, `signal.task.failed`, `signal.task.blocked`.
- `signal.handoff.requested`, `signal.handoff.accepted`, `signal.escalation.requested`, `signal.escalation.resolved`.
- `signal.artifact.linked`, `signal.summary.compacted`.

## Identity and Addressing

Principal strings:

- `user:local`
- `device:{uuid}`
- `agent:{id}`
- `service:signal-daemon`
- `project:{name}`
- `role:{name}`

Rules:

- Persist normalized principal strings; do not infer authority from display names.
- A device token authenticates `device:{uuid}`.
- A daemon internal action uses `service:signal-daemon`.
- Human actions from the paired PWA may be represented as both `user:local` and `device:{uuid}` in event data, with `actor` set to the authority actually checked.
- Project and role principals are grouping/authorization subjects, not proof of identity by themselves.

## SQLite Schema Direction

Add an event table beside existing tables. Prefer a new name such as `event_log` or `signal_events` so the current `events` table can remain untouched during migration.

Suggested shape:

```sql
CREATE TABLE event_log (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  event_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  source TEXT NOT NULL,
  actor TEXT NOT NULL,
  subject TEXT,
  visibility TEXT NOT NULL DEFAULT 'private',
  event_time TEXT NOT NULL,
  inserted_at TEXT NOT NULL,
  datacontenttype TEXT NOT NULL DEFAULT 'application/json',
  dataschema TEXT,
  data_json TEXT NOT NULL,
  extensions_json TEXT NOT NULL DEFAULT '{}',
  idempotency_key TEXT,
  correlation_id TEXT,
  causation_id TEXT,
  trace_id TEXT,
  span_id TEXT,
  resource TEXT,
  prev_hash TEXT,
  event_hash TEXT
);

CREATE UNIQUE INDEX ux_event_log_source_id
  ON event_log(source, event_id);

CREATE UNIQUE INDEX ux_event_log_idempotency
  ON event_log(idempotency_key)
  WHERE idempotency_key IS NOT NULL;

CREATE INDEX ix_event_log_type_seq ON event_log(event_type, seq);
CREATE INDEX ix_event_log_subject_seq ON event_log(subject, seq);
CREATE INDEX ix_event_log_correlation_seq ON event_log(correlation_id, seq);
```

Storage rules:

- Append only. Corrections are new events.
- Append and projection updates happen in one SQLite transaction when possible.
- If projection update fails, the event remains durable and the projector retries from its checkpoint.
- Optional hash chaining uses `prev_hash` and `event_hash`; null means not enabled.
- Store raw tokens nowhere. Only token hashes and short prefixes are allowed.

## Projections

Projections are derived, mutable read models. They can be rebuilt from `event_log`.

V1 projections:

- Messages: current `messages` table can serve as the compatibility projection.
- Replies: current `replies` table can serve as the compatibility projection.
- Inbox: thread-level summaries, latest event, unread/read, done, pending reply count, visibility.
- Grants: active/revoked grants and usage counters.
- Devices: current `devices`, `pairing_codes`, and `push_subscriptions` remain compatibility projections, with v1 events emitted for pairing/revocation.
- Projection checkpoints: one row per projector with last applied `seq`, updated atomically.

Later projections:

- Tasks: task state, dependencies, owners, claims, retries, and escalation deadlines.
- Artifacts: content-addressed file references or local path references with visibility metadata.
- Summaries: compact thread/task summaries for agents with bounded context.

Projection rules:

- Projection code must be deterministic for a given ordered event stream.
- Projectors must be idempotent; reapplying an event must not duplicate rows or counters.
- Compatibility reads should keep returning the existing JSON shapes until API versions are introduced.
- Projection rebuild should be available as a local maintenance command before event sourcing becomes mandatory.

## Grants and Capabilities

V1 grants are DB-backed opaque tokens:

- Generate a high-entropy random token.
- Show the raw token once.
- Store only `token_hash` and `token_prefix`.
- Evaluate caveats server-side, not inside the token.

Suggested tables:

```sql
CREATE TABLE grants (
  id TEXT PRIMARY KEY,
  token_hash TEXT NOT NULL UNIQUE,
  token_prefix TEXT NOT NULL,
  issued_to TEXT NOT NULL,
  issued_by TEXT NOT NULL,
  scopes_json TEXT NOT NULL,
  resources_json TEXT NOT NULL,
  expires_at TEXT,
  max_uses INTEGER,
  uses INTEGER NOT NULL DEFAULT 0,
  requires_human INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active',
  created_at TEXT NOT NULL,
  revoked_at TEXT,
  metadata_json TEXT
);

CREATE TABLE grant_uses (
  id TEXT PRIMARY KEY,
  grant_id TEXT NOT NULL,
  event_seq INTEGER,
  actor TEXT NOT NULL,
  scope TEXT NOT NULL,
  resource TEXT,
  decision TEXT NOT NULL,
  reason TEXT,
  created_at TEXT NOT NULL
);
```

Caveats:

- `scopes`: examples include `message:create`, `reply:read`, `reply:consume`, `thread:read`, `thread:done`, `device:pair`, `task:claim`.
- `resources`: subject/resource filters such as `project:signal`, `thread:{id}`, `agent:{id}`, or `message:{id}`.
- `expires_at`: deny after expiry.
- `max_uses`: increment in the same transaction as the authorized append.
- `requires_human`: allow only when the command is directly backed by `user:local` or an accepted human confirmation event.

Denied checks append `signal.auth.denied` without recording raw tokens or sensitive request bodies.

## Routing and Local Broker

The broker is a local coordination facade over the event log and projections. It should route work, claims, and notifications, not become a full external broker.

Subject examples:

- `thread.{thread_id}`
- `message.{message_id}`
- `reply.{reply_id}`
- `project.{name}`
- `agent.{id}.inbox`
- `task.{task_id}`
- `grant.{grant_id}`

V1 routing behavior:

- Route events by `subject`, `project`, `agent`, visibility, and grant scope.
- Maintain leases/claims for work that exactly one actor should own at a time.
- Represent handoffs and escalations as events, not hidden mutable flags.
- Use the current `outbox` and push path for phone notification fanout.

Lease direction:

```sql
CREATE TABLE leases (
  id TEXT PRIMARY KEY,
  subject TEXT NOT NULL,
  claimant TEXT NOT NULL,
  lease_kind TEXT NOT NULL,
  status TEXT NOT NULL,
  claimed_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  heartbeat_at TEXT,
  correlation_id TEXT,
  metadata_json TEXT
);
```

The lease projection can be rebuilt from `signal.task.claimed`, `signal.reply.consumed`, handoff, and timeout events once those event types exist.

## AI Coordination

Add coordination only after message/reply/grant paths are stable.

Core concepts:

- Task DAGs: tasks have dependencies, owners, state, and event-derived history.
- Claims: an agent claims a task or route subject with a lease and heartbeat.
- Handoffs: one actor asks another actor or human to take over a subject.
- Escalation rules: deadlines, failed claims, denied auth, or low-confidence summaries can request human input.
- Compact summaries: bounded summaries are artifacts/events linked to a thread/task and regenerated as context changes.
- Trace ids: one `trace_id` follows a multi-agent workflow; `span_id` changes per operation; `causation_id` links event-to-event cause.

This is intentionally smaller than a message broker. SQLite order plus leases is enough for local human/agent coordination.

## Compatibility Constraints

- Keep current daemon/PWA/push behavior intact.
- Keep current REST endpoints and CLI commands working until explicit API versions replace them.
- Keep current `messages`, `replies`, `devices`, `pairing_codes`, `push_subscriptions`, and `outbox` semantics during migration.
- Do not require existing devices to re-pair only because the event log is introduced.
- Do not put message content in push payloads.
- Do not expose Signal directly to the public internet as part of this plan.
- Do not require external broker, cloud database, or native mobile app.
- Migrations must be forward-only and preserve existing SQLite data.
- New auth/grant checks must support a compatibility path for the current daemon token while grants roll out.
- Every new write path should be safe to retry through idempotency.
