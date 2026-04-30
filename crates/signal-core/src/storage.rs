use crate::models::{
    Event, Message, MessageStatus, OutboxEntry, PushSubscription, Reply, ReplyStatus,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
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

impl Storage {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
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
    use crate::models::{Device, Message, PairingCode, PermissionLevel, PushSubscription};
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
