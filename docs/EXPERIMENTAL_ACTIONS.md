# Experimental Local Actions

Experimental actions are opt-in. The daemon stores requests and audit events; `signal-worker` is the only process that executes local commands or reads local files.

Start the daemon with:

```powershell
.\target\debug\signal-daemon.exe --host 127.0.0.1 --port 8791 --db-path C:\signal_ping\dist\signal\signal_demo.db --token dev-token --require-token-for-read --enable-web-push --enable-experimental-actions
```

Pair a phone in **Experimental Local Actions** mode to grant `agent.wake`, `artifact.request`, `approval.decide`, and `profile.run.low`.

Run a worker in observe-only mode:

```powershell
.\target\debug\signal-worker.exe --server http://127.0.0.1:8791 --token dev-token --agent-id codex --project signal
```

Run a worker with a policy:

```powershell
.\target\debug\signal-worker.exe --server http://127.0.0.1:8791 --token dev-token --agent-id codex --project signal --policy-path .\signal.worker.policy.example.json
```

High-risk and lab profiles require explicit worker flags:

```powershell
.\target\debug\signal-worker.exe --server http://127.0.0.1:8791 --token dev-token --agent-id codex --project signal --policy-path .\signal.worker.policy.example.json --allow-high-risk
```

The worker treats commands as exact executable plus argv arrays. File requests are canonicalized under allowed roots and are capped by extension and byte size.
