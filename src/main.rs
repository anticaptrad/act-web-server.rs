//! act-web-server — Supabase-authenticated HTTP API for the AntiCapTrad platform.
//!
//! Deployed to the k8s cluster at ~/codes/ores/k8s-cluster. Persistence is
//! Postgres (Supabase) via sea-orm; auth is Supabase-issued HS256 JWTs.
//!
//! ## Web/API data boundary
//!
//! Choose a route per operation; a query being read-only does not by itself make
//! it safe, cheap, authorized, or consistent.
//!
//! 1. **Direct database read — narrow SSR/readiness optimization.** Keep this
//!    process's optional pool limited to health checks and explicitly approved,
//!    bounded list/detail projections whose measured benefit justifies removing
//!    the API hop. It must use a distinct `__web_ro` principal with
//!    database-enforced `SELECT` allowlists, read-only transactions, timeouts,
//!    tenant/actor context, result limits, and negative write/isolation tests.
//!    Expose named reads rather than a raw SeaORM connection. Never use this
//!    route for product-domain writes or across an untrusted/remote boundary.
//! 2. **Stateless HTTP to the API cluster — default.** Use the generated client
//!    for every product mutation and for authorization-sensitive, composite,
//!    rapidly evolving, or consistency-sensitive reads. Call the load-balanced
//!    service endpoint with deadlines and trace/actor context. HTTP connection
//!    pooling may reuse TCP/QUIC underneath, but no application session belongs
//!    to a particular socket or replica. Retry only idempotent operations or
//!    mutations protected by an idempotency key.
//! 3. **Application-stateful TCP to the API cluster — streaming exception.** A
//!    framed TCP session is appropriate only for measured high-rate streaming
//!    or backpressure/resume requirements that HTTP streaming or WebSockets do
//!    not satisfy. Authenticate the connection and each logical stream, bound
//!    buffers and heartbeats, support versioned framing and resume tokens, and
//!    reconnect through a TCP-aware load balancer. Do not invent a custom TCP
//!    protocol for ordinary operator, form, CRUD, or SSR traffic.
//! 4. **NATS/message queue — asynchronous workflow.** Publish a typed, versioned
//!    command/event for durable analysis, fan-out, or work whose result is not
//!    needed to render the current response. Include actor/tenant context,
//!    correlation and idempotency keys, expiry, retry/dead-letter policy, and an
//!    audit trail. Persist the result and notify the browser by polling, SSE, or
//!    WebSocket. Broker request/reply is not the default synchronous RPC path.
//!
//! Location guide: browser/edge traffic uses HTTPS; an ordinary in-cluster web
//! handler uses stateless HTTP; a same-trust-zone SSR hot path may earn a
//! constrained direct read; a genuinely sessionful high-volume stream may earn
//! TCP; and background processing or integration events use NATS. After a
//! mutation, render from the API response, an API primary read, or an explicit
//! consistency token rather than assuming a read replica is current.
//!
//! `act-api-server` owns product-domain mutations, transaction invariants,
//! idempotency, auditing, provider credentials, and event publication. This web
//! server may own only isolated browser-session/PKCE/CSRF state and a bounded
//! render cache. The product `*-lib-core` package owns desired SQL, persistence
//! schema/JSON, reviewed migration inputs, generated SeaORM adapters, and named
//! operations; `act-interfaces` owns public wire contracts. Production DDL runs
//! only from a serialized one-shot `__migrator` job. Web/API replicas never
//! auto-migrate at startup, and code-first models in a server crate never become
//! a second schema authority.

mod auth;
mod config;
mod db;
mod routes;
mod state;
mod telemetry;
mod ui;

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
        jwt_issuer: cfg.supabase_jwt_iss.clone(),
        jwt_leeway_secs: cfg.supabase_jwt_leeway_secs,
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
