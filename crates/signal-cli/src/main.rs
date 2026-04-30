use clap::{Parser, Subcommand, ValueEnum};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "signal-cli")]
#[command(about = "Signal CLI - local-first push/reply protocol client", long_about = None)]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8787")]
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
    Pair {
        #[command(subcommand)]
        subcommand: PairSubcommand,
    },
    Devices {
        #[command(subcommand)]
        subcommand: DevicesSubcommand,
    },
}

#[derive(Subcommand)]
enum PairSubcommand {
    Start {
        #[arg(long)]
        name: String,
    },
}

#[derive(Subcommand)]
enum DevicesSubcommand {
    List,
    Revoke {
        #[arg(long)]
        id: String,
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

#[derive(Debug, Deserialize)]
struct PairStartResponse {
    pairing_code: String,
    qr_data: String,
    expires_in_seconds: u64,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
struct DeviceListResponse {
    devices: Vec<DeviceInfo>,
}

#[derive(Debug, Deserialize)]
struct DeviceRevokeResponse {
    success: bool,
    message: String,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let wait_timeout = match &cli.command {
        Commands::Ask { timeout, .. } => parse_timeout_seconds(timeout).unwrap_or(600) + 15,
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
        Commands::Pair { subcommand } => match subcommand {
            PairSubcommand::Start { name } => {
                let response = client.pair_start(name).await?;
                println!("Pairing code generated: {}", response.pairing_code);
                println!("QR Data: {}", response.qr_data);
                println!("Expires in: {} seconds", response.expires_in_seconds);
            }
        },
        Commands::Devices { subcommand } => match subcommand {
            DevicesSubcommand::List => {
                let devices = client.list_devices().await?;
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
                        println!();
                    }
                }
            }
            DevicesSubcommand::Revoke { id } => {
                let response = client.revoke_device(id).await?;
                println!("Device revoked: {}", response.message);
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_timeout_seconds;

    #[test]
    fn timeout_parsing_supports_seconds_minutes_hours() {
        assert_eq!(parse_timeout_seconds("600s").unwrap(), 600);
        assert_eq!(parse_timeout_seconds("10m").unwrap(), 600);
        assert_eq!(parse_timeout_seconds("1h").unwrap(), 3600);
        assert_eq!(parse_timeout_seconds("42").unwrap(), 42);
    }
}
