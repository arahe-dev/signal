use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use p256::PublicKey as P256PublicKey;
use serde::Serialize;
use sha2::{Digest, Sha256};
use signal_core::models::{Message, PushSubscription};
use std::time::Duration;
use tokio::time::timeout;
use web_push_native::{Auth, WebPushBuilder};

#[derive(Debug, Clone)]
pub struct VapidConfig {
    pub private_key: String,
    pub public_key: String,
    pub subject: String,
    pub public_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushDebug {
    pub endpoint_origin: Option<String>,
    pub endpoint_prefix: String,
    pub vapid_public_key_len: Option<usize>,
    pub vapid_public_key_first_byte: Option<u8>,
    pub vapid_subject: String,
    pub derived_audience: Option<String>,
    pub jwt_exp_seconds_from_now: u64,
    pub vapid_private_matches_public: bool,
    pub subscription_vapid_key_matches_current: Option<bool>,
    pub http_status: Option<u16>,
    pub http_response_body: Option<String>,
    pub library_error_body: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushResult {
    pub endpoint: String,
    pub success: bool,
    pub error: Option<String>,
    pub debug: PushDebug,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushSummary {
    pub attempted: usize,
    pub sent: usize,
    pub failed: usize,
    pub skipped: usize,
    pub skipped_revoked: usize,
    pub skipped_stale: usize,
    pub skipped_legacy: usize,
    pub results: Vec<PushResult>,
    pub errors: Vec<PushResult>,
}

const PUSH_TIMEOUT_SECS: u64 = 10;
const VAPID_EXP_SECONDS: u64 = 12 * 60 * 60;

pub fn vapid_public_key_hash(public_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public_key.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn endpoint_origin(endpoint: &str) -> Option<String> {
    let url = reqwest::Url::parse(endpoint).ok()?;
    Some(format!("{}://{}", url.scheme(), url.host_str()?))
}

fn endpoint_prefix(endpoint: &str) -> String {
    endpoint.chars().take(48).collect::<String>() + if endpoint.len() > 48 { "..." } else { "" }
}

fn public_key_diag(public_key: &str) -> (Option<usize>, Option<u8>) {
    match URL_SAFE_NO_PAD.decode(public_key) {
        Ok(bytes) => (Some(bytes.len()), bytes.first().copied()),
        Err(_) => (None, None),
    }
}

pub fn private_matches_public(private_key: &str, public_key: &str) -> bool {
    let private_bytes = match URL_SAFE_NO_PAD.decode(private_key) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signing_key = match p256::ecdsa::SigningKey::from_slice(&private_bytes) {
        Ok(key) => key,
        Err(_) => return false,
    };
    let derived = signing_key.verifying_key().to_encoded_point(false);
    URL_SAFE_NO_PAD.encode(derived.as_bytes()) == public_key
}

fn base_debug(subscription: &PushSubscription, vapid_config: &VapidConfig) -> PushDebug {
    let (len, first_byte) = public_key_diag(&vapid_config.public_key);
    let current_hash = vapid_public_key_hash(&vapid_config.public_key);
    PushDebug {
        endpoint_origin: endpoint_origin(&subscription.endpoint),
        endpoint_prefix: endpoint_prefix(&subscription.endpoint),
        vapid_public_key_len: len,
        vapid_public_key_first_byte: first_byte,
        vapid_subject: vapid_config.subject.clone(),
        derived_audience: endpoint_origin(&subscription.endpoint),
        jwt_exp_seconds_from_now: VAPID_EXP_SECONDS,
        vapid_private_matches_public: private_matches_public(
            &vapid_config.private_key,
            &vapid_config.public_key,
        ),
        subscription_vapid_key_matches_current: subscription
            .vapid_public_key_hash
            .as_ref()
            .map(|hash| hash == &current_hash),
        http_status: None,
        http_response_body: None,
        library_error_body: None,
    }
}

pub async fn send_web_push(
    subscription: &PushSubscription,
    vapid_config: &VapidConfig,
    payload: &str,
) -> Result<PushResult, Box<dyn std::error::Error + Send + Sync>> {
    let mut debug = base_debug(subscription, vapid_config);

    if debug.subscription_vapid_key_matches_current == Some(false) {
        return Ok(PushResult {
            endpoint: subscription.endpoint.clone(),
            success: false,
            error: Some("subscription_vapid_key_mismatch_resubscribe_required".to_string()),
            debug,
        });
    }

    let endpoint: &str = &subscription.endpoint;
    let p256dh = &subscription.p256dh;
    let auth = &subscription.auth;

    let p256dh_bytes = URL_SAFE_NO_PAD
        .decode(p256dh)
        .map_err(|e| format!("Invalid p256dh base64: {}", e))?;
    let auth_bytes = URL_SAFE_NO_PAD
        .decode(auth)
        .map_err(|e| format!("Invalid auth base64: {}", e))?;

    let vapid_key_bytes = URL_SAFE_NO_PAD
        .decode(&vapid_config.private_key)
        .map_err(|e| format!("Invalid VAPID private key: {}", e))?;

    let vapid_key_pair =
        web_push_native::jwt_simple::algorithms::ES256KeyPair::from_bytes(&vapid_key_bytes)
            .map_err(|e| format!("Invalid VAPID key: {}", e))?;

    let client_pubkey = P256PublicKey::from_sec1_bytes(&p256dh_bytes)
        .map_err(|e| format!("Invalid client public key: {}", e))?;

    let auth = Auth::clone_from_slice(&auth_bytes);

    let builder = WebPushBuilder::new(
        endpoint
            .parse()
            .map_err(|e| format!("Invalid endpoint: {}", e))?,
        client_pubkey,
        auth,
    )
    .with_vapid(&vapid_key_pair, &vapid_config.subject);

    let request = builder
        .build(payload.as_bytes().to_vec())
        .map_err(|e| e.to_string())?;

    let uri = request.uri().to_string();
    let method = reqwest::Method::from_bytes(request.method().as_str().as_bytes())
        .map_err(|e| format!("Invalid push request method: {}", e))?;

    let client = reqwest::Client::new();
    let mut req_builder = client
        .request(method, &uri)
        .timeout(Duration::from_secs(PUSH_TIMEOUT_SECS));

    for (name, value) in request.headers().iter() {
        req_builder = req_builder.header(name.as_str(), value.to_str().unwrap_or(""));
    }

    let body = request.body().clone();
    let response = req_builder.body(body).send().await?;

    let status = response.status();
    debug.http_status = Some(status.as_u16());
    let response_body = response.text().await.unwrap_or_default();
    if !response_body.is_empty() {
        debug.http_response_body = Some(response_body.chars().take(500).collect());
    }
    let success = status.is_success();

    Ok(PushResult {
        endpoint: subscription.endpoint.clone(),
        success,
        error: if success {
            None
        } else {
            Some(format!("HTTP {}", status))
        },
        debug,
    })
}

pub async fn send_web_push_to_all_active(
    subscriptions: &[PushSubscription],
    vapid_config: &VapidConfig,
    payload: &str,
) -> PushSummary {
    let mut attempted = 0;
    let mut sent = 0;
    let mut failed = 0;
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for subscription in subscriptions {
        if subscription.status != "active" {
            continue;
        }

        attempted += 1;

        let send_result = timeout(
            Duration::from_secs(PUSH_TIMEOUT_SECS),
            send_web_push(subscription, vapid_config, payload),
        )
        .await;

        match send_result {
            Ok(Ok(result)) => {
                if result.success {
                    sent += 1;
                    results.push(result);
                } else {
                    failed += 1;
                    results.push(result.clone());
                    errors.push(result);
                }
            }
            Ok(Err(e)) => {
                failed += 1;
                let result = PushResult {
                    endpoint: subscription.endpoint.clone(),
                    success: false,
                    error: Some(e.to_string()),
                    debug: {
                        let mut debug = base_debug(subscription, vapid_config);
                        debug.library_error_body = Some(e.to_string());
                        debug
                    },
                };
                results.push(result.clone());
                errors.push(result);
            }
            Err(_) => {
                failed += 1;
                let result = PushResult {
                    endpoint: subscription.endpoint.clone(),
                    success: false,
                    error: Some("push send timed out".to_string()),
                    debug: base_debug(subscription, vapid_config),
                };
                results.push(result.clone());
                errors.push(result);
            }
        }
    }

    PushSummary {
        attempted,
        sent,
        failed,
        skipped: 0,
        skipped_revoked: 0,
        skipped_stale: 0,
        skipped_legacy: 0,
        results,
        errors,
    }
}

pub fn build_generic_payload(public_base_url: Option<&str>) -> String {
    let url = if let Some(base) = public_base_url {
        format!("{}/app", base)
    } else {
        "/app".to_string()
    };

    serde_json::json!({
        "title": "Signal",
        "body": "New Signal message. Tap to open inbox.",
        "url": url
    })
    .to_string()
}

pub fn build_message_payload(
    message: &Message,
    public_base_url: Option<&str>,
    token: Option<&str>,
) -> String {
    let url = build_message_url(&message.id, public_base_url, token);

    let mut context = format!("New message from {}", message.source);
    if let Some(project) = &message.project {
        if !project.is_empty() {
            context = format!("[{}] {}", project, context);
        }
    }

    serde_json::json!({
        "title": format!("Signal: {}", message.title),
        "body": context.chars().take(180).collect::<String>(),
        "url": url,
        "message_id": message.id,
        "source": message.source,
        "project": message.project
    })
    .to_string()
}

pub fn build_message_url(
    message_id: &str,
    public_base_url: Option<&str>,
    _token: Option<&str>,
) -> String {
    let base = public_base_url.unwrap_or("");
    if base.is_empty() {
        format!("/app?message={}", message_id)
    } else {
        format!("{}/app?message={}", base.trim_end_matches('/'), message_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{build_ask_payload, build_message_payload, build_message_url};
    use signal_core::models::{Message, PermissionLevel};

    fn message() -> Message {
        let mut message = Message::new(
            "Need input".to_string(),
            "Reply yes".to_string(),
            "codex".to_string(),
            None,
            Some("codex".to_string()),
            Some("signal".to_string()),
            PermissionLevel::Actionable,
        );
        message.id = "message-123".to_string();
        message
    }

    #[test]
    fn message_url_generation_uses_pwa_deep_link_without_token() {
        assert_eq!(
            build_message_url(
                "message-123",
                Some("https://example.test"),
                Some("dev-token")
            ),
            "https://example.test/app?message=message-123"
        );
    }

    #[test]
    fn notification_payload_url_points_to_pwa_message() {
        let payload: serde_json::Value = serde_json::from_str(&build_message_payload(
            &message(),
            Some("https://example.test"),
            Some("dev-token"),
        ))
        .unwrap();
        assert_eq!(
            payload["url"],
            "https://example.test/app?message=message-123"
        );
    }

    #[test]
    fn ask_notification_payload_uses_generic_body_and_deep_link() {
        let payload: serde_json::Value = serde_json::from_str(&build_ask_payload(
            &message(),
            Some("https://example.test"),
            Some("dev-token"),
        ))
        .unwrap();
        assert_eq!(payload["title"], "Signal");
        assert_eq!(
            payload["url"],
            "https://example.test/app?message=message-123"
        );
    }
}

pub fn build_ask_payload(
    message: &Message,
    public_base_url: Option<&str>,
    token: Option<&str>,
) -> String {
    serde_json::json!({
        "title": "Signal",
        "body": format!("Reply requested from {}. Tap to open.", message.agent_id.as_deref().unwrap_or(&message.source)),
        "url": build_message_url(&message.id, public_base_url, token),
        "message_id": message.id,
        "source": message.source,
        "project": message.project
    })
    .to_string()
}
