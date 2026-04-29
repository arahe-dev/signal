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

pub fn create_html_router(
    storage: Arc<Storage>,
    token: Option<String>,
    require_token_for_read: bool,
) -> Router {
    Router::new()
        .route("/", get(inbox_page))
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
