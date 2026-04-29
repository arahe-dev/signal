use signal_core::Storage;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub token: Option<String>,
    pub require_token_for_read: bool,
}

impl AppState {
    pub fn new(storage: Arc<Storage>, token: Option<String>, require_token_for_read: bool) -> Self {
        Self {
            storage,
            token,
            require_token_for_read,
        }
    }

    pub fn check_token(&self, token: &str) -> bool {
        match &self.token {
            Some(expected) => token == expected,
            None => true,
        }
    }

    pub fn is_auth_required(&self) -> bool {
        self.token.is_some()
    }
}
