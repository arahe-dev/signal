use base64::Engine as _;
use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(name = "signal-worker")]
#[command(about = "Local opt-in worker for Signal action intents", long_about = None)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8791")]
    server: String,

    #[arg(long)]
    token: Option<String>,

    #[arg(long, default_value = "codex")]
    agent_id: String,

    #[arg(long)]
    project: Option<String>,

    #[arg(long, default_value_t = 2000)]
    interval_ms: u64,

    #[arg(long)]
    once: bool,

    #[arg(long)]
    state_path: Option<PathBuf>,

    #[arg(long)]
    command: Option<String>,

    #[arg(long = "command-arg")]
    command_args: Vec<String>,

    #[arg(long)]
    policy_path: Option<PathBuf>,

    #[arg(long)]
    worker_id: Option<String>,

    #[arg(long)]
    allow_high_risk: bool,

    #[arg(long)]
    allow_lab: bool,
}

#[derive(Debug, Deserialize, Clone)]
struct ActionIntent {
    id: String,
    message_id: String,
    kind: String,
    status: String,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    profile_id: Option<String>,
    risk: String,
    payload_json: String,
    payload_hash: String,
}

#[derive(Debug, Deserialize)]
struct ActionRun {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ClaimActionResponse {
    action: ActionIntent,
    run: ActionRun,
}

#[derive(Debug, Serialize)]
struct ClaimActionRequest<'a> {
    worker_id: &'a str,
    policy_hash: Option<&'a str>,
    lease_seconds: u64,
}

#[derive(Debug, Serialize)]
struct CompleteActionRequest<'a> {
    run_id: &'a str,
    exit_code: Option<i64>,
    output_summary: Option<&'a str>,
    error: Option<Value>,
}

#[derive(Debug, Serialize)]
struct UploadArtifactRequest<'a> {
    message_id: &'a str,
    snapshot_id: Option<&'a str>,
    kind: &'a str,
    media_type: &'a str,
    data_base64: String,
    width: Option<i64>,
    height: Option<i64>,
    expires_at: Option<&'a str>,
    pinned: Option<bool>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ArtifactMetadata {
    id: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WorkerState {
    seen_message_ids: BTreeSet<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WorkerPolicy {
    worker_id: Option<String>,
    agent_id: Option<String>,
    #[serde(default)]
    project_roots: BTreeMap<String, PathBuf>,
    #[serde(default)]
    profiles: BTreeMap<String, PolicyProfile>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct PolicyProfile {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    risk: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    cwd: Option<PathBuf>,
    #[serde(default)]
    allowed_roots: Vec<PathBuf>,
    #[serde(default)]
    allowed_extensions: Vec<String>,
    max_bytes: Option<u64>,
}

struct ApiClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl ApiClient {
    fn new(base_url: String, token: Option<String>) -> Self {
        Self {
            client: Client::new(),
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

    async fn list_actions(
        &self,
        status: &str,
        agent_id: &str,
        project: Option<&str>,
    ) -> Result<Vec<ActionIntent>, Box<dyn std::error::Error>> {
        let mut url = format!(
            "{}/api/actions?limit=25&status={}&agent_id={}",
            self.base_url, status, agent_id
        );
        if let Some(project) = project {
            url.push_str("&project=");
            url.push_str(project);
        }
        let response = self.add_auth(self.client.get(url)).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("action poll failed: HTTP {status} {body}").into());
        }
        Ok(response.json().await?)
    }

    async fn claim_action(
        &self,
        action_id: &str,
        worker_id: &str,
        policy_hash: Option<&str>,
    ) -> Result<ClaimActionResponse, Box<dyn std::error::Error>> {
        let response = self
            .add_auth(
                self.client
                    .post(format!("{}/api/actions/{}/claim", self.base_url, action_id)),
            )
            .json(&ClaimActionRequest {
                worker_id,
                policy_hash,
                lease_seconds: 120,
            })
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("claim failed: HTTP {status} {body}").into());
        }
        Ok(response.json().await?)
    }

    async fn start_action(
        &self,
        action_id: &str,
        run_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let response = self
            .add_auth(
                self.client
                    .post(format!("{}/api/actions/{}/start", self.base_url, action_id)),
            )
            .json(&CompleteActionRequest {
                run_id,
                exit_code: None,
                output_summary: None,
                error: None,
            })
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("start failed: HTTP {status} {body}").into());
        }
        Ok(())
    }

    async fn complete_action(
        &self,
        action_id: &str,
        run_id: &str,
        output_summary: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let response = self
            .add_auth(self.client.post(format!(
                "{}/api/actions/{}/complete",
                self.base_url, action_id
            )))
            .json(&CompleteActionRequest {
                run_id,
                exit_code: Some(0),
                output_summary: Some(output_summary),
                error: None,
            })
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("complete failed: HTTP {status} {body}").into());
        }
        Ok(())
    }

    async fn fail_action(
        &self,
        action_id: &str,
        run_id: &str,
        error: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let response = self
            .add_auth(
                self.client
                    .post(format!("{}/api/actions/{}/fail", self.base_url, action_id)),
            )
            .json(&CompleteActionRequest {
                run_id,
                exit_code: Some(1),
                output_summary: Some(error),
                error: Some(serde_json::json!({ "message": error })),
            })
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("fail failed: HTTP {status} {body}").into());
        }
        Ok(())
    }

    async fn upload_artifact(
        &self,
        request: &UploadArtifactRequest<'_>,
    ) -> Result<ArtifactMetadata, Box<dyn std::error::Error>> {
        let response = self
            .add_auth(
                self.client
                    .post(format!("{}/api/artifacts/upload", self.base_url)),
            )
            .json(request)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("artifact upload failed: HTTP {status} {body}").into());
        }
        Ok(response.json().await?)
    }
}

fn default_state_path(agent_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("signal-worker-{agent_id}.json"))
}

fn load_state(path: &PathBuf) -> WorkerState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn save_state(path: &PathBuf, state: &WorkerState) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

fn load_policy(path: Option<&Path>) -> WorkerPolicy {
    path.and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn should_handle_action(action: &ActionIntent, agent_id: &str, project: Option<&str>) -> bool {
    if !matches!(action.status.as_str(), "pending" | "approved") {
        return false;
    }
    if action.agent_id.as_deref() != Some(agent_id) {
        return false;
    }
    if project.is_some() && action.project.as_deref() != project {
        return false;
    }
    true
}

fn parse_payload(action: &ActionIntent) -> Value {
    serde_json::from_str(&action.payload_json).unwrap_or_else(|_| serde_json::json!({}))
}

fn run_command_profile(
    profile: &PolicyProfile,
    action: &ActionIntent,
) -> Result<String, Box<dyn std::error::Error>> {
    let Some(command) = &profile.command else {
        return Err("profile has no command".into());
    };
    let mut command_builder = Command::new(command);
    command_builder.args(&profile.args);
    if let Some(cwd) = &profile.cwd {
        command_builder.current_dir(cwd);
    }
    let output = command_builder
        .env("SIGNAL_ACTION_ID", &action.id)
        .env("SIGNAL_MESSAGE_ID", &action.message_id)
        .env("SIGNAL_ACTION_KIND", &action.kind)
        .env("SIGNAL_ACTION_PAYLOAD_HASH", &action.payload_hash)
        .output()?;
    let mut summary = String::new();
    if !output.stdout.is_empty() {
        summary.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        if !summary.is_empty() {
            summary.push('\n');
        }
        summary.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if summary.len() > 4000 {
        summary.truncate(4000);
        summary.push_str("\n[truncated]");
    }
    if !output.status.success() {
        return Err(format!("command exited with {}", output.status).into());
    }
    Ok(if summary.trim().is_empty() {
        "command completed".to_string()
    } else {
        summary
    })
}

fn default_wake_profile(args: &Args) -> Option<PolicyProfile> {
    args.command.as_ref().map(|command| PolicyProfile {
        kind: Some("command".to_string()),
        risk: Some("low".to_string()),
        command: Some(command.clone()),
        args: args.command_args.clone(),
        cwd: None,
        allowed_roots: Vec::new(),
        allowed_extensions: Vec::new(),
        max_bytes: None,
    })
}

fn find_wake_profile<'a>(
    policy: &'a WorkerPolicy,
    args: &'a Args,
    action: &ActionIntent,
) -> Option<PolicyProfile> {
    action
        .profile_id
        .as_deref()
        .and_then(|profile_id| policy.profiles.get(profile_id).cloned())
        .or_else(|| {
            action
                .agent_id
                .as_deref()
                .and_then(|agent_id| policy.profiles.get(&format!("wake_{agent_id}")).cloned())
        })
        .or_else(|| default_wake_profile(args))
}

fn ensure_risk_allowed(action: &ActionIntent, args: &Args) -> Result<(), String> {
    match action.risk.as_str() {
        "high" if !args.allow_high_risk => {
            Err("high-risk action requires --allow-high-risk".into())
        }
        "lab" if !args.allow_lab => Err("lab action requires --allow-lab".into()),
        _ => Ok(()),
    }
}

fn canonicalize_under_root(path: &Path, roots: &[PathBuf]) -> Result<PathBuf, String> {
    let full = path
        .canonicalize()
        .map_err(|error| format!("path not readable: {error}"))?;
    for root in roots {
        let root = root
            .canonicalize()
            .map_err(|error| format!("allowed root not readable: {error}"))?;
        if full.starts_with(&root) {
            return Ok(full);
        }
    }
    Err("path is outside allowed roots".to_string())
}

async fn handle_file_request(
    client: &ApiClient,
    policy: &WorkerPolicy,
    action: &ActionIntent,
) -> Result<String, Box<dyn std::error::Error>> {
    let payload = parse_payload(action);
    let requested_path = payload
        .get("path")
        .and_then(|value| value.as_str())
        .ok_or("file_request payload requires path")?;
    let profile = action
        .profile_id
        .as_deref()
        .and_then(|profile_id| policy.profiles.get(profile_id).cloned())
        .unwrap_or_default();
    let mut roots = profile.allowed_roots.clone();
    if let (Some(project), true) = (action.project.as_deref(), roots.is_empty()) {
        if let Some(project_root) = policy.project_roots.get(project) {
            roots.push(project_root.clone());
        }
    }
    if roots.is_empty() {
        return Err("file_request has no allowed roots in policy".into());
    }
    let full_path = canonicalize_under_root(Path::new(requested_path), &roots)?;
    let extension = full_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_default();
    let allowed_extensions = if profile.allowed_extensions.is_empty() {
        vec![".md".to_string(), ".txt".to_string()]
    } else {
        profile
            .allowed_extensions
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect()
    };
    if !allowed_extensions
        .iter()
        .any(|allowed| allowed == &extension)
    {
        return Err(format!("extension denied: {extension}").into());
    }
    let max_bytes = profile.max_bytes.unwrap_or(1024 * 1024);
    let data = std::fs::read(&full_path)?;
    if data.len() as u64 > max_bytes {
        return Err(format!("file too large: {} bytes", data.len()).into());
    }
    let media_type = if extension == ".md" {
        "text/markdown"
    } else {
        "text/plain"
    };
    let artifact = client
        .upload_artifact(&UploadArtifactRequest {
            message_id: &action.message_id,
            snapshot_id: None,
            kind: "file",
            media_type,
            data_base64: base64::engine::general_purpose::STANDARD.encode(&data),
            width: None,
            height: None,
            expires_at: None,
            pinned: Some(false),
            metadata: Some(serde_json::json!({
                "source_path": full_path.display().to_string(),
                "action_id": action.id
            })),
        })
        .await?;
    Ok(format!("uploaded artifact {}", artifact.id))
}

async fn handle_action(
    client: &ApiClient,
    args: &Args,
    policy: &WorkerPolicy,
    action: &ActionIntent,
) -> Result<String, Box<dyn std::error::Error>> {
    ensure_risk_allowed(action, args)?;
    match action.kind.as_str() {
        "wake_agent" => {
            let payload = parse_payload(action);
            println!(
                "Wake action received: {} [{}]",
                payload
                    .get("text")
                    .and_then(|value| value.as_str())
                    .unwrap_or("hello"),
                action.id
            );
            if let Some(profile) = find_wake_profile(policy, args, action) {
                run_command_profile(&profile, action)
            } else {
                Ok("wake observed; no command configured".to_string())
            }
        }
        "file_request" => handle_file_request(client, policy, action).await,
        "profile_run" => {
            let Some(profile_id) = action.profile_id.as_deref() else {
                return Err("profile_run requires profile_id".into());
            };
            let Some(profile) = policy.profiles.get(profile_id) else {
                return Err(format!("profile not found: {profile_id}").into());
            };
            run_command_profile(profile, action)
        }
        other => Err(format!("unsupported action kind: {other}").into()),
    }
}

async fn poll_once(
    client: &ApiClient,
    args: &Args,
    policy: &WorkerPolicy,
    state: &mut WorkerState,
) -> Result<usize, Box<dyn std::error::Error>> {
    let worker_id = args
        .worker_id
        .as_deref()
        .or(policy.worker_id.as_deref())
        .unwrap_or("signal-worker");
    let mut actions = client
        .list_actions("pending", &args.agent_id, args.project.as_deref())
        .await?;
    actions.extend(
        client
            .list_actions("approved", &args.agent_id, args.project.as_deref())
            .await?,
    );

    let mut handled = 0;
    for action in actions {
        if !should_handle_action(&action, &args.agent_id, args.project.as_deref()) {
            continue;
        }
        let claimed = match client.claim_action(&action.id, worker_id, None).await {
            Ok(claimed) => claimed,
            Err(error) => {
                eprintln!("Skipping action {}: {}", action.id, error);
                continue;
            }
        };
        client
            .start_action(&claimed.action.id, &claimed.run.id)
            .await?;
        match handle_action(client, args, policy, &claimed.action).await {
            Ok(summary) => {
                client
                    .complete_action(&claimed.action.id, &claimed.run.id, &summary)
                    .await?;
                state
                    .seen_message_ids
                    .insert(claimed.action.message_id.clone());
                handled += 1;
            }
            Err(error) => {
                let error = error.to_string();
                client
                    .fail_action(&claimed.action.id, &claimed.run.id, &error)
                    .await?;
                handled += 1;
            }
        }
    }

    Ok(handled)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let state_path = args
        .state_path
        .clone()
        .unwrap_or_else(|| default_state_path(&args.agent_id));
    let mut state = load_state(&state_path);
    let policy = load_policy(args.policy_path.as_deref());
    let client = ApiClient::new(args.server.clone(), args.token.clone());

    loop {
        let handled = poll_once(&client, &args, &policy, &mut state).await?;
        if handled > 0 {
            save_state(&state_path, &state)?;
        }
        if args.once {
            break;
        }
        sleep(Duration::from_millis(args.interval_ms)).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{canonicalize_under_root, should_handle_action, ActionIntent};

    fn action(status: &str, agent_id: Option<&str>, project: Option<&str>) -> ActionIntent {
        ActionIntent {
            id: "action-1".to_string(),
            message_id: "message-1".to_string(),
            kind: "wake_agent".to_string(),
            status: status.to_string(),
            agent_id: agent_id.map(|value| value.to_string()),
            project: project.map(|value| value.to_string()),
            profile_id: None,
            risk: "low".to_string(),
            payload_json: "{}".to_string(),
            payload_hash: "hash".to_string(),
        }
    }

    #[test]
    fn action_filter_matches_target_agent_and_project() {
        assert!(should_handle_action(
            &action("pending", Some("codex"), Some("signal")),
            "codex",
            Some("signal")
        ));
        assert!(should_handle_action(
            &action("approved", Some("codex"), Some("signal")),
            "codex",
            Some("signal")
        ));
        assert!(!should_handle_action(
            &action("awaiting_approval", Some("codex"), Some("signal")),
            "codex",
            Some("signal")
        ));
        assert!(!should_handle_action(
            &action("pending", Some("opencode"), Some("signal")),
            "codex",
            Some("signal")
        ));
    }

    #[test]
    fn path_guard_accepts_paths_under_allowed_root() {
        let root = std::env::current_dir().unwrap();
        let cargo = root.join("Cargo.toml");
        assert!(canonicalize_under_root(&cargo, &[root]).is_ok());
    }
}
