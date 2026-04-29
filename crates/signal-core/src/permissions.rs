use crate::models::{Message, PermissionLevel};

pub fn can_read_message(message: &Message, requester_agent_id: Option<&str>) -> bool {
    match message.permission_level {
        PermissionLevel::Private => {
            if let Some(req_id) = requester_agent_id {
                if req_id == message.agent_id.as_deref().unwrap_or("") {
                    return true;
                }
            }
            false
        }
        PermissionLevel::AiReadable => true,
        PermissionLevel::Actionable => true,
    }
}

pub fn can_consume_reply(message: &Message, requester_agent_id: Option<&str>) -> bool {
    match message.permission_level {
        PermissionLevel::Private => {
            if let Some(req_id) = requester_agent_id {
                if req_id == message.agent_id.as_deref().unwrap_or("") {
                    return true;
                }
            }
            false
        }
        PermissionLevel::AiReadable => true,
        PermissionLevel::Actionable => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Message;
    use chrono::Utc;

    fn test_message(permission: PermissionLevel, agent_id: Option<String>) -> Message {
        Message {
            id: "test-id".to_string(),
            thread_id: "thread-id".to_string(),
            title: "Test".to_string(),
            body: "Body".to_string(),
            source: "test".to_string(),
            source_device: None,
            agent_id,
            project: None,
            status: crate::models::MessageStatus::New,
            permission_level: permission,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_private_message_same_agent() {
        let msg = test_message(PermissionLevel::Private, Some("agent1".to_string()));
        assert!(can_read_message(&msg, Some("agent1")));
        assert!(can_consume_reply(&msg, Some("agent1")));
    }

    #[test]
    fn test_private_message_different_agent() {
        let msg = test_message(PermissionLevel::Private, Some("agent1".to_string()));
        assert!(!can_read_message(&msg, Some("agent2")));
        assert!(!can_consume_reply(&msg, Some("agent2")));
    }

    #[test]
    fn test_ai_readable_message() {
        let msg = test_message(PermissionLevel::AiReadable, Some("agent1".to_string()));
        assert!(can_read_message(&msg, Some("agent2")));
        assert!(can_consume_reply(&msg, Some("agent2")));
    }

    #[test]
    fn test_actionable_message() {
        let msg = test_message(PermissionLevel::Actionable, Some("agent1".to_string()));
        assert!(can_read_message(&msg, Some("any-agent")));
        assert!(can_consume_reply(&msg, Some("any-agent")));
    }
}
