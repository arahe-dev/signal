use crate::web_push_sender::{
    build_generic_payload, send_web_push_to_all_active, vapid_public_key_hash, PushSummary,
    VapidConfig,
};
use axum::{
    extract::State,
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
}

#[derive(Debug, Deserialize)]
pub struct PushSubscriptionRequest {
    endpoint: String,
    keys: PushKeys,
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
    web_push_enabled: bool,
    vapid_configured: bool,
}

#[derive(Serialize)]
pub struct VapidPublicKeyResponse {
    public_key: Option<String>,
}

pub fn create_push_router(
    storage: Arc<signal_core::Storage>,
    enable_web_push: bool,
    vapid_config: Option<VapidConfig>,
) -> Router {
    let state = PushState {
        storage,
        enabled: enable_web_push,
        vapid_config,
    };

    Router::new()
        .route("/api/push/vapid-public-key", get(vapid_public_key))
        .route("/api/push/subscribe", post(subscribe))
        .route("/api/push/test", post(test_push))
        .route("/api/push/status", get(push_status))
        .with_state(state)
}

async fn subscribe(
    State(state): State<PushState>,
    axum::extract::Json(payload): axum::extract::Json<PushSubscriptionRequest>,
) -> Result<Json<PushSubscriptionResponse>, axum::response::Response> {
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
) -> Result<Json<PushSubscriptionResponse>, axum::response::Response> {
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

    let subscriptions = state
        .storage
        .list_active_push_subscriptions()
        .map_err(|e| {
            axum::response::Response::builder()
                .status(500)
                .body(format!("Failed to list subscriptions: {}", e).into())
                .unwrap()
        })?;

    if subscriptions.is_empty() {
        return Ok(Json(PushSubscriptionResponse {
            success: false,
            message: Some("No active subscriptions found".to_string()),
            summary: None,
        }));
    }

    let payload = build_generic_payload(vapid_config.public_base_url.as_deref());
    let summary = send_web_push_to_all_active(&subscriptions, &vapid_config, &payload).await;

    for result in &summary.results {
        let Some(subscription) = subscriptions
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
            let _ = state
                .storage
                .update_push_subscription_error(&subscription.id, error);
        }
    }

    let success = summary.sent > 0 && summary.failed == 0;
    Ok(Json(PushSubscriptionResponse {
        success,
        message: Some(format!(
            "Attempted {}, sent {}, failed {}",
            summary.attempted, summary.sent, summary.failed
        )),
        summary: Some(summary),
    }))
}

async fn push_status(
    State(state): State<PushState>,
) -> Result<Json<PushStatusResponse>, axum::response::Response> {
    let count = if state.enabled {
        state
            .storage
            .list_active_push_subscriptions()
            .map(|s| s.len())
            .unwrap_or(0)
    } else {
        0
    };

    Ok(Json(PushStatusResponse {
        subscriptions_count: count,
        web_push_enabled: state.enabled,
        vapid_configured: state.vapid_config.is_some(),
    }))
}

async fn vapid_public_key(State(state): State<PushState>) -> Json<VapidPublicKeyResponse> {
    Json(VapidPublicKeyResponse {
        public_key: state.vapid_config.map(|config| config.public_key),
    })
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
