# Signal Desktop

Signal Desktop is a Tauri wrapper around the local Signal daemon. It gives Windows users one app surface instead of starting the daemon from PowerShell and opening the Tailnet dashboard manually.

## What Ships

- `signal-desktop.exe` Tauri app
- Bundled sidecars:
  - `signal-daemon.exe`
  - `signal-cli.exe`
  - `signal-worker.exe`
- MSI installer
- NSIS setup EXE

The sidecars are generated at build time and are not committed.

## Build

```powershell
npm install
npm run tauri:build
```

The build script runs:

```powershell
.\scripts\prepare_tauri_sidecars.ps1
```

That script builds the daemon, CLI, and worker in release mode and copies them into `src-tauri\binaries` with the target-triple suffix required by Tauri sidecars.

## Outputs

```text
target\release\signal-desktop.exe
target\release\bundle\msi\Signal_0.1.0_x64_en-US.msi
target\release\bundle\nsis\Signal_0.1.0_x64-setup.exe
```

## Runtime Behavior

- The app starts the bundled daemon on `127.0.0.1`.
- Daemon state is stored under the app data directory.
- The dashboard loads inside the Tauri window.
- Tailscale can be checked, installed through `winget`, and refreshed through `tailscale serve`.
- The app stops its managed daemon when the window closes.

## Release Caveat

These installers are unsigned. Windows SmartScreen warnings are expected until a signing path is added.
