# Risk Register

Ratings are pragmatic planning estimates: `Critical`, `High`, `Medium`, or `Low`. All risks remain open until a phase acceptance test proves the mitigation.

## Security Risks

| ID | Risk | Impact | Likelihood | Mitigation | Acceptance signal |
| --- | --- | --- | --- | --- | --- |
| SEC-01 | Phone or daemon path becomes arbitrary remote shell. | Critical | Medium | Keep daemon limited to message, push, and auth. Put all execution in `signal-worker`; phone actions can approve exact grants, not stream shell input. | Security test proves daemon has no command execution API or shell spawn path. |
| SEC-02 | Push payload leaks prompt text, file paths, tokens, or command output. | High | Medium | Use generic payloads and `/app?message=<id>` deep links. Fetch sensitive content after auth inside the app. Follow Web Push service-worker model from [MDN Push API](https://developer.mozilla.org/en-US/docs/Web/API/Push_API). | Captured push payload fixtures contain no sensitive body fields. |
| SEC-03 | Grant replay or stale approval runs a different command than the user approved. | High | Medium | Hash executable, argv, cwd, env allowlist, artifact policy, requester, and expiry. Make grants single-use where possible. | Mutating any hashed field causes worker rejection before spawn. |
| SEC-04 | Local IPC is reachable by another user or remote client. | High | Medium | Use named pipe ACLs for the current user and reject remote clients with `PIPE_REJECT_REMOTE_CLIENTS` where available; Microsoft documents named pipes as network-accessible unless restricted ([named pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes), [CreateNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea)). | Remote and wrong-user connection attempts fail in tests. |
| SEC-05 | CLI prompt injection or shell interpolation turns structured work into unintended commands. | High | Medium | Avoid shell strings; pass argv arrays. Keep env allowlisted, cwd explicit, timeout capped, output capped, and artifact capture explicit. | Adapter contract tests include shell metacharacters in args without command expansion. |
| SEC-06 | Interactive PTY becomes remote control from a phone. | Critical | Low | Limit ConPTY to a desktop worker or tray with local foreground confirmation. Use Microsoft Pseudoconsole APIs only when a human-present session is required ([Pseudoconsoles](https://learn.microsoft.com/en-us/windows/console/pseudoconsoles)). | Phone-origin inputs cannot create, attach to, or type into a PTY. |
| SEC-07 | Captured artifacts persist secrets longer than intended. | High | Medium | Store artifacts with `ttl`, `pinned`, media type, owner, and redaction hooks. Default logs and screenshots to short TTL unless pinned. | Pruner deletes expired unpinned artifacts and leaves audit metadata. |

## Storage Risks

| ID | Risk | Impact | Likelihood | Mitigation | Acceptance signal |
| --- | --- | --- | --- | --- | --- |
| STO-01 | Raw patches, logs, and screenshots bloat the database. | High | High | Store raw bytes in a content-addressed artifact directory; keep only metadata in `artifacts`. | Database size remains stable while repeated captures dedupe by `sha256`. |
| STO-02 | Artifact metadata points to missing or corrupt bytes. | Medium | Medium | Write temp file, fsync where practical, compute `sha256`, then atomically move into content-addressed location before committing metadata. Add orphan sweeper. | Integrity check can verify size and sha256 for every unexpired artifact. |
| STO-03 | Artifact pruning deletes something needed for an active task. | High | Low | Never prune pinned artifacts or artifacts referenced by active runs/tasks. Use a grace window after TTL expiry. | Pruner dry-run reports references before deletion. |
| STO-04 | Git capture mishandles unusual paths. | Medium | Medium | Use `git status --porcelain=v2 -z` and `git worktree list --porcelain -z`; Git documents these as script-friendly formats ([status](https://git-scm.com/docs/git-status), [worktree](https://git-scm.com/docs/git-worktree)). | Tests cover spaces, quotes, unicode filenames, and newline-containing paths where supported. |
| STO-05 | Binary patches and screenshots exceed practical phone/app review size. | Medium | Medium | Capture binary diffs only when requested; store screenshots as capped artifacts with thumbnails or metadata summaries. | Large artifacts are truncated or rejected with explicit metadata. |

## Compatibility Risks

| ID | Risk | Impact | Likelihood | Mitigation | Acceptance signal |
| --- | --- | --- | --- | --- | --- |
| COMP-01 | iOS Web Push works only for eligible Home Screen web apps and user gestures. | High | Medium | Make installability, manifest identity, permission state, and user-gesture subscription first-class; WebKit documents iOS Home Screen requirements ([WebKit](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/)). | iOS device test shows subscribe, receive, click, and reopen behavior. |
| COMP-02 | Notification actions vary by browser and OS. | Medium | High | Treat actions as progressive enhancement. Always include a click-through deep link fallback; ntfy documents browser action support differences ([ntfy web](https://docs.ntfy.sh/subscribe/web/)). | Unsupported action browser still opens `/app?message=<id>`. |
| COMP-03 | Manifest `id`, `scope`, and `start_url` behavior differs across browsers. | Medium | Medium | Use conservative manifest values and test install/update. Use documented manifest members from [web.dev](https://web.dev/learn/pwa/web-app-manifest), [MDN scope](https://developer.mozilla.org/docs/Web/Progressive_web_apps/Manifest/Reference/scope), and [MDN start_url](https://developer.mozilla.org/en-US/docs/Web/Manifest/start_url). | Existing install survives manifest update without duplicate app identity in target browsers. |
| COMP-04 | Windows process controls vary by OS version. | Medium | Medium | Feature-detect Job Object and ConPTY support; keep noninteractive execution usable without PTY. Microsoft documents ConPTY support from Windows 10 1809 ([CreatePseudoConsole](https://learn.microsoft.com/en-us/windows/console/createpseudoconsole)). | Worker startup reports capability flags and skips unsupported interactive features. |
| COMP-05 | Codex and OpenCode CLI/API flags change. | Medium | High | Version-check adapters, keep thin wrappers, and add contract tests for `codex exec`, `opencode run`, and OpenCode server/session APIs ([OpenCode CLI](https://opencode.ai/docs/cli/), [OpenCode server](https://opencode.ai/docs/server/)). | Adapter test fails clearly with remediation when tool version is unsupported. |
| COMP-06 | Git version differences affect porcelain output or patch id. | Low | Medium | Define minimum Git version and test commands used: status, rev-parse, worktree, diff, patch-id ([git docs](https://git-scm.com/docs)). | Startup check records Git version and disables unsupported capture features. |

## Product-Scope Risks

| ID | Risk | Impact | Likelihood | Mitigation | Acceptance signal |
| --- | --- | --- | --- | --- | --- |
| PROD-01 | Native mobile app work distracts from improving the existing PWA. | Medium | Medium | Keep Web Push/PWA primary. Open a native track only for a documented standards gap with measured user impact. | Roadmap issue links to measured PWA gap before native work starts. |
| PROD-02 | Daemon absorbs worker, Git, artifact, and agent responsibilities. | High | Medium | Enforce daemon boundary in code review: daemon can enqueue and notify only. Worker owns execution and capture. | Architecture tests or imports show daemon has no worker adapter dependencies. |
| PROD-03 | Coordination system becomes a broad agent platform before core flows are reliable. | Medium | High | Phase coordination after PWA, artifacts, and worker runtime. Keep MCP/stdio bridge as later work. | Phase gates block MCP bridge until worker grants and artifacts pass tests. |
| PROD-04 | Users expect exactly-once task execution. | Medium | Medium | Document at-least-once semantics. Use idempotency keys, leases, fencing tokens, and maintenance retries; do not market exactly-once behavior. | User-facing docs and API names use `attempt` and `retry`, not exactly-once language. |
| PROD-05 | Notification actions become too dense or confusing on mobile. | Low | Medium | Start with open, approve, reject, and cancel. Add more actions only when they replace a common app-open workflow. | Usability pass shows unsupported or hidden actions do not block task completion. |

## Review Cadence

- Revisit this register at the end of each roadmap phase.
- Close a risk only when an automated test, manual device test, or security review proves the acceptance signal.
- Add new risks when a phase expands daemon scope, introduces a new adapter, stores a new artifact type, or changes mobile notification behavior.
