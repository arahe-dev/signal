use crate::web_push_sender::{
    build_generic_payload, send_web_push_to_all_active, vapid_public_key_hash, PushSummary,
    VapidConfig,
};
use axum::{
    extract::State,
    http::HeaderMap,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use signal_core::models::PushSubscription;
use std::sync::Arc;

#[derive(Clone)]
struct PushState {
    storage: Arc<signal_core::Storage>,
    enabled: bool,
    vapid_config: Option<VapidConfig>,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PushSubscriptionRequest {
    endpoint: String,
    #[serde(default, rename = "expirationTime")]
    _expiration_time: Option<serde_json::Value>,
    keys: PushKeys,
}

#[derive(Debug, Default, Deserialize)]
pub struct TestPushRequest {
    title: Option<String>,
    body: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PushKeys {
    p256dh: String,
    auth: String,
}

#[derive(Serialize)]
pub struct PushSubscriptionResponse {
    success: bool,
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<PushSummary>,
}

#[derive(Serialize)]
pub struct PushStatusResponse {
    subscriptions_count: usize,
    active_subscriptions: usize,
    revoked_or_stale_subscriptions: usize,
    legacy_unbound_subscriptions: usize,
    total_subscriptions: usize,
    web_push_enabled: bool,
    vapid_configured: bool,
}

#[derive(Serialize)]
pub struct VapidPublicKeyResponse {
    #[serde(rename = "publicKey")]
    public_key_camel: Option<String>,
    public_key: Option<String>,
    length: Option<usize>,
    #[serde(rename = "firstByte")]
    first_byte: Option<u8>,
}

#[derive(Debug, Default)]
struct PushSelection {
    subscriptions: Vec<PushSubscription>,
    skipped_revoked: usize,
    skipped_stale: usize,
    skipped_legacy: usize,
}

impl PushSelection {
    fn skipped(&self) -> usize {
        self.skipped_revoked + self.skipped_stale + self.skipped_legacy
    }
}

pub fn create_push_router(
    storage: Arc<signal_core::Storage>,
    enable_web_push: bool,
    vapid_config: Option<VapidConfig>,
    token: Option<String>,
) -> Router {
    let state = PushState {
        storage,
        enabled: enable_web_push,
        vapid_config,
        token,
    };

    Router::new()
        .route("/api/push/vapid-public-key", get(vapid_public_key))
        .route("/api/push/subscribe", post(subscribe))
        .route("/api/push/test", post(test_push))
        .route("/api/push/status", get(push_status))
        .with_state(state)
}

fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Signal-Token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .map(|s| s.to_string())
        })
}

fn make_error_response(
    status: axum::http::StatusCode,
    error: &str,
    message: &str,
) -> axum::response::Response {
    let body = serde_json::json!({ "error": error, "message": message }).to_string();
    axum::response::Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(body.into())
        .unwrap()
}

fn authenticate_push(
    state: &PushState,
    headers: &HeaderMap,
) -> Result<Option<String>, axum::response::Response> {
    let Some(token) = token_from_headers(headers) else {
        if state.token.is_none() {
            return Ok(None);
        }
        return Err(make_error_response(
            axum::http::StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid token",
        ));
    };

    if state.token.as_deref() == Some(token.as_str()) {
        return Ok(None);
    }

    let device = state
        .storage
        .get_device_by_token_hash(&signal_core::hash_token(&token))
        .map_err(|_| {
            make_error_response(
                axum::http::StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing or invalid token",
            )
        })?;

    if !device.is_active() {
        return Err(make_error_response(
            axum::http::StatusCode::FORBIDDEN,
            "device_revoked",
            "This device has been revoked. Pair again.",
        ));
    }

    let _ = state.storage.update_device_last_seen(&device.id);
    Ok(Some(device.id))
}

fn select_push_subscriptions(
    storage: &signal_core::Storage,
    include_legacy: bool,
) -> Result<PushSelection, signal_core::StorageError> {
    if !include_legacy {
        let active_devices = storage
            .list_devices()?
            .into_iter()
            .filter(|device| device.is_active())
            .collect::<Vec<_>>();
        if active_devices.len() == 1 {
            let _ = storage.claim_active_legacy_push_subscriptions(&active_devices[0].id)?;
        }
    }

    let mut selection = PushSelection::default();
    for subscription in storage.list_push_subscriptions()? {
        if subscription.status != "active" {
            if subscription.status == "revoked" {
                selection.skipped_revoked += 1;
            } else {
                selection.skipped_stale += 1;
            }
            continue;
        }

        let Some(device_id) = subscription.device_id.as_deref() else {
            if include_legacy {
                selection.subscriptions.push(subscription);
            } else {
                selection.skipped_legacy += 1;
            }
            continue;
        };

        match storage.get_device(device_id) {
            Ok(device) if device.is_active() => selection.subscriptions.push(subscription),
            Ok(_) => selection.skipped_revoked += 1,
            Err(_) => selection.skipped_stale += 1,
        }
    }
    Ok(selection)
}

fn clamp_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn safe_debug_url(input: Option<&str>, public_base_url: Option<&str>) -> String {
    let Some(raw) = input.map(str::trim).filter(|value| !value.is_empty()) else {
        return "/app".to_string();
    };

    if raw.starts_with('/') && !raw.starts_with("//") {
        return raw.to_string();
    }

    let Some(base) = public_base_url.and_then(|base| reqwest::Url::parse(base).ok()) else {
        return "/app".to_string();
    };
    let Ok(candidate) = reqwest::Url::parse(raw) else {
        return "/app".to_string();
    };

    if candidate.scheme() == base.scheme()
        && candidate.host_str() == base.host_str()
        && candidate.port_or_known_default() == base.port_or_known_default()
    {
        candidate.to_string()
    } else {
        "/app".to_string()
    }
}

fn build_test_payload(request: Option<TestPushRequest>, public_base_url: Option<&str>) -> String {
    let Some(request) = request else {
        return build_generic_payload(public_base_url);
    };
    let title = request
        .title
        .as_deref()
        .map(|title| clamp_chars(title, 80))
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Signal test".to_string());
    let body = request
        .body
        .as_deref()
        .map(|body| clamp_chars(body, 240))
        .filter(|body| !body.trim().is_empty())
        .unwrap_or_else(|| "Debug push from Signal dashboard.".to_string());
    let url = safe_debug_url(request.url.as_deref(), public_base_url);

    serde_json::json!({
        "title": title,
        "body": body,
        "url": url
    })
    .to_string()
}

fn summarize_no_active(selection: &PushSelection) -> PushSummary {
    PushSummary {
        attempted: 0,
        sent: 0,
        failed: 0,
        skipped: selection.skipped(),
        skipped_revoked: selection.skipped_revoked,
        skipped_stale: selection.skipped_stale,
        skipped_legacy: selection.skipped_legacy,
        results: Vec::new(),
        errors: Vec::new(),
    }
}

async fn subscribe(
    State(state): State<PushState>,
    headers: HeaderMap,
    axum::extract::Json(payload): axum::extract::Json<PushSubscriptionRequest>,
) -> Result<Json<PushSubscriptionResponse>, axum::response::Response> {
    let device_id = authenticate_push(&state, &headers)?;

    if !state.enabled {
        return Ok(Json(PushSubscriptionResponse {
            success: false,
            message: Some("Web push is not enabled".to_string()),
            summary: None,
        }));
    }

    let mut subscription = PushSubscription::new(
        payload.endpoint,
        payload.keys.p256dh,
        payload.keys.auth,
        None,
    );
    subscription.device_id = device_id;
    if let Some(config) = &state.vapid_config {
        subscription.vapid_public_key_hash = Some(vapid_public_key_hash(&config.public_key));
    }

    state
        .storage
        .upsert_push_subscription(&subscription)
        .map_err(|e| {
            axum::response::Response::builder()
                .status(500)
                .body(format!("Failed to save subscription: {}", e).into())
                .unwrap()
        })?;

    Ok(Json(PushSubscriptionResponse {
        success: true,
        message: None,
        summary: None,
    }))
}

async fn test_push(
    State(state): State<PushState>,
    headers: HeaderMap,
    payload: Option<Json<TestPushRequest>>,
) -> Result<Json<PushSubscriptionResponse>, axum::response::Response> {
    authenticate_push(&state, &headers)?;

    if !state.enabled {
        return Ok(Json(PushSubscriptionResponse {
            success: false,
            message: Some("Web push is not enabled".to_string()),
            summary: None,
        }));
    }

    let Some(vapid_config) = state.vapid_config.clone() else {
        return Ok(Json(PushSubscriptionResponse {
            success: false,
            message: Some("VAPID not configured".to_string()),
            summary: None,
        }));
    };

    let selection = select_push_subscriptions(&state.storage, false).map_err(|e| {
        axum::response::Response::builder()
            .status(500)
            .body(format!("Failed to list subscriptions: {}", e).into())
            .unwrap()
    })?;

    if selection.subscriptions.is_empty() {
        let summary = summarize_no_active(&selection);
        return Ok(Json(PushSubscriptionResponse {
            success: true,
            message: Some("No active push subscriptions".to_string()),
            summary: Some(summary),
        }));
    }

    let payload = build_test_payload(
        payload.map(|json| json.0),
        vapid_config.public_base_url.as_deref(),
    );
    let mut summary =
        send_web_push_to_all_active(&selection.subscriptions, &vapid_config, &payload).await;
    summary.skipped = selection.skipped();
    summary.skipped_revoked = selection.skipped_revoked;
    summary.skipped_stale = selection.skipped_stale;
    summary.skipped_legacy = selection.skipped_legacy;

    for result in &summary.results {
        let Some(subscription) = selection
            .subscriptions
            .iter()
            .find(|subscription| subscription.endpoint == result.endpoint)
        else {
            continue;
        };

        if result.success {
            let _ = state
                .storage
                .update_push_subscription_success(&subscription.id);
        } else if let Some(error) = &result.error {
            if result.debug.http_status == Some(404) || result.debug.http_status == Some(410) {
                let _ = state
                    .storage
                    .mark_push_subscription_stale(&subscription.id, error);
            } else {
                let _ = state
                    .storage
                    .update_push_subscription_error(&subscription.id, error);
            }
        }
    }

    let success = summary.failed == 0;
    Ok(Json(PushSubscriptionResponse {
        success,
        message: Some(format!(
            "Attempted {}, sent {}, failed {}, skipped {}",
            summary.attempted, summary.sent, summary.failed, summary.skipped
        )),
        summary: Some(summary),
    }))
}

async fn push_status(
    State(state): State<PushState>,
    headers: HeaderMap,
) -> Result<Json<PushStatusResponse>, axum::response::Response> {
    authenticate_push(&state, &headers)?;

    let count = if state.enabled {
        state
            .storage
            .list_active_push_subscriptions()
            .map(|s| s.len())
            .unwrap_or(0)
    } else {
        0
    };
    let counts = state.storage.push_subscription_counts().unwrap_or_default();

    Ok(Json(PushStatusResponse {
        subscriptions_count: count,
        active_subscriptions: counts.active_bound,
        revoked_or_stale_subscriptions: counts.revoked_or_stale,
        legacy_unbound_subscriptions: counts.active_legacy,
        total_subscriptions: counts.total,
        web_push_enabled: state.enabled,
        vapid_configured: state.vapid_config.is_some(),
    }))
}

async fn vapid_public_key(
    State(state): State<PushState>,
    headers: HeaderMap,
) -> Result<Json<VapidPublicKeyResponse>, axum::response::Response> {
    authenticate_push(&state, &headers)?;

    let Some(config) = state.vapid_config else {
        return Err(make_error_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "vapid_not_configured",
            "Web Push/VAPID is not configured",
        ));
    };

    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        &config.public_key,
    )
    .map_err(|_| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "vapid_invalid_key",
            "VAPID public key is not valid base64url",
        )
    })?;

    Ok(Json(VapidPublicKeyResponse {
        public_key_camel: Some(config.public_key.clone()),
        public_key: Some(config.public_key),
        length: Some(decoded.len()),
        first_byte: decoded.first().copied(),
    }))
}

pub fn send_push_notifications(storage: &signal_core::Storage) {
    let subscriptions = match storage.list_active_push_subscriptions() {
        Ok(subs) => subs,
        Err(e) => {
            tracing::warn!("Failed to list push subscriptions: {}", e);
            return;
        }
    };

    if subscriptions.is_empty() {
        return;
    }

    // For now, just log that we'd send - full VAPID push requires more setup
    tracing::info!(
        "Would send push notification to {} subscriptions",
        subscriptions.len()
    );

    for sub in subscriptions {
        // In production, this would use web-push crate with VAPID
        // For demo, we just log the attempt
        tracing::debug!("Push to endpoint: {}", sub.endpoint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_core::models::{Device, PushSubscription};
    use tempfile::NamedTempFile;

    fn make_storage() -> signal_core::Storage {
        let file = NamedTempFile::new().unwrap();
        signal_core::Storage::new(file.path()).unwrap()
    }

    fn add_device(storage: &signal_core::Storage, id: &str) -> Device {
        let token = signal_core::generate_device_token();
        let device = Device::new(
            id.to_string(),
            id.to_string(),
            "phone".to_string(),
            signal_core::hash_token(&token),
            signal_core::get_token_prefix(&token),
        );
        storage.create_device(&device).unwrap();
        device
    }

    fn add_subscription(
        storage: &signal_core::Storage,
        endpoint: &str,
        device_id: Option<String>,
    ) -> PushSubscription {
        let mut subscription = PushSubscription::new(
            endpoint.to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        subscription.device_id = device_id;
        storage.upsert_push_subscription(&subscription).unwrap();
        subscription
    }

    #[test]
    fn push_selection_skips_revoked_and_legacy_subscriptions() {
        let storage = make_storage();
        let active = add_device(&storage, "active");
        let _active_two = add_device(&storage, "active-two");
        let revoked = add_device(&storage, "revoked");
        add_subscription(
            &storage,
            "https://web.push.apple.com/active",
            Some(active.id.clone()),
        );
        add_subscription(
            &storage,
            "https://web.push.apple.com/revoked",
            Some(revoked.id.clone()),
        );
        add_subscription(&storage, "https://web.push.apple.com/legacy", None);
        storage.revoke_device(&revoked.id).unwrap();

        let selection = select_push_subscriptions(&storage, false).unwrap();
        assert_eq!(selection.subscriptions.len(), 1);
        assert_eq!(selection.skipped_revoked, 1);
        assert_eq!(selection.skipped_legacy, 1);
    }

    #[test]
    fn push_selection_claims_legacy_when_only_one_active_device_exists() {
        let storage = make_storage();
        let active = add_device(&storage, "active");
        add_subscription(&storage, "https://web.push.apple.com/legacy", None);

        let selection = select_push_subscriptions(&storage, false).unwrap();

        assert_eq!(selection.subscriptions.len(), 1);
        assert_eq!(
            selection.subscriptions[0].device_id.as_deref(),
            Some(active.id.as_str())
        );
        assert_eq!(selection.skipped_legacy, 0);
    }

    #[test]
    fn no_active_push_summary_is_not_a_hard_failure_shape() {
        let mut selection = PushSelection::default();
        selection.skipped_revoked = 1;
        selection.skipped_stale = 1;
        let summary = summarize_no_active(&selection);

        assert_eq!(summary.attempted, 0);
        assert_eq!(summary.sent, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 2);
    }

    #[test]
    fn test_push_payload_accepts_custom_title_and_body() {
        let payload: serde_json::Value = serde_json::from_str(&build_test_payload(
            Some(TestPushRequest {
                title: Some("Signal custom test".to_string()),
                body: Some("Custom debug notification from laptop".to_string()),
                url: Some("/app".to_string()),
            }),
            Some("https://ari-legion.taild0cc8e.ts.net"),
        ))
        .unwrap();

        assert_eq!(payload["title"], "Signal custom test");
        assert_eq!(payload["body"], "Custom debug notification from laptop");
        assert_eq!(payload["url"], "/app");
    }

    #[test]
    fn test_push_payload_clamps_long_title_and_body() {
        let payload: serde_json::Value = serde_json::from_str(&build_test_payload(
            Some(TestPushRequest {
                title: Some("t".repeat(100)),
                body: Some("b".repeat(300)),
                url: Some("/app".to_string()),
            }),
            None,
        ))
        .unwrap();

        assert_eq!(payload["title"].as_str().unwrap().chars().count(), 80);
        assert_eq!(payload["body"].as_str().unwrap().chars().count(), 240);
    }

    #[test]
    fn test_push_payload_rejects_external_url() {
        let payload: serde_json::Value = serde_json::from_str(&build_test_payload(
            Some(TestPushRequest {
                title: None,
                body: None,
                url: Some("https://evil.example/app".to_string()),
            }),
            Some("https://ari-legion.taild0cc8e.ts.net"),
        ))
        .unwrap();

        assert_eq!(payload["url"], "/app");
    }

    #[test]
    fn test_push_payload_allows_same_origin_absolute_url() {
        let payload: serde_json::Value = serde_json::from_str(&build_test_payload(
            Some(TestPushRequest {
                title: None,
                body: None,
                url: Some("https://ari-legion.taild0cc8e.ts.net/app".to_string()),
            }),
            Some("https://ari-legion.taild0cc8e.ts.net"),
        ))
        .unwrap();

        assert_eq!(payload["url"], "https://ari-legion.taild0cc8e.ts.net/app");
    }
}
