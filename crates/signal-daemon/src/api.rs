use crate::app_state::AppState;
use crate::html;
use crate::web_push_sender::{
    build_ask_payload, build_message_payload, build_message_url, send_web_push_to_all_active,
    VapidConfig,
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, Response},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use signal_core::{
    events::{create_message_event, create_reply_consumed_event, create_reply_event},
    models::*,
    Storage,
};
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    limit: Option<i64>,
    status: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListRepliesQuery {
    agent_id: Option<String>,
    project: Option<String>,
    status: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct WaitQuery {
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct AskResponse {
    ask_id: String,
    message_id: String,
    status: String,
    expires_at: Option<String>,
    message_url: String,
}

#[derive(Debug, Serialize)]
pub struct AskWaitResponse {
    status: String,
    ask_id: String,
    message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_id: Option<String>,
    reply: Option<String>,
    elapsed_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

fn make_error_response(
    status: axum::http::StatusCode,
    error: &str,
    message: &str,
) -> axum::response::Response {
    let body = serde_json::to_string(&ErrorResponse {
        error: error.to_string(),
        message: message.to_string(),
    })
    .unwrap_or_default();
    axum::response::Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(body.into())
        .unwrap()
}

fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), axum::response::Response> {
    if state.token.is_none() {
        return Ok(());
    }

    let token = headers
        .get("X-Signal-Token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match token {
        Some(t) if state.check_token(&t) => Ok(()),
        _ => Err(make_error_response(
            axum::http::StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid token",
        )),
    }
}

fn check_read_auth(state: &AppState, headers: &HeaderMap) -> Result<(), axum::response::Response> {
    if !state.is_auth_required() || !state.require_token_for_read {
        return Ok(());
    }

    let token = headers
        .get("X-Signal-Token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match token {
        Some(t) if state.check_token(&t) => Ok(()),
        _ => Err(make_error_response(
            axum::http::StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid token",
        )),
    }
}

// Pairing request/response types
#[derive(Debug, Deserialize)]
pub struct PairStartRequest {
    pub device_name: String,
}

#[derive(Debug, Serialize)]
pub struct PairStartResponse {
    pub pairing_code: String,
    pub qr_data: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Deserialize)]
pub struct PairCompleteRequest {
    pub pairing_code: String,
    pub device_name: String,
    pub device_kind: String,
}

#[derive(Debug, Serialize)]
pub struct PairCompleteResponse {
    pub device_id: String,
    pub device_token: String,
    pub device_name: String,
}

// Pairing handlers
async fn pair_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Json(payload): axum::extract::Json<PairStartRequest>,
) -> Result<Json<PairStartResponse>, axum::response::Response> {
    check_auth(&state, &headers)?;

    // Generate a random pairing code using token generation
    let pairing_token = signal_core::generate_device_token();
    let code_hash = signal_core::hash_token(&pairing_token);
    let code_prefix = signal_core::get_token_prefix(&pairing_token);

    let pairing_code = signal_core::models::PairingCode::new(
        code_hash.clone(),
        code_prefix.clone(),
        300, // 5 minutes
    );

    state
        .storage
        .create_pairing_code(&pairing_code)
        .map_err(|e| {
            make_error_response(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "pairing_failed",
                &format!("Failed to create pairing code: {}", e),
            )
        })?;

    // Generate QR data - for now just the pairing code
    // In production this could be a full URL or QR format
    let qr_data = format!("signal://pair/{}", code_prefix);

    info!("Pairing code generated for device: {}", payload.device_name);

    Ok(Json(PairStartResponse {
        pairing_code: code_prefix,
        qr_data,
        expires_in_seconds: 300,
    }))
}

async fn pair_complete(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Json(payload): axum::extract::Json<PairCompleteRequest>,
) -> Result<Json<PairCompleteResponse>, axum::response::Response> {
    // Get pairing code
    let pairing_code = state
        .storage
        .get_pairing_code(&signal_core::auth::hash_token(&payload.pairing_code))
        .map_err(|_| {
            make_error_response(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid_pairing_code",
                "Pairing code not found or expired",
            )
        })?;

    // Check if pairing code is valid (not expired, not used)
    if !pairing_code.is_valid() {
        return Err(make_error_response(
            axum::http::StatusCode::BAD_REQUEST,
            "pairing_code_expired",
            "Pairing code has expired or already been used",
        ));
    }

    // Generate device token
    let device_token = signal_core::generate_device_token();
    let token_hash = signal_core::hash_token(&device_token);
    let token_prefix = signal_core::get_token_prefix(&device_token);

    // Create device
    let device_name = payload.device_name.clone();
    let device = signal_core::models::Device {
        id: Uuid::new_v4().to_string(),
        name: payload.device_name,
        kind: payload.device_kind,
        token_hash,
        token_prefix: token_prefix.clone(),
        paired_at: Utc::now(),
        last_seen_at: None,
        revoked_at: None,
        user_agent: headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        metadata_json: None,
    };

    let device_id = device.id.clone();
    state.storage.create_device(&device).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "device_creation_failed",
            &format!("Failed to create device: {}", e),
        )
    })?;

    // Mark pairing code as used
    state
        .storage
        .mark_pairing_code_used(&pairing_code.code_hash)
        .map_err(|e| {
            make_error_response(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "pairing_failed",
                &format!("Failed to mark pairing code as used: {}", e),
            )
        })?;

    info!("Device paired: {} ({})", device_id, device_name);

    Ok(Json(PairCompleteResponse {
        device_id,
        device_token,
        device_name: device.name,
    }))
}

// Device list/revoke response types
#[derive(Debug, Serialize)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceInfo>,
}

#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub token_prefix: String,
    pub paired_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
pub struct DeviceRevokeResponse {
    pub success: bool,
    pub message: String,
}

// Device handlers
async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DeviceListResponse>, axum::response::Response> {
    check_auth(&state, &headers)?;

    let devices = state.storage.list_devices().map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "list_failed",
            &format!("Failed to list devices: {}", e),
        )
    })?;

    let device_infos = devices
        .into_iter()
        .map(|d| {
            let is_active = d.is_active();
            DeviceInfo {
                id: d.id,
                name: d.name,
                kind: d.kind,
                token_prefix: d.token_prefix,
                paired_at: d.paired_at.to_rfc3339(),
                last_seen_at: d.last_seen_at.map(|dt| dt.to_rfc3339()),
                revoked_at: d.revoked_at.map(|dt| dt.to_rfc3339()),
                is_active,
            }
        })
        .collect();

    Ok(Json(DeviceListResponse {
        devices: device_infos,
    }))
}

async fn revoke_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceRevokeResponse>, axum::response::Response> {
    check_auth(&state, &headers)?;

    state.storage.revoke_device(&device_id).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "revoke_failed",
            &format!("Failed to revoke device: {}", e),
        )
    })?;

    info!("Device revoked: {}", device_id);

    Ok(Json(DeviceRevokeResponse {
        success: true,
        message: format!("Device {} revoked", device_id),
    }))
}

pub fn create_api_router(
    storage: Arc<Storage>,
    token: Option<String>,
    require_token_for_read: bool,
    enable_web_push: bool,
    vapid_config: Option<VapidConfig>,
) -> Router {
    let state = AppState::with_push(
        storage,
        token,
        require_token_for_read,
        enable_web_push,
        vapid_config,
    );

    Router::new()
        .route("/health", get(health))
        .route("/api/ask", post(create_ask))
        .route("/api/ask/{id}/wait", get(wait_for_ask))
        .route("/api/messages", get(list_messages).post(create_message))
        .route("/api/messages/{id}", get(get_message))
        .route(
            "/api/messages/{id}/replies",
            get(get_replies).post(create_reply),
        )
        .route("/api/replies/latest", get(get_latest_reply))
        .route("/api/replies/{id}/consume", post(consume_reply))
        .route("/api/pair/start", post(pair_start))
        .route("/api/pair/complete", post(pair_complete))
        .route("/api/devices", get(list_devices))
        .route("/api/devices/{id}/revoke", post(revoke_device))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn create_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Json(payload): axum::extract::Json<CreateMessageRequest>,
) -> Result<Json<Message>, axum::response::Response> {
    check_auth(&state, &headers)?;

    let permission_level = payload.permission_level.unwrap_or_default();
    let _status = payload.status.unwrap_or_default();

    let message = Message::new(
        payload.title,
        payload.body,
        payload.source,
        payload.source_device,
        payload.agent_id,
        payload.project,
        permission_level,
    );

    let event = create_message_event(
        &message.id,
        &message.title,
        &message.source,
        message.agent_id.as_deref(),
        message.project.as_deref(),
    );

    let storage = state.storage.as_ref();
    storage.create_message(&message).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to create message: {}", e),
        )
    })?;

    storage.create_event(&event).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to create event: {}", e),
        )
    })?;

    info!("Created message: {} from {}", message.id, message.source);
    send_message_push_notification(&state, &message).await;
    Ok(Json(message))
}

async fn create_ask(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Json(payload): axum::extract::Json<AskRequest>,
) -> Result<Json<AskResponse>, axum::response::Response> {
    check_auth(&state, &headers)?;

    let timeout_seconds = payload.timeout_seconds.unwrap_or(600).min(30 * 60);
    let mut message = Message::new(
        payload.title,
        payload.body,
        payload.source,
        None,
        payload.agent_id,
        payload.project,
        PermissionLevel::Actionable,
    );
    message.status = MessageStatus::PendingReply;
    message.expires_at = Some(Utc::now() + chrono::Duration::seconds(timeout_seconds as i64));
    message.priority = Some(payload.priority.unwrap_or_else(|| "normal".to_string()));
    message.reply_mode = Some(payload.reply_mode.unwrap_or_else(|| "text".to_string()));
    message.reply_options_json = payload
        .reply_options
        .map(|options| serde_json::to_string(&options).unwrap_or_default());

    let event = create_message_event(
        &message.id,
        &message.title,
        &message.source,
        message.agent_id.as_deref(),
        message.project.as_deref(),
    );
    let ask_event = signal_core::models::Event::new(
        "ask_created".to_string(),
        Some(message.source.clone()),
        None,
        serde_json::json!({
            "ask_id": message.id,
            "message_id": message.id,
            "expires_at": message.expires_at.map(|dt| dt.to_rfc3339()),
            "priority": message.priority
        })
        .to_string(),
    );

    let storage = state.storage.as_ref();
    storage.create_message(&message).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to create ask: {}", e),
        )
    })?;
    storage.create_event(&event).ok();
    storage.create_event(&ask_event).ok();

    send_ask_push_notification(&state, &message).await;

    let message_url = build_message_url(
        &message.id,
        state
            .vapid_config
            .as_ref()
            .and_then(|config| config.public_base_url.as_deref()),
        state.token.as_deref(),
    );

    Ok(Json(AskResponse {
        ask_id: message.id.clone(),
        message_id: message.id.clone(),
        status: message.status.to_string(),
        expires_at: message.expires_at.map(|dt| dt.to_rfc3339()),
        message_url,
    }))
}

async fn wait_for_ask(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<WaitQuery>,
) -> Result<Json<AskWaitResponse>, axum::response::Response> {
    check_auth(&state, &headers)?;
    let started = Instant::now();
    let timeout_seconds = query.timeout_seconds.unwrap_or(600).min(30 * 60);
    let deadline = started + Duration::from_secs(timeout_seconds);

    loop {
        let message = state.storage.get_message(&id).map_err(|e| {
            make_error_response(
                axum::http::StatusCode::NOT_FOUND,
                "not_found",
                &format!("Ask not found: {}", e),
            )
        })?;

        if matches!(
            message.status,
            MessageStatus::Consumed | MessageStatus::Archived
        ) {
            return Ok(Json(AskWaitResponse {
                status: "not_available".to_string(),
                ask_id: id.clone(),
                message_id: id.clone(),
                reply_id: None,
                reply: None,
                elapsed_seconds: started.elapsed().as_secs(),
                reason: Some("archived_or_consumed".to_string()),
            }));
        }

        let replies = state.storage.get_replies_for_message(&id).map_err(|e| {
            make_error_response(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to get replies: {}", e),
            )
        })?;
        if let Some(reply) = replies
            .into_iter()
            .find(|reply| reply.status == ReplyStatus::Pending)
        {
            return Ok(Json(AskWaitResponse {
                status: "replied".to_string(),
                ask_id: id.clone(),
                message_id: id.clone(),
                reply_id: Some(reply.id),
                reply: Some(reply.body),
                elapsed_seconds: started.elapsed().as_secs(),
                reason: None,
            }));
        }

        if Instant::now() >= deadline
            || message
                .expires_at
                .map(|expires_at| expires_at <= Utc::now())
                .unwrap_or(false)
        {
            let _ = state
                .storage
                .update_message_status(&id, MessageStatus::Timeout);
            let _ = state.storage.create_event(&signal_core::models::Event::new(
                "ask_timeout".to_string(),
                Some("signal-daemon".to_string()),
                None,
                serde_json::json!({"ask_id": id, "message_id": id}).to_string(),
            ));
            return Ok(Json(AskWaitResponse {
                status: "timeout".to_string(),
                ask_id: id.clone(),
                message_id: id.clone(),
                reply_id: None,
                reply: None,
                elapsed_seconds: started.elapsed().as_secs(),
                reason: None,
            }));
        }

        sleep(Duration::from_secs(1)).await;
    }
}

async fn send_message_push_notification(state: &AppState, message: &Message) {
    if !state.enable_web_push {
        return;
    }

    let Some(vapid_config) = state.vapid_config.clone() else {
        tracing::warn!("Skipping message push because VAPID is not configured");
        return;
    };

    let subscriptions = match state.storage.list_active_push_subscriptions() {
        Ok(subscriptions) => subscriptions,
        Err(e) => {
            tracing::warn!("Failed to list push subscriptions for message push: {}", e);
            return;
        }
    };

    if subscriptions.is_empty() {
        return;
    }

    let payload = build_message_payload(
        message,
        vapid_config.public_base_url.as_deref(),
        state.token.as_deref(),
    );
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

    info!(
        "Message push attempted {}, sent {}, failed {} for message {}",
        summary.attempted, summary.sent, summary.failed, message.id
    );
}

async fn send_ask_push_notification(state: &AppState, message: &Message) {
    if !state.enable_web_push {
        return;
    }

    let Some(vapid_config) = state.vapid_config.clone() else {
        return;
    };

    let subscriptions = match state.storage.list_active_push_subscriptions() {
        Ok(subscriptions) => subscriptions,
        Err(e) => {
            tracing::warn!("Failed to list push subscriptions for ask push: {}", e);
            return;
        }
    };

    if subscriptions.is_empty() {
        return;
    }

    let payload = build_ask_payload(
        message,
        vapid_config.public_base_url.as_deref(),
        state.token.as_deref(),
    );
    let summary = send_web_push_to_all_active(&subscriptions, &vapid_config, &payload).await;
    let event_type = if summary.failed == 0 {
        "push_sent"
    } else {
        "push_failed"
    };
    let _ = state.storage.create_event(&signal_core::models::Event::new(
        event_type.to_string(),
        Some("signal-daemon".to_string()),
        None,
        serde_json::json!({
            "message_id": message.id,
            "attempted": summary.attempted,
            "sent": summary.sent,
            "failed": summary.failed
        })
        .to_string(),
    ));
}

async fn list_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListMessagesQuery>,
) -> Result<Json<Vec<Message>>, axum::response::Response> {
    check_read_auth(&state, &headers)?;

    let status = query.status.and_then(|s| s.parse().ok());
    let storage = state.storage.as_ref();

    let messages = storage
        .list_messages(
            query.limit,
            status,
            query.project.as_deref(),
            query.agent_id.as_deref(),
        )
        .map_err(|e| {
            axum::response::Response::builder()
                .status(500)
                .body(format!("Failed to list messages: {}", e).into())
                .unwrap()
        })?;

    Ok(Json(messages))
}

async fn get_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<MessageWithReplies>, axum::response::Response> {
    check_read_auth(&state, &headers)?;

    let storage = state.storage.as_ref();

    let message = storage.get_message(&id).map_err(|e| {
        let status = if e.to_string().contains("Not found") {
            404
        } else {
            500
        };
        axum::response::Response::builder()
            .status(status)
            .body(format!("Error: {}", e).into())
            .unwrap()
    })?;

    let replies = storage.get_replies_for_message(&id).map_err(|e| {
        axum::response::Response::builder()
            .status(500)
            .body(format!("Failed to get replies: {}", e).into())
            .unwrap()
    })?;

    Ok(Json(MessageWithReplies { message, replies }))
}

async fn get_replies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> Result<Json<Vec<Reply>>, axum::response::Response> {
    check_read_auth(&state, &headers)?;

    let storage = state.storage.as_ref();

    let replies = storage.get_replies_for_message(&message_id).map_err(|e| {
        axum::response::Response::builder()
            .status(500)
            .body(format!("Failed to get replies: {}", e).into())
            .unwrap()
    })?;

    Ok(Json(replies))
}

async fn create_reply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    axum::extract::Json(payload): axum::extract::Json<CreateReplyRequest>,
) -> Result<Json<Reply>, axum::response::Response> {
    check_auth(&state, &headers)?;

    let storage = state.storage.as_ref();

    let _ = storage.get_message(&message_id).map_err(|e| {
        let status = if e.to_string().contains("Not found") {
            axum::http::StatusCode::NOT_FOUND
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        };
        make_error_response(status, "not_found", &format!("Message not found: {}", e))
    })?;

    let reply = Reply::new(
        message_id.clone(),
        payload.body,
        payload.source,
        payload.source_device,
    );

    let event = create_reply_event(&reply.id, &reply.message_id, &reply.body, &reply.source);

    storage.create_reply(&reply).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to create reply: {}", e),
        )
    })?;
    storage
        .update_message_status(&message_id, MessageStatus::Replied)
        .ok();

    storage.create_event(&event).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to create event: {}", e),
        )
    })?;

    info!("Created reply: {} for message: {}", reply.id, message_id);
    Ok(Json(reply))
}

async fn get_latest_reply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListRepliesQuery>,
) -> Result<Json<Option<Reply>>, axum::response::Response> {
    check_read_auth(&state, &headers)?;

    let storage = state.storage.as_ref();

    let reply = storage
        .get_latest_pending_reply(query.agent_id.as_deref(), query.project.as_deref())
        .map_err(|e| {
            axum::response::Response::builder()
                .status(500)
                .body(format!("Failed to get latest reply: {}", e).into())
                .unwrap()
        })?;

    Ok(Json(reply))
}

async fn consume_reply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Reply>, axum::response::Response> {
    check_auth(&state, &headers)?;

    let storage = state.storage.as_ref();

    let reply = storage.get_reply(&id).map_err(|e| {
        let status = if e.to_string().contains("Not found") {
            axum::http::StatusCode::NOT_FOUND
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        };
        make_error_response(status, "not_found", &format!("Reply not found: {}", e))
    })?;

    let _message = storage.get_message(&reply.message_id).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to get message: {}", e),
        )
    })?;

    storage
        .update_reply_status(&id, ReplyStatus::Consumed)
        .map_err(|e| {
            make_error_response(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to update reply status: {}", e),
            )
        })?;
    storage
        .update_message_status(&reply.message_id, MessageStatus::Consumed)
        .ok();

    let event = create_reply_consumed_event(&reply.id, &reply.message_id, "cli");
    storage.create_event(&event).ok();

    info!("Consumed reply: {} for message: {}", id, reply.message_id);

    let updated_reply = storage.get_reply(&id).map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to get updated reply: {}", e),
        )
    })?;

    Ok(Json(updated_reply))
}

fn escape_html(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}

// Dashboard handler
async fn dashboard(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
) -> Result<Html<String>, axum::response::Response> {
    check_html_read_auth(&state, query.token.as_deref())?;

    let device_list = state.storage.list_devices().unwrap_or_default();
    let active_devices = device_list.iter().filter(|d| d.is_active()).count();
    let revoked_devices = device_list.iter().filter(|d| !d.is_active()).count();

    let devices_html = if device_list.is_empty() {
        "<p class=\"note\">No devices paired yet.</p>".to_string()
    } else {
        device_list
            .iter()
            .map(|d| {
                format!(
                    r#"<tr style="border-bottom:1px solid #e5e5ea;">
                    <td style="padding:12px 0;"><strong>{}</strong></td>
                    <td style="color:#6e6e73;font-size:13px;">{}</td>
                    <td style="color:#6e6e73;font-size:13px;">{}</td>
                    <td style="text-align:right;">{}</td>
                    <td style="text-align:right;">{}</td>
                    </tr>"#,
                    escape_html(&d.name),
                    d.token_prefix,
                    escape_html(&d.kind),
                    if d.is_active() {
                        "<span style=\"color:green;\">active</span>"
                    } else {
                        "<span style=\"color:red;\">revoked</span>"
                    },
                    if d.is_active() {
                        format!(
                            r#"<button onclick="revokeDevice('{}')">Revoke</button>"#,
                            d.id
                        )
                    } else {
                        String::new()
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Signal - Dashboard</title>
    <style>
        * {{
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #f5f5f7;
            color: #1d1d1f;
            padding: 20px;
        }}
        .container {{
            max-width: 900px;
            margin: 0 auto;
        }}
        h1 {{
            font-size: 28px;
            margin-bottom: 24px;
        }}
        .card {{
            background: white;
            border-radius: 12px;
            padding: 20px;
            margin-bottom: 16px;
            box-shadow: 0 1px 3px rgba(0,0,0,0.1);
        }}
        .stats {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 16px;
            margin-bottom: 16px;
        }}
        .stat {{
            background: #f5f5f7;
            padding: 16px;
            border-radius: 10px;
            text-align: center;
        }}
        .stat-value {{
            font-size: 32px;
            font-weight: 600;
            color: #007aff;
        }}
        .stat-label {{
            font-size: 13px;
            color: #6e6e73;
            margin-top: 8px;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
        }}
        th {{
            text-align: left;
            padding: 12px 0;
            font-weight: 600;
            border-bottom: 2px solid #e5e5ea;
        }}
        .btn {{
            padding: 8px 16px;
            background: #007aff;
            color: white;
            border: none;
            border-radius: 8px;
            font-size: 13px;
            cursor: pointer;
        }}
        .btn:hover {{
            background: #0051d5;
        }}
        .note {{
            font-size: 13px;
            color: #86868b;
            margin-top: 16px;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Signal Dashboard</h1>
        
        <div class="card">
            <h2>Status</h2>
            <div class="stats">
                <div class="stat">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Active Devices</div>
                </div>
                <div class="stat">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Revoked Devices</div>
                </div>
            </div>
        </div>

        <div class="card">
            <h2>Paired Devices</h2>
            <table>
                <thead>
                    <tr>
                        <th>Name</th>
                        <th>Token</th>
                        <th>Type</th>
                        <th>Status</th>
                        <th>Action</th>
                    </tr>
                </thead>
                <tbody>
                    {}
                </tbody>
            </table>
            <p class="note">Revoke a device to prevent it from accessing messages.</p>
        </div>

        <div class="card">
            <h2>Pairing</h2>
            <p>Add a new device to Signal. Each device gets a unique token.</p>
            <div style="margin-top: 16px;">
                <input type="text" id="device-name-input" placeholder="Device name (e.g., iPhone, iPad)" style="padding: 8px; border: 1px solid #e5e5ea; border-radius: 8px; width: 100%; margin-bottom: 12px; font-size: 14px;">
                <button id="start-pairing-btn" class="btn" style="margin-top: 0;">Start Pairing</button>
            </div>
            <div id="pairing-status" style="margin-top: 16px;"></div>
        </div>
    </div>

    <script>
        // Initialize token from URL and localStorage
        function initializeToken() {{
            const params = new URLSearchParams(window.location.search);
            const urlToken = params.get('token');
            if (urlToken) {{
                localStorage.setItem('signal_admin_token', urlToken);
            }}
        }}

        function getToken() {{
            const params = new URLSearchParams(window.location.search);
            return params.get('token') || localStorage.getItem('signal_admin_token') || '';
        }}

        // Device revocation
        async function revokeDevice(deviceId) {{
            if (!confirm('Are you sure you want to revoke this device?')) {{
                return;
            }}
            
            try {{
                const response = await fetch(`/api/devices/${{deviceId}}/revoke`, {{
                    method: 'POST',
                    headers: {{
                        'X-Signal-Token': getToken()
                    }}
                }});
                
                if (response.ok) {{
                    alert('Device revoked successfully');
                    location.reload();
                }} else {{
                    alert('Failed to revoke device');
                }}
            }} catch (error) {{
                alert('Error: ' + error.message);
            }}
        }}

        // Pairing functionality
        document.getElementById('start-pairing-btn')?.addEventListener('click', startPairing);

        async function startPairing() {{
            const deviceNameInput = document.getElementById('device-name-input');
            const deviceName = deviceNameInput.value.trim() || 'Device';
            const statusDiv = document.getElementById('pairing-status');
            const token = getToken();

            if (!token) {{
                showPairingError(statusDiv, 'No authentication token found. Please reload with ?token=dev-token');
                return;
            }}

            statusDiv.innerHTML = '<p style="color: #007aff;">Starting pairing...</p>';

            try {{
                const response = await fetch('/api/pair/start', {{
                    method: 'POST',
                    headers: {{
                        'Content-Type': 'application/json',
                        'X-Signal-Token': token
                    }},
                    body: JSON.stringify({{ device_name: deviceName }})
                }});

                if (!response.ok) {{
                    const errorData = await response.json().catch(() => ({{}}));
                    throw new Error(errorData.message || `HTTP ${{response.status}}`);
                }}

                const data = await response.json();
                displayPairingCode(statusDiv, data);
            }} catch (error) {{
                showPairingError(statusDiv, 'Failed to start pairing: ' + error.message);
            }}
        }}

        function displayPairingCode(statusDiv, data) {{
            const publicBaseUrl = 'https://your-device.your-tailnet.ts.net';
            const pairingUrl = `${{publicBaseUrl}}/pair?code=${{data.pairing_code}}`;
            
            const expiresIn = data.expires_in_seconds || 300;
            const minutesLeft = Math.floor(expiresIn / 60);

            const html = `
                <div style="background: #f5f5f7; border-radius: 8px; padding: 16px; margin-top: 12px;">
                    <p style="font-size: 13px; color: #6e6e73; margin-bottom: 12px;">
                        Pairing code expires in ~${{minutesLeft}} minute(s)
                    </p>
                    
                    <div style="background: white; padding: 12px; border-radius: 6px; margin-bottom: 12px; border: 1px solid #e5e5ea;">
                        <p style="font-size: 12px; color: #6e6e73; margin-bottom: 6px;">Pairing Code:</p>
                        <div style="display: flex; gap: 8px; align-items: center;">
                            <code style="flex: 1; font-family: monospace; font-size: 14px; padding: 8px; background: #f9f9f9; border-radius: 4px; word-break: break-all;">${{data.pairing_code}}</code>
                            <button onclick="copyToClipboard('${{data.pairing_code}}')" class="btn" style="margin: 0; white-space: nowrap;">Copy</button>
                        </div>
                    </div>

                    <p style="font-size: 13px; margin-bottom: 8px; color: #1d1d1f;"><strong>Mobile Device:</strong></p>
                    <a href="${{pairingUrl}}" target="_blank" style="display: inline-block; padding: 12px 16px; background: #007aff; color: white; text-decoration: none; border-radius: 8px; margin-bottom: 12px; font-size: 13px;">
                        Open Pairing Link →
                    </a>

                    <p style="font-size: 12px; color: #6e6e73; margin-top: 12px;">
                        <strong>Or enter code on mobile:</strong><br>
                        ${{pairingUrl}}
                    </p>

                    <button onclick="resetPairing()" class="btn" style="background: #f5f5f7; color: #007aff; border: 1px solid #e5e5ea; margin-top: 12px;">
                        Generate New Code
                    </button>
                </div>
            `;
            statusDiv.innerHTML = html;
        }}

        function showPairingError(statusDiv, message) {{
            statusDiv.innerHTML = `
                <div style="background: #ffebee; border: 1px solid #ef5350; border-radius: 8px; padding: 12px; color: #c62828; font-size: 13px;">
                    <strong>Error:</strong> ${{message}}
                </div>
            `;
        }}

        function copyToClipboard(text) {{
            navigator.clipboard.writeText(text).then(() => {{
                alert('Pairing code copied to clipboard');
            }}).catch(() => {{
                alert('Failed to copy');
            }});
        }}

        function resetPairing() {{
            document.getElementById('pairing-status').innerHTML = '';
            document.getElementById('device-name-input').value = '';
            document.getElementById('device-name-input').focus();
        }}

        // Initialize on page load
        initializeToken();
    </script>
</body>
</html>"#,
        active_devices, revoked_devices, devices_html
    );

    Ok(Html(html))
}

pub fn create_html_router(
    storage: Arc<Storage>,
    token: Option<String>,
    require_token_for_read: bool,
) -> Router {
    Router::new()
        .route("/", get(inbox_page))
        .route("/dashboard", get(dashboard))
        .route("/message/{id}", get(message_detail_page))
        .route("/api/messages/{id}/replies/form", post(reply_form_handler))
        .with_state(AppState::new(storage, token, require_token_for_read))
}

#[derive(Debug, Deserialize)]
pub struct TokenQuery {
    token: Option<String>,
}

fn check_html_read_auth(
    state: &AppState,
    token_from_query: Option<&str>,
) -> Result<(), axum::response::Response> {
    if !state.is_auth_required() || !state.require_token_for_read {
        return Ok(());
    }

    match token_from_query {
        Some(t) if state.check_token(t) => Ok(()),
        _ => {
            let body = r#"<!DOCTYPE html><html><head><title>401 Unauthorized</title></head><body style="font-family:system-ui;padding:40px;text-align:center;"><h1>401 Unauthorized</h1><p>Token required. Add ?token=dev-token to URL.</p><p><a href="/app?token=dev-token">Open Signal app with dev token</a></p></body></html>"#;
            Err(axum::response::Response::builder()
                .status(401)
                .header("Content-Type", "text/html")
                .body(body.into())
                .unwrap())
        }
    }
}

async fn inbox_page(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
) -> Result<Html<String>, axum::response::Response> {
    check_html_read_auth(&state, query.token.as_deref())?;

    let storage = state.storage.as_ref();
    let messages = storage
        .list_messages(Some(50), None, None, None)
        .unwrap_or_default();
    let token = query.token.or(state.token.clone());
    Ok(Html(html::render_inbox(&messages, token.as_deref())))
}

async fn message_detail_page(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TokenQuery>,
) -> Result<Html<String>, axum::response::Response> {
    check_html_read_auth(&state, query.token.as_deref())?;

    let storage = state.storage.as_ref();

    let message = storage.get_message(&id).map_err(|_| {
        axum::response::Response::builder()
            .status(404)
            .body("Not found".into())
            .unwrap()
    })?;

    let replies = storage.get_replies_for_message(&id).unwrap_or_default();
    let token = query.token.or(state.token.clone());

    Ok(Html(html::render_message_detail(
        &message,
        &replies,
        token.as_deref(),
    )))
}

async fn reply_form_handler(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
    axum::extract::Form(form): axum::extract::Form<ReplyFormDataWithToken>,
) -> Result<impl IntoResponse, axum::response::Response> {
    if let Some(ref expected_token) = state.token {
        match &form.token {
            Some(t) if t == expected_token => {}
            _ => {
                return Err(make_error_response(
                    axum::http::StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "missing or invalid token",
                ));
            }
        }
    }

    let storage = state.storage.as_ref();

    let _ = storage.get_message(&message_id).map_err(|_| {
        axum::response::Response::builder()
            .status(404)
            .body("Message not found".into())
            .unwrap()
    })?;

    let reply = Reply::new(
        message_id.clone(),
        form.body.clone(),
        "phone".to_string(),
        Some("iphone".to_string()),
    );

    let event = create_reply_event(&reply.id, &reply.message_id, &reply.body, &reply.source);

    storage.create_reply(&reply).map_err(|e| {
        axum::response::Response::builder()
            .status(500)
            .body(format!("Failed to create reply: {}", e).into())
            .unwrap()
    })?;
    storage
        .update_message_status(&message_id, MessageStatus::Replied)
        .ok();

    storage.create_event(&event).ok();

    info!(
        "Created reply via form: {} for message: {}",
        reply.id, message_id
    );

    Ok(axum::response::Redirect::to(&format!(
        "/message/{}?token={}",
        message_id,
        form.token.as_deref().unwrap_or("")
    )))
}

#[derive(Deserialize)]
pub struct ReplyFormDataWithToken {
    body: String,
    token: Option<String>,
}

pub fn create_pwa_router() -> Router {
    Router::new()
        .route("/app", get(pwa_app))
        .route("/manifest.webmanifest", get(pwa_manifest))
        .route("/service-worker.js", get(pwa_service_worker))
        .route("/apple-touch-icon.png", get(pwa_apple_touch_icon))
        .route(
            "/apple-touch-icon-precomposed.png",
            get(pwa_apple_touch_icon),
        )
        .route("/apple-touch-icon-180x180.png", get(pwa_apple_touch_icon))
        .route("/icon-192.png", get(pwa_icon_192))
        .route("/icon-512.png", get(pwa_icon_512))
}

async fn pwa_app() -> Html<String> {
    Html(include_str!("app.html").to_string())
}

async fn pwa_manifest() -> impl IntoResponse {
    Response::builder()
        .header("Content-Type", "application/manifest+json")
        .body(
            serde_json::json!({
                "name": "Signal",
                "short_name": "Signal",
                "description": "Local-first human-agent handoff inbox",
                "start_url": "/app?token=dev-token",
                "scope": "/",
                "display": "standalone",
                "background_color": "#f5f5f7",
                "theme_color": "#f5f5f7",
                "icons": [
                    {"src": "/icon-192.png", "sizes": "192x192", "type": "image/png"},
                    {"src": "/icon-512.png", "sizes": "512x512", "type": "image/png"}
                ]
            })
            .to_string(),
        )
        .unwrap()
}

async fn pwa_service_worker() -> impl IntoResponse {
    axum::response::Response::builder()
        .header("Content-Type", "application/javascript")
        .body(include_str!("service-worker.js").to_string())
        .unwrap()
}

async fn pwa_apple_touch_icon() -> impl IntoResponse {
    let bytes = Bytes::from_static(include_bytes!("apple-touch-icon.png"));
    (
        [
            ("Content-Type", "image/png"),
            ("Cache-Control", "no-cache, no-store, must-revalidate"),
        ],
        bytes,
    )
}

async fn pwa_icon_192() -> impl IntoResponse {
    let bytes = Bytes::from_static(include_bytes!("icon-192.png"));
    (
        [
            ("Content-Type", "image/png"),
            ("Cache-Control", "public, max-age=3600"),
        ],
        bytes,
    )
}

async fn pwa_icon_512() -> impl IntoResponse {
    let bytes = Bytes::from_static(include_bytes!("icon-512.png"));
    (
        [
            ("Content-Type", "image/png"),
            ("Cache-Control", "public, max-age=3600"),
        ],
        bytes,
    )
}
