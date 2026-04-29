mod api;
mod app_state;
mod html;
mod push;

use axum::Router;
use clap::Parser;
use signal_core::Storage;
use std::net::SocketAddr;
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

    #[arg(long, default_value_t = 8787)]
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

    let api_router = api::create_api_router(
        storage.clone(),
        args.token.clone(),
        args.require_token_for_read,
    );
    let html_router = api::create_html_router(
        storage.clone(),
        args.token.clone(),
        args.require_token_for_read,
    );

    let push_router = if args.enable_web_push {
        info!("Web Push enabled");
        push::create_push_router(storage.clone(), true)
    } else {
        info!("Web Push disabled");
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
