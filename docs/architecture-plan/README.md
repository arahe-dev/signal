# Signal Architecture Plan

This plan keeps the current daemon, CLI, PWA, Web Push, Tailscale Serve, pairing, messages, and replies working while adding a local coordination kernel underneath them. The goal is not to replace Signal with a cloud service or a general message broker. The goal is to make the current local handoff system durable, auditable, permissioned, and usable by multiple agents without losing the simple phone reply workflow.

## Index

- [01-core-architecture.md](01-core-architecture.md) - core kernel, event envelope, SQLite event store, projections, grants, routing, and compatibility constraints.
- [02-implementation-roadmap.md](02-implementation-roadmap.md) - phased implementation plan, dependencies, and acceptance criteria.
- [03-research-matrix.md](03-research-matrix.md) - feature-to-source mapping and implementation takeaways.
- [04-risk-register.md](04-risk-register.md) - security, storage, compatibility, and product-scope risks with mitigations.

## Executive Plan

Signal should evolve into a small local coordination kernel with three layers:

1. Existing surfaces: daemon HTTP API, CLI, dashboard, phone PWA, Web Push, device pairing, and current message/reply tables.
2. Additive kernel: CloudEvents-inspired envelopes, append-only SQLite event log, idempotency, correlation/causation links, principals, visibility, and DB-backed grants.
3. Coordination projections: inbox, threads, messages, replies, grants, and later task DAGs, artifact references, leases, handoffs, and escalation state.

Use the event log as the source of truth. Use projections for fast reads and compatibility with existing APIs. Existing commands and browser flows should continue to work during every migration step.

## Design Principles

- Additive first: introduce new tables and adapters before changing current call sites.
- Local first: keep state in the local SQLite database and keep network exposure limited to the existing private HTTPS/Tailscale model.
- Event sourced core: every state transition that matters is an event; mutable tables are projections.
- Idempotent writes: every command path that can retry must carry or derive an idempotency key.
- Explicit authority: `source` is the producer; `actor` is the principal whose authority is being used.
- Least privilege grants: v1 grants are opaque DB-backed tokens, hashed at rest, with server-side caveats.
- Observable coordination: use correlation, causation, and OpenTelemetry-like trace ids to follow agent workflows across events.
- Small broker, not full broker: route local subjects and leases through SQLite-backed state; do not introduce Kafka/NATS/RabbitMQ semantics.

CloudEvents is useful as a proven shape for portable event metadata, especially `id`, `source`, `type`, `subject`, and source plus id uniqueness ([spec](https://github.com/cloudevents/spec/blob/main/cloudevents/spec.md)). SQLite remains appropriate for the local source of truth, with WAL mode considered for better reader/writer behavior ([SQLite WAL](https://www.sqlite.org/wal.html)). Trace fields should align with W3C/OpenTelemetry conventions where practical ([W3C Trace Context](https://www.w3.org/TR/trace-context/), [OpenTelemetry overview](https://opentelemetry.io/docs/specs/otel/overview/)).

## Delivery Phases

### Phase 0: Baseline and Boundaries

- Freeze the current public behavior as compatibility requirements.
- Document current tables and API flows.
- Add no breaking migrations.
- Define the event envelope and v1 event type registry.

### Phase 1: Add the Kernel

- Add an append-only event table beside the current `events`, `messages`, `replies`, `devices`, and push tables.
- Wrap existing create/update paths so they append v1 events in the same transaction as current writes.
- Add idempotency keys for message creation, reply creation, reply consumption, device pairing/revocation, and grants.
- Add principal parsing and normalization for `user:local`, `device:{uuid}`, `agent:{id}`, `service:signal-daemon`, `project:{name}`, and `role:{name}`.

### Phase 2: Make Projections First Class

- Treat current `messages` and `replies` as compatibility projections while adding projection checkpoints.
- Add inbox/thread projections for read/done state and phone-friendly summaries.
- Add grants and grant usage tables.
- Add a projection rebuild command for local recovery and migration validation.

### Phase 3: Add Coordination

- Add a tiny local broker abstraction over event append, route subjects, and lease/claim projections.
- Add task DAG projections and events after message/reply/grant flows are stable.
- Add handoff, escalation, compact summary, and artifact reference events.
- Use trace ids to group multi-agent work and causation ids to explain why each event exists.

### Phase 4: Harden

- Add retention and compaction rules for projections and summaries.
- Add optional event hash chaining for tamper evidence.
- Add structured audit views for grants, auth denials, device changes, and human-required actions.
- Add backup/export/import checks that include the event log and current SQLite WAL/shm files when applicable.

## Non-Goals

- No public hosted Signal service in this plan.
- No replacement of the current PWA or Web Push stack.
- No mandatory native mobile app.
- No distributed consensus, external broker, or multi-node queue.
- No raw token storage.
- No event log mutation after append, except explicit administrative repair tools that create audit records.

## Open Decisions

- Final event table name: prefer `event_log` or `signal_events` to avoid disrupting the current `events` table.
- Projection naming: keep current `messages` and `replies` as projections, or add explicit `projection_*` tables and expose compatibility views.
- Idempotency key derivation for legacy clients that do not send a key.
- Grant hash algorithm and token prefix length.
- Exact route subject grammar for tasks, threads, projects, and agents.
- Whether optional hash chaining is per-database, per-subject, or per-visibility partition.
