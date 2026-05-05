use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    New,
    #[serde(rename = "pending_reply", alias = "pendingreply", alias = "pending")]
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
    #[serde(rename = "ai_readable", alias = "aireadable")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLogEntry {
    pub seq: Option<i64>,
    pub event_id: String,
    pub event_type: String,
    pub source: String,
    pub actor: String,
    pub subject: Option<String>,
    pub visibility: PermissionLevel,
    pub event_time: DateTime<Utc>,
    pub inserted_at: DateTime<Utc>,
    pub datacontenttype: String,
    pub dataschema: Option<String>,
    pub data_json: String,
    pub extensions_json: String,
    pub idempotency_key: Option<String>,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub resource: Option<String>,
    pub prev_hash: Option<String>,
    pub event_hash: String,
}

impl EventLogEntry {
    pub fn new(event_type: String, source: String, actor: String, data_json: String) -> Self {
        let now = Utc::now();
        Self {
            seq: None,
            event_id: Uuid::new_v4().to_string(),
            event_type,
            source,
            actor,
            subject: None,
            visibility: PermissionLevel::Private,
            event_time: now,
            inserted_at: now,
            datacontenttype: "application/json".to_string(),
            dataschema: None,
            data_json,
            extensions_json: "{}".to_string(),
            idempotency_key: None,
            correlation_id: None,
            causation_id: None,
            trace_id: None,
            span_id: None,
            resource: None,
            prev_hash: None,
            event_hash: String::new(),
        }
    }

    pub fn message_created(message: &Message) -> Self {
        let data = serde_json::json!({
            "message_id": message.id,
            "thread_id": message.thread_id,
            "title": message.title,
            "source": message.source,
            "agent_id": message.agent_id,
            "project": message.project,
            "status": message.status.to_string(),
            "permission_level": message.permission_level.to_string()
        })
        .to_string();
        let mut event = Self::new(
            "signal.message.created".to_string(),
            "service:signal-daemon".to_string(),
            principal_for_message(message),
            data,
        );
        event.subject = Some(format!(
            "thread:{}/message:{}",
            message.thread_id, message.id
        ));
        event.visibility = message.permission_level.clone();
        event.correlation_id = Some(message.thread_id.clone());
        event.resource = Some(format!("message:{}", message.id));
        event
    }

    pub fn reply_created(reply: &Reply, message: &Message) -> Self {
        let data = serde_json::json!({
            "reply_id": reply.id,
            "message_id": reply.message_id,
            "thread_id": message.thread_id,
            "source": reply.source,
            "source_device": reply.source_device,
            "status": reply.status.to_string()
        })
        .to_string();
        let mut event = Self::new(
            "signal.reply.created".to_string(),
            "service:signal-daemon".to_string(),
            principal_for_source(&reply.source, reply.source_device.as_deref()),
            data,
        );
        event.subject = Some(format!(
            "thread:{}/message:{}/reply:{}",
            message.thread_id, reply.message_id, reply.id
        ));
        event.visibility = message.permission_level.clone();
        event.correlation_id = Some(message.thread_id.clone());
        event.causation_id = Some(format!("message:{}", reply.message_id));
        event.resource = Some(format!("reply:{}", reply.id));
        event
    }

    pub fn reply_consumed(reply: &Reply, message: &Message, actor: &str) -> Self {
        let data = serde_json::json!({
            "reply_id": reply.id,
            "message_id": reply.message_id,
            "thread_id": message.thread_id,
            "consumed_by": actor
        })
        .to_string();
        let mut event = Self::new(
            "signal.reply.consumed".to_string(),
            "service:signal-daemon".to_string(),
            principal_for_source(actor, None),
            data,
        );
        event.subject = Some(format!(
            "thread:{}/message:{}/reply:{}",
            message.thread_id, reply.message_id, reply.id
        ));
        event.visibility = message.permission_level.clone();
        event.correlation_id = Some(message.thread_id.clone());
        event.causation_id = Some(format!("reply:{}", reply.id));
        event.resource = Some(format!("reply:{}", reply.id));
        event
    }
}

pub fn principal_for_message(message: &Message) -> String {
    message
        .agent_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|agent_id| format!("agent:{}", agent_id))
        .unwrap_or_else(|| principal_for_source(&message.source, message.source_device.as_deref()))
}

pub fn principal_for_source(source: &str, source_device: Option<&str>) -> String {
    if let Some(device) = source_device.filter(|value| !value.trim().is_empty()) {
        return format!("device:{}", device);
    }
    match source {
        "phone" | "pwa" => "user:local".to_string(),
        "signal-daemon" | "daemon" => "service:signal-daemon".to_string(),
        value if value.starts_with("agent:") || value.starts_with("device:") => value.to_string(),
        value if !value.trim().is_empty() => format!("agent:{}", value),
        _ => "service:unknown".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEventLogRequest {
    pub event_type: String,
    pub source: String,
    pub actor: String,
    pub subject: Option<String>,
    pub visibility: Option<PermissionLevel>,
    pub data: Value,
    pub extensions: Option<Value>,
    pub idempotency_key: Option<String>,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub resource: Option<String>,
}

impl CreateEventLogRequest {
    pub fn into_entry(self) -> EventLogEntry {
        let mut entry = EventLogEntry::new(
            self.event_type,
            self.source,
            self.actor,
            serde_json::to_string(&self.data).unwrap_or_else(|_| "{}".to_string()),
        );
        entry.subject = self.subject;
        entry.visibility = self.visibility.unwrap_or_default();
        entry.extensions_json = self
            .extensions
            .map(|value| serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());
        entry.idempotency_key = self.idempotency_key;
        entry.correlation_id = self.correlation_id;
        entry.causation_id = self.causation_id;
        entry.trace_id = self.trace_id;
        entry.span_id = self.span_id;
        entry.resource = self.resource;
        entry
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grant {
    pub id: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub issued_to: String,
    pub issued_by: String,
    pub scopes_json: String,
    pub resources_json: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<i64>,
    pub uses: i64,
    pub requires_human: bool,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub metadata_json: Option<String>,
}

impl Grant {
    pub fn new(
        token_hash: String,
        token_prefix: String,
        issued_to: String,
        issued_by: String,
        scopes: Vec<String>,
        resources: Value,
        ttl_seconds: Option<u64>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            token_hash,
            token_prefix,
            issued_to,
            issued_by,
            scopes_json: serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".to_string()),
            resources_json: serde_json::to_string(&resources).unwrap_or_else(|_| "{}".to_string()),
            expires_at: ttl_seconds.map(|seconds| now + chrono::Duration::seconds(seconds as i64)),
            max_uses: Some(1),
            uses: 0,
            requires_human: true,
            status: "active".to_string(),
            created_at: now,
            revoked_at: None,
            metadata_json: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantUse {
    pub id: String,
    pub grant_id: String,
    pub event_seq: Option<i64>,
    pub actor: String,
    pub scope: String,
    pub resource: Option<String>,
    pub decision: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub id: String,
    pub message_id: String,
    pub captured_at: DateTime<Utc>,
    pub source: String,
    pub stage: String,
    pub repo_root_hash: Option<String>,
    pub repo_root_display: Option<String>,
    pub git_common_dir_hash: Option<String>,
    pub worktree_id: Option<String>,
    pub worktree_path_display: Option<String>,
    pub branch: Option<String>,
    pub head_oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<i64>,
    pub behind: Option<i64>,
    pub dirty: bool,
    pub staged_count: i64,
    pub unstaged_count: i64,
    pub untracked_count: i64,
    pub status_json: String,
    pub worktrees_json: Option<String>,
    pub staged_patch_id: Option<String>,
    pub staged_patch_sha256: Option<String>,
    pub unstaged_patch_id: Option<String>,
    pub unstaged_patch_sha256: Option<String>,
    pub post_commit_oid: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub pinned: bool,
}

impl ContextSnapshot {
    pub fn new(message_id: String, source: String, stage: String, status_json: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            message_id,
            captured_at: Utc::now(),
            source,
            stage,
            repo_root_hash: None,
            repo_root_display: None,
            git_common_dir_hash: None,
            worktree_id: None,
            worktree_path_display: None,
            branch: None,
            head_oid: None,
            upstream: None,
            ahead: None,
            behind: None,
            dirty: false,
            staged_count: 0,
            unstaged_count: 0,
            untracked_count: 0,
            status_json,
            worktrees_json: None,
            staged_patch_id: None,
            staged_patch_sha256: None,
            unstaged_patch_id: None,
            unstaged_patch_sha256: None,
            post_commit_oid: None,
            expires_at: None,
            pinned: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub id: String,
    pub message_id: String,
    pub snapshot_id: Option<String>,
    pub kind: String,
    pub media_type: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub storage_uri: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub pinned: bool,
    pub metadata_json: Option<String>,
}

impl ArtifactMetadata {
    pub fn new(
        message_id: String,
        kind: String,
        media_type: String,
        sha256: String,
        size_bytes: i64,
        storage_uri: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            message_id,
            snapshot_id: None,
            kind,
            media_type,
            sha256,
            size_bytes,
            storage_uri,
            width: None,
            height: None,
            created_at: Utc::now(),
            expires_at: None,
            pinned: false,
            metadata_json: None,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{MessageStatus, PermissionLevel};

    #[test]
    fn serde_accepts_display_names_for_multi_word_enums() {
        let permission: PermissionLevel = serde_json::from_str("\"ai_readable\"").unwrap();
        assert_eq!(permission, PermissionLevel::AiReadable);

        let status: MessageStatus = serde_json::from_str("\"pending_reply\"").unwrap();
        assert_eq!(status, MessageStatus::PendingReply);
    }

    #[test]
    fn serde_keeps_back_compat_for_old_lowercase_enum_names() {
        let permission: PermissionLevel = serde_json::from_str("\"aireadable\"").unwrap();
        assert_eq!(permission, PermissionLevel::AiReadable);

        let status: MessageStatus = serde_json::from_str("\"pendingreply\"").unwrap();
        assert_eq!(status, MessageStatus::PendingReply);
    }
}
