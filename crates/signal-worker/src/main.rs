use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(name = "signal-worker")]
#[command(about = "Local opt-in worker for Signal wake pings", long_about = None)]
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
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WorkerState {
    seen_message_ids: BTreeSet<String>,
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

    async fn list_messages(
        &self,
        agent_id: &str,
        project: Option<&str>,
    ) -> Result<Vec<Message>, Box<dyn std::error::Error>> {
        let mut url = format!(
            "{}/api/messages?limit=25&status=new&agent_id={}",
            self.base_url, agent_id
        );
        if let Some(project) = project {
            url.push_str("&project=");
            url.push_str(project);
        }
        let response = self.add_auth(self.client.get(url)).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("message poll failed: HTTP {status} {body}").into());
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

fn should_handle_message(message: &Message, agent_id: &str, project: Option<&str>) -> bool {
    if message.status != "new" {
        return false;
    }
    if message.agent_id.as_deref() != Some(agent_id) {
        return false;
    }
    if project.is_some() && message.project.as_deref() != project {
        return false;
    }
    message.title.to_ascii_lowercase().starts_with("wake ")
}

fn run_command(
    program: &str,
    args: &[String],
    message: &Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(program)
        .args(args)
        .env("SIGNAL_MESSAGE_ID", &message.id)
        .env("SIGNAL_MESSAGE_TITLE", &message.title)
        .env("SIGNAL_MESSAGE_BODY", &message.body)
        .env("SIGNAL_MESSAGE_SOURCE", &message.source)
        .status()?;
    if !status.success() {
        return Err(format!("worker command exited with {status}").into());
    }
    Ok(())
}

async fn poll_once(
    client: &ApiClient,
    args: &Args,
    state: &mut WorkerState,
) -> Result<usize, Box<dyn std::error::Error>> {
    let messages = client
        .list_messages(&args.agent_id, args.project.as_deref())
        .await?;
    let mut handled = 0;

    for message in messages {
        if state.seen_message_ids.contains(&message.id) {
            continue;
        }
        if !should_handle_message(&message, &args.agent_id, args.project.as_deref()) {
            continue;
        }

        println!(
            "Wake ping received: {} [{}] from {}",
            message.title, message.id, message.source
        );
        println!("{}", message.body);

        if let Some(program) = &args.command {
            run_command(program, &args.command_args, &message)?;
        }

        state.seen_message_ids.insert(message.id);
        handled += 1;
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
    let client = ApiClient::new(args.server.clone(), args.token.clone());

    loop {
        let handled = poll_once(&client, &args, &mut state).await?;
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
    use super::{should_handle_message, Message};

    fn message(
        title: &str,
        status: &str,
        agent_id: Option<&str>,
        project: Option<&str>,
    ) -> Message {
        Message {
            id: "message-1".to_string(),
            title: title.to_string(),
            body: "hello".to_string(),
            source: "pwa".to_string(),
            status: status.to_string(),
            project: project.map(|value| value.to_string()),
            agent_id: agent_id.map(|value| value.to_string()),
        }
    }

    #[test]
    fn wake_messages_match_target_agent_and_project() {
        assert!(should_handle_message(
            &message("Wake codex", "new", Some("codex"), Some("signal")),
            "codex",
            Some("signal")
        ));
        assert!(!should_handle_message(
            &message("Wake codex", "new", Some("opencode"), Some("signal")),
            "codex",
            Some("signal")
        ));
        assert!(!should_handle_message(
            &message("Status", "new", Some("codex"), Some("signal")),
            "codex",
            Some("signal")
        ));
        assert!(!should_handle_message(
            &message("Wake codex", "consumed", Some("codex"), Some("signal")),
            "codex",
            Some("signal")
        ));
    }
}
