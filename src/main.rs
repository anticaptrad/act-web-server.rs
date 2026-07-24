//! act-web-server — Supabase-authenticated HTTP API for the AntiCapTrad platform.
//!
//! Deployed to the k8s cluster at ~/codes/ores/k8s-cluster. Persistence is
//! Postgres (Supabase) via sea-orm; auth is Supabase-issued HS256 JWTs.

mod auth;
mod config;
mod db;
mod routes;
mod state;
mod telemetry;

use std::net::SocketAddr;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::Config::from_env();
    telemetry::init(&cfg.service_name)?;

    let db = match &cfg.database_url {
        Some(url) => db::connect(url).await,
        None => {
            tracing::info!("DATABASE_URL not set; running without persistence");
            None
        }
    };

    if cfg.supabase_jwt_secret.is_none() {
        tracing::warn!("SUPABASE_JWT_SECRET not set; protected routes will reject all requests");
    }

    let state = state::AppState {
        db,
        jwt_secret: cfg.supabase_jwt_secret.clone(),
        jwt_aud: cfg.supabase_jwt_aud.clone(),
    };

    let app = routes::router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, service = %cfg.service_name, "act-web-server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("shutdown complete");
    telemetry::shutdown();
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
