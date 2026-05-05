mod api;
mod app_state;
mod html;
mod push;
mod vapid;
mod web_push_sender;

use axum::Router;
use clap::Parser;
use signal_core::Storage;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "signal-daemon")]
#[command(about = "Signal daemon - local-first ping/inbox system", long_about = None)]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 8791)]
    port: u16,

    #[arg(long, default_value = "./signal_demo.db")]
    db_path: String,

    #[arg(long)]
    token: Option<String>,

    #[arg(long)]
    require_token_for_read: bool,

    #[arg(long)]
    allow_unauthenticated_lan: bool,

    #[arg(long)]
    enable_web_push: bool,

    #[arg(long)]
    enable_experimental_actions: bool,

    #[arg(long, default_value = "./signal_vapid.json")]
    vapid_file: PathBuf,

    #[arg(long, default_value = "mailto:araheemimami@gmail.com")]
    vapid_subject: String,

    #[arg(long)]
    public_base_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    let bind_addr = format!("{}:{}", args.host, args.port);

    if args.host == "0.0.0.0" {
        if args.token.is_none() && !args.allow_unauthenticated_lan {
            error!("Refusing to bind 0.0.0.0 without --token. Use --allow-unauthenticated-lan for local-only throwaway testing.");
            std::process::exit(1);
        }
        warn!("Warning: listening on all interfaces. Use only on trusted/private networks such as Tailscale.");
    }

    info!("Starting Signal daemon on {}", bind_addr);
    info!("Database path: {}", args.db_path);
    if args.token.is_some() {
        info!("Token authentication enabled");
        if args.require_token_for_read {
            info!("Token required for all API endpoints (including reads)");
        }
    } else if args.allow_unauthenticated_lan {
        info!("Running in LAN-only throwaway mode (no authentication)");
    }

    let storage = Storage::new(&args.db_path)?;
    let storage = Arc::new(storage);

    let vapid_config = if args.enable_web_push {
        info!("Web Push enabled");
        let vapid_keys = vapid::load_or_generate_vapid_keys(&args.vapid_file)?;
        let diagnostics = vapid::get_diagnostics(&vapid_keys.public_key)?;
        info!(
            "VAPID public key loaded: length={}, first_byte=0x{:02x}",
            diagnostics.length, diagnostics.first_byte
        );
        Some(web_push_sender::VapidConfig {
            private_key: vapid_keys.private_key,
            public_key: vapid_keys.public_key,
            subject: args.vapid_subject.clone(),
            public_base_url: args.public_base_url.clone(),
        })
    } else {
        info!("Web Push disabled");
        None
    };

    let html_router = api::create_html_router(
        storage.clone(),
        args.token.clone(),
        args.require_token_for_read,
        args.enable_web_push,
        args.enable_experimental_actions,
        vapid_config.clone(),
        args.db_path.clone(),
    );

    let api_router = api::create_api_router(
        storage.clone(),
        args.token.clone(),
        args.require_token_for_read,
        args.enable_web_push,
        args.enable_experimental_actions,
        vapid_config.clone(),
        args.db_path.clone(),
    );

    let push_router = if args.enable_web_push {
        push::create_push_router(storage.clone(), true, vapid_config, args.token.clone())
    } else {
        Router::new()
    };

    let pwa_router = api::create_pwa_router();

    let app = Router::new()
        .merge(api_router)
        .merge(html_router)
        .merge(push_router)
        .merge(pwa_router);

    let addr: SocketAddr = bind_addr.parse()?;
    let listener = TcpListener::bind(addr).await?;

    info!("Server listening on http://{}", addr);
    info!("Open http://{}/ in your browser to view the inbox", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::Parser;

    #[test]
    fn cli_defaults_match_release_config_defaults() {
        let args = Args::parse_from(["signal-daemon"]);
        assert_eq!(args.host, "127.0.0.1");
        assert_eq!(args.port, 8791);
        assert_eq!(args.db_path, "./signal_demo.db");
        assert!(!args.require_token_for_read);
        assert!(!args.enable_web_push);
        assert!(!args.enable_experimental_actions);
    }

    #[test]
    fn cli_parses_release_config_flags() {
        let args = Args::parse_from([
            "signal-daemon",
            "--host",
            "127.0.0.1",
            "--port",
            "8791",
            "--db-path",
            ".\\signal_demo.db",
            "--token",
            "dev-token",
            "--require-token-for-read",
            "--enable-web-push",
            "--enable-experimental-actions",
            "--public-base-url",
            "https://example.test",
        ]);
        assert_eq!(args.port, 8791);
        assert_eq!(args.token.as_deref(), Some("dev-token"));
        assert!(args.require_token_for_read);
        assert!(args.enable_web_push);
        assert!(args.enable_experimental_actions);
        assert_eq!(
            args.public_base_url.as_deref(),
            Some("https://example.test")
        );
    }
}
