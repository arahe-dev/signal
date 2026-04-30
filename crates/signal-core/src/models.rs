use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    New,
    PendingReply,
    Replied,
    Timeout,
    Consumed,
    Archived,
    Failed,
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
            MessageStatus::PendingReply => write!(f, "pending_reply"),
            MessageStatus::Replied => write!(f, "replied"),
            MessageStatus::Timeout => write!(f, "timeout"),
            MessageStatus::Consumed => write!(f, "consumed"),
            MessageStatus::Archived => write!(f, "archived"),
            MessageStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for MessageStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "new" => Ok(MessageStatus::New),
            "pending" | "pending_reply" => Ok(MessageStatus::PendingReply),
            "replied" => Ok(MessageStatus::Replied),
            "timeout" => Ok(MessageStatus::Timeout),
            "consumed" => Ok(MessageStatus::Consumed),
            "archived" => Ok(MessageStatus::Archived),
            "failed" => Ok(MessageStatus::Failed),
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
    Expired,
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
            ReplyStatus::Expired => write!(f, "expired"),
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
            "expired" => Ok(ReplyStatus::Expired),
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
    pub expires_at: Option<DateTime<Utc>>,
    pub priority: Option<String>,
    pub reply_mode: Option<String>,
    pub reply_options_json: Option<String>,
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
            expires_at: None,
            priority: None,
            reply_mode: None,
            reply_options_json: None,
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
    pub consumed_at: Option<DateTime<Utc>>,
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
            consumed_at: None,
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
    pub device_id: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskRequest {
    pub agent_id: Option<String>,
    pub project: Option<String>,
    pub title: String,
    pub body: String,
    pub timeout_seconds: Option<u64>,
    pub priority: Option<String>,
    pub reply_mode: Option<String>,
    pub reply_options: Option<Vec<String>>,
    pub source: String,
}

impl PushSubscription {
    pub fn new(endpoint: String, p256dh: String, auth: String, user_agent: Option<String>) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            device_id: None,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String, // pc, phone, cli, agent, unknown
    pub token_hash: String,
    pub token_prefix: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub user_agent: Option<String>,
    pub metadata_json: Option<String>,
}

impl Device {
    pub fn new(
        id: String,
        name: String,
        kind: String,
        token_hash: String,
        token_prefix: String,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            token_hash,
            token_prefix,
            paired_at: Utc::now(),
            last_seen_at: None,
            revoked_at: None,
            user_agent: None,
            metadata_json: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingCode {
    pub code_hash: String,
    pub code_prefix: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub device_name_hint: Option<String>,
    pub metadata_json: Option<String>,
}

impl PairingCode {
    pub fn new(code_hash: String, code_prefix: String, ttl_seconds: u64) -> Self {
        let now = Utc::now();
        Self {
            code_hash,
            code_prefix,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(ttl_seconds as i64),
            used_at: None,
            device_name_hint: None,
            metadata_json: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn is_used(&self) -> bool {
        self.used_at.is_some()
    }

    pub fn is_valid(&self) -> bool {
        !self.is_expired() && !self.is_used()
    }
}
