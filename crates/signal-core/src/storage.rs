use crate::models::{
    ActionApproval, ActionIntent, ActionRun, ArtifactMetadata, ContextSnapshot, DeviceCapability,
    Event, EventLogEntry, Grant, GrantUse, Message, MessageStatus, OutboxEntry, PushSubscription,
    Reply, ReplyStatus,
};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;
use thiserror::Error;
use tracing::info;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

#[derive(Debug, Clone, Default)]
pub struct PushSubscriptionCounts {
    pub total: usize,
    pub active_bound: usize,
    pub active_legacy: usize,
    pub revoked_or_stale: usize,
    pub revoked: usize,
    pub stale: usize,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceResetSummary {
    pub devices_revoked: usize,
    pub subscriptions_revoked: usize,
    pub pairing_codes_cleared: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PushSubscriptionCleanupSummary {
    pub revoked_deleted: usize,
    pub stale_deleted: usize,
    pub legacy_deleted: usize,
}

pub struct Storage {
    conn: Mutex<Connection>,
}

fn push_subscription_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PushSubscription> {
    let created_at_str: String = row.get(6)?;
    let last_success_at_str: Option<String> = row.get(7)?;
    Ok(PushSubscription {
        id: row.get(0)?,
        device_id: row.get(1)?,
        endpoint: row.get(2)?,
        p256dh: row.get(3)?,
        auth: row.get(4)?,
        user_agent: row.get(5)?,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        last_success_at: last_success_at_str.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .ok()
        }),
        last_error: row.get(8)?,
        status: row.get(9)?,
        vapid_public_key_hash: row.get(10)?,
    })
}

fn parse_utc(value: String) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(&value)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

fn parse_optional_utc(value: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
    value.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok()
    })
}

fn event_log_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventLogEntry> {
    Ok(EventLogEntry {
        seq: Some(row.get(0)?),
        event_id: row.get(1)?,
        event_type: row.get(2)?,
        source: row.get(3)?,
        actor: row.get(4)?,
        subject: row.get(5)?,
        visibility: row.get::<_, String>(6)?.parse().unwrap_or_default(),
        event_time: parse_utc(row.get(7)?),
        inserted_at: parse_utc(row.get(8)?),
        datacontenttype: row.get(9)?,
        dataschema: row.get(10)?,
        data_json: row.get(11)?,
        extensions_json: row.get(12)?,
        idempotency_key: row.get(13)?,
        correlation_id: row.get(14)?,
        causation_id: row.get(15)?,
        trace_id: row.get(16)?,
        span_id: row.get(17)?,
        resource: row.get(18)?,
        prev_hash: row.get(19)?,
        event_hash: row.get(20)?,
    })
}

fn grant_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Grant> {
    Ok(Grant {
        id: row.get(0)?,
        token_hash: row.get(1)?,
        token_prefix: row.get(2)?,
        issued_to: row.get(3)?,
        issued_by: row.get(4)?,
        scopes_json: row.get(5)?,
        resources_json: row.get(6)?,
        expires_at: parse_optional_utc(row.get(7)?),
        max_uses: row.get(8)?,
        uses: row.get(9)?,
        requires_human: row.get::<_, i64>(10)? != 0,
        status: row.get(11)?,
        created_at: parse_utc(row.get(12)?),
        revoked_at: parse_optional_utc(row.get(13)?),
        metadata_json: row.get(14)?,
    })
}

fn device_capability_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeviceCapability> {
    Ok(DeviceCapability {
        id: row.get(0)?,
        device_id: row.get(1)?,
        capability: row.get(2)?,
        granted_by: row.get(3)?,
        granted_at: parse_utc(row.get(4)?),
        expires_at: parse_optional_utc(row.get(5)?),
        revoked_at: parse_optional_utc(row.get(6)?),
        metadata_json: row.get(7)?,
    })
}

fn action_intent_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActionIntent> {
    Ok(ActionIntent {
        id: row.get(0)?,
        message_id: row.get(1)?,
        kind: row.get(2)?,
        status: row.get(3)?,
        requested_by_device_id: row.get(4)?,
        agent_id: row.get(5)?,
        project: row.get(6)?,
        profile_id: row.get(7)?,
        risk: row.get(8)?,
        required_capability: row.get(9)?,
        payload_json: row.get(10)?,
        payload_hash: row.get(11)?,
        approval_id: row.get(12)?,
        grant_id: row.get(13)?,
        created_at: parse_utc(row.get(14)?),
        updated_at: parse_utc(row.get(15)?),
        expires_at: parse_optional_utc(row.get(16)?),
    })
}

fn action_run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActionRun> {
    Ok(ActionRun {
        id: row.get(0)?,
        intent_id: row.get(1)?,
        worker_id: row.get(2)?,
        status: row.get(3)?,
        policy_hash: row.get(4)?,
        claimed_at: parse_utc(row.get(5)?),
        lease_until: parse_optional_utc(row.get(6)?),
        started_at: parse_optional_utc(row.get(7)?),
        completed_at: parse_optional_utc(row.get(8)?),
        exit_code: row.get(9)?,
        stdout_artifact_id: row.get(10)?,
        stderr_artifact_id: row.get(11)?,
        output_summary: row.get(12)?,
        error_json: row.get(13)?,
    })
}

fn action_approval_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActionApproval> {
    Ok(ActionApproval {
        id: row.get(0)?,
        intent_id: row.get(1)?,
        status: row.get(2)?,
        nonce_hash: row.get(3)?,
        nonce_prefix: row.get(4)?,
        payload_hash: row.get(5)?,
        requested_at: parse_utc(row.get(6)?),
        expires_at: parse_utc(row.get(7)?),
        approved_by_device_id: row.get(8)?,
        approved_at: parse_optional_utc(row.get(9)?),
        denied_at: parse_optional_utc(row.get(10)?),
    })
}

fn context_snapshot_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextSnapshot> {
    Ok(ContextSnapshot {
        id: row.get(0)?,
        message_id: row.get(1)?,
        captured_at: parse_utc(row.get(2)?),
        source: row.get(3)?,
        stage: row.get(4)?,
        repo_root_hash: row.get(5)?,
        repo_root_display: row.get(6)?,
        git_common_dir_hash: row.get(7)?,
        worktree_id: row.get(8)?,
        worktree_path_display: row.get(9)?,
        branch: row.get(10)?,
        head_oid: row.get(11)?,
        upstream: row.get(12)?,
        ahead: row.get(13)?,
        behind: row.get(14)?,
        dirty: row.get::<_, i64>(15)? != 0,
        staged_count: row.get(16)?,
        unstaged_count: row.get(17)?,
        untracked_count: row.get(18)?,
        status_json: row.get(19)?,
        worktrees_json: row.get(20)?,
        staged_patch_id: row.get(21)?,
        staged_patch_sha256: row.get(22)?,
        unstaged_patch_id: row.get(23)?,
        unstaged_patch_sha256: row.get(24)?,
        post_commit_oid: row.get(25)?,
        expires_at: parse_optional_utc(row.get(26)?),
        pinned: row.get::<_, i64>(27)? != 0,
    })
}

fn artifact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactMetadata> {
    Ok(ArtifactMetadata {
        id: row.get(0)?,
        message_id: row.get(1)?,
        snapshot_id: row.get(2)?,
        kind: row.get(3)?,
        media_type: row.get(4)?,
        sha256: row.get(5)?,
        size_bytes: row.get(6)?,
        storage_uri: row.get(7)?,
        width: row.get(8)?,
        height: row.get(9)?,
        created_at: parse_utc(row.get(10)?),
        expires_at: parse_optional_utc(row.get(11)?),
        pinned: row.get::<_, i64>(12)? != 0,
        metadata_json: row.get(13)?,
    })
}

fn event_log_select_sql() -> &'static str {
    "SELECT seq, event_id, event_type, source, actor, subject, visibility, event_time,
            inserted_at, datacontenttype, dataschema, data_json, extensions_json,
            idempotency_key, correlation_id, causation_id, trace_id, span_id, resource,
            prev_hash, event_hash
     FROM event_log"
}

fn action_intent_select_sql() -> &'static str {
    "SELECT id, message_id, kind, status, requested_by_device_id, agent_id, project,
            profile_id, risk, required_capability, payload_json, payload_hash,
            approval_id, grant_id, created_at, updated_at, expires_at
     FROM action_intents"
}

fn action_run_select_sql() -> &'static str {
    "SELECT id, intent_id, worker_id, status, policy_hash, claimed_at, lease_until,
            started_at, completed_at, exit_code, stdout_artifact_id, stderr_artifact_id,
            output_summary, error_json
     FROM action_runs"
}

fn action_approval_select_sql() -> &'static str {
    "SELECT id, intent_id, status, nonce_hash, nonce_prefix, payload_hash, requested_at,
            expires_at, approved_by_device_id, approved_at, denied_at
     FROM action_approvals"
}

fn compute_event_hash(event: &EventLogEntry, prev_hash: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    for part in [
        prev_hash.unwrap_or(""),
        &event.event_id,
        &event.event_type,
        &event.source,
        &event.actor,
        event.subject.as_deref().unwrap_or(""),
        &event.visibility.to_string(),
        &event.event_time.to_rfc3339(),
        &event.data_json,
        &event.extensions_json,
        event.idempotency_key.as_deref().unwrap_or(""),
        event.correlation_id.as_deref().unwrap_or(""),
        event.causation_id.as_deref().unwrap_or(""),
        event.trace_id.as_deref().unwrap_or(""),
        event.span_id.as_deref().unwrap_or(""),
        event.resource.as_deref().unwrap_or(""),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

impl Storage {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA journal_mode = WAL;",
        )?;
        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.init_tables()?;
        Ok(storage)
    }

    fn init_tables(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                actor TEXT,
                source_device TEXT,
                created_at TEXT NOT NULL,
                payload_json TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                source TEXT NOT NULL,
                source_device TEXT,
                agent_id TEXT,
                project TEXT,
                status TEXT NOT NULL DEFAULT 'new',
                permission_level TEXT NOT NULL DEFAULT 'private',
                expires_at TEXT,
                priority TEXT,
                reply_mode TEXT,
                reply_options_json TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS replies (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                body TEXT NOT NULL,
                source TEXT NOT NULL,
                source_device TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                consumed_at TEXT,
                FOREIGN KEY (message_id) REFERENCES messages(id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS outbox (
                id TEXT PRIMARY KEY,
                destination TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS push_subscriptions (
                id TEXT PRIMARY KEY,
                device_id TEXT,
                endpoint TEXT NOT NULL,
                p256dh TEXT NOT NULL,
                auth TEXT NOT NULL,
                user_agent TEXT,
                created_at TEXT NOT NULL,
                last_success_at TEXT,
                last_error TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                vapid_public_key_hash TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                token_hash TEXT NOT NULL,
                token_prefix TEXT NOT NULL,
                paired_at TEXT NOT NULL,
                last_seen_at TEXT,
                revoked_at TEXT,
                user_agent TEXT,
                metadata_json TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS pairing_codes (
                code_hash TEXT PRIMARY KEY,
                code_prefix TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                used_at TEXT,
                device_name_hint TEXT,
                metadata_json TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS event_log (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                source TEXT NOT NULL,
                actor TEXT NOT NULL,
                subject TEXT,
                visibility TEXT NOT NULL DEFAULT 'private',
                event_time TEXT NOT NULL,
                inserted_at TEXT NOT NULL,
                datacontenttype TEXT NOT NULL DEFAULT 'application/json',
                dataschema TEXT,
                data_json TEXT NOT NULL,
                extensions_json TEXT NOT NULL DEFAULT '{}',
                idempotency_key TEXT,
                correlation_id TEXT,
                causation_id TEXT,
                trace_id TEXT,
                span_id TEXT,
                resource TEXT,
                prev_hash TEXT,
                event_hash TEXT NOT NULL,
                UNIQUE(source, event_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS grants (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                token_prefix TEXT NOT NULL,
                issued_to TEXT NOT NULL,
                issued_by TEXT NOT NULL,
                scopes_json TEXT NOT NULL,
                resources_json TEXT NOT NULL,
                expires_at TEXT,
                max_uses INTEGER,
                uses INTEGER NOT NULL DEFAULT 0,
                requires_human INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TEXT NOT NULL,
                revoked_at TEXT,
                metadata_json TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS grant_uses (
                id TEXT PRIMARY KEY,
                grant_id TEXT NOT NULL,
                event_seq INTEGER,
                actor TEXT NOT NULL,
                scope TEXT NOT NULL,
                resource TEXT,
                decision TEXT NOT NULL,
                reason TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (grant_id) REFERENCES grants(id),
                FOREIGN KEY (event_seq) REFERENCES event_log(seq)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS device_capabilities (
                id TEXT PRIMARY KEY,
                device_id TEXT NOT NULL,
                capability TEXT NOT NULL,
                granted_by TEXT NOT NULL,
                granted_at TEXT NOT NULL,
                expires_at TEXT,
                revoked_at TEXT,
                metadata_json TEXT,
                FOREIGN KEY (device_id) REFERENCES devices(id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS action_intents (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                requested_by_device_id TEXT,
                agent_id TEXT,
                project TEXT,
                profile_id TEXT,
                risk TEXT NOT NULL,
                required_capability TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                payload_hash TEXT NOT NULL,
                approval_id TEXT,
                grant_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT,
                FOREIGN KEY (message_id) REFERENCES messages(id),
                FOREIGN KEY (requested_by_device_id) REFERENCES devices(id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS action_runs (
                id TEXT PRIMARY KEY,
                intent_id TEXT NOT NULL,
                worker_id TEXT NOT NULL,
                status TEXT NOT NULL,
                policy_hash TEXT,
                claimed_at TEXT NOT NULL,
                lease_until TEXT,
                started_at TEXT,
                completed_at TEXT,
                exit_code INTEGER,
                stdout_artifact_id TEXT,
                stderr_artifact_id TEXT,
                output_summary TEXT,
                error_json TEXT,
                FOREIGN KEY (intent_id) REFERENCES action_intents(id),
                FOREIGN KEY (stdout_artifact_id) REFERENCES artifacts(id),
                FOREIGN KEY (stderr_artifact_id) REFERENCES artifacts(id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS action_approvals (
                id TEXT PRIMARY KEY,
                intent_id TEXT NOT NULL,
                status TEXT NOT NULL,
                nonce_hash TEXT NOT NULL,
                nonce_prefix TEXT NOT NULL,
                payload_hash TEXT NOT NULL,
                requested_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                approved_by_device_id TEXT,
                approved_at TEXT,
                denied_at TEXT,
                FOREIGN KEY (intent_id) REFERENCES action_intents(id),
                FOREIGN KEY (approved_by_device_id) REFERENCES devices(id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_snapshots (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                captured_at TEXT NOT NULL,
                source TEXT NOT NULL,
                stage TEXT NOT NULL,
                repo_root_hash TEXT,
                repo_root_display TEXT,
                git_common_dir_hash TEXT,
                worktree_id TEXT,
                worktree_path_display TEXT,
                branch TEXT,
                head_oid TEXT,
                upstream TEXT,
                ahead INTEGER,
                behind INTEGER,
                dirty INTEGER NOT NULL DEFAULT 0,
                staged_count INTEGER NOT NULL DEFAULT 0,
                unstaged_count INTEGER NOT NULL DEFAULT 0,
                untracked_count INTEGER NOT NULL DEFAULT 0,
                status_json TEXT NOT NULL,
                worktrees_json TEXT,
                staged_patch_id TEXT,
                staged_patch_sha256 TEXT,
                unstaged_patch_id TEXT,
                unstaged_patch_sha256 TEXT,
                post_commit_oid TEXT,
                expires_at TEXT,
                pinned INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (message_id) REFERENCES messages(id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                snapshot_id TEXT,
                kind TEXT NOT NULL,
                media_type TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                storage_uri TEXT NOT NULL,
                width INTEGER,
                height INTEGER,
                created_at TEXT NOT NULL,
                expires_at TEXT,
                pinned INTEGER NOT NULL DEFAULT 0,
                metadata_json TEXT,
                FOREIGN KEY (message_id) REFERENCES messages(id),
                FOREIGN KEY (snapshot_id) REFERENCES context_snapshots(id)
            )",
            [],
        )?;

        let _ = conn.execute(
            "ALTER TABLE push_subscriptions ADD COLUMN vapid_public_key_hash TEXT",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE push_subscriptions ADD COLUMN device_id TEXT",
            [],
        );
        for migration in [
            "ALTER TABLE messages ADD COLUMN expires_at TEXT",
            "ALTER TABLE messages ADD COLUMN priority TEXT",
            "ALTER TABLE messages ADD COLUMN reply_mode TEXT",
            "ALTER TABLE messages ADD COLUMN reply_options_json TEXT",
            "ALTER TABLE replies ADD COLUMN consumed_at TEXT",
        ] {
            let _ = conn.execute(migration, []);
        }

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_push_subscriptions_endpoint ON push_subscriptions(endpoint)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_push_subscriptions_device_id ON push_subscriptions(device_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_status ON messages(status)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_project ON messages(project)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_agent_id ON messages(agent_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_replies_message_id ON replies(message_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_replies_status ON replies(status)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_log_seq ON event_log(seq)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_log_type_seq ON event_log(event_type, seq)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_log_subject_seq ON event_log(subject, seq)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_log_correlation_seq ON event_log(correlation_id, seq)",
            [],
        )?;
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS ux_event_log_idempotency
             ON event_log(idempotency_key)
             WHERE idempotency_key IS NOT NULL",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_grants_token_hash ON grants(token_hash)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_grants_status_expires ON grants(status, expires_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_grant_uses_grant_id ON grant_uses(grant_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_device_capabilities_device_status
             ON device_capabilities(device_id, capability, revoked_at, expires_at)",
            [],
        )?;
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS ux_device_capabilities_active
             ON device_capabilities(device_id, capability)
             WHERE revoked_at IS NULL",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_action_intents_status
             ON action_intents(status, created_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_action_intents_agent_project
             ON action_intents(agent_id, project, status, created_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_action_intents_message
             ON action_intents(message_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_action_runs_intent
             ON action_runs(intent_id, claimed_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_action_approvals_status
             ON action_approvals(status, expires_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_snapshots_message_id
             ON context_snapshots(message_id, captured_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_artifacts_message_id
             ON artifacts(message_id, created_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_artifacts_snapshot_id
             ON artifacts(snapshot_id)",
            [],
        )?;

        info!("Database tables initialized");
        Ok(())
    }

    pub fn create_message(&self, message: &Message) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, thread_id, title, body, source, source_device, agent_id, project, status, permission_level, expires_at, priority, reply_mode, reply_options_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                message.id,
                message.thread_id,
                message.title,
                message.body,
                message.source,
                message.source_device,
                message.agent_id,
                message.project,
                message.status.to_string(),
                message.permission_level.to_string(),
                message.expires_at.map(|dt| dt.to_rfc3339()),
                message.priority,
                message.reply_mode,
                message.reply_options_json,
                message.created_at.to_rfc3339(),
                message.updated_at.to_rfc3339(),
            ],
        )?;
        info!("Created message: {}", message.id);
        Ok(())
    }

    pub fn get_message(&self, id: &str) -> Result<Message, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, title, body, source, source_device, agent_id, project, status, permission_level, expires_at, priority, reply_mode, reply_options_json, created_at, updated_at
             FROM messages WHERE id = ?1",
        )?;

        let message = stmt
            .query_row(params![id], |row| {
                let status_str: String = row.get(8)?;
                let perm_str: String = row.get(9)?;
                Ok(Message {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    title: row.get(2)?,
                    body: row.get(3)?,
                    source: row.get(4)?,
                    source_device: row.get(5)?,
                    agent_id: row.get(6)?,
                    project: row.get(7)?,
                    status: status_str.parse().unwrap_or_default(),
                    permission_level: perm_str.parse().unwrap_or_default(),
                    expires_at: row.get::<_, Option<String>>(10)?.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                    priority: row.get(11)?,
                    reply_mode: row.get(12)?,
                    reply_options_json: row.get(13)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(14)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(15)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                })
            })
            .map_err(|_| StorageError::NotFound(format!("Message not found: {}", id)))?;

        Ok(message)
    }

    pub fn list_messages(
        &self,
        limit: Option<i64>,
        status: Option<MessageStatus>,
        project: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<Message>, StorageError> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT id, thread_id, title, body, source, source_device, agent_id, project, status, permission_level, expires_at, priority, reply_mode, reply_options_json, created_at, updated_at
             FROM messages WHERE 1=1"
        );
        let mut params: Vec<rusqlite::types::Value> = Vec::new();

        if status.is_some() {
            sql.push_str(" AND status = ?");
            params.push(rusqlite::types::Value::Text(status.unwrap().to_string()));
        }
        if project.is_some() {
            sql.push_str(" AND project = ?");
            params.push(rusqlite::types::Value::Text(project.unwrap().to_string()));
        }
        if agent_id.is_some() {
            sql.push_str(" AND agent_id = ?");
            params.push(rusqlite::types::Value::Text(agent_id.unwrap().to_string()));
        }
        sql.push_str(" ORDER BY created_at DESC");

        let limit_value = limit.unwrap_or(50);
        sql.push_str(" LIMIT ?");
        params.push(rusqlite::types::Value::Integer(limit_value));

        let mut stmt = conn.prepare(&sql)?;

        let mut messages = Vec::new();
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let status_str: String = row.get(8)?;
            let perm_str: String = row.get(9)?;
            Ok(Message {
                id: row.get(0)?,
                thread_id: row.get(1)?,
                title: row.get(2)?,
                body: row.get(3)?,
                source: row.get(4)?,
                source_device: row.get(5)?,
                agent_id: row.get(6)?,
                project: row.get(7)?,
                status: status_str.parse().unwrap_or_default(),
                permission_level: perm_str.parse().unwrap_or_default(),
                expires_at: row.get::<_, Option<String>>(10)?.and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .ok()
                }),
                priority: row.get(11)?,
                reply_mode: row.get(12)?,
                reply_options_json: row.get(13)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(14)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(15)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            })
        })?;

        for row in rows {
            messages.push(row?);
        }

        Ok(messages)
    }

    pub fn update_message_status(
        &self,
        id: &str,
        status: MessageStatus,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE messages SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.to_string(), now, id],
        )?;
        info!("Updated message {} status to {}", id, status);
        Ok(())
    }

    pub fn create_reply(&self, reply: &Reply) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO replies (id, message_id, body, source, source_device, status, created_at, consumed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                reply.id,
                reply.message_id,
                reply.body,
                reply.source,
                reply.source_device,
                reply.status.to_string(),
                reply.created_at.to_rfc3339(),
                reply.consumed_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        info!(
            "Created reply: {} for message: {}",
            reply.id, reply.message_id
        );
        Ok(())
    }

    pub fn get_replies_for_message(&self, message_id: &str) -> Result<Vec<Reply>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, body, source, source_device, status, created_at, consumed_at
             FROM replies WHERE message_id = ?1 ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map(params![message_id], |row| {
            let status_str: String = row.get(5)?;
            Ok(Reply {
                id: row.get(0)?,
                message_id: row.get(1)?,
                body: row.get(2)?,
                source: row.get(3)?,
                source_device: row.get(4)?,
                status: status_str.parse().unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                consumed_at: row.get::<_, Option<String>>(7)?.and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .ok()
                }),
            })
        })?;

        let mut replies = Vec::new();
        for row in rows {
            replies.push(row?);
        }
        Ok(replies)
    }

    pub fn get_reply(&self, id: &str) -> Result<Reply, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, body, source, source_device, status, created_at, consumed_at
             FROM replies WHERE id = ?1",
        )?;

        stmt.query_row(params![id], |row| {
            let status_str: String = row.get(5)?;
            Ok(Reply {
                id: row.get(0)?,
                message_id: row.get(1)?,
                body: row.get(2)?,
                source: row.get(3)?,
                source_device: row.get(4)?,
                status: status_str.parse().unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                consumed_at: row.get::<_, Option<String>>(7)?.and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .ok()
                }),
            })
        })
        .map_err(|_| StorageError::NotFound(format!("Reply not found: {}", id)))
    }

    pub fn get_latest_pending_reply(
        &self,
        agent_id: Option<&str>,
        project: Option<&str>,
    ) -> Result<Option<Reply>, StorageError> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT r.id, r.message_id, r.body, r.source, r.source_device, r.status, r.created_at, r.consumed_at
             FROM replies r
             JOIN messages m ON r.message_id = m.id
             WHERE r.status = 'pending'",
        );
        let mut params: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(agent_id) = agent_id {
            sql.push_str(" AND m.agent_id = ?");
            params.push(rusqlite::types::Value::Text(agent_id.to_string()));
        }
        if let Some(project) = project {
            sql.push_str(" AND m.project = ?");
            params.push(rusqlite::types::Value::Text(project.to_string()));
        }

        sql.push_str(" ORDER BY r.created_at DESC LIMIT 1");

        let mut stmt = conn.prepare(&sql)?;

        let result = stmt.query_row(rusqlite::params_from_iter(params.iter()), |row| {
            let status_str: String = row.get(5)?;
            Ok(Reply {
                id: row.get(0)?,
                message_id: row.get(1)?,
                body: row.get(2)?,
                source: row.get(3)?,
                source_device: row.get(4)?,
                status: status_str.parse().unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                consumed_at: row.get::<_, Option<String>>(7)?.and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .ok()
                }),
            })
        });

        match result {
            Ok(reply) => Ok(Some(reply)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Database(e)),
        }
    }

    pub fn update_reply_status(&self, id: &str, status: ReplyStatus) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let consumed_at = if status == ReplyStatus::Consumed {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };
        conn.execute(
            "UPDATE replies SET status = ?1, consumed_at = COALESCE(?2, consumed_at) WHERE id = ?3",
            params![status.to_string(), consumed_at, id],
        )?;
        info!("Updated reply {} status to {}", id, status);
        Ok(())
    }

    pub fn create_event(&self, event: &Event) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (id, event_type, actor, source_device, created_at, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.id,
                event.event_type,
                event.actor,
                event.source_device,
                event.created_at.to_rfc3339(),
                event.payload_json,
            ],
        )?;
        info!("Created event: {} - {}", event.id, event.event_type);
        Ok(())
    }

    pub fn list_events(&self, limit: i64) -> Result<Vec<Event>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, event_type, actor, source_device, created_at, payload_json
             FROM events ORDER BY created_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            Ok(Event {
                id: row.get(0)?,
                event_type: row.get(1)?,
                actor: row.get(2)?,
                source_device: row.get(3)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                payload_json: row.get(5)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn append_event_log(&self, event: &EventLogEntry) -> Result<EventLogEntry, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut event = event.clone();
        let prev_hash = conn
            .query_row(
                "SELECT event_hash FROM event_log ORDER BY seq DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        event.seq = None;
        event.inserted_at = chrono::Utc::now();
        event.prev_hash = prev_hash.clone();
        event.event_hash = compute_event_hash(&event, prev_hash.as_deref());

        let inserted = conn.execute(
            "INSERT OR IGNORE INTO event_log (
                event_id, event_type, source, actor, subject, visibility, event_time,
                inserted_at, datacontenttype, dataschema, data_json, extensions_json,
                idempotency_key, correlation_id, causation_id, trace_id, span_id,
                resource, prev_hash, event_hash
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                event.event_id,
                event.event_type,
                event.source,
                event.actor,
                event.subject,
                event.visibility.to_string(),
                event.event_time.to_rfc3339(),
                event.inserted_at.to_rfc3339(),
                event.datacontenttype,
                event.dataschema,
                event.data_json,
                event.extensions_json,
                event.idempotency_key,
                event.correlation_id,
                event.causation_id,
                event.trace_id,
                event.span_id,
                event.resource,
                event.prev_hash,
                event.event_hash,
            ],
        )?;

        if inserted == 0 {
            if let Some(idempotency_key) = event.idempotency_key.as_deref() {
                let sql = format!("{} WHERE idempotency_key = ?1", event_log_select_sql());
                let existing =
                    conn.query_row(&sql, params![idempotency_key], event_log_from_row)?;
                return Ok(existing);
            }
        }

        let sql = format!(
            "{} WHERE source = ?1 AND event_id = ?2",
            event_log_select_sql()
        );
        let inserted = conn.query_row(
            &sql,
            params![event.source, event.event_id],
            event_log_from_row,
        )?;
        Ok(inserted)
    }

    pub fn get_event_log_by_source_id(
        &self,
        source: &str,
        event_id: &str,
    ) -> Result<EventLogEntry, StorageError> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "{} WHERE source = ?1 AND event_id = ?2",
            event_log_select_sql()
        );
        conn.query_row(&sql, params![source, event_id], event_log_from_row)
            .map_err(|_| StorageError::NotFound(format!("Event not found: {source}/{event_id}")))
    }

    pub fn list_event_log(
        &self,
        after_seq: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<EventLogEntry>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.unwrap_or(100).clamp(1, 1000);
        let mut events = Vec::new();

        if let Some(after_seq) = after_seq {
            let sql = format!(
                "{} WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2",
                event_log_select_sql()
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![after_seq, limit], event_log_from_row)?;
            for row in rows {
                events.push(row?);
            }
        } else {
            let sql = format!("{} ORDER BY seq ASC LIMIT ?1", event_log_select_sql());
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![limit], event_log_from_row)?;
            for row in rows {
                events.push(row?);
            }
        }

        Ok(events)
    }

    pub fn create_grant(&self, grant: &Grant) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO grants (
                id, token_hash, token_prefix, issued_to, issued_by, scopes_json,
                resources_json, expires_at, max_uses, uses, requires_human, status,
                created_at, revoked_at, metadata_json
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                grant.id,
                grant.token_hash,
                grant.token_prefix,
                grant.issued_to,
                grant.issued_by,
                grant.scopes_json,
                grant.resources_json,
                grant.expires_at.map(|dt| dt.to_rfc3339()),
                grant.max_uses,
                grant.uses,
                if grant.requires_human { 1 } else { 0 },
                grant.status,
                grant.created_at.to_rfc3339(),
                grant.revoked_at.map(|dt| dt.to_rfc3339()),
                grant.metadata_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_grant(&self, id: &str) -> Result<Grant, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, token_hash, token_prefix, issued_to, issued_by, scopes_json,
                    resources_json, expires_at, max_uses, uses, requires_human, status,
                    created_at, revoked_at, metadata_json
             FROM grants WHERE id = ?1",
            params![id],
            grant_from_row,
        )
        .map_err(|_| StorageError::NotFound(format!("Grant not found: {id}")))
    }

    pub fn get_active_grant_by_token_hash(&self, token_hash: &str) -> Result<Grant, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, token_hash, token_prefix, issued_to, issued_by, scopes_json,
                    resources_json, expires_at, max_uses, uses, requires_human, status,
                    created_at, revoked_at, metadata_json
             FROM grants
             WHERE token_hash = ?1 AND status = 'active'
             LIMIT 1",
            params![token_hash],
            grant_from_row,
        )
        .map_err(|_| StorageError::NotFound("Active grant not found".to_string()))
    }

    pub fn revoke_grant(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE grants
             SET status = 'revoked', revoked_at = ?1
             WHERE id = ?2",
            params![chrono::Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn use_grant(
        &self,
        id: &str,
        actor: &str,
        scope: &str,
        resource: Option<&str>,
        event_seq: Option<i64>,
    ) -> Result<GrantUse, StorageError> {
        let conn = self.conn.lock().unwrap();
        let grant = conn
            .query_row(
                "SELECT id, token_hash, token_prefix, issued_to, issued_by, scopes_json,
                        resources_json, expires_at, max_uses, uses, requires_human, status,
                        created_at, revoked_at, metadata_json
                 FROM grants WHERE id = ?1",
                params![id],
                grant_from_row,
            )
            .map_err(|_| StorageError::NotFound(format!("Grant not found: {id}")))?;

        if grant.status != "active" {
            return Err(StorageError::PermissionDenied(format!(
                "Grant is not active: {}",
                grant.status
            )));
        }
        if grant
            .expires_at
            .is_some_and(|expires_at| chrono::Utc::now() > expires_at)
        {
            return Err(StorageError::PermissionDenied(
                "Grant has expired".to_string(),
            ));
        }
        if grant
            .max_uses
            .is_some_and(|max_uses| grant.uses >= max_uses)
        {
            return Err(StorageError::PermissionDenied(
                "Grant max uses reached".to_string(),
            ));
        }

        let scopes: Vec<String> = serde_json::from_str(&grant.scopes_json).unwrap_or_default();
        if !scopes.iter().any(|item| item == "*" || item == scope) {
            return Err(StorageError::PermissionDenied(format!(
                "Grant scope denied: {scope}"
            )));
        }

        let grant_use = GrantUse {
            id: Uuid::new_v4().to_string(),
            grant_id: id.to_string(),
            event_seq,
            actor: actor.to_string(),
            scope: scope.to_string(),
            resource: resource.map(|value| value.to_string()),
            decision: "allowed".to_string(),
            reason: None,
            created_at: chrono::Utc::now(),
        };

        conn.execute(
            "UPDATE grants SET uses = uses + 1 WHERE id = ?1",
            params![id],
        )?;
        conn.execute(
            "INSERT INTO grant_uses (
                id, grant_id, event_seq, actor, scope, resource, decision, reason, created_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                grant_use.id,
                grant_use.grant_id,
                grant_use.event_seq,
                grant_use.actor,
                grant_use.scope,
                grant_use.resource,
                grant_use.decision,
                grant_use.reason,
                grant_use.created_at.to_rfc3339(),
            ],
        )?;

        Ok(grant_use)
    }

    pub fn grant_device_capability(
        &self,
        capability: &DeviceCapability,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO device_capabilities (
                id, device_id, capability, granted_by, granted_at, expires_at,
                revoked_at, metadata_json
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(device_id, capability) WHERE revoked_at IS NULL
             DO UPDATE SET
                granted_by = excluded.granted_by,
                granted_at = excluded.granted_at,
                expires_at = excluded.expires_at,
                metadata_json = excluded.metadata_json",
            params![
                capability.id,
                capability.device_id,
                capability.capability,
                capability.granted_by,
                capability.granted_at.to_rfc3339(),
                capability.expires_at.map(|dt| dt.to_rfc3339()),
                capability.revoked_at.map(|dt| dt.to_rfc3339()),
                capability.metadata_json,
            ],
        )?;
        Ok(())
    }

    pub fn list_device_capabilities(
        &self,
        device_id: &str,
    ) -> Result<Vec<DeviceCapability>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, device_id, capability, granted_by, granted_at, expires_at,
                    revoked_at, metadata_json
             FROM device_capabilities
             WHERE device_id = ?1
             ORDER BY capability ASC, granted_at DESC",
        )?;
        let rows = stmt.query_map(params![device_id], device_capability_from_row)?;
        let mut capabilities = Vec::new();
        for row in rows {
            capabilities.push(row?);
        }
        Ok(capabilities)
    }

    pub fn list_active_device_capabilities(
        &self,
        device_id: &str,
    ) -> Result<Vec<String>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT capability
             FROM device_capabilities
             WHERE device_id = ?1
               AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > ?2)
             ORDER BY capability ASC",
        )?;
        let rows = stmt.query_map(params![device_id, chrono::Utc::now().to_rfc3339()], |row| {
            row.get::<_, String>(0)
        })?;
        let mut capabilities = Vec::new();
        for row in rows {
            capabilities.push(row?);
        }
        Ok(capabilities)
    }

    pub fn device_has_capability(
        &self,
        device_id: &str,
        capability: &str,
    ) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM device_capabilities
             WHERE device_id = ?1
               AND capability = ?2
               AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > ?3)",
            params![device_id, capability, chrono::Utc::now().to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn revoke_device_capability(
        &self,
        device_id: &str,
        capability: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE device_capabilities
             SET revoked_at = ?1
             WHERE device_id = ?2 AND capability = ?3 AND revoked_at IS NULL",
            params![chrono::Utc::now().to_rfc3339(), device_id, capability],
        )?;
        Ok(())
    }

    pub fn create_action_intent(&self, intent: &ActionIntent) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO action_intents (
                id, message_id, kind, status, requested_by_device_id, agent_id, project,
                profile_id, risk, required_capability, payload_json, payload_hash,
                approval_id, grant_id, created_at, updated_at, expires_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                intent.id,
                intent.message_id,
                intent.kind,
                intent.status,
                intent.requested_by_device_id,
                intent.agent_id,
                intent.project,
                intent.profile_id,
                intent.risk,
                intent.required_capability,
                intent.payload_json,
                intent.payload_hash,
                intent.approval_id,
                intent.grant_id,
                intent.created_at.to_rfc3339(),
                intent.updated_at.to_rfc3339(),
                intent.expires_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn get_action_intent(&self, id: &str) -> Result<ActionIntent, StorageError> {
        let conn = self.conn.lock().unwrap();
        let sql = format!("{} WHERE id = ?1", action_intent_select_sql());
        conn.query_row(&sql, params![id], action_intent_from_row)
            .map_err(|_| StorageError::NotFound(format!("Action intent not found: {id}")))
    }

    pub fn get_action_intent_for_message(
        &self,
        message_id: &str,
    ) -> Result<Option<ActionIntent>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "{} WHERE message_id = ?1 ORDER BY created_at DESC LIMIT 1",
            action_intent_select_sql()
        );
        conn.query_row(&sql, params![message_id], action_intent_from_row)
            .optional()
            .map_err(StorageError::Database)
    }

    pub fn list_action_intents(
        &self,
        status: Option<&str>,
        agent_id: Option<&str>,
        project: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ActionIntent>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "{} WHERE (?1 IS NULL OR status = ?1)
                  AND (?2 IS NULL OR agent_id = ?2)
                  AND (?3 IS NULL OR project = ?3)
                ORDER BY created_at ASC
                LIMIT ?4",
            action_intent_select_sql()
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![status, agent_id, project, limit.unwrap_or(50).clamp(1, 500)],
            action_intent_from_row,
        )?;
        let mut actions = Vec::new();
        for row in rows {
            actions.push(row?);
        }
        Ok(actions)
    }

    pub fn update_action_status(
        &self,
        id: &str,
        status: &str,
        approval_id: Option<&str>,
        grant_id: Option<&str>,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE action_intents
             SET status = ?1,
                 approval_id = COALESCE(?2, approval_id),
                 grant_id = COALESCE(?3, grant_id),
                 updated_at = ?4
             WHERE id = ?5",
            params![
                status,
                approval_id,
                grant_id,
                chrono::Utc::now().to_rfc3339(),
                id
            ],
        )?;
        Ok(())
    }

    pub fn claim_action_intent(
        &self,
        id: &str,
        worker_id: &str,
        policy_hash: Option<&str>,
        lease_seconds: Option<u64>,
    ) -> Result<ActionRun, StorageError> {
        let conn = self.conn.lock().unwrap();
        let sql = format!("{} WHERE id = ?1", action_intent_select_sql());
        let intent = conn
            .query_row(&sql, params![id], action_intent_from_row)
            .map_err(|_| StorageError::NotFound(format!("Action intent not found: {id}")))?;

        if !matches!(intent.status.as_str(), "pending" | "approved") {
            return Err(StorageError::PermissionDenied(format!(
                "Action is not claimable: {}",
                intent.status
            )));
        }
        if intent
            .expires_at
            .is_some_and(|expires_at| chrono::Utc::now() > expires_at)
        {
            return Err(StorageError::PermissionDenied(
                "Action has expired".to_string(),
            ));
        }

        let run = ActionRun::new(
            id.to_string(),
            worker_id.to_string(),
            policy_hash.map(|value| value.to_string()),
            lease_seconds,
        );
        let updated = conn.execute(
            "UPDATE action_intents
             SET status = 'claimed', updated_at = ?1
             WHERE id = ?2 AND status IN ('pending', 'approved')",
            params![chrono::Utc::now().to_rfc3339(), id],
        )?;
        if updated != 1 {
            return Err(StorageError::PermissionDenied(
                "Action was claimed by another worker".to_string(),
            ));
        }
        conn.execute(
            "INSERT INTO action_runs (
                id, intent_id, worker_id, status, policy_hash, claimed_at, lease_until,
                started_at, completed_at, exit_code, stdout_artifact_id, stderr_artifact_id,
                output_summary, error_json
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                run.id,
                run.intent_id,
                run.worker_id,
                run.status,
                run.policy_hash,
                run.claimed_at.to_rfc3339(),
                run.lease_until.map(|dt| dt.to_rfc3339()),
                run.started_at.map(|dt| dt.to_rfc3339()),
                run.completed_at.map(|dt| dt.to_rfc3339()),
                run.exit_code,
                run.stdout_artifact_id,
                run.stderr_artifact_id,
                run.output_summary,
                run.error_json,
            ],
        )?;
        Ok(run)
    }

    pub fn mark_action_run_started(&self, run_id: &str) -> Result<ActionRun, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE action_runs SET status = 'running', started_at = ?1 WHERE id = ?2",
            params![now, run_id],
        )?;
        conn.execute(
            "UPDATE action_intents
             SET status = 'running', updated_at = ?1
             WHERE id = (SELECT intent_id FROM action_runs WHERE id = ?2)",
            params![chrono::Utc::now().to_rfc3339(), run_id],
        )?;
        let sql = format!("{} WHERE id = ?1", action_run_select_sql());
        conn.query_row(&sql, params![run_id], action_run_from_row)
            .map_err(|_| StorageError::NotFound(format!("Action run not found: {run_id}")))
    }

    pub fn complete_action_run(
        &self,
        run_id: &str,
        status: &str,
        exit_code: Option<i64>,
        output_summary: Option<&str>,
        error_json: Option<&str>,
    ) -> Result<ActionRun, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let intent_status = if status == "succeeded" {
            "succeeded"
        } else {
            "failed"
        };
        conn.execute(
            "UPDATE action_runs
             SET status = ?1,
                 completed_at = ?2,
                 exit_code = ?3,
                 output_summary = ?4,
                 error_json = ?5
             WHERE id = ?6",
            params![status, now, exit_code, output_summary, error_json, run_id],
        )?;
        conn.execute(
            "UPDATE action_intents
             SET status = ?1, updated_at = ?2
             WHERE id = (SELECT intent_id FROM action_runs WHERE id = ?3)",
            params![intent_status, chrono::Utc::now().to_rfc3339(), run_id],
        )?;
        let sql = format!("{} WHERE id = ?1", action_run_select_sql());
        conn.query_row(&sql, params![run_id], action_run_from_row)
            .map_err(|_| StorageError::NotFound(format!("Action run not found: {run_id}")))
    }

    pub fn create_action_approval(&self, approval: &ActionApproval) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO action_approvals (
                id, intent_id, status, nonce_hash, nonce_prefix, payload_hash,
                requested_at, expires_at, approved_by_device_id, approved_at, denied_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                approval.id,
                approval.intent_id,
                approval.status,
                approval.nonce_hash,
                approval.nonce_prefix,
                approval.payload_hash,
                approval.requested_at.to_rfc3339(),
                approval.expires_at.to_rfc3339(),
                approval.approved_by_device_id,
                approval.approved_at.map(|dt| dt.to_rfc3339()),
                approval.denied_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        conn.execute(
            "UPDATE action_intents
             SET approval_id = ?1, status = 'awaiting_approval', updated_at = ?2
             WHERE id = ?3",
            params![
                approval.id,
                chrono::Utc::now().to_rfc3339(),
                approval.intent_id
            ],
        )?;
        Ok(())
    }

    pub fn list_action_approvals(
        &self,
        status: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ActionApproval>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "{} WHERE (?1 IS NULL OR status = ?1)
                ORDER BY requested_at DESC
                LIMIT ?2",
            action_approval_select_sql()
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![status, limit.unwrap_or(50).clamp(1, 500)],
            action_approval_from_row,
        )?;
        let mut approvals = Vec::new();
        for row in rows {
            approvals.push(row?);
        }
        Ok(approvals)
    }

    pub fn decide_action_approval(
        &self,
        id: &str,
        decision: &str,
        nonce: Option<&str>,
        approved_by_device_id: Option<&str>,
    ) -> Result<ActionApproval, StorageError> {
        let conn = self.conn.lock().unwrap();
        let sql = format!("{} WHERE id = ?1", action_approval_select_sql());
        let approval = conn
            .query_row(&sql, params![id], action_approval_from_row)
            .map_err(|_| StorageError::NotFound(format!("Approval not found: {id}")))?;

        if approval.status != "pending" {
            return Err(StorageError::PermissionDenied(format!(
                "Approval is not pending: {}",
                approval.status
            )));
        }
        if approval.expires_at < chrono::Utc::now() {
            return Err(StorageError::PermissionDenied(
                "Approval has expired".to_string(),
            ));
        }

        let now = chrono::Utc::now().to_rfc3339();
        match decision {
            "approve" => {
                let Some(nonce) = nonce else {
                    return Err(StorageError::PermissionDenied(
                        "Approval nonce is required".to_string(),
                    ));
                };
                if crate::hash_token(nonce) != approval.nonce_hash {
                    return Err(StorageError::PermissionDenied(
                        "Approval nonce mismatch".to_string(),
                    ));
                }
                conn.execute(
                    "UPDATE action_approvals
                     SET status = 'approved', approved_by_device_id = ?1, approved_at = ?2
                     WHERE id = ?3",
                    params![approved_by_device_id, now, id],
                )?;
                conn.execute(
                    "UPDATE action_intents
                     SET status = 'approved', updated_at = ?1
                     WHERE id = ?2 AND status = 'awaiting_approval'",
                    params![chrono::Utc::now().to_rfc3339(), approval.intent_id],
                )?;
            }
            "deny" => {
                conn.execute(
                    "UPDATE action_approvals
                     SET status = 'denied', denied_at = ?1
                     WHERE id = ?2",
                    params![now, id],
                )?;
                conn.execute(
                    "UPDATE action_intents
                     SET status = 'denied', updated_at = ?1
                     WHERE id = ?2",
                    params![chrono::Utc::now().to_rfc3339(), approval.intent_id],
                )?;
            }
            other => {
                return Err(StorageError::PermissionDenied(format!(
                    "Unknown approval decision: {other}"
                )));
            }
        }

        let sql = format!("{} WHERE id = ?1", action_approval_select_sql());
        conn.query_row(&sql, params![id], action_approval_from_row)
            .map_err(|_| StorageError::NotFound(format!("Approval not found: {id}")))
    }

    pub fn create_context_snapshot(&self, snapshot: &ContextSnapshot) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO context_snapshots (
                id, message_id, captured_at, source, stage, repo_root_hash,
                repo_root_display, git_common_dir_hash, worktree_id, worktree_path_display,
                branch, head_oid, upstream, ahead, behind, dirty, staged_count,
                unstaged_count, untracked_count, status_json, worktrees_json,
                staged_patch_id, staged_patch_sha256, unstaged_patch_id,
                unstaged_patch_sha256, post_commit_oid, expires_at, pinned
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                     ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
            params![
                snapshot.id,
                snapshot.message_id,
                snapshot.captured_at.to_rfc3339(),
                snapshot.source,
                snapshot.stage,
                snapshot.repo_root_hash,
                snapshot.repo_root_display,
                snapshot.git_common_dir_hash,
                snapshot.worktree_id,
                snapshot.worktree_path_display,
                snapshot.branch,
                snapshot.head_oid,
                snapshot.upstream,
                snapshot.ahead,
                snapshot.behind,
                if snapshot.dirty { 1 } else { 0 },
                snapshot.staged_count,
                snapshot.unstaged_count,
                snapshot.untracked_count,
                snapshot.status_json,
                snapshot.worktrees_json,
                snapshot.staged_patch_id,
                snapshot.staged_patch_sha256,
                snapshot.unstaged_patch_id,
                snapshot.unstaged_patch_sha256,
                snapshot.post_commit_oid,
                snapshot.expires_at.map(|dt| dt.to_rfc3339()),
                if snapshot.pinned { 1 } else { 0 },
            ],
        )?;
        Ok(())
    }

    pub fn get_context_snapshot(&self, id: &str) -> Result<ContextSnapshot, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, message_id, captured_at, source, stage, repo_root_hash,
                    repo_root_display, git_common_dir_hash, worktree_id, worktree_path_display,
                    branch, head_oid, upstream, ahead, behind, dirty, staged_count,
                    unstaged_count, untracked_count, status_json, worktrees_json,
                    staged_patch_id, staged_patch_sha256, unstaged_patch_id,
                    unstaged_patch_sha256, post_commit_oid, expires_at, pinned
             FROM context_snapshots WHERE id = ?1",
            params![id],
            context_snapshot_from_row,
        )
        .map_err(|_| StorageError::NotFound(format!("Context snapshot not found: {id}")))
    }

    pub fn list_context_snapshots(
        &self,
        message_id: &str,
    ) -> Result<Vec<ContextSnapshot>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, captured_at, source, stage, repo_root_hash,
                    repo_root_display, git_common_dir_hash, worktree_id, worktree_path_display,
                    branch, head_oid, upstream, ahead, behind, dirty, staged_count,
                    unstaged_count, untracked_count, status_json, worktrees_json,
                    staged_patch_id, staged_patch_sha256, unstaged_patch_id,
                    unstaged_patch_sha256, post_commit_oid, expires_at, pinned
             FROM context_snapshots
             WHERE message_id = ?1
             ORDER BY captured_at ASC",
        )?;
        let rows = stmt.query_map(params![message_id], context_snapshot_from_row)?;
        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    pub fn create_artifact_metadata(
        &self,
        artifact: &ArtifactMetadata,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO artifacts (
                id, message_id, snapshot_id, kind, media_type, sha256, size_bytes,
                storage_uri, width, height, created_at, expires_at, pinned, metadata_json
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                artifact.id,
                artifact.message_id,
                artifact.snapshot_id,
                artifact.kind,
                artifact.media_type,
                artifact.sha256,
                artifact.size_bytes,
                artifact.storage_uri,
                artifact.width,
                artifact.height,
                artifact.created_at.to_rfc3339(),
                artifact.expires_at.map(|dt| dt.to_rfc3339()),
                if artifact.pinned { 1 } else { 0 },
                artifact.metadata_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_artifact_metadata(&self, id: &str) -> Result<ArtifactMetadata, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, message_id, snapshot_id, kind, media_type, sha256, size_bytes,
                    storage_uri, width, height, created_at, expires_at, pinned, metadata_json
             FROM artifacts WHERE id = ?1",
            params![id],
            artifact_from_row,
        )
        .map_err(|_| StorageError::NotFound(format!("Artifact metadata not found: {id}")))
    }

    pub fn list_artifacts_for_message(
        &self,
        message_id: &str,
    ) -> Result<Vec<ArtifactMetadata>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, snapshot_id, kind, media_type, sha256, size_bytes,
                    storage_uri, width, height, created_at, expires_at, pinned, metadata_json
             FROM artifacts
             WHERE message_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![message_id], artifact_from_row)?;
        let mut artifacts = Vec::new();
        for row in rows {
            artifacts.push(row?);
        }
        Ok(artifacts)
    }

    pub fn sweep_expired_artifact_metadata(&self) -> Result<usize, StorageError> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM artifacts
             WHERE pinned = 0
               AND expires_at IS NOT NULL
               AND expires_at < ?1",
            params![chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(count)
    }

    pub fn create_outbox_entry(&self, entry: &OutboxEntry) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO outbox (id, destination, payload_json, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.id,
                entry.destination,
                entry.payload_json,
                entry.status.to_string(),
                entry.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_outbox(&self, limit: i64) -> Result<Vec<OutboxEntry>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, destination, payload_json, status, created_at
             FROM outbox WHERE status = 'pending' ORDER BY created_at ASC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            let status_str: String = row.get(3)?;
            Ok(OutboxEntry {
                id: row.get(0)?,
                destination: row.get(1)?,
                payload_json: row.get(2)?,
                status: status_str.parse().unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn upsert_push_subscription(
        &self,
        subscription: &PushSubscription,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();

        if subscription.device_id.is_some() {
            let existing_id = conn
                .query_row(
                    "SELECT id FROM push_subscriptions WHERE endpoint = ?1 ORDER BY created_at DESC LIMIT 1",
                    params![subscription.endpoint],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            if let Some(existing_id) = existing_id {
                conn.execute(
                    "UPDATE push_subscriptions
                     SET device_id = ?1,
                         endpoint = ?2,
                         p256dh = ?3,
                         auth = ?4,
                         user_agent = ?5,
                         status = 'active',
                         vapid_public_key_hash = ?6,
                         last_error = NULL
                     WHERE id = ?7",
                    params![
                        subscription.device_id,
                        subscription.endpoint,
                        subscription.p256dh,
                        subscription.auth,
                        subscription.user_agent,
                        subscription.vapid_public_key_hash,
                        existing_id,
                    ],
                )?;
                conn.execute(
                    "UPDATE push_subscriptions
                     SET status = 'revoked', last_error = 'duplicate_endpoint_replaced'
                     WHERE endpoint = ?1 AND id != ?2",
                    params![subscription.endpoint, existing_id],
                )?;
                info!(
                    "Claimed existing push subscription endpoint for device: {}",
                    existing_id
                );
                return Ok(());
            }
        }

        conn.execute(
            "INSERT INTO push_subscriptions (id, device_id, endpoint, p256dh, auth, user_agent, created_at, status, vapid_public_key_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                device_id = excluded.device_id,
                endpoint = excluded.endpoint,
                p256dh = excluded.p256dh,
                auth = excluded.auth,
                user_agent = excluded.user_agent,
                status = excluded.status,
                vapid_public_key_hash = excluded.vapid_public_key_hash,
                last_error = NULL",
            params![
                subscription.id,
                subscription.device_id,
                subscription.endpoint,
                subscription.p256dh,
                subscription.auth,
                subscription.user_agent,
                subscription.created_at.to_rfc3339(),
                subscription.status,
                subscription.vapid_public_key_hash,
            ],
        )?;
        info!("Upserted push subscription: {}", subscription.id);
        Ok(())
    }

    pub fn claim_active_legacy_push_subscriptions(
        &self,
        device_id: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE push_subscriptions
             SET device_id = ?1, status = 'active', last_error = NULL
             WHERE device_id IS NULL AND status = 'active'",
            params![device_id],
        )?;
        Ok(updated)
    }

    pub fn list_active_push_subscriptions(&self) -> Result<Vec<PushSubscription>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.device_id, s.endpoint, s.p256dh, s.auth, s.user_agent, s.created_at, s.last_success_at, s.last_error, s.status, s.vapid_public_key_hash
             FROM push_subscriptions s
             LEFT JOIN devices d ON s.device_id = d.id
             WHERE s.status = 'active' AND (s.device_id IS NULL OR d.revoked_at IS NULL)",
        )?;

        let rows = stmt.query_map([], push_subscription_from_row)?;

        let mut subs = Vec::new();
        for row in rows {
            subs.push(row?);
        }
        Ok(subs)
    }

    pub fn list_push_subscriptions(&self) -> Result<Vec<PushSubscription>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, device_id, endpoint, p256dh, auth, user_agent, created_at, last_success_at, last_error, status, vapid_public_key_hash
             FROM push_subscriptions ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([], push_subscription_from_row)?;

        let mut subs = Vec::new();
        for row in rows {
            subs.push(row?);
        }
        Ok(subs)
    }

    pub fn push_subscription_counts(&self) -> Result<PushSubscriptionCounts, StorageError> {
        let subscriptions = self.list_push_subscriptions()?;
        let devices = self.list_devices()?;
        let active_devices = devices
            .into_iter()
            .map(|device| {
                let is_active = device.is_active();
                (device.id, is_active)
            })
            .collect::<std::collections::HashMap<_, _>>();

        let mut counts = PushSubscriptionCounts {
            total: subscriptions.len(),
            ..Default::default()
        };

        for subscription in subscriptions {
            if subscription.status != "active" {
                counts.revoked_or_stale += 1;
                if subscription.status == "revoked" {
                    counts.revoked += 1;
                } else {
                    counts.stale += 1;
                }
                continue;
            }

            match subscription.device_id.as_deref() {
                None => counts.active_legacy += 1,
                Some(device_id) => match active_devices.get(device_id) {
                    Some(true) => counts.active_bound += 1,
                    Some(false) => {
                        counts.revoked_or_stale += 1;
                        counts.revoked += 1;
                    }
                    None => {
                        counts.revoked_or_stale += 1;
                        counts.stale += 1;
                    }
                },
            }
        }

        Ok(counts)
    }

    pub fn update_push_subscription_error(
        &self,
        id: &str,
        error: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE push_subscriptions SET last_error = ?1 WHERE id = ?2",
            params![error, id],
        )?;
        Ok(())
    }

    pub fn mark_push_subscription_stale(&self, id: &str, error: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE push_subscriptions SET status = 'stale', last_error = ?1 WHERE id = ?2",
            params![error, id],
        )?;
        Ok(())
    }

    pub fn update_push_subscription_success(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE push_subscriptions SET last_success_at = ?1, last_error = NULL WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    // Device methods
    pub fn create_device(&self, device: &crate::models::Device) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO devices (id, name, kind, token_hash, token_prefix, paired_at, last_seen_at, revoked_at, user_agent, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                device.id,
                device.name,
                device.kind,
                device.token_hash,
                device.token_prefix,
                device.paired_at.to_rfc3339(),
                device.last_seen_at.map(|dt| dt.to_rfc3339()),
                device.revoked_at.map(|dt| dt.to_rfc3339()),
                device.user_agent,
                device.metadata_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_device(&self, id: &str) -> Result<crate::models::Device, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, token_hash, token_prefix, paired_at, last_seen_at, revoked_at, user_agent, metadata_json
             FROM devices WHERE id = ?1",
        )?;

        let device = stmt
            .query_row(params![id], |row| {
                Ok(crate::models::Device {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    token_hash: row.get(3)?,
                    token_prefix: row.get(4)?,
                    paired_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    last_seen_at: row.get::<_, Option<String>>(6)?.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                    revoked_at: row.get::<_, Option<String>>(7)?.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                    user_agent: row.get(8)?,
                    metadata_json: row.get(9)?,
                })
            })
            .map_err(|_| StorageError::NotFound(format!("Device not found: {}", id)))?;

        Ok(device)
    }

    pub fn get_device_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<crate::models::Device, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, token_hash, token_prefix, paired_at, last_seen_at, revoked_at, user_agent, metadata_json
             FROM devices WHERE token_hash = ?1",
        )?;

        let device = stmt
            .query_row(params![token_hash], |row| {
                Ok(crate::models::Device {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    token_hash: row.get(3)?,
                    token_prefix: row.get(4)?,
                    paired_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    last_seen_at: row.get::<_, Option<String>>(6)?.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                    revoked_at: row.get::<_, Option<String>>(7)?.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                    user_agent: row.get(8)?,
                    metadata_json: row.get(9)?,
                })
            })
            .map_err(|_| {
                StorageError::NotFound(format!("Device not found with token hash: {}", token_hash))
            })?;

        Ok(device)
    }

    pub fn list_devices(&self) -> Result<Vec<crate::models::Device>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, token_hash, token_prefix, paired_at, last_seen_at, revoked_at, user_agent, metadata_json
             FROM devices ORDER BY paired_at DESC",
        )?;

        let devices = stmt
            .query_map([], |row| {
                Ok(crate::models::Device {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    token_hash: row.get(3)?,
                    token_prefix: row.get(4)?,
                    paired_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    last_seen_at: row.get::<_, Option<String>>(6)?.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                    revoked_at: row.get::<_, Option<String>>(7)?.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                    user_agent: row.get(8)?,
                    metadata_json: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(devices)
    }

    pub fn update_device_last_seen(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE devices SET last_seen_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
            params![now, id],
        )?;
        Ok(())
    }

    pub fn revoke_device(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE devices SET revoked_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        conn.execute(
            "UPDATE push_subscriptions SET status = 'revoked', last_error = 'device_revoked' WHERE device_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn reset_all_devices(&self) -> Result<DeviceResetSummary, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        let devices_revoked = conn.execute(
            "UPDATE devices SET revoked_at = ?1 WHERE revoked_at IS NULL",
            params![now],
        )?;
        let subscriptions_revoked = conn.execute(
            "UPDATE push_subscriptions
             SET status = 'revoked', last_error = 'device_reset'
             WHERE status != 'revoked'",
            [],
        )?;
        let pairing_codes_cleared = conn.execute(
            "DELETE FROM pairing_codes WHERE used_at IS NULL OR expires_at <= ?1",
            params![now],
        )?;

        Ok(DeviceResetSummary {
            devices_revoked,
            subscriptions_revoked,
            pairing_codes_cleared,
        })
    }

    pub fn clear_inactive_push_subscriptions(
        &self,
    ) -> Result<PushSubscriptionCleanupSummary, StorageError> {
        let conn = self.conn.lock().unwrap();

        let revoked_deleted = conn.execute(
            "DELETE FROM push_subscriptions WHERE status = 'revoked'",
            [],
        )?;
        let stale_deleted = conn.execute(
            "DELETE FROM push_subscriptions WHERE status != 'active'",
            [],
        )?;
        let legacy_deleted = conn.execute(
            "DELETE FROM push_subscriptions WHERE status = 'active' AND device_id IS NULL",
            [],
        )?;

        Ok(PushSubscriptionCleanupSummary {
            revoked_deleted,
            stale_deleted,
            legacy_deleted,
        })
    }

    pub fn mark_push_subscriptions_revoked_for_device(
        &self,
        device_id: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE push_subscriptions SET status = 'revoked', last_error = 'device_revoked' WHERE device_id = ?1",
            params![device_id],
        )?;
        Ok(())
    }

    // Pairing code methods
    pub fn create_pairing_code(
        &self,
        code: &crate::models::PairingCode,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO pairing_codes (code_hash, code_prefix, created_at, expires_at, used_at, device_name_hint, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                code.code_hash,
                code.code_prefix,
                code.created_at.to_rfc3339(),
                code.expires_at.to_rfc3339(),
                code.used_at.map(|dt| dt.to_rfc3339()),
                code.device_name_hint,
                code.metadata_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_pairing_code(
        &self,
        code_hash: &str,
    ) -> Result<crate::models::PairingCode, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT code_hash, code_prefix, created_at, expires_at, used_at, device_name_hint, metadata_json
             FROM pairing_codes WHERE code_hash = ?1",
        )?;

        let code = stmt
            .query_row(params![code_hash], |row| {
                Ok(crate::models::PairingCode {
                    code_hash: row.get(0)?,
                    code_prefix: row.get(1)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    expires_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    used_at: row.get::<_, Option<String>>(4)?.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .ok()
                    }),
                    device_name_hint: row.get(5)?,
                    metadata_json: row.get(6)?,
                })
            })
            .map_err(|_| {
                StorageError::NotFound(format!("Pairing code not found: {}", code_hash))
            })?;

        Ok(code)
    }

    pub fn mark_pairing_code_used(&self, code_hash: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE pairing_codes SET used_at = ?1 WHERE code_hash = ?2",
            params![now, code_hash],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ActionIntent, ArtifactMetadata, ContextSnapshot, Device, DeviceCapability, EventLogEntry,
        Grant, Message, PairingCode, PermissionLevel, PushSubscription,
    };
    use tempfile::NamedTempFile;

    fn make_storage() -> Storage {
        let file = NamedTempFile::new().unwrap();
        Storage::new(file.path()).unwrap()
    }

    fn make_message(
        title: &str,
        status: MessageStatus,
        project: Option<&str>,
        agent_id: Option<&str>,
    ) -> Message {
        let mut message = Message::new(
            title.to_string(),
            "body".to_string(),
            "test".to_string(),
            None,
            agent_id.map(|s| s.to_string()),
            project.map(|s| s.to_string()),
            PermissionLevel::Actionable,
        );
        message.status = status;
        message
    }

    #[test]
    fn list_messages_with_default_limit_returns_message() {
        let storage = make_storage();
        let message = make_message(
            "one",
            MessageStatus::PendingReply,
            Some("ivy"),
            Some("codex"),
        );
        storage.create_message(&message).unwrap();

        let messages = storage.list_messages(None, None, None, None).unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn list_messages_with_limit_works() {
        let storage = make_storage();
        for i in 0..3 {
            let message = make_message(
                &format!("m{i}"),
                MessageStatus::PendingReply,
                Some("ivy"),
                Some("codex"),
            );
            storage.create_message(&message).unwrap();
        }

        let messages = storage.list_messages(Some(2), None, None, None).unwrap();
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn list_messages_with_filters_works() {
        let storage = make_storage();
        let keep = make_message(
            "keep",
            MessageStatus::PendingReply,
            Some("ivy"),
            Some("codex"),
        );
        let other_status =
            make_message("other", MessageStatus::Consumed, Some("ivy"), Some("codex"));
        let other_project = make_message(
            "other2",
            MessageStatus::PendingReply,
            Some("moon"),
            Some("codex"),
        );
        let other_agent = make_message(
            "other3",
            MessageStatus::PendingReply,
            Some("ivy"),
            Some("other-agent"),
        );
        storage.create_message(&keep).unwrap();
        storage.create_message(&other_status).unwrap();
        storage.create_message(&other_project).unwrap();
        storage.create_message(&other_agent).unwrap();

        assert_eq!(
            storage
                .list_messages(None, Some(MessageStatus::PendingReply), None, None)
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            storage
                .list_messages(None, None, Some("ivy"), None)
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            storage
                .list_messages(None, None, None, Some("codex"))
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            storage
                .list_messages(
                    None,
                    Some(MessageStatus::PendingReply),
                    Some("ivy"),
                    Some("codex")
                )
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn latest_pending_reply_with_filters_works() {
        let storage = make_storage();
        let message = make_message(
            "reply-target",
            MessageStatus::PendingReply,
            Some("ivy"),
            Some("codex"),
        );
        storage.create_message(&message).unwrap();
        let reply = Reply::new(
            message.id.clone(),
            "hello".to_string(),
            "phone".to_string(),
            None,
        );
        storage.create_reply(&reply).unwrap();

        let latest = storage
            .get_latest_pending_reply(Some("codex"), Some("ivy"))
            .unwrap();
        assert!(latest.is_some());
    }

    #[test]
    fn ask_message_creation_sets_pending_reply() {
        let storage = make_storage();
        let mut message = make_message(
            "ask",
            MessageStatus::PendingReply,
            Some("signal"),
            Some("codex"),
        );
        message.expires_at = Some(chrono::Utc::now() + chrono::Duration::minutes(10));
        message.priority = Some("normal".to_string());
        message.reply_mode = Some("text".to_string());
        message.reply_options_json = Some("[\"yes\",\"no\"]".to_string());

        storage.create_message(&message).unwrap();
        let stored = storage.get_message(&message.id).unwrap();

        assert_eq!(stored.status, MessageStatus::PendingReply);
        assert!(stored.expires_at.is_some());
        assert_eq!(stored.priority.as_deref(), Some("normal"));
        assert_eq!(stored.reply_mode.as_deref(), Some("text"));
    }

    #[test]
    fn event_log_append_is_idempotent_and_hash_chained() {
        let storage = make_storage();
        let mut event = EventLogEntry::new(
            "signal.test.created".to_string(),
            "test".to_string(),
            "agent:codex".to_string(),
            serde_json::json!({"ok": true}).to_string(),
        );
        event.idempotency_key = Some("test-idempotency-key".to_string());

        let first = storage.append_event_log(&event).unwrap();
        let second = storage.append_event_log(&event).unwrap();
        assert_eq!(first.seq, second.seq);
        assert_eq!(first.event_hash.len(), 64);

        let next = EventLogEntry::new(
            "signal.test.updated".to_string(),
            "test".to_string(),
            "agent:codex".to_string(),
            serde_json::json!({"ok": false}).to_string(),
        );
        let next = storage.append_event_log(&next).unwrap();
        assert_eq!(next.prev_hash.as_deref(), Some(first.event_hash.as_str()));

        let events = storage.list_event_log(first.seq, Some(10)).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "signal.test.updated");
    }

    #[test]
    fn grant_use_enforces_scope_and_max_uses() {
        let storage = make_storage();
        let grant = Grant::new(
            crate::hash_token("grant-token"),
            "grant".to_string(),
            "agent:codex".to_string(),
            "user:local".to_string(),
            vec!["messages:send".to_string()],
            serde_json::json!({"messages": "*"}),
            Some(60),
        );
        storage.create_grant(&grant).unwrap();

        let grant_use = storage
            .use_grant(
                &grant.id,
                "agent:codex",
                "messages:send",
                Some("message:test"),
                None,
            )
            .unwrap();
        assert_eq!(grant_use.decision, "allowed");
        assert_eq!(storage.get_grant(&grant.id).unwrap().uses, 1);

        assert!(storage
            .use_grant(&grant.id, "agent:codex", "messages:send", None, None)
            .is_err());
        assert!(storage
            .use_grant(&grant.id, "agent:codex", "artifacts:write", None, None)
            .is_err());
    }

    #[test]
    fn device_capabilities_can_be_granted_and_revoked() {
        let storage = make_storage();
        let device = Device::new(
            "device-cap".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            crate::hash_token("device-token"),
            "sig_dev_cap".to_string(),
        );
        storage.create_device(&device).unwrap();
        let capability = DeviceCapability::new(
            device.id.clone(),
            "agent.wake".to_string(),
            "test".to_string(),
        );

        storage.grant_device_capability(&capability).unwrap();
        assert!(storage
            .device_has_capability(&device.id, "agent.wake")
            .unwrap());
        assert_eq!(
            storage.list_active_device_capabilities(&device.id).unwrap(),
            vec!["agent.wake".to_string()]
        );

        storage
            .revoke_device_capability(&device.id, "agent.wake")
            .unwrap();
        assert!(!storage
            .device_has_capability(&device.id, "agent.wake")
            .unwrap());
    }

    #[test]
    fn action_intent_claim_and_completion_lifecycle_works() {
        let storage = make_storage();
        let device = Device::new(
            "device-1".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            crate::hash_token("device-token"),
            "sig_dev_action".to_string(),
        );
        storage.create_device(&device).unwrap();
        let message = make_message("wake", MessageStatus::New, Some("signal"), Some("codex"));
        storage.create_message(&message).unwrap();
        let action = ActionIntent::new(
            message.id.clone(),
            "wake_agent".to_string(),
            Some("device-1".to_string()),
            Some("codex".to_string()),
            Some("signal".to_string()),
            None,
            "low".to_string(),
            "agent.wake".to_string(),
            serde_json::json!({"text": "hello"}),
            Some(60),
        );

        storage.create_action_intent(&action).unwrap();
        assert_eq!(
            storage
                .list_action_intents(Some("pending"), Some("codex"), Some("signal"), Some(10))
                .unwrap()
                .len(),
            1
        );

        let run = storage
            .claim_action_intent(&action.id, "worker-1", Some("policy"), Some(120))
            .unwrap();
        assert_eq!(
            storage.get_action_intent(&action.id).unwrap().status,
            "claimed"
        );

        storage.mark_action_run_started(&run.id).unwrap();
        assert_eq!(
            storage.get_action_intent(&action.id).unwrap().status,
            "running"
        );

        let completed = storage
            .complete_action_run(&run.id, "succeeded", Some(0), Some("ok"), None)
            .unwrap();
        assert_eq!(completed.status, "succeeded");
        assert_eq!(
            storage.get_action_intent(&action.id).unwrap().status,
            "succeeded"
        );
    }

    #[test]
    fn context_and_artifact_metadata_roundtrip() {
        let storage = make_storage();
        let message = make_message("context", MessageStatus::PendingReply, Some("signal"), None);
        storage.create_message(&message).unwrap();

        let mut snapshot = ContextSnapshot::new(
            message.id.clone(),
            "agent:codex".to_string(),
            "ping-sent".to_string(),
            serde_json::json!({"branch": "signal-architecture-plan"}).to_string(),
        );
        snapshot.branch = Some("signal-architecture-plan".to_string());
        snapshot.dirty = true;
        snapshot.staged_count = 1;
        storage.create_context_snapshot(&snapshot).unwrap();

        let snapshots = storage.list_context_snapshots(&message.id).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            snapshots[0].branch.as_deref(),
            Some("signal-architecture-plan")
        );

        let mut artifact = ArtifactMetadata::new(
            message.id.clone(),
            "screenshot".to_string(),
            "image/png".to_string(),
            "abc123".to_string(),
            123,
            "file://artifacts/abc123.png".to_string(),
        );
        artifact.snapshot_id = Some(snapshot.id.clone());
        artifact.expires_at = Some(chrono::Utc::now() - chrono::Duration::minutes(1));
        storage.create_artifact_metadata(&artifact).unwrap();

        let artifacts = storage.list_artifacts_for_message(&message.id).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts[0].snapshot_id.as_deref(),
            Some(snapshot.id.as_str())
        );
        assert_eq!(storage.sweep_expired_artifact_metadata().unwrap(), 1);
        assert!(storage
            .list_artifacts_for_message(&message.id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn reply_and_consume_state_updates_work() {
        let storage = make_storage();
        let message = make_message("ask", MessageStatus::PendingReply, None, Some("codex"));
        storage.create_message(&message).unwrap();
        let reply = Reply::new(
            message.id.clone(),
            "yes".to_string(),
            "phone".to_string(),
            None,
        );
        storage.create_reply(&reply).unwrap();
        storage
            .update_message_status(&message.id, MessageStatus::Replied)
            .unwrap();
        storage
            .update_reply_status(&reply.id, ReplyStatus::Consumed)
            .unwrap();

        assert_eq!(
            storage.get_message(&message.id).unwrap().status,
            MessageStatus::Replied
        );
        let consumed = storage.get_reply(&reply.id).unwrap();
        assert_eq!(consumed.status, ReplyStatus::Consumed);
        assert!(consumed.consumed_at.is_some());
    }

    #[test]
    fn pair_code_is_single_use_and_stores_hash_only() {
        let storage = make_storage();
        let raw_code = crate::generate_pairing_code();
        let code_hash = crate::hash_token(&raw_code);
        let pairing = PairingCode::new(code_hash.clone(), crate::get_token_prefix(&raw_code), 300);
        storage.create_pairing_code(&pairing).unwrap();

        let stored = storage.get_pairing_code(&code_hash).unwrap();
        assert_eq!(stored.code_hash, code_hash);
        assert_ne!(stored.code_hash, raw_code);
        assert!(stored.is_valid());

        storage.mark_pairing_code_used(&code_hash).unwrap();
        assert!(!storage.get_pairing_code(&code_hash).unwrap().is_valid());
    }

    #[test]
    fn revoked_device_push_subscriptions_are_skipped() {
        let storage = make_storage();
        let raw_token = crate::generate_device_token();
        let device = Device::new(
            "device-1".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            crate::hash_token(&raw_token),
            crate::get_token_prefix(&raw_token),
        );
        storage.create_device(&device).unwrap();

        let mut sub = PushSubscription::new(
            "https://web.push.apple.com/test".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        sub.device_id = Some(device.id.clone());
        storage.upsert_push_subscription(&sub).unwrap();
        assert_eq!(storage.list_active_push_subscriptions().unwrap().len(), 1);

        storage.revoke_device(&device.id).unwrap();
        assert_eq!(storage.list_active_push_subscriptions().unwrap().len(), 0);
    }

    #[test]
    fn reset_all_devices_revokes_devices_and_subscriptions() {
        let storage = make_storage();
        let raw_token = crate::generate_device_token();
        let device = Device::new(
            "device-reset".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            crate::hash_token(&raw_token),
            crate::get_token_prefix(&raw_token),
        );
        storage.create_device(&device).unwrap();

        let mut sub = PushSubscription::new(
            "https://web.push.apple.com/reset".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        sub.device_id = Some(device.id.clone());
        storage.upsert_push_subscription(&sub).unwrap();

        let raw_code = crate::generate_pairing_code();
        let pairing = PairingCode::new(
            crate::hash_token(&raw_code),
            crate::get_token_prefix(&raw_code),
            300,
        );
        storage.create_pairing_code(&pairing).unwrap();

        let summary = storage.reset_all_devices().unwrap();
        assert_eq!(summary.devices_revoked, 1);
        assert_eq!(summary.subscriptions_revoked, 1);
        assert_eq!(summary.pairing_codes_cleared, 1);
        assert!(!storage.get_device(&device.id).unwrap().is_active());
        assert_eq!(storage.list_active_push_subscriptions().unwrap().len(), 0);
        assert!(storage.get_pairing_code(&pairing.code_hash).is_err());
    }

    #[test]
    fn reset_all_devices_keeps_messages_and_replies() {
        let storage = make_storage();
        let message = make_message("keep", MessageStatus::PendingReply, Some("signal"), None);
        storage.create_message(&message).unwrap();
        let reply = Reply::new(
            message.id.clone(),
            "yes".to_string(),
            "phone".to_string(),
            None,
        );
        storage.create_reply(&reply).unwrap();

        storage.reset_all_devices().unwrap();

        assert_eq!(storage.get_message(&message.id).unwrap().id, message.id);
        assert_eq!(storage.get_reply(&reply.id).unwrap().id, reply.id);
    }

    #[test]
    fn revoking_one_device_keeps_other_active_subscription_sendable() {
        let storage = make_storage();
        let device_1 = Device::new(
            "device-one".to_string(),
            "old phone".to_string(),
            "phone".to_string(),
            crate::hash_token("token-one"),
            "token-one".to_string(),
        );
        let device_2 = Device::new(
            "device-two".to_string(),
            "new phone".to_string(),
            "phone".to_string(),
            crate::hash_token("token-two"),
            "token-two".to_string(),
        );
        storage.create_device(&device_1).unwrap();
        storage.create_device(&device_2).unwrap();

        let mut sub_1 = PushSubscription::new(
            "https://web.push.apple.com/one".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        sub_1.device_id = Some(device_1.id.clone());
        storage.upsert_push_subscription(&sub_1).unwrap();

        let mut sub_2 = PushSubscription::new(
            "https://web.push.apple.com/two".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        sub_2.device_id = Some(device_2.id.clone());
        storage.upsert_push_subscription(&sub_2).unwrap();

        storage.revoke_device(&device_1.id).unwrap();
        let active = storage.list_active_push_subscriptions().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].device_id.as_deref(), Some(device_2.id.as_str()));
    }

    #[test]
    fn push_subscription_counts_distinguish_active_revoked_and_legacy() {
        let storage = make_storage();
        let active_device = Device::new(
            "active-device".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            crate::hash_token("active-token"),
            "active".to_string(),
        );
        let revoked_device = Device::new(
            "revoked-device".to_string(),
            "old phone".to_string(),
            "phone".to_string(),
            crate::hash_token("revoked-token"),
            "revoked".to_string(),
        );
        storage.create_device(&active_device).unwrap();
        storage.create_device(&revoked_device).unwrap();

        let mut bound_active = PushSubscription::new(
            "https://web.push.apple.com/active".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        bound_active.device_id = Some(active_device.id.clone());
        storage.upsert_push_subscription(&bound_active).unwrap();

        let legacy = PushSubscription::new(
            "https://web.push.apple.com/legacy".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        storage.upsert_push_subscription(&legacy).unwrap();

        let mut bound_revoked = PushSubscription::new(
            "https://web.push.apple.com/revoked".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        bound_revoked.device_id = Some(revoked_device.id.clone());
        storage.upsert_push_subscription(&bound_revoked).unwrap();
        storage.revoke_device(&revoked_device.id).unwrap();

        let counts = storage.push_subscription_counts().unwrap();
        assert_eq!(counts.active_bound, 1);
        assert_eq!(counts.active_legacy, 1);
        assert_eq!(counts.revoked_or_stale, 1);
    }

    #[test]
    fn device_push_subscribe_claims_existing_endpoint() {
        let storage = make_storage();
        let device = Device::new(
            "claim-device".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            crate::hash_token("claim-token"),
            "claim".to_string(),
        );
        storage.create_device(&device).unwrap();

        let legacy = PushSubscription::new(
            "https://web.push.apple.com/claim".to_string(),
            "old-p256dh".to_string(),
            "old-auth".to_string(),
            None,
        );
        storage.upsert_push_subscription(&legacy).unwrap();

        let mut current = PushSubscription::new(
            "https://web.push.apple.com/claim".to_string(),
            "new-p256dh".to_string(),
            "new-auth".to_string(),
            None,
        );
        current.device_id = Some(device.id.clone());
        storage.upsert_push_subscription(&current).unwrap();

        let active = storage.list_active_push_subscriptions().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].device_id.as_deref(), Some(device.id.as_str()));
        assert_eq!(active[0].p256dh, "new-p256dh");
        assert_eq!(active[0].auth, "new-auth");
    }

    #[test]
    fn active_legacy_push_subscriptions_can_be_claimed() {
        let storage = make_storage();
        let device = Device::new(
            "legacy-claim-device".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            crate::hash_token("legacy-claim-token"),
            "legacy".to_string(),
        );
        storage.create_device(&device).unwrap();

        let legacy = PushSubscription::new(
            "https://web.push.apple.com/legacy-claim".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        storage.upsert_push_subscription(&legacy).unwrap();
        assert_eq!(storage.push_subscription_counts().unwrap().active_legacy, 1);

        let claimed = storage
            .claim_active_legacy_push_subscriptions(&device.id)
            .unwrap();
        assert_eq!(claimed, 1);
        let counts = storage.push_subscription_counts().unwrap();
        assert_eq!(counts.active_legacy, 0);
        assert_eq!(counts.active_bound, 1);
    }

    #[test]
    fn clear_inactive_push_subscriptions_removes_only_inactive_and_legacy() {
        let storage = make_storage();
        let active_device = Device::new(
            "cleanup-active".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            crate::hash_token("cleanup-active-token"),
            "cleanup-active".to_string(),
        );
        let revoked_device = Device::new(
            "cleanup-revoked".to_string(),
            "old phone".to_string(),
            "phone".to_string(),
            crate::hash_token("cleanup-revoked-token"),
            "cleanup-revoked".to_string(),
        );
        storage.create_device(&active_device).unwrap();
        storage.create_device(&revoked_device).unwrap();

        let mut active = PushSubscription::new(
            "https://web.push.apple.com/cleanup-active".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        active.device_id = Some(active_device.id.clone());
        storage.upsert_push_subscription(&active).unwrap();

        let mut revoked = PushSubscription::new(
            "https://web.push.apple.com/cleanup-revoked".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        revoked.device_id = Some(revoked_device.id.clone());
        storage.upsert_push_subscription(&revoked).unwrap();
        storage.revoke_device(&revoked_device.id).unwrap();

        let legacy = PushSubscription::new(
            "https://web.push.apple.com/cleanup-legacy".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        storage.upsert_push_subscription(&legacy).unwrap();

        let summary = storage.clear_inactive_push_subscriptions().unwrap();

        assert_eq!(summary.revoked_deleted, 1);
        assert_eq!(summary.stale_deleted, 0);
        assert_eq!(summary.legacy_deleted, 1);
        let counts = storage.push_subscription_counts().unwrap();
        assert_eq!(counts.total, 1);
        assert_eq!(counts.active_bound, 1);
    }
}
