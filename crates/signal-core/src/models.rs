use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    New,
    Pending,
    Consumed,
    Archived,
}

impl Default for MessageStatus {
    fn default() -> Self {
        MessageStatus::New
    }
}

impl std::fmt::Display for MessageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageStatus::New => write!(f, "new"),
            MessageStatus::Pending => write!(f, "pending"),
            MessageStatus::Consumed => write!(f, "consumed"),
            MessageStatus::Archived => write!(f, "archived"),
        }
    }
}

impl std::str::FromStr for MessageStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "new" => Ok(MessageStatus::New),
            "pending" => Ok(MessageStatus::Pending),
            "consumed" => Ok(MessageStatus::Consumed),
            "archived" => Ok(MessageStatus::Archived),
            _ => Err(format!("Unknown status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    Private,
    AiReadable,
    Actionable,
}

impl Default for PermissionLevel {
    fn default() -> Self {
        PermissionLevel::Private
    }
}

impl std::fmt::Display for PermissionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionLevel::Private => write!(f, "private"),
            PermissionLevel::AiReadable => write!(f, "ai_readable"),
            PermissionLevel::Actionable => write!(f, "actionable"),
        }
    }
}

impl std::str::FromStr for PermissionLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "private" => Ok(PermissionLevel::Private),
            "ai_readable" => Ok(PermissionLevel::AiReadable),
            "actionable" => Ok(PermissionLevel::Actionable),
            _ => Err(format!("Unknown permission level: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReplyStatus {
    Pending,
    Consumed,
    Archived,
}

impl Default for ReplyStatus {
    fn default() -> Self {
        ReplyStatus::Pending
    }
}

impl std::fmt::Display for ReplyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplyStatus::Pending => write!(f, "pending"),
            ReplyStatus::Consumed => write!(f, "consumed"),
            ReplyStatus::Archived => write!(f, "archived"),
        }
    }
}

impl std::str::FromStr for ReplyStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(ReplyStatus::Pending),
            "consumed" => Ok(ReplyStatus::Consumed),
            "archived" => Ok(ReplyStatus::Archived),
            _ => Err(format!("Unknown reply status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    pub title: String,
    pub body: String,
    pub source: String,
    pub source_device: Option<String>,
    pub agent_id: Option<String>,
    pub project: Option<String>,
    pub status: MessageStatus,
    pub permission_level: PermissionLevel,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Message {
    pub fn new(
        title: String,
        body: String,
        source: String,
        source_device: Option<String>,
        agent_id: Option<String>,
        project: Option<String>,
        permission_level: PermissionLevel,
    ) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let thread_id = Uuid::new_v4().to_string();
        Self {
            id,
            thread_id,
            title,
            body,
            source,
            source_device,
            agent_id,
            project,
            status: MessageStatus::New,
            permission_level,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub id: String,
    pub message_id: String,
    pub body: String,
    pub source: String,
    pub source_device: Option<String>,
    pub status: ReplyStatus,
    pub created_at: DateTime<Utc>,
}

impl Reply {
    pub fn new(
        message_id: String,
        body: String,
        source: String,
        source_device: Option<String>,
    ) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            message_id,
            body,
            source,
            source_device,
            status: ReplyStatus::Pending,
            created_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event_type: String,
    pub actor: Option<String>,
    pub source_device: Option<String>,
    pub created_at: DateTime<Utc>,
    pub payload_json: String,
}

impl Event {
    pub fn new(
        event_type: String,
        actor: Option<String>,
        source_device: Option<String>,
        payload_json: String,
    ) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            event_type,
            actor,
            source_device,
            created_at: now,
            payload_json,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutboxStatus {
    Pending,
    Sent,
    Failed,
}

impl Default for OutboxStatus {
    fn default() -> Self {
        OutboxStatus::Pending
    }
}

impl std::fmt::Display for OutboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutboxStatus::Pending => write!(f, "pending"),
            OutboxStatus::Sent => write!(f, "sent"),
            OutboxStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for OutboxStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(OutboxStatus::Pending),
            "sent" => Ok(OutboxStatus::Sent),
            "failed" => Ok(OutboxStatus::Failed),
            _ => Err(format!("Unknown outbox status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub id: String,
    pub destination: String,
    pub payload_json: String,
    pub status: OutboxStatus,
    pub created_at: DateTime<Utc>,
}

impl OutboxEntry {
    pub fn new(destination: String, payload_json: String) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            destination,
            payload_json,
            status: OutboxStatus::Pending,
            created_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageRequest {
    pub title: String,
    pub body: String,
    pub source: String,
    pub source_device: Option<String>,
    pub agent_id: Option<String>,
    pub project: Option<String>,
    pub status: Option<MessageStatus>,
    pub permission_level: Option<PermissionLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReplyRequest {
    pub body: String,
    pub source: String,
    pub source_device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageWithReplies {
    pub message: Message,
    pub replies: Vec<Reply>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSubscription {
    pub id: String,
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub status: String,
    pub vapid_public_key_hash: Option<String>,
}

impl PushSubscription {
    pub fn new(endpoint: String, p256dh: String, auth: String, user_agent: Option<String>) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            endpoint,
            p256dh,
            auth,
            user_agent,
            created_at: now,
            last_success_at: None,
            last_error: None,
            status: "active".to_string(),
            vapid_public_key_hash: None,
        }
    }
}
