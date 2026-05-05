# Implementation Roadmap

This plan keeps the current daemon, PWA, and push path active. All work is additive in a separate branch or worktree until a phase meets its acceptance criteria.

## Guardrails

- Keep the daemon responsible for message delivery, push delivery, and auth only.
- Put Codex, OpenCode, script execution, Git capture, and artifact capture in a separate `signal-worker`.
- Do not add arbitrary remote shell execution. Phone-origin requests can approve specific work, not open an unconstrained shell.
- Keep Web Push and the PWA as the primary mobile path. Android should use the same standards path rather than a divergent native path.
- Capture Git context and artifacts from the CLI or worker at workflow boundaries. Do not make the daemon scan repositories.
- Treat task execution as at-least-once. Use idempotency keys, leases, fencing tokens, and maintenance repair rather than promising exactly-once delivery.

## Phase 0: Isolation And Baseline

Objective: Establish a safe development lane without changing the live notification path.

Depends on: None.

Scope:

- Confirm current daemon, PWA, and push delivery remain unchanged and runnable.
- Create feature flags for PWA hardening, worker IPC, artifact capture, and coordination tables.
- Add migration scaffolding for future tables, with flags off by default.
- Document rollback steps for each flag.

Acceptance criteria:

- Existing phone notification flow still works with all new flags disabled.
- No daemon code path shells out or scans Git repositories.
- Migrations can be applied and rolled back in a test database.
- Branch/worktree can be deleted without affecting the live `C:\signal_ping` install.

## Phase 1: PWA And Web Push Hardening

Objective: Improve the mobile experience while staying on standards-based Web Push.

Depends on: Phase 0.

Scope:

- Add stable manifest `id`, explicit `scope`, and token-free `start_url`.
- Make `/app?message=<id>` the deep-link target for notification clicks.
- Replace message contents in push payloads with private generic payloads plus a message id.
- Add explicit permission-state UI for `default`, `denied`, unsupported, and granted states.
- Add a tiny app-shell service worker cache for `/app`, core CSS, core JS, icons, and minimal offline state.
- Add dark mode using system preference plus an in-app override if the current UI already has settings.
- Introduce `NotificationAdapter` with `WebPushNotificationAdapter` as the first implementation.
- Add progressive notification actions where supported, with graceful fallback to opening `/app?message=<id>`.

Acceptance criteria:

- Installed PWA launches at the token-free app entrypoint and stays inside its declared scope.
- Notification click opens `/app?message=<id>` and fetches message content after auth.
- Push payloads visible to browser push services contain no prompt text, tokens, file paths, or command output.
- Permission UI does not call `requestPermission()` except from direct user action.
- Android browsers and iOS Home Screen web apps use the same Web Push code path, gated by feature detection.
- The PWA renders usable light and dark states and can load the app shell after a cold restart.

References: [MDN Push API](https://developer.mozilla.org/en-US/docs/Web/API/Push_API), [WebKit iOS Web Push](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/), [Apple web push](https://developer.apple.com/documentation/usernotifications/sending-web-push-notifications-in-web-apps-and-browsers), [web.dev manifest](https://web.dev/learn/pwa/web-app-manifest), [MDN start_url](https://developer.mozilla.org/en-US/docs/Web/Manifest/start_url), [MDN scope](https://developer.mozilla.org/docs/Web/Progressive_web_apps/Manifest/Reference/scope), [web.dev caching](https://web.dev/learn/pwa/caching).

## Phase 2: Git Context And Artifact Capture

Objective: Store enough run context for review and replay without bloating the database or scanning in the daemon.

Depends on: Phase 0.

Scope:

- Add `context_snapshots` metadata table.
- Add `artifacts` metadata table with `sha256`, `size`, `media_type`, `ttl`, `pinned`, `created_by`, and `created_at`.
- Store raw patches, screenshots, logs, and binary blobs in a content-addressed artifact directory.
- Capture `git status --porcelain=v2 -z`, `git rev-parse --verify HEAD`, and `git worktree list --porcelain -z` from the worker or CLI wrapper.
- Capture staged patches with `git diff --cached --binary --full-index` only when the task needs patch replay.
- Compute `git patch-id --stable` for fuzzy duplicate patch detection where applicable.
- Add artifact pruning for expired, unpinned blobs.

Acceptance criteria:

- Each noninteractive run can attach a context snapshot without daemon repository access.
- Paths with spaces, tabs, and newlines are parsed through `-z` formats, not line splitting.
- Raw artifact bytes are addressed by content hash and are not duplicated when captured twice.
- Deleting expired unpinned artifacts does not delete pinned records or break metadata queries.
- Binary patch capture is optional and size capped.

References: [git status porcelain](https://git-scm.com/docs/git-status), [git rev-parse](https://git-scm.com/docs/git-rev-parse), [git worktree porcelain](https://git-scm.com/docs/git-worktree), [git diff --binary --full-index](https://git-scm.com/docs/git-diff), [git patch-id --stable](https://git-scm.com/docs/git-patch-id).

## Phase 3: Worker Runtime And Noninteractive Adapters

Objective: Move command execution into a local worker with explicit contracts, caps, and approvals.

Depends on: Phase 0 and Phase 2 metadata.

Scope:

- Add `signal-worker` as a separate process from the daemon.
- Use named pipes or equivalent local-only IPC with current-user ACLs.
- Define structured run requests: executable id, argv array, cwd, env allowlist, timeout, output cap, artifact policy, and approval id.
- Use Windows Job Objects for noninteractive process groups, timeouts, and child cleanup.
- Add exact command-hash grants over executable, argv, cwd, env allowlist, artifact policy, and expiry.
- Add adapters for `codex exec`, `opencode run`, and `opencode serve` or session APIs.
- Return structured run results with exit code, timeout state, capped stdout/stderr, artifact ids, and context snapshot id.

Acceptance criteria:

- The daemon cannot execute a command even if given a crafted message.
- A worker run with an expired, missing, or mismatched grant is rejected before process spawn.
- A timed-out run terminates its child process tree.
- stdout/stderr cannot exceed configured caps; overflow is represented as truncation metadata.
- Codex and OpenCode adapters can run a simple noninteractive prompt through structured args.
- Worker IPC rejects remote clients and unrecognized principals.

References: [Microsoft named pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes), [CreateNamedPipe remote-client modes](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea), [Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), [Codex noninteractive mode](https://developers.openai.com/codex/noninteractive), [Codex CLI reference](https://developers.openai.com/codex/cli/reference), [OpenCode CLI](https://opencode.ai/docs/cli/), [OpenCode server](https://opencode.ai/docs/server/), [OpenCode permissions](https://opencode.ai/docs/permissions).

## Phase 4: Desktop Interactive Worker

Objective: Support interactive local sessions without turning phone notifications into a remote shell.

Depends on: Phase 3.

Scope:

- Add a desktop worker or tray surface for human-present interactive sessions.
- Use ConPTY only for commands that actually need terminal interactivity.
- Require foreground local confirmation for starting or attaching to an interactive PTY.
- Capture transcripts as capped artifacts with redaction hooks.
- Keep phone actions limited to approve, reject, open, summarize, or cancel.

Acceptance criteria:

- Interactive sessions cannot be created through daemon-only or phone-only inputs.
- Closing the tray session terminates or detaches according to an explicit user choice.
- ConPTY is feature-detected and has a noninteractive fallback for unsupported hosts.
- Transcript capture obeys the same artifact limits, TTLs, and pinning rules as Phase 2.

References: [Microsoft Pseudoconsoles](https://learn.microsoft.com/en-us/windows/console/pseudoconsoles), [CreatePseudoConsole](https://learn.microsoft.com/en-us/windows/console/createpseudoconsole).

## Phase 5: AI Coordination Model

Objective: Coordinate multiple agents and scripts through explicit tasks rather than ad hoc messages.

Depends on: Phase 2 and Phase 3.

Scope:

- Add `tasks`, `task_edges`, `task_claims`, `agents`, `subscriptions`, and `summaries`.
- Represent dependencies as a DAG in `task_edges`.
- Use leases with fencing tokens for claims.
- Store agent heartbeats, capabilities, current task, and last observed fencing token.
- Add subscription rows for user, device, task, run, and summary notifications.
- Add maintenance loop for expired leases, orphaned claims, old summaries, and artifact TTL cleanup.
- Keep MCP or stdio bridge support as a later adapter behind the same worker contract.

Acceptance criteria:

- Two workers cannot both commit a claim update with the same current fencing token.
- Expired claims are made available for retry by maintenance without losing prior run artifacts.
- A task can be delivered at least once and processed idempotently by task id plus attempt id.
- Task summaries can be updated without overwriting newer fenced updates.
- Notifications can be subscribed by task or run without embedding sensitive content in push payloads.

References: [SQLite transactions](https://www.sqlite.org/lang_transaction.html), [SQLite WAL](https://www.sqlite.org/wal.html), [NATS queue groups](https://docs.nats.io/nats-concepts/core-nats/queue), [NATS JetStream consumers](https://docs.nats.io/nats-concepts/jetstream/consumers), [fencing tokens](https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html), [MCP transports](https://modelcontextprotocol.io/docs/concepts/transports).

## Phase 6: Rollout And Hardening

Objective: Move from internal prototype to daily-use reliability.

Depends on: Phases 1 through 5.

Scope:

- Add end-to-end tests for push deep links, worker grants, artifact pruning, and claim expiry.
- Add structured health checks for daemon, PWA, worker, artifact store, and database migrations.
- Add operator docs for enabling flags, reverting flags, pruning artifacts, and reading run records.
- Add compatibility matrix for Windows versions, browser support, Git version, Codex CLI version, and OpenCode version.
- Run staged rollout: local-only, one trusted repo, multiple repos, then multi-agent coordination.

Acceptance criteria:

- Feature flags can disable worker execution without disabling current message/push delivery.
- A failed worker or stuck task does not block normal Signal notification delivery.
- Storage quotas and artifact pruning are observable and tested.
- The system documents at-least-once semantics and required idempotency behavior.
- Security review signs off on no arbitrary remote shell, no token-bearing push payloads, and local-only IPC.

## Later

- MCP/stdio bridge as another worker adapter, not as a daemon feature.
- Native mobile wrappers only if standards Web Push cannot meet a specific measured requirement.
- Multi-machine workers only after local-only IPC, grants, leases, and artifact redaction are proven locally.
