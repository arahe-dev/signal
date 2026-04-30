use crate::web_push_sender::VapidConfig;
use signal_core::Storage;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum AuthIdentity {
    Admin,
    Device { device_id: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthFailure {
    Invalid,
    Revoked,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub token: Option<String>,
    pub require_token_for_read: bool,
    pub enable_web_push: bool,
    pub vapid_config: Option<VapidConfig>,
    pub db_path: String,
}

impl AppState {
    #[cfg(test)]
    pub fn new(storage: Arc<Storage>, token: Option<String>, require_token_for_read: bool) -> Self {
        Self::with_push(
            storage,
            token,
            require_token_for_read,
            false,
            None,
            String::new(),
        )
    }

    pub fn with_push(
        storage: Arc<Storage>,
        token: Option<String>,
        require_token_for_read: bool,
        enable_web_push: bool,
        vapid_config: Option<VapidConfig>,
        db_path: String,
    ) -> Self {
        Self {
            storage,
            token,
            require_token_for_read,
            enable_web_push,
            vapid_config,
            db_path,
        }
    }

    pub fn authenticate_token(&self, token: &str) -> Result<AuthIdentity, AuthFailure> {
        if let Some(expected) = &self.token {
            if token == expected {
                return Ok(AuthIdentity::Admin);
            }
        }

        let device = self
            .storage
            .get_device_by_token_hash(&signal_core::hash_token(token))
            .map_err(|_| AuthFailure::Invalid)?;

        if !device.is_active() {
            return Err(AuthFailure::Revoked);
        }

        let _ = self.storage.update_device_last_seen(&device.id);
        Ok(AuthIdentity::Device {
            device_id: device.id,
        })
    }

    pub fn is_auth_required(&self) -> bool {
        self.token.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, AuthFailure, AuthIdentity};
    use signal_core::{generate_device_token, hash_token, models::Device, Storage};
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    fn state_with_device() -> (AppState, String, String) {
        let file = NamedTempFile::new().unwrap();
        let storage = Arc::new(Storage::new(file.path()).unwrap());
        let token = generate_device_token();
        let device = Device::new(
            "device-1".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            hash_token(&token),
            "sig_dev_test".to_string(),
        );
        storage.create_device(&device).unwrap();
        (
            AppState::new(storage, Some("dev-token".to_string()), true),
            token,
            device.id,
        )
    }

    #[test]
    fn active_device_token_authenticates() {
        let (state, token, device_id) = state_with_device();
        assert_eq!(
            state.authenticate_token(&token).unwrap(),
            AuthIdentity::Device { device_id }
        );
    }

    #[test]
    fn revoked_device_token_fails_auth() {
        let (state, token, device_id) = state_with_device();
        state.storage.revoke_device(&device_id).unwrap();
        assert_eq!(
            state.authenticate_token(&token).unwrap_err(),
            AuthFailure::Revoked
        );
    }
}
