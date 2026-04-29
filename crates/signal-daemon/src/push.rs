use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use signal_core::models::PushSubscription;
use std::sync::Arc;

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
}

#[derive(Serialize)]
pub struct PushStatusResponse {
    subscriptions_count: usize,
    web_push_enabled: bool,
}

pub fn create_push_router(storage: Arc<signal_core::Storage>, enable_web_push: bool) -> Router {
    Router::new()
        .route("/api/push/subscribe", post(subscribe))
        .route("/api/push/test", post(test_push))
        .route("/api/push/status", get(push_status))
        .with_state((storage, enable_web_push))
}

async fn subscribe(
    State((storage, enabled)): State<(Arc<signal_core::Storage>, bool)>,
    axum::extract::Json(payload): axum::extract::Json<PushSubscriptionRequest>,
) -> Result<Json<PushSubscriptionResponse>, axum::response::Response> {
    if !enabled {
        return Ok(Json(PushSubscriptionResponse {
            success: false,
            message: Some("Web push is not enabled".to_string()),
        }));
    }

    let subscription = PushSubscription::new(
        payload.endpoint,
        payload.keys.p256dh,
        payload.keys.auth,
        None,
    );

    storage.upsert_push_subscription(&subscription).map_err(|e| {
        axum::response::Response::builder()
            .status(500)
            .body(format!("Failed to save subscription: {}", e).into())
            .unwrap()
    })?;

    Ok(Json(PushSubscriptionResponse {
        success: true,
        message: None,
    }))
}

async fn test_push(
    State((storage, enabled)): State<(Arc<signal_core::Storage>, bool)>,
) -> Result<Json<PushSubscriptionResponse>, axum::response::Response> {
    if !enabled {
        return Ok(Json(PushSubscriptionResponse {
            success: false,
            message: Some("Web push is not enabled".to_string()),
        }));
    }

    let subscriptions = storage.list_active_push_subscriptions().map_err(|e| {
        axum::response::Response::builder()
            .status(500)
            .body(format!("Failed to list subscriptions: {}", e).into())
            .unwrap()
    })?;

    if subscriptions.is_empty() {
        return Ok(Json(PushSubscriptionResponse {
            success: false,
            message: Some("No active subscriptions found".to_string()),
        }));
    }

    // For demo, we'll just return success - actual push requires VAPID keys
    Ok(Json(PushSubscriptionResponse {
        success: true,
        message: Some(format!("Would send to {} subscriptions (VAPID not configured)", subscriptions.len())),
    }))
}

async fn push_status(
    State((storage, enabled)): State<(Arc<signal_core::Storage>, bool)>,
) -> Result<Json<PushStatusResponse>, axum::response::Response> {
    let count = if enabled {
        storage.list_active_push_subscriptions().map(|s| s.len()).unwrap_or(0)
    } else {
        0
    };

    Ok(Json(PushStatusResponse {
        subscriptions_count: count,
        web_push_enabled: enabled,
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
    tracing::info!("Would send push notification to {} subscriptions", subscriptions.len());

    for sub in subscriptions {
        // In production, this would use web-push crate with VAPID
        // For demo, we just log the attempt
        tracing::debug!("Push to endpoint: {}", sub.endpoint);
    }
}