const { invoke } = window.__TAURI__.core;

const els = {
  statusPill: document.getElementById("status-pill"),
  start: document.getElementById("start-btn"),
  stop: document.getElementById("stop-btn"),
  reloadFrame: document.getElementById("reload-frame-btn"),
  port: document.getElementById("port-input"),
  token: document.getElementById("token-input"),
  publicUrl: document.getElementById("public-url-input"),
  vapid: document.getElementById("vapid-input"),
  experimental: document.getElementById("experimental-input"),
  tailscaleRefresh: document.getElementById("tailscale-refresh-input"),
  stopExisting: document.getElementById("stop-existing-input"),
  daemonStatus: document.getElementById("daemon-status"),
  tailscaleStatus: document.getElementById("tailscale-status"),
  dataDir: document.getElementById("data-dir"),
  dashboardLink: document.getElementById("dashboard-link"),
  localAppLink: document.getElementById("local-app-link"),
  phoneLink: document.getElementById("phone-link"),
  frame: document.getElementById("dashboard-frame"),
  workspace: document.querySelector(".workspace"),
  tailscaleCheck: document.getElementById("tailscale-check-btn"),
  tailscaleInstall: document.getElementById("tailscale-install-btn"),
  tailscaleServe: document.getElementById("tailscale-serve-btn"),
  logsRefresh: document.getElementById("logs-refresh-btn"),
  logs: document.getElementById("logs")
};

let lastStatus = null;

function configFromForm() {
  return {
    port: Number(els.port.value || 8791),
    token: els.token.value || "dev-token",
    publicBaseUrl: els.publicUrl.value.trim(),
    vapidSubject: els.vapid.value.trim(),
    enableExperimentalActions: els.experimental.checked,
    refreshTailscale: els.tailscaleRefresh.checked,
    stopExistingSignalDaemons: els.stopExisting.checked
  };
}

function setBusy(isBusy) {
  for (const button of [
    els.start,
    els.stop,
    els.tailscaleCheck,
    els.tailscaleInstall,
    els.tailscaleServe
  ]) {
    button.disabled = isBusy;
  }
}

function setPill(text, cls = "") {
  els.statusPill.textContent = text;
  els.statusPill.className = `pill ${cls}`.trim();
}

function applyConfig(config) {
  els.port.value = config.port;
  els.token.value = config.token;
  els.publicUrl.value = config.publicBaseUrl;
  els.vapid.value = config.vapidSubject;
  els.experimental.checked = config.enableExperimentalActions;
  els.tailscaleRefresh.checked = config.refreshTailscale;
  els.stopExisting.checked = config.stopExistingSignalDaemons;
}

function applyStatus(status) {
  lastStatus = status;
  const cls = status.healthy ? "ok" : status.running ? "warn" : "";
  setPill(status.healthy ? "Running" : status.running ? "Starting" : "Stopped", cls);

  els.daemonStatus.textContent = status.message;
  els.tailscaleStatus.textContent = status.tailscaleInstalled
    ? status.tailscaleDetail || "Installed"
    : status.tailscaleDetail || "Not installed";
  els.dataDir.textContent = status.dataDir || "Unknown";

  els.dashboardLink.href = status.dashboardUrl;
  els.dashboardLink.textContent = status.dashboardUrl;
  els.localAppLink.href = status.localAppUrl;
  els.localAppLink.textContent = status.localAppUrl;
  if (status.phoneUrl) {
    els.phoneLink.href = status.phoneUrl;
    els.phoneLink.textContent = status.phoneUrl;
  } else {
    els.phoneLink.removeAttribute("href");
    els.phoneLink.textContent = "Set Public Tailnet URL";
  }

  if (status.healthy) {
    if (els.frame.src !== status.dashboardUrl) {
      els.frame.src = status.dashboardUrl;
    }
    els.workspace.classList.add("loaded");
  }
}

function showError(error) {
  setPill("Error", "error");
  els.daemonStatus.textContent = error?.message || String(error);
}

async function refreshLogs() {
  const logs = await invoke("daemon_logs");
  els.logs.textContent = logs.length ? logs.slice(-80).join("\n") : "No logs yet.";
}

async function refreshStatus() {
  try {
    const status = await invoke("get_status");
    applyStatus(status);
    await refreshLogs();
  } catch (error) {
    showError(error);
  }
}

async function start() {
  setBusy(true);
  setPill("Starting", "warn");
  try {
    const status = await invoke("start_daemon", { config: configFromForm() });
    applyStatus(status);
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
    await refreshLogs();
  }
}

async function stop() {
  setBusy(true);
  try {
    const status = await invoke("stop_daemon");
    applyStatus(status);
    if (!status.healthy) {
      els.workspace.classList.remove("loaded");
      els.frame.removeAttribute("src");
    }
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
    await refreshLogs();
  }
}

async function checkTailscale() {
  setBusy(true);
  try {
    const status = await invoke("check_tailscale");
    els.tailscaleStatus.textContent = status.installed
      ? status.detail || "Installed"
      : status.detail || "Not installed";
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
  }
}

async function installTailscale() {
  if (!confirm("Start the Tailscale installer with winget?")) return;
  setBusy(true);
  try {
    els.tailscaleStatus.textContent = await invoke("install_tailscale");
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
    await refreshLogs();
  }
}

async function refreshServe() {
  setBusy(true);
  try {
    els.tailscaleStatus.textContent = await invoke("refresh_tailscale_serve", {
      config: configFromForm()
    });
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
    await refreshLogs();
  }
}

function reloadFrame() {
  const url = lastStatus?.dashboardUrl || els.frame.src;
  if (url) {
    els.frame.src = url;
    els.workspace.classList.add("loaded");
  }
}

els.start.addEventListener("click", start);
els.stop.addEventListener("click", stop);
els.reloadFrame.addEventListener("click", reloadFrame);
els.tailscaleCheck.addEventListener("click", checkTailscale);
els.tailscaleInstall.addEventListener("click", installTailscale);
els.tailscaleServe.addEventListener("click", refreshServe);
els.logsRefresh.addEventListener("click", refreshLogs);

const config = await invoke("default_config");
applyConfig(config);
await refreshStatus();
setInterval(refreshStatus, 5000);
