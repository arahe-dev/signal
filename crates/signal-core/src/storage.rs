use crate::models::{Event, Message, MessageStatus, OutboxEntry, PushSubscription, Reply, ReplyStatus};
use rusqlite::{params, Connection};
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

pub struct Storage {
    conn: Mutex<Connection>,
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
                endpoint TEXT NOT NULL,
                p256dh TEXT NOT NULL,
                auth TEXT NOT NULL,
                user_agent TEXT,
                created_at TEXT NOT NULL,
                last_success_at TEXT,
                last_error TEXT,
                status TEXT NOT NULL DEFAULT 'active'
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_push_subscriptions_endpoint ON push_subscriptions(endpoint)",
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
            "INSERT INTO messages (id, thread_id, title, body, source, source_device, agent_id, project, status, permission_level, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
            "SELECT id, thread_id, title, body, source, source_device, agent_id, project, status, permission_level, created_at, updated_at
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
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
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
            "SELECT id, thread_id, title, body, source, source_device, agent_id, project, status, permission_level, created_at, updated_at
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
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
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
            "INSERT INTO replies (id, message_id, body, source, source_device, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                reply.id,
                reply.message_id,
                reply.body,
                reply.source,
                reply.source_device,
                reply.status.to_string(),
                reply.created_at.to_rfc3339(),
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
            "SELECT id, message_id, body, source, source_device, status, created_at
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
            "SELECT id, message_id, body, source, source_device, status, created_at
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
            "SELECT r.id, r.message_id, r.body, r.source, r.source_device, r.status, r.created_at
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
        conn.execute(
            "UPDATE replies SET status = ?1 WHERE id = ?2",
            params![status.to_string(), id],
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

    pub fn upsert_push_subscription(&self, subscription: &PushSubscription) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO push_subscriptions (id, endpoint, p256dh, auth, user_agent, created_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                p256dh = excluded.p256dh,
                auth = excluded.auth,
                user_agent = excluded.user_agent,
                last_error = NULL",
            params![
                subscription.id,
                subscription.endpoint,
                subscription.p256dh,
                subscription.auth,
                subscription.user_agent,
                subscription.created_at.to_rfc3339(),
                subscription.status,
            ],
        )?;
        info!("Upserted push subscription: {}", subscription.id);
        Ok(())
    }

    pub fn list_active_push_subscriptions(&self) -> Result<Vec<PushSubscription>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, endpoint, p256dh, auth, user_agent, created_at, last_success_at, last_error, status
             FROM push_subscriptions WHERE status = 'active'",
        )?;

        let rows = stmt.query_map([], |row| {
            let created_at_str: String = row.get(5)?;
            let last_success_at_str: Option<String> = row.get(6)?;
            Ok(PushSubscription {
                id: row.get(0)?,
                endpoint: row.get(1)?,
                p256dh: row.get(2)?,
                auth: row.get(3)?,
                user_agent: row.get(4)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                last_success_at: last_success_at_str.and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .ok()
                }),
                last_error: row.get(7)?,
                status: row.get(8)?,
            })
        })?;

        let mut subs = Vec::new();
        for row in rows {
            subs.push(row?);
        }
        Ok(subs)
    }

    pub fn update_push_subscription_error(&self, id: &str, error: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE push_subscriptions SET last_error = ?1 WHERE id = ?2",
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Message, PermissionLevel};
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
        let message = make_message("one", MessageStatus::Pending, Some("ivy"), Some("codex"));
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
                MessageStatus::Pending,
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
        let keep = make_message("keep", MessageStatus::Pending, Some("ivy"), Some("codex"));
        let other_status =
            make_message("other", MessageStatus::Consumed, Some("ivy"), Some("codex"));
        let other_project = make_message(
            "other2",
            MessageStatus::Pending,
            Some("moon"),
            Some("codex"),
        );
        let other_agent = make_message(
            "other3",
            MessageStatus::Pending,
            Some("ivy"),
            Some("other-agent"),
        );
        storage.create_message(&keep).unwrap();
        storage.create_message(&other_status).unwrap();
        storage.create_message(&other_project).unwrap();
        storage.create_message(&other_agent).unwrap();

        assert_eq!(
            storage
                .list_messages(None, Some(MessageStatus::Pending), None, None)
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
                    Some(MessageStatus::Pending),
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
            MessageStatus::Pending,
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
}
