use crate::models::Event;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCreatedPayload {
    pub message_id: String,
    pub title: String,
    pub source: String,
    pub agent_id: Option<String>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyCreatedPayload {
    pub reply_id: String,
    pub message_id: String,
    pub body: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyConsumedPayload {
    pub reply_id: String,
    pub message_id: String,
    pub consumed_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStatusChangedPayload {
    pub message_id: String,
    pub old_status: String,
    pub new_status: String,
}

pub fn create_message_event(
    message_id: &str,
    title: &str,
    source: &str,
    agent_id: Option<&str>,
    project: Option<&str>,
) -> Event {
    let payload = MessageCreatedPayload {
        message_id: message_id.to_string(),
        title: title.to_string(),
        source: source.to_string(),
        agent_id: agent_id.map(String::from),
        project: project.map(String::from),
    };
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    Event::new(
        "message.created".to_string(),
        Some(source.to_string()),
        None,
        payload_json,
    )
}

pub fn create_reply_event(reply_id: &str, message_id: &str, body: &str, source: &str) -> Event {
    let payload = ReplyCreatedPayload {
        reply_id: reply_id.to_string(),
        message_id: message_id.to_string(),
        body: body.to_string(),
        source: source.to_string(),
    };
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    Event::new(
        "reply.created".to_string(),
        Some(source.to_string()),
        None,
        payload_json,
    )
}

pub fn create_reply_consumed_event(reply_id: &str, message_id: &str, consumed_by: &str) -> Event {
    let payload = ReplyConsumedPayload {
        reply_id: reply_id.to_string(),
        message_id: message_id.to_string(),
        consumed_by: consumed_by.to_string(),
    };
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    Event::new(
        "reply.consumed".to_string(),
        Some(consumed_by.to_string()),
        None,
        payload_json,
    )
}
