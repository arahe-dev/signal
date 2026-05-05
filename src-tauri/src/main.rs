#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopConfig {
    port: u16,
    token: String,
    public_base_url: String,
    vapid_subject: String,
    enable_experimental_actions: bool,
    refresh_tailscale: bool,
    stop_existing_signal_daemons: bool,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            port: 8791,
            token: "dev-token".to_string(),
            public_base_url: "https://your-device.your-tailnet.ts.net".to_string(),
            vapid_subject: "mailto:you@example.com".to_string(),
            enable_experimental_actions: true,
            refresh_tailscale: true,
            stop_existing_signal_daemons: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopStatus {
    running: bool,
    managed: bool,
    healthy: bool,
    message: String,
    dashboard_url: String,
    local_app_url: String,
    phone_url: String,
    data_dir: String,
    tailscale_installed: bool,
    tailscale_detail: String,
    config: DesktopConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TailscaleStatus {
    installed: bool,
    detail: String,
}

struct ManagedDaemon {
    child: CommandChild,
    config: DesktopConfig,
}

#[derive(Default)]
struct DesktopState {
    daemon: Mutex<Option<ManagedDaemon>>,
    logs: Arc<Mutex<Vec<String>>>,
    last_config: Mutex<Option<DesktopConfig>>,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn push_log(logs: &Arc<Mutex<Vec<String>>>, line: impl Into<String>) {
    let mut logs = logs.lock().unwrap();
    logs.push(format!("{} {}", now_ms(), line.into()));
    if logs.len() > 400 {
        let excess = logs.len() - 400;
        logs.drain(0..excess);
    }
}

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn dashboard_url(config: &DesktopConfig) -> String {
    format!(
        "http://127.0.0.1:{}/dashboard?token={}",
        config.port, config.token
    )
}

fn local_app_url(config: &DesktopConfig) -> String {
    format!("http://127.0.0.1:{}/app", config.port)
}

fn phone_url(config: &DesktopConfig) -> String {
    format!("{}/app", config.public_base_url.trim_end_matches('/'))
}

fn validate_config(config: &DesktopConfig) -> Result<(), String> {
    if config.port == 0 {
        return Err("Port must be greater than zero.".to_string());
    }
    if config.token.trim().is_empty() {
        return Err("Admin token cannot be empty.".to_string());
    }
    if !config.vapid_subject.starts_with("mailto:") && !config.vapid_subject.starts_with("https://")
    {
        return Err("VAPID contact must start with mailto: or https://.".to_string());
    }
    if reqwest::Url::parse(&config.public_base_url).is_err() {
        return Err("Public base URL must be a valid URL.".to_string());
    }
    Ok(())
}

async fn health_ok(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|value| value.get("ok").and_then(|ok| ok.as_bool()))
            .unwrap_or(false),
        _ => false,
    }
}

async fn wait_for_health(port: u16, attempts: usize) -> bool {
    for _ in 0..attempts {
        if health_ok(port).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

fn check_tailscale_inner() -> TailscaleStatus {
    match Command::new("tailscale").arg("version").output() {
        Ok(output) if output.status.success() => {
            let detail = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("Tailscale installed")
                .to_string();
            TailscaleStatus {
                installed: true,
                detail,
            }
        }
        Ok(output) => TailscaleStatus {
            installed: false,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(_) => TailscaleStatus {
            installed: false,
            detail: "Tailscale CLI not found on PATH.".to_string(),
        },
    }
}

fn stop_known_signal_daemons() {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/IM", "signal-daemon.exe", "/F"])
            .output();
    }
}

fn refresh_tailscale_serve_inner(
    logs: &Arc<Mutex<Vec<String>>>,
    config: &DesktopConfig,
) -> Result<String, String> {
    validate_config(config)?;
    let local = format!("http://127.0.0.1:{}", config.port);
    push_log(
        logs,
        format!("Refreshing Tailscale Serve: https=443 -> {local}"),
    );
    let output = Command::new("tailscale")
        .args(["serve", "--bg", "--https=443", &local])
        .output()
        .map_err(|e| format!("Unable to run tailscale serve: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        push_log(logs, format!("Tailscale Serve failed: {stderr}"));
        return Err(if stderr.is_empty() {
            "tailscale serve failed.".to_string()
        } else {
            stderr
        });
    }
    push_log(logs, "Tailscale Serve refreshed");
    Ok("Tailscale Serve refreshed.".to_string())
}

fn status_from(
    app: &tauri::AppHandle,
    state: &DesktopState,
    config: DesktopConfig,
    healthy: bool,
    message: impl Into<String>,
) -> Result<DesktopStatus, String> {
    let managed = state.daemon.lock().unwrap().is_some();
    let tailscale = check_tailscale_inner();
    Ok(DesktopStatus {
        running: healthy || managed,
        managed,
        healthy,
        message: message.into(),
        dashboard_url: dashboard_url(&config),
        local_app_url: local_app_url(&config),
        phone_url: phone_url(&config),
        data_dir: app_data_dir(app)?.display().to_string(),
        tailscale_installed: tailscale.installed,
        tailscale_detail: tailscale.detail,
        config,
    })
}

#[tauri::command]
fn default_config() -> DesktopConfig {
    DesktopConfig::default()
}

#[tauri::command]
fn check_tailscale() -> TailscaleStatus {
    check_tailscale_inner()
}

#[tauri::command]
fn install_tailscale(state: State<'_, DesktopState>) -> Result<String, String> {
    push_log(&state.logs, "Starting Tailscale installer through winget");
    Command::new("winget")
        .args([
            "install",
            "--id",
            "Tailscale.Tailscale",
            "--exact",
            "--source",
            "winget",
        ])
        .spawn()
        .map(|_| "Tailscale installer started through winget.".to_string())
        .map_err(|e| format!("Unable to start winget: {e}"))
}

#[tauri::command]
fn refresh_tailscale_serve(
    state: State<'_, DesktopState>,
    config: DesktopConfig,
) -> Result<String, String> {
    refresh_tailscale_serve_inner(&state.logs, &config)
}

#[tauri::command]
async fn get_status(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopStatus, String> {
    let config = state
        .last_config
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    let healthy = health_ok(config.port).await;
    status_from(&app, &state, config, healthy, "Status refreshed.")
}

#[tauri::command]
async fn start_daemon(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    config: DesktopConfig,
) -> Result<DesktopStatus, String> {
    validate_config(&config)?;
    *state.last_config.lock().unwrap() = Some(config.clone());

    if state.daemon.lock().unwrap().is_some() {
        let healthy = health_ok(config.port).await;
        return status_from(
            &app,
            &state,
            config,
            healthy,
            "Signal daemon is already managed.",
        );
    }

    if config.stop_existing_signal_daemons {
        push_log(&state.logs, "Stopping existing signal-daemon.exe processes");
        stop_known_signal_daemons();
    } else if health_ok(config.port).await {
        return status_from(
            &app,
            &state,
            config,
            true,
            "A daemon is already running on this port. The desktop app will use it.",
        );
    }

    let data_dir = app_data_dir(&app)?;
    let db_path = data_dir.join("signal_desktop.db");
    let vapid_path = data_dir.join("signal_vapid.json");
    let mut args = vec![
        "--host=127.0.0.1".to_string(),
        format!("--port={}", config.port),
        format!("--db-path={}", db_path.display()),
        format!("--token={}", config.token),
        "--require-token-for-read".to_string(),
        "--enable-web-push".to_string(),
        format!("--vapid-file={}", vapid_path.display()),
        format!("--vapid-subject={}", config.vapid_subject),
        format!("--public-base-url={}", config.public_base_url),
    ];
    if config.enable_experimental_actions {
        args.push("--enable-experimental-actions".to_string());
    }

    push_log(
        &state.logs,
        format!("Starting signal-daemon on port {}", config.port),
    );
    let sidecar = app
        .shell()
        .sidecar("signal-daemon")
        .map_err(|e| format!("Unable to load signal-daemon sidecar: {e}"))?
        .args(args);
    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| format!("Unable to start signal-daemon: {e}"))?;
    let logs = state.logs.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    push_log(
                        &logs,
                        format!("daemon: {}", String::from_utf8_lossy(&bytes).trim()),
                    );
                }
                CommandEvent::Stderr(bytes) => {
                    push_log(
                        &logs,
                        format!("daemon error: {}", String::from_utf8_lossy(&bytes).trim()),
                    );
                }
                CommandEvent::Terminated(payload) => {
                    push_log(&logs, format!("daemon exited: {:?}", payload));
                }
                _ => {}
            }
        }
    });

    *state.daemon.lock().unwrap() = Some(ManagedDaemon {
        child,
        config: config.clone(),
    });

    if config.refresh_tailscale && check_tailscale_inner().installed {
        let _ = refresh_tailscale_serve_inner(&state.logs, &config);
    }

    let healthy = wait_for_health(config.port, 24).await;
    let message = if healthy {
        "Signal daemon started."
    } else {
        "Signal daemon was launched, but /health did not respond yet. Check logs."
    };
    status_from(&app, &state, config, healthy, message)
}

#[tauri::command]
async fn stop_daemon(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopStatus, String> {
    let config = state
        .last_config
        .lock()
        .unwrap()
        .clone()
        .or_else(|| {
            state
                .daemon
                .lock()
                .unwrap()
                .as_ref()
                .map(|daemon| daemon.config.clone())
        })
        .unwrap_or_default();

    let daemon = state.daemon.lock().unwrap().take();
    if let Some(daemon) = daemon {
        push_log(&state.logs, "Stopping managed signal-daemon");
        let _ = daemon.child.kill();
    }
    let healthy = health_ok(config.port).await;
    status_from(&app, &state, config, healthy, "Stop requested.")
}

#[tauri::command]
fn daemon_logs(state: State<'_, DesktopState>) -> Vec<String> {
    state.logs.lock().unwrap().clone()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            default_config,
            check_tailscale,
            install_tailscale,
            refresh_tailscale_serve,
            get_status,
            start_daemon,
            stop_daemon,
            daemon_logs
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(state) = window.try_state::<DesktopState>() {
                    if let Some(daemon) = state.daemon.lock().unwrap().take() {
                        let _ = daemon.child.kill();
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Signal desktop");
}
