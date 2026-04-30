use crate::app_state::{AppState, AuthFailure, AuthIdentity};
use crate::html;
use crate::web_push_sender::{
    build_ask_payload, build_message_payload, build_message_url, private_matches_public,
    send_web_push_to_all_active, VapidConfig,
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

#[derive(Debug, Serialize)]
pub struct DiagnosticsResponse {
    daemon_running: bool,
    version: String,
    db_path: String,
    public_base_url: Option<String>,
    web_push_enabled: bool,
    vapid_public_key_length: Option<usize>,
    vapid_public_key_first_byte: Option<u8>,
    active_devices: usize,
    revoked_devices: usize,
    active_subscriptions: usize,
    revoked_or_stale_subscriptions: usize,
    legacy_unbound_subscriptions: usize,
    last_push_success_at: Option<String>,
    last_push_error: Option<String>,
    last_ask_or_reply_event: Option<String>,
    suggested_fix: Option<String>,
    daemon: DiagnosticsDaemon,
    config: DiagnosticsConfig,
    vapid: DiagnosticsVapid,
    devices: DiagnosticsDevices,
    push_subscriptions: DiagnosticsPushSubscriptions,
    messages: DiagnosticsMessages,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsDaemon {
    ok: bool,
    version: String,
    db_path: String,
    server_time: String,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsConfig {
    public_base_url: Option<String>,
    web_push_enabled: bool,
    require_token_for_read: bool,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsVapid {
    public_key_present: bool,
    public_key_len: Option<usize>,
    public_key_first_byte: Option<u8>,
    private_matches_public: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsDevices {
    active: usize,
    revoked: usize,
    total: usize,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsPushSubscriptions {
    active: usize,
    revoked: usize,
    stale: usize,
    legacy: usize,
    total: usize,
    last_success_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsMessages {
    active_pending: usize,
    last_ask_at: Option<String>,
    last_reply_at: Option<String>,
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

fn auth_error_response(error: AuthFailure) -> axum::response::Response {
    match error {
        AuthFailure::Revoked => make_error_response(
            axum::http::StatusCode::FORBIDDEN,
            "device_revoked",
            "This device has been revoked. Pair again.",
        ),
        AuthFailure::Invalid => make_error_response(
            axum::http::StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid token",
        ),
    }
}

fn check_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthIdentity, axum::response::Response> {
    if state.token.is_none() {
        if let Some(token) = token_from_headers(headers) {
            return state
                .authenticate_token(&token)
                .map_err(auth_error_response);
        }
        return Ok(AuthIdentity::Admin);
    }

    let token = token_from_headers(headers);

    match token {
        Some(t) => state.authenticate_token(&t).map_err(auth_error_response),
        None => Err(make_error_response(
            axum::http::StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid token",
        )),
    }
}

fn check_admin_auth(state: &AppState, headers: &HeaderMap) -> Result<(), axum::response::Response> {
    match check_auth(state, headers)? {
        AuthIdentity::Admin => Ok(()),
        AuthIdentity::Device { .. } => Err(make_error_response(
            axum::http::StatusCode::FORBIDDEN,
            "admin_required",
            "admin token required",
        )),
    }
}

fn check_read_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthIdentity, axum::response::Response> {
    if !state.is_auth_required() || !state.require_token_for_read {
        if let Some(token) = token_from_headers(headers) {
            return state
                .authenticate_token(&token)
                .map_err(auth_error_response);
        }
        return Ok(AuthIdentity::Admin);
    }

    let token = token_from_headers(headers);

    match token {
        Some(t) => state.authenticate_token(&t).map_err(auth_error_response),
        None => Err(make_error_response(
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
    pub code_prefix: String,
    pub pair_url: String,
    pub qr_data: String,
    pub qr_svg: Option<String>,
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
    check_admin_auth(&state, &headers)?;

    // Generate a pairing code (format: pair_<base64>)
    let full_pairing_code = signal_core::generate_pairing_code();
    let code_hash = signal_core::hash_token(&full_pairing_code);
    let code_prefix = signal_core::get_token_prefix(&full_pairing_code);

    let pairing_code_model = signal_core::models::PairingCode::new(
        code_hash.clone(),
        code_prefix.clone(),
        300, // 5 minutes
    );

    state
        .storage
        .create_pairing_code(&pairing_code_model)
        .map_err(|e| {
            make_error_response(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "pairing_failed",
                &format!("Failed to create pairing code: {}", e),
            )
        })?;

    let request_base_url = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_end_matches('/').to_string())
        .or_else(|| {
            headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .map(|host| {
                    let proto = headers
                        .get("x-forwarded-proto")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("http");
                    format!("{}://{}", proto, host)
                })
        });

    let public_base_url = state
        .vapid_config
        .as_ref()
        .and_then(|vc| vc.public_base_url.clone())
        .or(request_base_url)
        .unwrap_or_else(|| "http://127.0.0.1:8791".to_string());

    // Build pair URL with full pairing code
    let pair_url = format!(
        "{}/pair?code={}",
        public_base_url.trim_end_matches('/'),
        full_pairing_code
    );
    let qr_data = pair_url.clone();
    let qr_svg = qrcode::QrCode::new(qr_data.as_bytes()).ok().map(|code| {
        code.render::<qrcode::render::svg::Color>()
            .min_dimensions(220, 220)
            .build()
    });

    info!("Pairing code generated for device: {}", payload.device_name);

    Ok(Json(PairStartResponse {
        pairing_code: full_pairing_code,
        code_prefix,
        pair_url,
        qr_data,
        qr_svg,
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

#[derive(Debug, Serialize)]
pub struct DeviceResetResponse {
    pub success: bool,
    pub devices_revoked: usize,
    pub subscriptions_revoked: usize,
    pub pairing_codes_cleared: usize,
}

// Device handlers
async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DeviceListResponse>, axum::response::Response> {
    check_admin_auth(&state, &headers)?;

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
    check_admin_auth(&state, &headers)?;

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

fn build_diagnostics(state: &AppState) -> DiagnosticsResponse {
    let devices = state.storage.list_devices().unwrap_or_default();
    let active_devices = devices.iter().filter(|device| device.is_active()).count();
    let revoked_devices = devices.len().saturating_sub(active_devices);
    let push_counts = state.storage.push_subscription_counts().unwrap_or_default();
    let subscriptions = state.storage.list_push_subscriptions().unwrap_or_default();
    let last_push_success_at = subscriptions
        .iter()
        .filter_map(|subscription| subscription.last_success_at)
        .max()
        .map(|dt| dt.to_rfc3339());
    let last_push_error = subscriptions
        .iter()
        .filter_map(|subscription| subscription.last_error.clone())
        .next();
    let events = state.storage.list_events(100).unwrap_or_default();
    let last_ask_at = events
        .iter()
        .find(|event| event.event_type.contains("ask") || event.event_type.contains("message"))
        .map(|event| event.created_at.to_rfc3339());
    let last_reply_at = events
        .iter()
        .find(|event| event.event_type.contains("reply"))
        .map(|event| event.created_at.to_rfc3339());
    let last_ask_or_reply_event = events
        .iter()
        .find(|event| {
            event.event_type.contains("ask")
                || event.event_type.contains("reply")
                || event.event_type.contains("message")
        })
        .map(|event| format!("{} at {}", event.event_type, event.created_at.to_rfc3339()));
    let active_pending = state
        .storage
        .list_messages(None, Some(MessageStatus::PendingReply), None, None)
        .map(|messages| messages.len())
        .unwrap_or_default();
    let (vapid_public_key_length, vapid_public_key_first_byte) = state
        .vapid_config
        .as_ref()
        .and_then(|config| {
            base64::Engine::decode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                &config.public_key,
            )
            .ok()
        })
        .map(|bytes| (Some(bytes.len()), bytes.first().copied()))
        .unwrap_or((None, None));
    let public_base_url = state
        .vapid_config
        .as_ref()
        .and_then(|config| config.public_base_url.clone());
    let vapid_private_matches_public = state
        .vapid_config
        .as_ref()
        .map(|config| private_matches_public(&config.private_key, &config.public_key));
    let suggested_fix = if state.enable_web_push && push_counts.active_bound == 0 {
        Some(
            "Pair a phone, open /app on that phone, tap Enable Notifications, then retry Test Push."
                .to_string(),
        )
    } else {
        None
    };

    let daemon = DiagnosticsDaemon {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        db_path: state.db_path.clone(),
        server_time: Utc::now().to_rfc3339(),
    };
    let config = DiagnosticsConfig {
        public_base_url: public_base_url.clone(),
        web_push_enabled: state.enable_web_push,
        require_token_for_read: state.require_token_for_read,
    };
    let vapid = DiagnosticsVapid {
        public_key_present: state.vapid_config.is_some(),
        public_key_len: vapid_public_key_length,
        public_key_first_byte: vapid_public_key_first_byte,
        private_matches_public: vapid_private_matches_public,
    };
    let devices_nested = DiagnosticsDevices {
        active: active_devices,
        revoked: revoked_devices,
        total: devices.len(),
    };
    let push_subscriptions = DiagnosticsPushSubscriptions {
        active: push_counts.active_bound,
        revoked: push_counts.revoked,
        stale: push_counts.stale,
        legacy: push_counts.active_legacy,
        total: push_counts.total,
        last_success_at: last_push_success_at.clone(),
        last_error: last_push_error.clone(),
    };
    let messages = DiagnosticsMessages {
        active_pending,
        last_ask_at,
        last_reply_at,
    };

    DiagnosticsResponse {
        daemon_running: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        db_path: state.db_path.clone(),
        public_base_url,
        web_push_enabled: state.enable_web_push,
        vapid_public_key_length,
        vapid_public_key_first_byte,
        active_devices,
        revoked_devices,
        active_subscriptions: push_counts.active_bound,
        revoked_or_stale_subscriptions: push_counts.revoked_or_stale,
        legacy_unbound_subscriptions: push_counts.active_legacy,
        last_push_success_at,
        last_push_error,
        last_ask_or_reply_event,
        suggested_fix,
        daemon,
        config,
        vapid,
        devices: devices_nested,
        push_subscriptions,
        messages,
    }
}

async fn diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DiagnosticsResponse>, axum::response::Response> {
    check_admin_auth(&state, &headers)?;
    Ok(Json(build_diagnostics(&state)))
}

async fn reset_all_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DeviceResetResponse>, axum::response::Response> {
    check_admin_auth(&state, &headers)?;

    let summary = state.storage.reset_all_devices().map_err(|e| {
        make_error_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "reset_failed",
            &format!("Failed to reset devices: {}", e),
        )
    })?;

    info!(
        "Device reset: revoked {} devices, disabled {} subscriptions, cleared {} pairing codes",
        summary.devices_revoked, summary.subscriptions_revoked, summary.pairing_codes_cleared
    );

    Ok(Json(DeviceResetResponse {
        success: true,
        devices_revoked: summary.devices_revoked,
        subscriptions_revoked: summary.subscriptions_revoked,
        pairing_codes_cleared: summary.pairing_codes_cleared,
    }))
}

pub fn create_api_router(
    storage: Arc<Storage>,
    token: Option<String>,
    require_token_for_read: bool,
    enable_web_push: bool,
    vapid_config: Option<VapidConfig>,
    db_path: String,
) -> Router {
    let state = AppState::with_push(
        storage,
        token,
        require_token_for_read,
        enable_web_push,
        vapid_config,
        db_path,
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
        .route("/api/devices/reset-all", post(reset_all_devices))
        .route("/api/devices/{id}/revoke", post(revoke_device))
        .route("/api/diagnostics", get(diagnostics))
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
    if state.token.is_some() {
        match query
            .token
            .as_deref()
            .and_then(|token| state.authenticate_token(token).ok())
        {
            Some(AuthIdentity::Admin) => {}
            Some(AuthIdentity::Device { .. }) => {
                return Err(make_error_response(
                    axum::http::StatusCode::FORBIDDEN,
                    "admin_required",
                    "Dashboard requires admin token",
                ));
            }
            None => {
                return Err(make_error_response(
                    axum::http::StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "Dashboard requires ?token=<admin-token>",
                ));
            }
        }
    }

    let device_list = state.storage.list_devices().unwrap_or_default();
    let active_devices = device_list.iter().filter(|d| d.is_active()).count();
    let revoked_devices = device_list.iter().filter(|d| !d.is_active()).count();
    let push_counts = state.storage.push_subscription_counts().unwrap_or_default();
    let diagnostics = build_diagnostics(&state);
    let suggested_fix_html = diagnostics
        .suggested_fix
        .as_ref()
        .map(|fix| {
            format!(
                r#"<p class="note"><strong>Suggested fix:</strong> {}</p>"#,
                escape_html(fix)
            )
        })
        .unwrap_or_default();
    let setup_next_action = diagnostics.suggested_fix.as_deref().unwrap_or_else(|| {
        if diagnostics.active_devices == 0 {
            "Pair a phone"
        } else if diagnostics.active_subscriptions == 0 {
            "Open /app on iPhone and tap Enable Notifications"
        } else {
            "Run signal-cli doctor --check-push"
        }
    });

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
                <div class="stat">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Active Push Subscriptions</div>
                </div>
                <div class="stat">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Revoked/Stale Push</div>
                </div>
                <div class="stat">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Legacy/Unbound Push</div>
                </div>
            </div>
        </div>

        <div class="card">
            <h2>Setup Health</h2>
            <p class="note">Local daemon: <strong>pass</strong></p>
            <p class="note">Public URL: <strong>{}</strong></p>
            <p class="note">Web Push: <strong>{}</strong></p>
            <p class="note">Active devices: <strong>{}</strong></p>
            <p class="note">Active subscriptions: <strong>{}</strong></p>
            <p class="note">Last push: <strong>{}</strong></p>
            <p class="note">Suggested next action: <strong>{}</strong></p>
            <p class="note">CLI: <code>signal-cli --server http://127.0.0.1:8791 --token dev-token doctor --check-push</code></p>
        </div>

        <div class="card">
            <h2>Diagnostics</h2>
            <div class="stats">
                <div class="stat">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Web Push</div>
                </div>
                <div class="stat">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">VAPID Key</div>
                </div>
                <div class="stat">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Last Push Success</div>
                </div>
            </div>
            <p class="note">DB: <code>{}</code></p>
            <p class="note">Public Base URL: <code>{}</code></p>
            <p class="note">Last Push Error: <code>{}</code></p>
            <p class="note">Last Ask/Reply Event: <code>{}</code></p>
            {}
            <p class="note">JSON diagnostics: <code>/api/diagnostics</code> with <code>X-Signal-Token</code>.</p>
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
            <button id="reset-all-devices-btn" class="btn" style="margin-top: 12px; background: #c62828;">Reset all devices</button>
            <div id="device-reset-status" style="margin-top: 12px;"></div>
        </div>

        <div class="card">
            <h2>Push</h2>
            <p>Send a diagnostic push to active device-bound subscriptions.</p>
            <label style="display:block;font-size:13px;color:#6e6e73;margin-top:12px;">Debug notification title</label>
            <input type="text" id="push-title-input" value="Signal custom test" maxlength="120" style="padding:8px;border:1px solid #e5e5ea;border-radius:8px;width:100%;margin-top:4px;font-size:14px;">
            <label style="display:block;font-size:13px;color:#6e6e73;margin-top:12px;">Debug notification body</label>
            <textarea id="push-body-input" maxlength="320" style="padding:8px;border:1px solid #e5e5ea;border-radius:8px;width:100%;margin-top:4px;font-size:14px;min-height:80px;">This is a custom debug push from the dashboard.</textarea>
            <label style="display:block;font-size:13px;color:#6e6e73;margin-top:12px;">Optional URL/path</label>
            <input type="text" id="push-url-input" value="/app" style="padding:8px;border:1px solid #e5e5ea;border-radius:8px;width:100%;margin-top:4px;font-size:14px;">
            <button id="test-push-btn" class="btn" style="margin-top: 12px;">Send Test Push</button>
            <div id="push-test-status" style="margin-top: 12px;"></div>
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

        async function parseJsonResponse(response) {{
            const text = await response.text();
            try {{ return text ? JSON.parse(text) : {{}}; }} catch (_) {{ return {{ message: text }}; }}
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
                
                const body = await parseJsonResponse(response);
                if (response.ok && body.success !== false) {{
                    alert('Device revoked successfully');
                    location.reload();
                }} else {{
                    alert('Failed to revoke device: ' + (body.message || response.status));
                }}
            }} catch (error) {{
                alert('Error: ' + error.message);
            }}
        }}

        document.getElementById('reset-all-devices-btn')?.addEventListener('click', resetAllDevices);

        async function resetAllDevices() {{
            const statusDiv = document.getElementById('device-reset-status');
            if (!confirm('This will revoke all paired devices and disable all push subscriptions. Messages and replies are kept. Continue?')) {{
                return;
            }}
            const token = getToken();
            if (!token) {{
                statusDiv.innerHTML = '<div style="color:#c62828;">Missing admin token.</div>';
                return;
            }}
            statusDiv.innerHTML = '<div style="color:#007aff;">Resetting devices and subscriptions...</div>';
            try {{
                const response = await fetch('/api/devices/reset-all', {{
                    method: 'POST',
                    headers: {{ 'X-Signal-Token': token }}
                }});
                const body = await parseJsonResponse(response);
                if (!response.ok || body.success === false) {{
                    throw new Error(body.message || ('HTTP ' + response.status));
                }}
                statusDiv.innerHTML = '<pre style="white-space:pre-wrap;background:#f5f5f7;padding:12px;border-radius:8px;">' + JSON.stringify(body, null, 2) + '</pre>';
                setTimeout(() => location.reload(), 1200);
            }} catch (error) {{
                statusDiv.innerHTML = '<div style="color:#c62828;">Reset failed: ' + error.message + '</div>';
            }}
        }}

        document.getElementById('test-push-btn')?.addEventListener('click', testPush);

        async function testPush() {{
            const statusDiv = document.getElementById('push-test-status');
            const token = getToken();
            if (!token) {{
                statusDiv.innerHTML = '<div style="color:#c62828;">Missing admin token.</div>';
                return;
            }}
            statusDiv.innerHTML = '<div style="color:#007aff;">Sending test push...</div>';
            try {{
                const payload = {{
                    title: document.getElementById('push-title-input')?.value || 'Signal test',
                    body: document.getElementById('push-body-input')?.value || 'Debug push from Signal dashboard.',
                    url: document.getElementById('push-url-input')?.value || '/app'
                }};
                const response = await fetch('/api/push/test', {{
                    method: 'POST',
                    headers: {{
                        'Content-Type': 'application/json',
                        'X-Signal-Token': token
                    }},
                    body: JSON.stringify(payload)
                }});
                const body = await parseJsonResponse(response);
                if (!response.ok) {{
                    throw new Error(body.message || ('HTTP ' + response.status));
                }}
                const color = body.success === false ? '#c77c02' : '#2e7d32';
                let reason = '';
                if (body.summary && body.summary.attempted === 0) {{
                    if (body.summary.skipped_legacy > 0) {{
                        reason = '<div style="color:#c77c02;margin-top:6px;">No active device-bound subscription. Enable notifications from the paired phone, or re-pair and enable notifications.</div>';
                    }} else if (body.summary.skipped_revoked > 0) {{
                        reason = '<div style="color:#c77c02;margin-top:6px;">Only revoked subscriptions were found. Pair a phone and enable notifications again.</div>';
                    }} else {{
                        reason = '<div style="color:#c77c02;margin-top:6px;">No active subscriptions are registered yet.</div>';
                    }}
                }}
                statusDiv.innerHTML = '<div style="color:' + color + ';">' + (body.message || 'Push test completed') + '</div>' + reason +
                    '<pre style="white-space:pre-wrap;background:#f5f5f7;padding:12px;border-radius:8px;margin-top:8px;">' + JSON.stringify(body, null, 2) + '</pre>';
            }} catch (error) {{
                statusDiv.innerHTML = '<div style="color:#c62828;">Push failed: ' + error.message + '</div>';
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
            let pairingUrl = data.pair_url || new URL('/pair?code=' + encodeURIComponent(data.pairing_code), window.location.origin).toString();
            try {{
                const parsed = new URL(pairingUrl);
                if ((parsed.hostname === '127.0.0.1' || parsed.hostname === 'localhost') &&
                    window.location.hostname !== '127.0.0.1' && window.location.hostname !== 'localhost') {{
                    pairingUrl = new URL('/pair?code=' + encodeURIComponent(data.pairing_code), window.location.origin).toString();
                }}
            }} catch (_) {{
                pairingUrl = new URL('/pair?code=' + encodeURIComponent(data.pairing_code), window.location.origin).toString();
            }}
            const qrSvg = data.qr_svg || '';
            
            const expiresIn = data.expires_in_seconds || 300;
            const minutesLeft = Math.floor(expiresIn / 60);

            const html = `
                <div style="background: #f5f5f7; border-radius: 8px; padding: 16px; margin-top: 12px;">
                    <p style="font-size: 13px; color: #6e6e73; margin-bottom: 12px;">
                        Pairing code expires in ~${{minutesLeft}} minute(s)
                    </p>
                    
                    <div style="background: white; padding: 12px; border-radius: 6px; margin-bottom: 12px; border: 1px solid #e5e5ea;">
                        <p style="font-size: 12px; color: #6e6e73; margin-bottom: 6px;">Pairing Code (Full):</p>
                        <div style="display: flex; gap: 8px; align-items: center;">
                            <code style="flex: 1; font-family: monospace; font-size: 12px; padding: 8px; background: #f9f9f9; border-radius: 4px; word-break: break-all;">${{data.pairing_code}}</code>
                            <button onclick="copyToClipboard('${{data.pairing_code}}')" class="btn" style="margin: 0; white-space: nowrap;">Copy</button>
                        </div>
                        <p style="font-size: 11px; color: #9c9c9e; margin-top: 6px;">Prefix: ${{data.code_prefix}}</p>
                    </div>

                    <p style="font-size: 13px; margin-bottom: 8px; color: #1d1d1f;"><strong>Mobile Device:</strong></p>
                    ${{qrSvg ? `<div style="background:white;display:inline-block;padding:12px;border-radius:8px;margin-bottom:12px;">${{qrSvg}}</div>` : ''}}
                    <a href="${{pairingUrl}}" target="_blank" style="display: inline-block; padding: 12px 16px; background: #007aff; color: white; text-decoration: none; border-radius: 8px; margin-bottom: 12px; font-size: 13px;">
                        Open Pairing Link →
                    </a>

                    <p style="font-size: 12px; color: #6e6e73; margin-top: 12px;">
                        <strong>Or share this URL:</strong><br>
                        <code style="font-size: 11px; word-break: break-all; display: block; background: #fff; padding: 8px; border-radius: 4px; margin-top: 4px;">${{pairingUrl}}</code>
                        <button onclick="copyToClipboard('${{pairingUrl}}')" class="btn" style="margin-top: 8px;">Copy Pairing URL</button>
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
        active_devices,
        revoked_devices,
        push_counts.active_bound,
        push_counts.revoked_or_stale,
        push_counts.active_legacy,
        if diagnostics.public_base_url.is_some() {
            "configured"
        } else {
            "missing"
        },
        if diagnostics.web_push_enabled {
            "pass"
        } else {
            "fail"
        },
        diagnostics.active_devices,
        diagnostics.active_subscriptions,
        diagnostics
            .last_push_success_at
            .as_deref()
            .or(diagnostics.last_push_error.as_deref())
            .unwrap_or("none"),
        escape_html(setup_next_action),
        if diagnostics.web_push_enabled {
            "on"
        } else {
            "off"
        },
        diagnostics
            .vapid_public_key_length
            .map(|length| format!(
                "{}/{}",
                length,
                diagnostics.vapid_public_key_first_byte.unwrap_or_default()
            ))
            .unwrap_or_else(|| "n/a".to_string()),
        diagnostics
            .last_push_success_at
            .clone()
            .unwrap_or_else(|| "none".to_string()),
        escape_html(&diagnostics.db_path),
        escape_html(diagnostics.public_base_url.as_deref().unwrap_or("none")),
        escape_html(diagnostics.last_push_error.as_deref().unwrap_or("none")),
        escape_html(
            diagnostics
                .last_ask_or_reply_event
                .as_deref()
                .unwrap_or("none")
        ),
        suggested_fix_html,
        devices_html
    );

    Ok(Html(html))
}

// Pairing page query
#[derive(Debug, Deserialize)]
pub struct PairQuery {
    code: Option<String>,
}

// Mobile-friendly pairing page
async fn pair_page(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<PairQuery>,
) -> Result<Html<&'static str>, axum::response::Response> {
    let code = match &query.code {
        Some(c) => c,
        None => {
            return Ok(Html(
                r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Signal Pairing</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: #fff; color: #1d1d1f; }
        .container { max-width: 600px; margin: 0 auto; padding: 20px; display: flex; flex-direction: column; min-height: 100vh; justify-content: center; }
        .card { background: #f5f5f7; border-radius: 12px; padding: 24px; margin: 12px 0; }
        h1 { font-size: 28px; margin-bottom: 8px; }
        p { font-size: 15px; color: #6e6e73; margin: 12px 0; line-height: 1.5; }
        .error { color: #d70015; background: #ffebee; border: 1px solid #ef5350; border-radius: 8px; padding: 12px; margin: 12px 0; }
    </style>
</head>
<body>
    <div class="container">
        <div class="card">
            <h1>🔗 Pairing Code Missing</h1>
            <p>This page requires a pairing code. Please use the pairing link from your desktop.</p>
            <p style="font-size: 13px; color: #9c9c9e; margin-top: 20px;">If you have a pairing code, visit this page with ?code=your_code</p>
        </div>
    </div>
</body>
</html>"#,
            ));
        }
    };

    // Check if pairing code is valid
    let pairing_code_hash = signal_core::hash_token(code);
    let pairing_code = match state.storage.get_pairing_code(&pairing_code_hash) {
        Ok(pc) => pc,
        Err(_) => {
            return Ok(Html(
                r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Signal Pairing</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: #fff; color: #1d1d1f; }
        .container { max-width: 600px; margin: 0 auto; padding: 20px; display: flex; flex-direction: column; min-height: 100vh; justify-content: center; }
        .card { background: #f5f5f7; border-radius: 12px; padding: 24px; margin: 12px 0; }
        h1 { font-size: 28px; margin-bottom: 8px; }
        p { font-size: 15px; color: #6e6e73; margin: 12px 0; line-height: 1.5; }
        .error { color: #d70015; background: #ffebee; border: 1px solid #ef5350; border-radius: 8px; padding: 12px; margin: 12px 0; }
    </style>
</head>
<body>
    <div class="container">
        <div class="card">
            <h1>❌ Invalid or Expired Code</h1>
            <div class="error">This pairing code is invalid, expired, or has already been used.</div>
            <p>Please request a new pairing code from your desktop and try again.</p>
        </div>
    </div>
</body>
</html>"#,
            ));
        }
    };

    if !pairing_code.is_valid() {
        return Ok(Html(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Signal Pairing</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: #fff; color: #1d1d1f; }
        .container { max-width: 600px; margin: 0 auto; padding: 20px; display: flex; flex-direction: column; min-height: 100vh; justify-content: center; }
        .card { background: #f5f5f7; border-radius: 12px; padding: 24px; margin: 12px 0; }
        h1 { font-size: 28px; margin-bottom: 8px; }
        p { font-size: 15px; color: #6e6e73; margin: 12px 0; line-height: 1.5; }
        .error { color: #d70015; background: #ffebee; border: 1px solid #ef5350; border-radius: 8px; padding: 12px; margin: 12px 0; }
    </style>
</head>
<body>
    <div class="container">
        <div class="card">
            <h1>❌ Code Expired or Used</h1>
            <div class="error">This pairing code has expired or has already been used.</div>
            <p>Please request a new pairing code from your desktop and try again.</p>
        </div>
    </div>
</body>
</html>"#,
        ));
    }

    // Return the pairing page with hidden code
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Pair Device to Signal</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: #fff; color: #1d1d1f; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; display: flex; flex-direction: column; min-height: 100vh; justify-content: center; }}
        .card {{ background: #f5f5f7; border-radius: 12px; padding: 24px; margin: 12px 0; }}
        h1 {{ font-size: 28px; margin-bottom: 8px; }}
        p {{ font-size: 15px; color: #6e6e73; margin: 12px 0; line-height: 1.5; }}
        input {{ width: 100%; padding: 12px; border: 1px solid #e5e5ea; border-radius: 8px; font-size: 15px; margin: 12px 0; font-family: inherit; }}
        button {{ width: 100%; padding: 12px; background: #007aff; color: white; border: none; border-radius: 8px; font-size: 15px; font-weight: 600; cursor: pointer; margin-top: 12px; }}
        button:disabled {{ background: #d1d1d6; cursor: not-allowed; }}
        .status {{ padding: 12px; border-radius: 8px; margin: 12px 0; display: none; }}
        .status.info {{ background: #e3f2fd; color: #1565c0; display: block; }}
        .status.error {{ background: #ffebee; color: #d70015; display: block; }}
        .status.success {{ background: #e8f5e9; color: #2e7d32; display: block; }}
        code {{ font-family: monospace; font-size: 13px; background: white; padding: 8px; border-radius: 4px; display: block; margin: 8px 0; word-break: break-all; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="card">
            <h1>🔗 Pair This Device</h1>
            <p>Enter a name for this device and tap Pair to connect it to Signal.</p>
            
            <input type="text" id="device-name" placeholder="Device name (e.g., iPhone, iPad)" autofocus>
            <button id="pair-btn" onclick="completePair()">Pair Device</button>
            
            <div id="status" class="status"></div>
            
            <p style="font-size: 13px; color: #9c9c9e; margin-top: 20px;">After pairing, you will be able to send and receive messages on this device.</p>
        </div>
    </div>

    <script>
        const PAIRING_CODE = '{code}';
        
        function showStatus(message, type) {{
            const status = document.getElementById('status');
            status.textContent = message;
            status.className = 'status ' + type;
        }}
        
        async function completePair() {{
            const deviceName = document.getElementById('device-name').value.trim() || 'Device';
            const btn = document.getElementById('pair-btn');
            
            btn.disabled = true;
            showStatus('Pairing...', 'info');
            
            try {{
                const response = await fetch('/api/pair/complete', {{
                    method: 'POST',
                    headers: {{
                        'Content-Type': 'application/json'
                    }},
                    body: JSON.stringify({{
                        pairing_code: PAIRING_CODE,
                        device_name: deviceName,
                        device_kind: 'phone'
                    }})
                }});
                
                const data = await response.json();
                
                if (!response.ok) {{
                    throw new Error(data.message || 'Pairing failed');
                }}
                
                // Store device token
                localStorage.setItem('signal_device_token', data.device_token);
                showStatus('✓ Pairing successful!', 'success');
                
                // Redirect to app after 1 second
                setTimeout(() => {{
                    window.location.href = '/app';
                }}, 1000);
                
            }} catch (error) {{
                console.error('Pairing error:', error);
                showStatus('Error: ' + error.message, 'error');
                btn.disabled = false;
            }}
        }}
        
        // Allow Enter to submit
        document.getElementById('device-name').addEventListener('keypress', function(e) {{
            if (e.key === 'Enter') completePair();
        }});
    </script>
</body>
</html>"#,
        code = code
    );

    Ok(Html(Box::leak(html.into_boxed_str())))
}

async fn diagnostics_page(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
) -> Result<Html<String>, axum::response::Response> {
    if state.token.is_some() {
        match query
            .token
            .as_deref()
            .and_then(|token| state.authenticate_token(token).ok())
        {
            Some(AuthIdentity::Admin) => {}
            _ => {
                return Err(make_error_response(
                    axum::http::StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "Diagnostics requires ?token=<admin-token>",
                ));
            }
        }
    }

    let diagnostics = build_diagnostics(&state);
    let suggested_fix = diagnostics.suggested_fix.as_deref().unwrap_or("none");
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Signal Diagnostics</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background:#f5f5f7; color:#1d1d1f; padding:20px; }}
        main {{ max-width: 900px; margin: 0 auto; background:white; border-radius:12px; padding:20px; box-shadow:0 1px 3px rgba(0,0,0,.1); }}
        h1 {{ margin-bottom:16px; }}
        table {{ width:100%; border-collapse:collapse; }}
        td {{ border-bottom:1px solid #e5e5ea; padding:10px 0; vertical-align:top; }}
        td:first-child {{ color:#6e6e73; width:34%; }}
        code {{ background:#f5f5f7; padding:2px 5px; border-radius:4px; word-break:break-all; }}
    </style>
</head>
<body>
<main>
    <h1>Signal Diagnostics</h1>
    <table>
        <tr><td>Daemon running</td><td>{}</td></tr>
        <tr><td>Version</td><td>{}</td></tr>
        <tr><td>DB path</td><td><code>{}</code></td></tr>
        <tr><td>Public base URL</td><td><code>{}</code></td></tr>
        <tr><td>Web Push enabled</td><td>{}</td></tr>
        <tr><td>VAPID public key</td><td>length={:?}, firstByte={:?}</td></tr>
        <tr><td>Devices</td><td>active={}, revoked={}</td></tr>
        <tr><td>Subscriptions</td><td>active={}, revoked/stale={}, legacy/unbound={}</td></tr>
        <tr><td>Last push success</td><td>{}</td></tr>
        <tr><td>Last push error</td><td>{}</td></tr>
        <tr><td>Last ask/reply event</td><td>{}</td></tr>
        <tr><td>Suggested fix</td><td>{}</td></tr>
    </table>
</main>
</body>
</html>"#,
        diagnostics.daemon_running,
        escape_html(&diagnostics.version),
        escape_html(&diagnostics.db_path),
        escape_html(diagnostics.public_base_url.as_deref().unwrap_or("none")),
        diagnostics.web_push_enabled,
        diagnostics.vapid_public_key_length,
        diagnostics.vapid_public_key_first_byte,
        diagnostics.active_devices,
        diagnostics.revoked_devices,
        diagnostics.active_subscriptions,
        diagnostics.revoked_or_stale_subscriptions,
        diagnostics.legacy_unbound_subscriptions,
        escape_html(
            diagnostics
                .last_push_success_at
                .as_deref()
                .unwrap_or("none")
        ),
        escape_html(diagnostics.last_push_error.as_deref().unwrap_or("none")),
        escape_html(
            diagnostics
                .last_ask_or_reply_event
                .as_deref()
                .unwrap_or("none")
        ),
        escape_html(suggested_fix),
    );
    Ok(Html(html))
}

pub fn create_html_router(
    storage: Arc<Storage>,
    token: Option<String>,
    require_token_for_read: bool,
    enable_web_push: bool,
    vapid_config: Option<VapidConfig>,
    db_path: String,
) -> Router {
    Router::new()
        .route("/", get(inbox_page))
        .route("/dashboard", get(dashboard))
        .route("/diagnostics", get(diagnostics_page))
        .route("/pair", get(pair_page))
        .route("/message/{id}", get(message_detail_page))
        .route("/api/messages/{id}/replies/form", post(reply_form_handler))
        .with_state(AppState::with_push(
            storage,
            token,
            require_token_for_read,
            enable_web_push,
            vapid_config,
            db_path,
        ))
}

#[derive(Debug, Deserialize)]
pub struct TokenQuery {
    token: Option<String>,
    device_token: Option<String>,
}

fn check_html_read_auth(
    state: &AppState,
    token_from_query: Option<&str>,
) -> Result<(), axum::response::Response> {
    if !state.is_auth_required() || !state.require_token_for_read {
        return Ok(());
    }

    match token_from_query {
        Some(t) => state
            .authenticate_token(t)
            .map(|_| ())
            .map_err(|error| match error {
                AuthFailure::Revoked => make_error_response(
                    axum::http::StatusCode::FORBIDDEN,
                    "device_revoked",
                    "This device has been revoked. Pair again.",
                ),
                AuthFailure::Invalid => make_error_response(
                    axum::http::StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "missing or invalid token",
                ),
            }),
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
    let query_token = query.device_token.as_deref().or(query.token.as_deref());
    check_html_read_auth(&state, query_token)?;

    let storage = state.storage.as_ref();
    let messages = storage
        .list_messages(Some(50), None, None, None)
        .unwrap_or_default();
    let token = query.device_token.or(query.token).or(state.token.clone());
    Ok(Html(html::render_inbox(&messages, token.as_deref())))
}

async fn message_detail_page(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TokenQuery>,
) -> Result<Html<String>, axum::response::Response> {
    let query_token = query.device_token.as_deref().or(query.token.as_deref());
    check_html_read_auth(&state, query_token)?;

    let storage = state.storage.as_ref();

    let message = storage.get_message(&id).map_err(|_| {
        axum::response::Response::builder()
            .status(404)
            .body("Not found".into())
            .unwrap()
    })?;

    let replies = storage.get_replies_for_message(&id).unwrap_or_default();
    let token = query.device_token.or(query.token).or(state.token.clone());

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
    if state.token.is_some() {
        let Some(token) = form.token.as_deref() else {
            return Err(make_error_response(
                axum::http::StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing or invalid token",
            ));
        };
        state
            .authenticate_token(token)
            .map_err(auth_error_response)?;
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

#[cfg(test)]
mod tests {
    use super::build_diagnostics;
    use crate::app_state::AppState;
    use crate::web_push_sender::VapidConfig;
    use signal_core::models::{Device, PushSubscription};
    use signal_core::{hash_token, Storage};
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    #[test]
    fn diagnostics_summary_counts_devices_and_push_state() {
        let file = NamedTempFile::new().unwrap();
        let storage = Arc::new(Storage::new(file.path()).unwrap());
        let active = Device::new(
            "active".to_string(),
            "phone".to_string(),
            "phone".to_string(),
            hash_token("active-token"),
            "sig_dev_active".to_string(),
        );
        let revoked = Device::new(
            "revoked".to_string(),
            "old phone".to_string(),
            "phone".to_string(),
            hash_token("revoked-token"),
            "sig_dev_revoked".to_string(),
        );
        storage.create_device(&active).unwrap();
        storage.create_device(&revoked).unwrap();
        storage.revoke_device(&revoked.id).unwrap();

        let mut active_sub = PushSubscription::new(
            "https://web.push.apple.com/active".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        active_sub.device_id = Some(active.id.clone());
        storage.upsert_push_subscription(&active_sub).unwrap();

        let legacy_sub = PushSubscription::new(
            "https://web.push.apple.com/legacy".to_string(),
            "p256dh".to_string(),
            "auth".to_string(),
            None,
        );
        storage.upsert_push_subscription(&legacy_sub).unwrap();

        let state = AppState::with_push(
            storage,
            Some("dev-token".to_string()),
            true,
            true,
            Some(VapidConfig {
                private_key: "private".to_string(),
                public_key: "invalid".to_string(),
                subject: "mailto:test@example.local".to_string(),
                public_base_url: Some("https://example.test".to_string()),
            }),
            ".\\test.db".to_string(),
        );

        let diagnostics = build_diagnostics(&state);
        assert!(diagnostics.daemon_running);
        assert_eq!(diagnostics.active_devices, 1);
        assert_eq!(diagnostics.revoked_devices, 1);
        assert_eq!(diagnostics.active_subscriptions, 1);
        assert_eq!(diagnostics.legacy_unbound_subscriptions, 1);
        assert_eq!(diagnostics.daemon.ok, true);
        assert_eq!(diagnostics.vapid.public_key_present, true);
        assert_eq!(diagnostics.devices.total, 2);
        assert_eq!(diagnostics.push_subscriptions.active, 1);
        assert_eq!(
            diagnostics.public_base_url.as_deref(),
            Some("https://example.test")
        );
    }
}
