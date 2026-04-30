# Phase E Release Smoke

1. Build release:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

2. Start release daemon:

```powershell
cd .\dist\signal
powershell -ExecutionPolicy Bypass -File .\scripts\start_release.ps1 -StopExisting -Port 8791 -RunDoctor
```

3. Confirm doctor passes or only shows expected warnings on a fresh DB.

4. Open dashboard:

```text
http://127.0.0.1:8791/dashboard?token=dev-token
```

5. Pair phone from dashboard.

6. Enable notifications from `/app` on the phone.

7. Send custom debug push from dashboard and confirm iPhone receives it.

8. Confirm ask/reply:

```powershell
.\signal-cli.exe --server http://127.0.0.1:8791 --token dev-token ask --title "Release smoke" --body "Reply yes" --source manual --agent-id smoke --project signal --timeout 2m --json
```

9. Revoke device and confirm old device token is blocked.

10. Reset all devices and confirm messages/replies remain.

11. Package zip:

```powershell
powershell -ExecutionPolicy Bypass -File ..\..\scripts\package_release.ps1 -NoBuild
```
