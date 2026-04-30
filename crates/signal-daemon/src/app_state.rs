use crate::web_push_sender::VapidConfig;
use signal_core::Storage;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub token: Option<String>,
    pub require_token_for_read: bool,
    pub enable_web_push: bool,
    pub vapid_config: Option<VapidConfig>,
}

impl AppState {
    pub fn new(storage: Arc<Storage>, token: Option<String>, require_token_for_read: bool) -> Self {
        Self::with_push(storage, token, require_token_for_read, false, None)
    }

    pub fn with_push(
        storage: Arc<Storage>,
        token: Option<String>,
        require_token_for_read: bool,
        enable_web_push: bool,
        vapid_config: Option<VapidConfig>,
    ) -> Self {
        Self {
            storage,
            token,
            require_token_for_read,
            enable_web_push,
            vapid_config,
        }
    }

    pub fn check_token(&self, token: &str) -> bool {
        // If no admin token is required, accept any valid device token
        if self.token.is_none() {
            return self.check_device_token(token);
        }

        // Check against hardcoded admin token first
        if let Some(expected) = &self.token {
            if token == expected {
                return true;
            }
        }

        // Check against device tokens as fallback
        self.check_device_token(token)
    }

    pub fn check_device_token(&self, token: &str) -> bool {
        // Check against device tokens
        if let Ok(device) = self
            .storage
            .get_device_by_token_hash(&signal_core::hash_token(token))
        {
            // Device must be active (not revoked)
            return device.is_active();
        }
        false
    }

    pub fn is_auth_required(&self) -> bool {
        self.token.is_some()
    }
}
