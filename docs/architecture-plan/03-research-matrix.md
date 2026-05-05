# Research Matrix

This matrix maps the proposed architecture to the research sources and the implementation choices they support.

## Mobile And PWA

| Feature | Inspiration or docs | Implementation takeaway |
| --- | --- | --- |
| Standards Web Push as primary path | [MDN Push API](https://developer.mozilla.org/en-US/docs/Web/API/Push_API), [WebKit iOS Web Push](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/), [Apple web push](https://developer.apple.com/documentation/usernotifications/sending-web-push-notifications-in-web-apps-and-browsers) | Keep Web Push/PWA as the core delivery path. Use feature detection and service workers instead of browser-specific branches. |
| iOS Home Screen support | [WebKit iOS Web Push](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/) | iOS push works for Home Screen web apps, so the app must be installable, scoped, and permission-driven from a user gesture. |
| Android path | [MDN Push API](https://developer.mozilla.org/en-US/docs/Web/API/Push_API), [web.dev PWA](https://web.dev/learn/pwa/) | Treat Android as the same standards path first. Do not create a separate native Android delivery stack unless a measured gap remains. |
| Manifest identity | [web.dev manifest](https://web.dev/learn/pwa/web-app-manifest), [MDN web manifest](https://developer.mozilla.org/en-US/docs/Web/Manifest) | Add stable `id`, clear `name`/`short_name`, icons, theme colors, and `display: standalone` so the install identity survives future URL changes. |
| Scope and launch URL | [MDN scope](https://developer.mozilla.org/docs/Web/Progressive_web_apps/Manifest/Reference/scope), [MDN start_url](https://developer.mozilla.org/en-US/docs/Web/Manifest/start_url) | Set explicit `scope` and a token-free `start_url`; do not rely on an install-time URL that may contain query tokens. |
| Token-free notification open | [MDN start_url](https://developer.mozilla.org/en-US/docs/Web/Manifest/start_url), [Home Assistant notification URLs](https://companion.home-assistant.io/docs/notifications/notifications-basic) | Use `/app?message=<id>` as a deep link and fetch private content after local auth. |
| Permission states | [MDN Notification.permission](https://developer.mozilla.org/en-US/docs/Web/API/Notification/permission), [MDN requestPermission](https://developer.mozilla.org/en-US/docs/Web/API/Notification/requestPermission_static) | Model `unsupported`, `default`, `denied`, and `granted` explicitly. Request permission only after a user action. |
| Private generic push payloads | [MDN Push API](https://developer.mozilla.org/en-US/docs/Web/API/Push_API) | Treat push endpoints and payloads as sensitive. Payload should contain only generic text, message id, and routing metadata needed by the service worker. |
| App-shell cache | [web.dev caching](https://web.dev/learn/pwa/caching) | Cache only the minimum app shell: `/app`, JS, CSS, icons, and a minimal offline page. Avoid caching message content by default. |
| Notification actions | [MDN Notification](https://developer.mozilla.org/docs/Web/API/Notification), [Home Assistant actionable notifications](https://companion.home-assistant.io/docs/notifications/actionable-notifications/), [ntfy web app notification support](https://docs.ntfy.sh/subscribe/web/) | Add actions progressively. Each action should be unique to the message or task where needed and fall back to opening the app. |
| Notification priority, ttl, and acknowledgement ideas | [Pushover API](https://pushover.net/api), [Pushover receipts](https://pushover.net/api/receipts), [Gotify message extras](https://gotify.net/docs/msgextras) | Keep first implementation simple, but reserve fields for priority, ttl, acknowledgement, and client-display hints. |
| Dark mode | [MDN prefers-color-scheme](https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-color-scheme) | Use system preference first and persist an override only if the current app settings model already supports it. |
| Notification adapter | Web Push docs above plus project architecture | Define `NotificationAdapter` with Web Push first. Later adapters must satisfy the same privacy, action, and deep-link contract. |

## Runtime And Adapters

| Feature | Inspiration or docs | Implementation takeaway |
| --- | --- | --- |
| Daemon boundary | Current architecture conclusion | Keep daemon focused on messages, push, and auth. It should enqueue work or notify, not spawn commands. |
| Separate `signal-worker` | [Microsoft named pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes), [CreateNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea) | Use a local worker process with local-only IPC and narrow request schemas. Reject remote pipe clients and broad ACLs. |
| Structured noninteractive runs | [Codex noninteractive mode](https://developers.openai.com/codex/noninteractive), [Codex CLI reference](https://developers.openai.com/codex/cli/reference), [OpenCode CLI](https://opencode.ai/docs/cli/) | Represent commands as executable plus argv array, cwd, env allowlist, timeout, output cap, and artifact policy. Avoid shell strings. |
| Process cleanup and limits | [Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), [AssignProcessToJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject) | Put every noninteractive child tree in a Job Object so timeout, kill, and accounting apply to descendants. |
| Interactive terminal support | [Microsoft Pseudoconsoles](https://learn.microsoft.com/en-us/windows/console/pseudoconsoles), [CreatePseudoConsole](https://learn.microsoft.com/en-us/windows/console/createpseudoconsole) | Use ConPTY only in the desktop worker or tray when a human is present. Do not expose PTY interaction over phone push actions. |
| Exact grants | Project security conclusion | Hash executable, argv, cwd, env allowlist, artifact policy, expiry, and requester. Approval is single-purpose, expiring, and auditable. |
| Codex adapter | [Codex noninteractive mode](https://developers.openai.com/codex/noninteractive), [Codex CLI reference](https://developers.openai.com/codex/cli/reference), [OpenAI Codex repo](https://github.com/openai/codex) | Use `codex exec` for noninteractive runs. Capture JSON/last-message outputs where available and place raw logs behind artifact caps. |
| OpenCode adapter | [OpenCode CLI](https://opencode.ai/docs/cli/), [OpenCode server](https://opencode.ai/docs/server/), [OpenCode permissions](https://opencode.ai/docs/permissions) | Support `opencode run` first, then `opencode serve` and session APIs when the worker needs long-lived session control. |
| MCP bridge later | [MCP transports](https://modelcontextprotocol.io/docs/concepts/transports) | Add MCP/stdio as another worker adapter later. The daemon still should not become an MCP host or command runner. |

## Git And Artifacts

| Feature | Inspiration or docs | Implementation takeaway |
| --- | --- | --- |
| Context snapshots | [git status](https://git-scm.com/docs/git-status), [git rev-parse](https://git-scm.com/docs/git-rev-parse), [git worktree](https://git-scm.com/docs/git-worktree) | Store repository state as metadata captured at run start and finish. Include HEAD, branch, worktree, dirty status, and command ids. |
| Machine-parseable status | [git status porcelain](https://git-scm.com/docs/git-status) | Use `git status --porcelain=v2 -z`; parse NUL-delimited output instead of splitting human status lines. |
| Worktree inventory | [git worktree list](https://git-scm.com/docs/git-worktree) | Use `git worktree list --porcelain -z` so paths with unusual characters stay parseable. |
| Patch capture | [git diff](https://git-scm.com/docs/git-diff) | Use `git diff --cached --binary --full-index` only when patch replay or review needs raw patch bytes. |
| Fuzzy patch dedupe | [git patch-id](https://git-scm.com/docs/git-patch-id) | Use `git patch-id --stable` as a secondary dedupe signal; keep raw hash metadata for exact byte identity. |
| Artifact storage | Content-addressed storage pattern plus project conclusion | Store raw bytes outside the relational database, keyed by `sha256`, with metadata rows for `size`, `media_type`, `ttl`, and `pinned`. |
| Capture boundary | Project architecture conclusion | Capture artifacts in the CLI wrapper or worker, where command context is available. Do not make the daemon discover files later. |

## AI Coordination

| Feature | Inspiration or docs | Implementation takeaway |
| --- | --- | --- |
| Task graph | Project coordination conclusion | Add `tasks` and `task_edges`; represent dependencies explicitly rather than relying on message order. |
| Agent registry | Project coordination conclusion | Add `agents` with capabilities, heartbeat, current task, and version so dispatch can be explicit and observable. |
| Claims and leases | [SQLite transactions](https://www.sqlite.org/lang_transaction.html), [SQLite WAL](https://www.sqlite.org/wal.html), [NATS JetStream consumers](https://docs.nats.io/nats-concepts/jetstream/consumers) | Use transactional claims and leases. SQLite can work locally, but design the schema around clear claim ownership and retry. |
| Fencing tokens | [Fencing-token writeup](https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html) | Every claim mutation should carry the current fencing token; stale workers cannot overwrite newer task state after lease expiry. |
| At-least-once processing | Coordination conclusion and queue practice | Design handlers to be idempotent by task id plus attempt id. Maintenance retries expired or failed claims. |
| Subscriptions | Notification product examples above | Store subscriptions by task, run, user, and device so push notifications can stay generic and pull details after auth. |
| Summaries | Project coordination conclusion | Store generated summaries separately from raw artifacts and task state. Summaries are replaceable, raw artifacts are retained by policy. |
| Maintenance loop | SQLite/PostgreSQL concurrency docs above | Add a periodic reconciler for expired leases, orphaned artifacts, stale subscriptions, and summary refreshes. |
