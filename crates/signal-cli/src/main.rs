use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "signal-cli")]
#[command(about = "Signal CLI - interact with the local-first inbox", long_about = None)]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8787")]
    server: String,

    #[arg(long)]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
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
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateMessageRequest {
    title: String,
    body: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Deserialize)]
struct HealthResponse {
    ok: bool,
}

struct ApiClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl ApiClient {
    fn new(base_url: String, token: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url,
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

    async fn health(&self) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/health", self.base_url);
        let response = self.add_auth(self.client.get(&url)).send().await?;
        if response.status().is_success() {
            println!("✓ Server is healthy");
            Ok(())
        } else {
            Err(format!("Server returned status: {}", response.status()).into())
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
        let request = CreateMessageRequest {
            title,
            body,
            source,
            agent_id,
            project,
        };

        let response = self
            .add_auth(self.client.post(&url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await?;
            return Err(format!("Failed to create message: {}", text).into());
        }

        let message: Message = response.json().await?;
        Ok(message)
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

        if !response.status().is_success() {
            let text = response.text().await?;
            return Err(format!("Failed to list messages: {}", text).into());
        }

        let messages: Vec<Message> = response.json().await?;
        Ok(messages)
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
            if url.contains('?') {
                url.push_str(&format!("&project={}", p));
            } else {
                url.push_str(&format!("?project={}", p));
            }
        }

        let response = self.add_auth(self.client.get(&url)).send().await?;

        if !response.status().is_success() {
            let text = response.text().await?;
            return Err(format!("Failed to get latest reply: {}", text).into());
        }

        let reply: Option<Reply> = response.json().await?;
        Ok(reply)
    }

    async fn consume_reply(&self, id: &str) -> Result<Reply, Box<dyn std::error::Error>> {
        let url = format!("{}/api/replies/{}/consume", self.base_url, id);

        let response = self.add_auth(self.client.post(&url)).send().await?;

        if !response.status().is_success() {
            let text = response.text().await?;
            return Err(format!("Failed to consume reply: {}", text).into());
        }

        let reply: Reply = response.json().await?;
        Ok(reply)
    }

    async fn create_reply(
        &self,
        message_id: String,
        body: String,
        source: String,
    ) -> Result<Reply, Box<dyn std::error::Error>> {
        let url = format!("{}/api/messages/{}/replies", self.base_url, message_id);
        let request = CreateReplyRequest {
            body,
            source,
            source_device: None,
        };

        let response = self
            .add_auth(self.client.post(&url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await?;
            return Err(format!("Failed to create reply: {}", text).into());
        }

        let reply: Reply = response.json().await?;
        Ok(reply)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let client = ApiClient::new(cli.server.clone(), cli.token.clone());

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
            println!("✓ Message created: {}", message.id);
            println!("  Title: {}", message.title);
            println!("  Status: {}", message.status);
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
                    println!("  From: {} | Status: {}", msg.source, msg.status);
                    println!("  Created: {}", msg.created_at);
                    if let Some(p) = &msg.project {
                        println!("  Project: {}", p);
                    }
                    println!();
                }
            }
        }
        Commands::LatestReply {
            agent_id,
            project,
            consume,
        } => {
            let reply = client.get_latest_reply(agent_id, project).await?;

            match reply {
                Some(r) => {
                    println!("Latest pending reply:");
                    println!("  ID: {}", r.id);
                    println!("  Message ID: {}", r.message_id);
                    println!("  Body: {}", r.body);
                    println!("  From: {}", r.source);
                    println!("  Status: {}", r.status);
                    println!("  Created: {}", r.created_at);

                    if consume.unwrap_or(false) {
                        println!("\nConsuming reply...");
                        let updated = client.consume_reply(&r.id).await?;
                        println!("✓ Reply consumed: {}", updated.id);
                        println!("  New status: {}", updated.status);
                    }
                }
                None => {
                    println!("No pending replies found.");
                }
            }
        }
        Commands::Reply {
            message_id,
            body,
            source,
        } => {
            println!("Creating reply...");
            let reply = client.create_reply(message_id, body, source).await?;
            println!("✓ Reply created: {}", reply.id);
            println!("  Status: {}", reply.status);
        }
    }

    Ok(())
}
