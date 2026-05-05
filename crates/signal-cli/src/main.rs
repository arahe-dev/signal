use base64::Engine as _;
use clap::{Parser, Subcommand, ValueEnum};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "signal-cli")]
#[command(about = "Signal CLI - local-first push/reply protocol client", long_about = None)]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8791")]
    server: String,

    #[arg(long)]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Debug, ValueEnum)]
enum Priority {
    Low,
    Normal,
    Urgent,
    Silent,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::Low => write!(f, "low"),
            Priority::Normal => write!(f, "normal"),
            Priority::Urgent => write!(f, "urgent"),
            Priority::Silent => write!(f, "silent"),
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    Send {
        #[arg(long)]
        title: String,
        #[arg(long)]
        body: String,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
    },
    Ask {
        #[arg(long)]
        title: String,
        #[arg(long)]
        body: String,
        #[arg(long, default_value = "cli")]
        source: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value = "10m")]
        timeout: String,
        #[arg(long)]
        no_wait: bool,
        #[arg(long = "reply-option")]
        reply_options: Vec<String>,
        #[arg(long, value_enum, default_value_t = Priority::Normal)]
        priority: Priority,
        #[arg(long)]
        consume: bool,
        #[arg(long)]
        json: bool,
    },
    Inbox {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
    LatestReply {
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        consume: Option<bool>,
    },
    Reply {
        #[arg(long)]
        message_id: String,
        #[arg(long)]
        body: String,
        #[arg(long, default_value = "cli")]
        source: String,
    },
    Events {
        #[arg(long)]
        after_seq: Option<i64>,
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long)]
        json: bool,
    },
    Context {
        #[command(subcommand)]
        subcommand: ContextSubcommand,
    },
    Artifact {
        #[command(subcommand)]
        subcommand: ArtifactSubcommand,
    },
    Pair {
        #[command(subcommand)]
        subcommand: PairSubcommand,
    },
    Devices {
        #[command(subcommand)]
        subcommand: DevicesSubcommand,
    },
    Doctor {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        public_url: Option<String>,
        #[arg(long)]
        check_public: bool,
        #[arg(long)]
        check_push: bool,
        #[arg(long, default_value_t = 10)]
        timeout_seconds: u64,
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Subcommand)]
enum ContextSubcommand {
    Capture {
        #[arg(long)]
        message_id: String,
        #[arg(long, default_value = "progress")]
        stage: String,
        #[arg(long, default_value = "agent:codex")]
        source: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ArtifactSubcommand {
    Upload {
        #[arg(long)]
        message_id: String,
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value = "screenshot")]
        kind: String,
        #[arg(long)]
        media_type: Option<String>,
        #[arg(long)]
        snapshot_id: Option<String>,
        #[arg(long)]
        pinned: bool,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        message_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum PairSubcommand {
    Start {
        #[arg(long)]
        name: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum DevicesSubcommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Revoke {
        #[arg(long)]
        id: String,
        #[arg(long)]
        json: bool,
    },
    #[command(alias = "reset")]
    ResetAll {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
struct CreateMessageRequest {
    title: String,
    body: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
}

#[derive(Debug, Serialize)]
struct AskRequest {
    agent_id: Option<String>,
    project: Option<String>,
    title: String,
    body: String,
    timeout_seconds: u64,
    priority: String,
    reply_mode: String,
    reply_options: Vec<String>,
    source: String,
}

#[derive(Debug, Serialize)]
struct CreateReplyRequest {
    body: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_device: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateContextSnapshotRequest {
    source: String,
    stage: String,
    status: Value,
    repo_root_display: Option<String>,
    git_common_dir_hash: Option<String>,
    worktree_path_display: Option<String>,
    branch: Option<String>,
    head_oid: Option<String>,
    upstream: Option<String>,
    ahead: Option<i64>,
    behind: Option<i64>,
    dirty: bool,
    staged_count: i64,
    unstaged_count: i64,
    untracked_count: i64,
    worktrees: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ContextSnapshot {
    id: String,
    message_id: String,
    captured_at: String,
    source: String,
    stage: String,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    head_oid: Option<String>,
    #[serde(default)]
    dirty: bool,
    staged_count: i64,
    unstaged_count: i64,
    untracked_count: i64,
}

#[derive(Debug, Serialize)]
struct UploadArtifactRequest {
    message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_id: Option<String>,
    kind: String,
    media_type: String,
    data_base64: String,
    pinned: bool,
    metadata: Value,
}

#[derive(Debug, Deserialize, Serialize)]
struct ArtifactMetadata {
    id: String,
    message_id: String,
    #[serde(default)]
    snapshot_id: Option<String>,
    kind: String,
    media_type: String,
    sha256: String,
    size_bytes: i64,
    storage_uri: String,
    created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct EventLogEntry {
    seq: Option<i64>,
    event_id: String,
    event_type: String,
    source: String,
    actor: String,
    #[serde(default)]
    subject: Option<String>,
    event_time: String,
    inserted_at: String,
    data_json: String,
    #[serde(default)]
    resource: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Message {
    id: String,
    title: String,
    body: String,
    source: String,
    status: String,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct Reply {
    id: String,
    message_id: String,
    body: String,
    source: String,
    status: String,
    created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct AskResponse {
    ask_id: String,
    message_id: String,
    status: String,
    expires_at: Option<String>,
    message_url: String,
}

#[derive(Debug, Deserialize)]
struct WaitResponse {
    status: String,
    ask_id: String,
    message_id: String,
    reply_id: Option<String>,
    reply: Option<String>,
    elapsed_seconds: u64,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct AskOutput {
    status: String,
    message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_id: Option<String>,
    reply: Option<String>,
    elapsed_seconds: u64,
    timed_out: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ask_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PairStartResponse {
    pairing_code: String,
    code_prefix: String,
    pair_url: String,
    qr_data: String,
    #[serde(default)]
    qr_svg: Option<String>,
    expires_in_seconds: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeviceInfo {
    id: String,
    name: String,
    kind: String,
    token_prefix: String,
    paired_at: String,
    last_seen_at: Option<String>,
    revoked_at: Option<String>,
    is_active: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeviceListResponse {
    devices: Vec<DeviceInfo>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeviceRevokeResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeviceResetResponse {
    success: bool,
    devices_revoked: usize,
    subscriptions_revoked: usize,
    pairing_codes_cleared: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct HealthResponse {
    ok: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct DiagnosticsResponse {
    #[serde(default)]
    daemon_running: bool,
    #[serde(default)]
    version: String,
    #[serde(default)]
    db_path: String,
    #[serde(default)]
    public_base_url: Option<String>,
    #[serde(default)]
    web_push_enabled: bool,
    #[serde(default)]
    vapid_public_key_length: Option<usize>,
    #[serde(default)]
    vapid_public_key_first_byte: Option<u8>,
    #[serde(default)]
    active_devices: usize,
    #[serde(default)]
    revoked_devices: usize,
    #[serde(default)]
    active_subscriptions: usize,
    #[serde(default)]
    revoked_or_stale_subscriptions: usize,
    #[serde(default)]
    legacy_unbound_subscriptions: usize,
    #[serde(default)]
    last_push_success_at: Option<String>,
    #[serde(default)]
    last_push_error: Option<String>,
    #[serde(default)]
    suggested_fix: Option<String>,
    #[serde(default)]
    daemon: Option<DiagnosticsDaemon>,
    #[serde(default)]
    config: Option<DiagnosticsConfig>,
    #[serde(default)]
    vapid: Option<DiagnosticsVapid>,
    #[serde(default)]
    devices: Option<DiagnosticsDevices>,
    #[serde(default)]
    push_subscriptions: Option<DiagnosticsPushSubscriptions>,
    #[serde(default)]
    messages: Option<DiagnosticsMessages>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct DiagnosticsDaemon {
    ok: bool,
    version: String,
    db_path: String,
    server_time: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct DiagnosticsConfig {
    public_base_url: Option<String>,
    web_push_enabled: bool,
    require_token_for_read: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct DiagnosticsVapid {
    public_key_present: bool,
    public_key_len: Option<usize>,
    public_key_first_byte: Option<u8>,
    private_matches_public: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct DiagnosticsDevices {
    active: usize,
    revoked: usize,
    total: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct DiagnosticsPushSubscriptions {
    active: usize,
    revoked: usize,
    stale: usize,
    legacy: usize,
    total: usize,
    last_success_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct DiagnosticsMessages {
    active_pending: usize,
    last_ask_at: Option<String>,
    last_reply_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct PushTestRequest {
    title: String,
    body: String,
    url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct PushTestResponse {
    success: bool,
    message: Option<String>,
    summary: Option<PushSummary>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct PushSummary {
    attempted: usize,
    sent: usize,
    failed: usize,
    skipped: usize,
    skipped_revoked: usize,
    skipped_stale: usize,
    skipped_legacy: usize,
    #[serde(default)]
    errors: Vec<PushError>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct PushError {
    error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum DoctorStatus {
    Pass,
    Warn,
    Fail,
    Info,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DoctorCheck {
    name: String,
    status: DoctorStatus,
    message: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct DoctorSummary {
    passes: usize,
    warnings: usize,
    failures: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DoctorOutput {
    ok: bool,
    checks: Vec<DoctorCheck>,
    summary: DoctorSummary,
    suggested_next_steps: Vec<String>,
}

struct ApiClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl ApiClient {
    fn new(base_url: String, token: Option<String>, timeout: Duration) -> Self {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    fn add_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.token {
            request.header("X-Signal-Token", token)
        } else {
            request
        }
    }

    async fn send_message(
        &self,
        title: String,
        body: String,
        source: String,
        agent_id: Option<String>,
        project: Option<String>,
    ) -> Result<Message, Box<dyn std::error::Error>> {
        let url = format!("{}/api/messages", self.base_url);
        let response = self
            .add_auth(self.client.post(&url))
            .json(&CreateMessageRequest {
                title,
                body,
                source,
                agent_id,
                project,
            })
            .send()
            .await?;
        parse_response(response, "create message").await
    }

    async fn create_ask(
        &self,
        request: &AskRequest,
    ) -> Result<AskResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/api/ask", self.base_url);
        let response = self
            .add_auth(self.client.post(&url))
            .json(request)
            .send()
            .await?;
        parse_response(response, "create ask").await
    }

    async fn wait_for_ask(
        &self,
        ask_id: &str,
        timeout_seconds: u64,
    ) -> Result<WaitResponse, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/api/ask/{}/wait?timeout_seconds={}",
            self.base_url, ask_id, timeout_seconds
        );
        let response = self.add_auth(self.client.get(&url)).send().await?;
        parse_response(response, "wait for ask").await
    }

    async fn list_messages(
        &self,
        project: Option<String>,
        agent_id: Option<String>,
        status: Option<String>,
        limit: i64,
    ) -> Result<Vec<Message>, Box<dyn std::error::Error>> {
        let mut url = format!("{}/api/messages?limit={}", self.base_url, limit);
        if let Some(p) = &project {
            url.push_str(&format!("&project={}", p));
        }
        if let Some(a) = &agent_id {
            url.push_str(&format!("&agent_id={}", a));
        }
        if let Some(s) = &status {
            url.push_str(&format!("&status={}", s));
        }
        let response = self.add_auth(self.client.get(&url)).send().await?;
        parse_response(response, "list messages").await
    }

    async fn get_latest_reply(
        &self,
        agent_id: Option<String>,
        project: Option<String>,
    ) -> Result<Option<Reply>, Box<dyn std::error::Error>> {
        let mut url = format!("{}/api/replies/latest", self.base_url);
        if let Some(a) = &agent_id {
            url.push_str(&format!("?agent_id={}", a));
        }
        if let Some(p) = &project {
            url.push_str(if url.contains('?') {
                "&project="
            } else {
                "?project="
            });
            url.push_str(p);
        }
        let response = self.add_auth(self.client.get(&url)).send().await?;
        parse_response(response, "get latest reply").await
    }

    async fn consume_reply(&self, id: &str) -> Result<Reply, Box<dyn std::error::Error>> {
        let url = format!("{}/api/replies/{}/consume", self.base_url, id);
        let response = self.add_auth(self.client.post(&url)).send().await?;
        parse_response(response, "consume reply").await
    }

    async fn create_reply(
        &self,
        message_id: String,
        body: String,
        source: String,
    ) -> Result<Reply, Box<dyn std::error::Error>> {
        let url = format!("{}/api/messages/{}/replies", self.base_url, message_id);
        let response = self
            .add_auth(self.client.post(&url))
            .json(&CreateReplyRequest {
                body,
                source,
                source_device: None,
            })
            .send()
            .await?;
        parse_response(response, "create reply").await
    }

    async fn list_events(
        &self,
        after_seq: Option<i64>,
        limit: i64,
    ) -> Result<Vec<EventLogEntry>, Box<dyn std::error::Error>> {
        let mut url = format!("{}/api/events?limit={}", self.base_url, limit);
        if let Some(after_seq) = after_seq {
            url.push_str(&format!("&after_seq={}", after_seq));
        }
        let response = self.add_auth(self.client.get(&url)).send().await?;
        parse_response(response, "list events").await
    }

    async fn capture_context(
        &self,
        message_id: &str,
        request: &CreateContextSnapshotRequest,
    ) -> Result<ContextSnapshot, Box<dyn std::error::Error>> {
        let url = format!("{}/api/messages/{}/context", self.base_url, message_id);
        let response = self
            .add_auth(self.client.post(&url))
            .json(request)
            .send()
            .await?;
        parse_response(response, "capture context").await
    }

    async fn upload_artifact(
        &self,
        request: &UploadArtifactRequest,
    ) -> Result<ArtifactMetadata, Box<dyn std::error::Error>> {
        let url = format!("{}/api/artifacts/upload", self.base_url);
        let response = self
            .add_auth(self.client.post(&url))
            .json(request)
            .send()
            .await?;
        parse_response(response, "upload artifact").await
    }

    async fn list_artifacts(
        &self,
        message_id: &str,
    ) -> Result<Vec<ArtifactMetadata>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/messages/{}/artifacts", self.base_url, message_id);
        let response = self.add_auth(self.client.get(&url)).send().await?;
        parse_response(response, "list artifacts").await
    }

    async fn pair_start(
        &self,
        device_name: String,
    ) -> Result<PairStartResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/api/pair/start", self.base_url);
        let response = self
            .add_auth(self.client.post(&url))
            .json(&serde_json::json!({
                "device_name": device_name
            }))
            .send()
            .await?;
        parse_response(response, "pair start").await
    }

    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/devices", self.base_url);
        let response = self.add_auth(self.client.get(&url)).send().await?;
        let device_list: DeviceListResponse = parse_response(response, "list devices").await?;
        Ok(device_list.devices)
    }

    async fn revoke_device(
        &self,
        device_id: String,
    ) -> Result<DeviceRevokeResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/api/devices/{}/revoke", self.base_url, device_id);
        let response = self.add_auth(self.client.post(&url)).send().await?;
        parse_response(response, "revoke device").await
    }

    async fn reset_all_devices(&self) -> Result<DeviceResetResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/api/devices/reset-all", self.base_url);
        let response = self.add_auth(self.client.post(&url)).send().await?;
        parse_response(response, "reset all devices").await
    }

    async fn health_at(
        &self,
        base_url: &str,
    ) -> Result<HealthResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/health", base_url.trim_end_matches('/'));
        let response = self.client.get(&url).send().await?;
        parse_response(response, "health").await
    }

    async fn diagnostics_at(
        &self,
        base_url: &str,
    ) -> Result<DiagnosticsResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/api/diagnostics", base_url.trim_end_matches('/'));
        let response = self.add_auth(self.client.get(&url)).send().await?;
        parse_response(response, "diagnostics").await
    }

    async fn test_push(&self) -> Result<PushTestResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/api/push/test", self.base_url);
        let response = self
            .add_auth(self.client.post(&url))
            .json(&PushTestRequest {
                title: "Signal doctor".to_string(),
                body: "Diagnostic push test".to_string(),
                url: "/app".to_string(),
            })
            .send()
            .await?;
        parse_response(response, "push test").await
    }
}

async fn parse_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
    action: &str,
) -> Result<T, Box<dyn std::error::Error>> {
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Failed to {}: HTTP {} {}", action, status, text).into());
    }
    Ok(response.json().await?)
}

fn run_git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string(),
    )
}

fn parse_status_counts(status: &str) -> (bool, i64, i64, i64) {
    let mut staged = 0;
    let mut unstaged = 0;
    let mut untracked = 0;

    for line in status.lines().filter(|line| !line.starts_with("##")) {
        if line.starts_with("??") {
            untracked += 1;
            continue;
        }
        let mut chars = line.chars();
        let index = chars.next().unwrap_or(' ');
        let worktree = chars.next().unwrap_or(' ');
        if index != ' ' {
            staged += 1;
        }
        if worktree != ' ' {
            unstaged += 1;
        }
    }

    (
        staged + unstaged + untracked > 0,
        staged,
        unstaged,
        untracked,
    )
}

fn parse_ahead_behind(value: Option<String>) -> (Option<i64>, Option<i64>) {
    let Some(value) = value else {
        return (None, None);
    };
    let parts = value
        .split_whitespace()
        .filter_map(|part| part.parse::<i64>().ok())
        .collect::<Vec<_>>();
    if parts.len() == 2 {
        (Some(parts[1]), Some(parts[0]))
    } else {
        (None, None)
    }
}

fn build_context_snapshot_request(
    repo: &Path,
    source: String,
    stage: String,
) -> CreateContextSnapshotRequest {
    let status = run_git(repo, &["status", "--porcelain=v1", "-b"]).unwrap_or_default();
    let (dirty, staged_count, unstaged_count, untracked_count) = parse_status_counts(&status);
    let (ahead, behind) = parse_ahead_behind(run_git(
        repo,
        &["rev-list", "--left-right", "--count", "@{u}...HEAD"],
    ));
    let repo_root = run_git(repo, &["rev-parse", "--show-toplevel"]).or_else(|| {
        repo.canonicalize()
            .ok()
            .map(|path| path.display().to_string())
    });
    let worktree_path = repo
        .canonicalize()
        .ok()
        .map(|path| path.display().to_string());
    let worktrees = run_git(repo, &["worktree", "list", "--porcelain"])
        .map(|raw| serde_json::json!({ "porcelain": raw }));

    CreateContextSnapshotRequest {
        source,
        stage,
        status: serde_json::json!({
            "git_status_porcelain": status,
            "captured_by": "signal-cli",
        }),
        repo_root_display: repo_root,
        git_common_dir_hash: None,
        worktree_path_display: worktree_path,
        branch: run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        head_oid: run_git(repo, &["rev-parse", "HEAD"]),
        upstream: run_git(
            repo,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        ),
        ahead,
        behind,
        dirty,
        staged_count,
        unstaged_count,
        untracked_count,
        worktrees,
    }
}

fn infer_media_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "txt" | "log" => "text/plain",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn build_upload_artifact_request(
    message_id: String,
    path: PathBuf,
    kind: String,
    media_type: Option<String>,
    snapshot_id: Option<String>,
    pinned: bool,
) -> Result<UploadArtifactRequest, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(&path)?;
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact")
        .to_string();
    Ok(UploadArtifactRequest {
        message_id,
        snapshot_id,
        kind,
        media_type: media_type.unwrap_or_else(|| infer_media_type(&path)),
        data_base64,
        pinned,
        metadata: serde_json::json!({
            "file_name": file_name,
            "uploaded_by": "signal-cli"
        }),
    })
}

pub fn parse_timeout_seconds(input: &str) -> Result<u64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("timeout cannot be empty".to_string());
    }
    let (number, multiplier) = match trimmed.chars().last().unwrap() {
        's' | 'S' => (&trimmed[..trimmed.len() - 1], 1),
        'm' | 'M' => (&trimmed[..trimmed.len() - 1], 60),
        'h' | 'H' => (&trimmed[..trimmed.len() - 1], 60 * 60),
        c if c.is_ascii_digit() => (trimmed, 1),
        _ => return Err(format!("unsupported timeout: {}", input)),
    };
    let value: u64 = number
        .parse()
        .map_err(|_| format!("invalid timeout: {}", input))?;
    Ok(value.saturating_mul(multiplier))
}

fn check(name: &str, status: DoctorStatus, message: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        status,
        message: message.into(),
    }
}

fn summarize_doctor(checks: Vec<DoctorCheck>, strict: bool) -> DoctorOutput {
    let mut summary = DoctorSummary::default();
    let mut suggested_next_steps = Vec::new();
    for check in &checks {
        match check.status {
            DoctorStatus::Pass => summary.passes += 1,
            DoctorStatus::Warn => summary.warnings += 1,
            DoctorStatus::Fail => summary.failures += 1,
            DoctorStatus::Info => {}
        }

        if check.status != DoctorStatus::Pass {
            match check.name.as_str() {
                "active_device" => {
                    suggested_next_steps.push("Pair a phone from the dashboard".to_string())
                }
                "active_push_subscription" => {
                    suggested_next_steps.push("Tap Enable Notifications in the PWA".to_string())
                }
                "public_url" => suggested_next_steps.push(
                    "Run tailscale serve --bg --https=443 http://127.0.0.1:<port>".to_string(),
                ),
                "vapid" => suggested_next_steps.push(
                    "Check VAPID config and re-subscribe the phone if keys changed".to_string(),
                ),
                "local_daemon" => {
                    suggested_next_steps.push("Start the daemon or check the port".to_string())
                }
                _ => {}
            }
        }
    }
    suggested_next_steps.sort();
    suggested_next_steps.dedup();
    let ok = summary.failures == 0 && (!strict || summary.warnings == 0);
    DoctorOutput {
        ok,
        checks,
        summary,
        suggested_next_steps,
    }
}

fn evaluate_diagnostics(d: &DiagnosticsResponse) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    checks.push(check(
        "diagnostics",
        DoctorStatus::Pass,
        format!("db={} version={}", d.db_path, d.version),
    ));

    if d.web_push_enabled {
        checks.push(check("web_push", DoctorStatus::Pass, "enabled"));
    } else {
        checks.push(check("web_push", DoctorStatus::Fail, "disabled"));
    }

    let vapid_private_matches = d.vapid.as_ref().and_then(|v| v.private_matches_public);
    if d.vapid_public_key_length == Some(65)
        && d.vapid_public_key_first_byte == Some(4)
        && vapid_private_matches != Some(false)
    {
        checks.push(check(
            "vapid",
            DoctorStatus::Pass,
            "public key valid: len=65 first=4",
        ));
    } else {
        let mut message = format!(
            "invalid VAPID public key: len={:?} first={:?}",
            d.vapid_public_key_length, d.vapid_public_key_first_byte
        );
        if vapid_private_matches == Some(false) {
            message.push_str("; private/public mismatch");
        }
        checks.push(check("vapid", DoctorStatus::Fail, message));
    }

    if d.active_devices == 0 {
        checks.push(check(
            "active_device",
            DoctorStatus::Warn,
            "No active paired devices",
        ));
    } else {
        checks.push(check(
            "active_device",
            DoctorStatus::Pass,
            format!("{} active device(s)", d.active_devices),
        ));
    }

    if d.revoked_devices > 0 {
        checks.push(check(
            "revoked_devices",
            DoctorStatus::Info,
            format!("{} revoked device(s)", d.revoked_devices),
        ));
    }

    if d.active_subscriptions == 0 {
        checks.push(check(
            "active_push_subscription",
            DoctorStatus::Warn,
            "No active device-bound push subscriptions",
        ));
    } else {
        checks.push(check(
            "active_push_subscription",
            DoctorStatus::Pass,
            format!(
                "{} active device-bound subscription(s)",
                d.active_subscriptions
            ),
        ));
    }

    if d.legacy_unbound_subscriptions > 0 {
        checks.push(check(
            "legacy_subscriptions",
            DoctorStatus::Warn,
            format!(
                "{} legacy/unbound subscription(s)",
                d.legacy_unbound_subscriptions
            ),
        ));
    }
    if d.revoked_or_stale_subscriptions > 0 {
        checks.push(check(
            "revoked_or_stale_subscriptions",
            DoctorStatus::Info,
            format!(
                "{} revoked/stale subscription(s)",
                d.revoked_or_stale_subscriptions
            ),
        ));
    }
    if let Some(success) = &d.last_push_success_at {
        checks.push(check(
            "last_push_success",
            DoctorStatus::Info,
            format!("last success at {}", success),
        ));
    }
    if let Some(error) = &d.last_push_error {
        checks.push(check(
            "last_push_error",
            DoctorStatus::Info,
            format!("last error: {}", error),
        ));
    }
    checks
}

fn evaluate_push_test(response: &PushTestResponse) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let Some(summary) = &response.summary else {
        checks.push(check(
            "push_test",
            DoctorStatus::Fail,
            response
                .message
                .clone()
                .unwrap_or_else(|| "missing push summary".to_string()),
        ));
        return checks;
    };

    if summary.attempted == 0 {
        let reason = if summary.skipped_legacy > 0 {
            "No active device-bound subscriptions; only legacy/unbound subscriptions were skipped"
        } else if summary.skipped_revoked > 0 {
            "No active subscriptions; only revoked subscriptions were skipped"
        } else if summary.skipped_stale > 0 {
            "No active subscriptions; only stale subscriptions were skipped"
        } else {
            "No active push subscriptions"
        };
        checks.push(check("push_test", DoctorStatus::Warn, reason));
    } else if summary.failed > 0 && summary.sent == 0 {
        checks.push(check(
            "push_test",
            DoctorStatus::Fail,
            format!(
                "attempted {}, sent {}, failed {}",
                summary.attempted, summary.sent, summary.failed
            ),
        ));
    } else if summary.failed > 0 {
        checks.push(check(
            "push_test",
            DoctorStatus::Warn,
            format!(
                "partial success: attempted {}, sent {}, failed {}",
                summary.attempted, summary.sent, summary.failed
            ),
        ));
    } else {
        checks.push(check(
            "push_test",
            DoctorStatus::Pass,
            format!(
                "attempted {}, sent {}, failed {}, skipped {}",
                summary.attempted, summary.sent, summary.failed, summary.skipped
            ),
        ));
    }

    for error in &summary.errors {
        if let Some(message) = &error.error {
            checks.push(check("push_error", DoctorStatus::Info, message.clone()));
        }
    }
    checks
}

fn print_doctor_human(output: &DoctorOutput) {
    println!("Signal Doctor");
    println!("=============");
    for check in &output.checks {
        let label = match check.status {
            DoctorStatus::Pass => "PASS",
            DoctorStatus::Warn => "WARN",
            DoctorStatus::Fail => "FAIL",
            DoctorStatus::Info => "INFO",
        };
        println!("[{}] {}: {}", label, check.name, check.message);
    }
    println!(
        "\nSummary: {} pass, {} warn, {} fail",
        output.summary.passes, output.summary.warnings, output.summary.failures
    );
    if !output.suggested_next_steps.is_empty() {
        println!("\nSuggested next steps:");
        for step in &output.suggested_next_steps {
            println!("- {}", step);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let wait_timeout = match &cli.command {
        Commands::Ask { timeout, .. } => parse_timeout_seconds(timeout).unwrap_or(600) + 15,
        Commands::Doctor {
            timeout_seconds, ..
        } => *timeout_seconds,
        _ => 10,
    };
    let client = ApiClient::new(
        cli.server.clone(),
        cli.token.clone(),
        Duration::from_secs(wait_timeout),
    );

    match cli.command {
        Commands::Send {
            title,
            body,
            source,
            agent_id,
            project,
        } => {
            println!("Sending message...");
            let message = client
                .send_message(title, body, source, agent_id, project)
                .await?;
            println!("Message created: {}", message.id);
            println!("Title: {}", message.title);
            println!("Status: {}", message.status);
        }
        Commands::Ask {
            title,
            body,
            source,
            agent_id,
            project,
            timeout,
            no_wait,
            reply_options,
            priority,
            consume,
            json,
        } => {
            let timeout_seconds = parse_timeout_seconds(&timeout)?;
            let ask = client
                .create_ask(&AskRequest {
                    agent_id,
                    project,
                    title,
                    body,
                    timeout_seconds,
                    priority: priority.to_string(),
                    reply_mode: "text".to_string(),
                    reply_options,
                    source,
                })
                .await?;

            if no_wait {
                if json {
                    println!("{}", serde_json::to_string(&ask)?);
                } else {
                    println!("Ask created: {}", ask.message_id);
                    println!("Status: {}", ask.status);
                    println!("URL: {}", ask.message_url);
                }
                return Ok(());
            }

            let wait = client.wait_for_ask(&ask.ask_id, timeout_seconds).await?;
            let mut output = AskOutput {
                status: wait.status.clone(),
                message_id: wait.message_id.clone(),
                reply_id: wait.reply_id.clone(),
                reply: wait.reply.clone(),
                elapsed_seconds: wait.elapsed_seconds,
                timed_out: wait.status == "timeout",
                ask_id: Some(wait.ask_id.clone()),
                message_url: Some(ask.message_url),
                reason: wait.reason,
            };

            if consume {
                if let Some(reply_id) = &wait.reply_id {
                    let consumed = client.consume_reply(reply_id).await?;
                    output.reply_id = Some(consumed.id);
                }
            }

            if json {
                println!("{}", serde_json::to_string(&output)?);
            } else if output.timed_out {
                println!("Ask timed out after {}s", output.elapsed_seconds);
            } else if output.status == "replied" {
                println!("Reply: {}", output.reply.as_deref().unwrap_or(""));
            } else {
                println!("Ask ended with status: {}", output.status);
            }
        }
        Commands::Inbox {
            project,
            agent_id,
            status,
            limit,
        } => {
            let messages = client
                .list_messages(project, agent_id, status, limit)
                .await?;
            if messages.is_empty() {
                println!("No messages found.");
            } else {
                println!("Found {} message(s):\n", messages.len());
                for msg in &messages {
                    println!(
                        "[{}] {}",
                        msg.id.chars().take(8).collect::<String>(),
                        msg.title
                    );
                    println!("From: {} | Status: {}", msg.source, msg.status);
                    println!("Created: {}", msg.created_at);
                    if let Some(p) = &msg.project {
                        println!("Project: {}", p);
                    }
                    if let Some(a) = &msg.agent_id {
                        println!("Agent: {}", a);
                    }
                    println!("Body: {}", msg.body);
                    println!();
                }
            }
        }
        Commands::LatestReply {
            agent_id,
            project,
            consume,
        } => match client.get_latest_reply(agent_id, project).await? {
            Some(r) => {
                println!("Latest pending reply:");
                println!("ID: {}", r.id);
                println!("Message ID: {}", r.message_id);
                println!("Body: {}", r.body);
                println!("From: {}", r.source);
                println!("Status: {}", r.status);
                println!("Created: {}", r.created_at);
                if consume.unwrap_or(false) {
                    let updated = client.consume_reply(&r.id).await?;
                    println!("Reply consumed: {}", updated.id);
                }
            }
            None => println!("No pending replies found."),
        },
        Commands::Reply {
            message_id,
            body,
            source,
        } => {
            let reply = client.create_reply(message_id, body, source).await?;
            println!("Reply created: {}", reply.id);
            println!("Status: {}", reply.status);
        }
        Commands::Events {
            after_seq,
            limit,
            json,
        } => {
            let events = client.list_events(after_seq, limit).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&events)?);
            } else if events.is_empty() {
                println!("No events found.");
            } else {
                for event in &events {
                    println!(
                        "#{} {} {}",
                        event.seq.unwrap_or_default(),
                        event.event_type,
                        event.inserted_at
                    );
                    println!("Source: {} | Actor: {}", event.source, event.actor);
                    if let Some(subject) = &event.subject {
                        println!("Subject: {}", subject);
                    }
                    if let Some(resource) = &event.resource {
                        println!("Resource: {}", resource);
                    }
                    println!("Data: {}", event.data_json);
                    println!();
                }
            }
        }
        Commands::Context { subcommand } => match subcommand {
            ContextSubcommand::Capture {
                message_id,
                stage,
                source,
                repo,
                json,
            } => {
                let request = build_context_snapshot_request(&repo, source, stage);
                let snapshot = client.capture_context(&message_id, &request).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&snapshot)?);
                } else {
                    println!("Context snapshot captured: {}", snapshot.id);
                    println!("Message: {}", snapshot.message_id);
                    println!("Stage: {}", snapshot.stage);
                    println!(
                        "Branch: {}",
                        snapshot.branch.as_deref().unwrap_or("unknown")
                    );
                    println!(
                        "HEAD: {}",
                        snapshot.head_oid.as_deref().unwrap_or("unknown")
                    );
                    println!(
                        "Dirty: {} (staged={}, unstaged={}, untracked={})",
                        snapshot.dirty,
                        snapshot.staged_count,
                        snapshot.unstaged_count,
                        snapshot.untracked_count
                    );
                }
            }
        },
        Commands::Artifact { subcommand } => match subcommand {
            ArtifactSubcommand::Upload {
                message_id,
                path,
                kind,
                media_type,
                snapshot_id,
                pinned,
                json,
            } => {
                let request = build_upload_artifact_request(
                    message_id,
                    path,
                    kind,
                    media_type,
                    snapshot_id,
                    pinned,
                )?;
                let artifact = client.upload_artifact(&request).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&artifact)?);
                } else {
                    println!("Artifact uploaded: {}", artifact.id);
                    println!("Kind: {} | Type: {}", artifact.kind, artifact.media_type);
                    println!("Size: {} bytes", artifact.size_bytes);
                    println!("SHA-256: {}", artifact.sha256);
                    println!("Content URL: /api/artifacts/{}/content", artifact.id);
                }
            }
            ArtifactSubcommand::List { message_id, json } => {
                let artifacts = client.list_artifacts(&message_id).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&artifacts)?);
                } else if artifacts.is_empty() {
                    println!("No artifacts found.");
                } else {
                    for artifact in &artifacts {
                        println!("{} {}", artifact.id, artifact.kind);
                        println!(
                            "Type: {} | Size: {}",
                            artifact.media_type, artifact.size_bytes
                        );
                        println!("Content URL: /api/artifacts/{}/content", artifact.id);
                        println!();
                    }
                }
            }
        },
        Commands::Pair { subcommand } => {
            match subcommand {
                PairSubcommand::Start { name, json } => {
                    let response = client.pair_start(name).await?;
                    if json {
                        println!("{}", serde_json::to_string(&response)?);
                        return Ok(());
                    }
                    println!("\n✓ Pairing code generated\n");
                    println!("Pairing URL:");
                    println!("{}\n", response.pair_url);
                    if response.pair_url.contains("127.0.0.1")
                        || response.pair_url.contains("localhost")
                    {
                        println!("Note: if opening on phone, replace localhost with your Tailscale URL.\n");
                    }
                    println!("Full Code: {}", response.pairing_code);
                    println!("Code Prefix: {}", response.code_prefix);
                    println!("Expires in: {} seconds\n", response.expires_in_seconds);
                }
            }
        }
        Commands::Devices { subcommand } => match subcommand {
            DevicesSubcommand::List { json } => {
                let devices = client.list_devices().await?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string(&DeviceListResponse { devices })?
                    );
                    return Ok(());
                }
                if devices.is_empty() {
                    println!("No paired devices.");
                } else {
                    println!("Paired devices:\n");
                    for device in &devices {
                        println!("ID: {}", device.id);
                        println!("Name: {}", device.name);
                        println!("Type: {}", device.kind);
                        println!("Token: {}", device.token_prefix);
                        println!(
                            "Status: {}",
                            if device.is_active {
                                "active"
                            } else {
                                "revoked"
                            }
                        );
                        println!("Paired: {}", device.paired_at);
                        if let Some(seen) = &device.last_seen_at {
                            println!("Last seen: {}", seen);
                        }
                        if let Some(revoked) = &device.revoked_at {
                            println!("Revoked: {}", revoked);
                        }
                        println!();
                    }
                }
            }
            DevicesSubcommand::Revoke { id, json } => {
                let response = client.revoke_device(id).await?;
                if json {
                    println!("{}", serde_json::to_string(&response)?);
                } else if response.success {
                    println!("Device revoked: {}", response.message);
                } else {
                    println!("Device revoke failed: {}", response.message);
                }
            }
            DevicesSubcommand::ResetAll { json } => {
                let response = client.reset_all_devices().await?;
                if json {
                    println!("{}", serde_json::to_string(&response)?);
                } else if response.success {
                    println!("Reset all devices complete.");
                    println!("Devices revoked: {}", response.devices_revoked);
                    println!("Subscriptions revoked: {}", response.subscriptions_revoked);
                    println!("Pairing codes cleared: {}", response.pairing_codes_cleared);
                } else {
                    println!("Reset all devices failed.");
                }
            }
        },
        Commands::Doctor {
            json,
            public_url,
            check_public,
            check_push,
            timeout_seconds: _,
            strict,
        } => {
            let mut checks = Vec::new();
            let mut diagnostics = None;

            match client.health_at(&client.base_url).await {
                Ok(health) if health.ok => checks.push(check(
                    "local_daemon",
                    DoctorStatus::Pass,
                    format!("reachable: {}", client.base_url),
                )),
                Ok(_) => checks.push(check(
                    "local_daemon",
                    DoctorStatus::Fail,
                    format!("health returned not ok: {}", client.base_url),
                )),
                Err(error) => checks.push(check(
                    "local_daemon",
                    DoctorStatus::Fail,
                    format!("unreachable {}: {}", client.base_url, error),
                )),
            }

            match client.diagnostics_at(&client.base_url).await {
                Ok(d) => {
                    checks.push(check("auth", DoctorStatus::Pass, "accepted"));
                    checks.extend(evaluate_diagnostics(&d));
                    diagnostics = Some(d);
                }
                Err(error) => {
                    let message = error.to_string();
                    let auth_message = if message.contains("401") || message.contains("403") {
                        "Token rejected. Use dashboard token/dev token or pair device again."
                            .to_string()
                    } else {
                        format!("diagnostics failed: {}", message)
                    };
                    checks.push(check("auth", DoctorStatus::Fail, auth_message));
                }
            }

            let public_target = if check_public {
                public_url.or_else(|| diagnostics.as_ref().and_then(|d| d.public_base_url.clone()))
            } else {
                public_url
            };
            if check_public || public_target.is_some() {
                if let Some(public_url) = public_target {
                    let public_url = public_url.trim_end_matches('/').to_string();
                    match client.health_at(&public_url).await {
                        Ok(health) if health.ok => checks.push(check(
                            "public_url",
                            DoctorStatus::Pass,
                            format!("reachable: {}", public_url),
                        )),
                        Ok(_) => checks.push(check(
                            "public_url",
                            DoctorStatus::Fail,
                            format!("health not ok: {}", public_url),
                        )),
                        Err(error) => checks.push(check(
                            "public_url",
                            DoctorStatus::Fail,
                            format!("unreachable {}: {}", public_url, error),
                        )),
                    }
                    match client.diagnostics_at(&public_url).await {
                        Ok(_) => checks.push(check(
                            "public_diagnostics",
                            DoctorStatus::Pass,
                            "public diagnostics reachable",
                        )),
                        Err(error) => checks.push(check(
                            "public_diagnostics",
                            DoctorStatus::Fail,
                            format!("public diagnostics failed: {}", error),
                        )),
                    }
                } else {
                    checks.push(check(
                        "public_url",
                        DoctorStatus::Warn,
                        "no public URL configured",
                    ));
                }
            }

            if check_push {
                match client.test_push().await {
                    Ok(response) => checks.extend(evaluate_push_test(&response)),
                    Err(error) => checks.push(check(
                        "push_test",
                        DoctorStatus::Fail,
                        format!("push test failed: {}", error),
                    )),
                }
            }

            let output = summarize_doctor(checks, strict);
            if json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                print_doctor_human(&output);
            }
            if !output.ok {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_diagnostics, evaluate_push_test, parse_timeout_seconds, summarize_doctor,
        DiagnosticsResponse, DoctorStatus, PushSummary, PushTestResponse,
    };

    #[test]
    fn timeout_parsing_supports_seconds_minutes_hours() {
        assert_eq!(parse_timeout_seconds("600s").unwrap(), 600);
        assert_eq!(parse_timeout_seconds("10m").unwrap(), 600);
        assert_eq!(parse_timeout_seconds("1h").unwrap(), 3600);
        assert_eq!(parse_timeout_seconds("42").unwrap(), 42);
    }

    #[test]
    fn doctor_evaluation_marks_valid_diagnostics_as_pass() {
        let diagnostics = DiagnosticsResponse {
            daemon_running: true,
            version: "0.1.0".to_string(),
            db_path: ".\\signal.db".to_string(),
            public_base_url: Some("https://example.test".to_string()),
            web_push_enabled: true,
            vapid_public_key_length: Some(65),
            vapid_public_key_first_byte: Some(4),
            active_devices: 1,
            active_subscriptions: 1,
            ..Default::default()
        };
        let checks = evaluate_diagnostics(&diagnostics);
        assert!(checks
            .iter()
            .any(|check| check.name == "vapid" && check.status == DoctorStatus::Pass));
        assert!(checks
            .iter()
            .any(|check| check.name == "active_push_subscription"
                && check.status == DoctorStatus::Pass));
    }

    #[test]
    fn doctor_evaluation_fails_invalid_vapid() {
        let diagnostics = DiagnosticsResponse {
            web_push_enabled: true,
            vapid_public_key_length: Some(64),
            vapid_public_key_first_byte: Some(2),
            active_devices: 1,
            active_subscriptions: 1,
            ..Default::default()
        };
        let checks = evaluate_diagnostics(&diagnostics);
        assert!(checks
            .iter()
            .any(|check| check.name == "vapid" && check.status == DoctorStatus::Fail));
    }

    #[test]
    fn doctor_evaluation_warns_on_zero_active_subscriptions() {
        let diagnostics = DiagnosticsResponse {
            web_push_enabled: true,
            vapid_public_key_length: Some(65),
            vapid_public_key_first_byte: Some(4),
            active_devices: 1,
            active_subscriptions: 0,
            ..Default::default()
        };
        let checks = evaluate_diagnostics(&diagnostics);
        assert!(checks.iter().any(|check| {
            check.name == "active_push_subscription" && check.status == DoctorStatus::Warn
        }));
    }

    #[test]
    fn doctor_evaluation_warns_on_legacy_subscriptions() {
        let diagnostics = DiagnosticsResponse {
            web_push_enabled: true,
            vapid_public_key_length: Some(65),
            vapid_public_key_first_byte: Some(4),
            active_devices: 1,
            active_subscriptions: 1,
            legacy_unbound_subscriptions: 2,
            ..Default::default()
        };
        let checks = evaluate_diagnostics(&diagnostics);
        assert!(checks.iter().any(
            |check| check.name == "legacy_subscriptions" && check.status == DoctorStatus::Warn
        ));
    }

    #[test]
    fn doctor_json_shape_has_summary_and_next_steps() {
        let diagnostics = DiagnosticsResponse {
            web_push_enabled: true,
            vapid_public_key_length: Some(65),
            vapid_public_key_first_byte: Some(4),
            active_devices: 0,
            active_subscriptions: 0,
            ..Default::default()
        };
        let output = summarize_doctor(evaluate_diagnostics(&diagnostics), false);
        let value = serde_json::to_value(&output).unwrap();
        assert!(value.get("ok").is_some());
        assert!(value.get("checks").is_some());
        assert!(value.get("summary").is_some());
        assert!(value.get("suggested_next_steps").is_some());
        assert!(!output.suggested_next_steps.is_empty());
    }

    #[test]
    fn push_test_attempted_zero_is_warning_not_failure() {
        let response = PushTestResponse {
            success: true,
            message: Some("No active push subscriptions".to_string()),
            summary: Some(PushSummary {
                attempted: 0,
                skipped_legacy: 1,
                ..Default::default()
            }),
        };
        let checks = evaluate_push_test(&response);
        assert_eq!(checks[0].status, DoctorStatus::Warn);
    }
}
