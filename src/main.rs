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
mod data_plane;
mod db;
mod flags;
mod routes;
mod state;
mod telemetry;
mod ui;
#[cfg(feature = "ui-dioxus")]
mod ui_dioxus;
#[cfg(feature = "ui-leptos")]
mod ui_leptos;

use std::{net::SocketAddr, sync::Arc};
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(output) = flags::process_control().map_err(std::io::Error::other)? {
        print!("{output}");
        return Ok(());
    }
    let cfg = config::Config::from_env()?;
    let _telemetry = telemetry::init(&cfg.service_name)?;

    let shared_auth = cfg
        .shared_auth
        .as_ref()
        .map(|auth| {
            auth::SharedAuthVerifier::new(
                auth.base_url.clone(),
                auth.service_credential.clone(),
                auth.audience.clone(),
            )
        })
        .transpose()?;
    if shared_auth.is_none() {
        tracing::warn!("Shared Auth is not configured; protected routes are closed");
    }

    let direct = match cfg.direct_database.as_ref() {
        Some(config) => match db::DirectReadStore::connect(config).await {
            Ok(store) => Some(store),
            Err(error) => {
                tracing::warn!(error = %error, "read-only database unavailable; direct mode disabled");
                None
            }
        },
        None => None,
    };
    let tcp = cfg
        .mtls
        .as_ref()
        .map(data_plane::PersistentMtlsClient::from_config)
        .transpose()?
        .map(Arc::new);
    let jetstream = match cfg.nats_url.as_deref() {
        Some(url) => match async_nats::connect(url).await {
            Ok(client) => Some(async_nats::jetstream::new(client)),
            Err(error) => {
                tracing::warn!(error = %error, "NATS unavailable; async mode disabled");
                None
            }
        },
        None => None,
    };
    let direct_database_connected = direct.is_some();
    let stateless_http_configured = cfg.api_url.is_some();
    let stateful_mtls_configured = tcp.is_some();
    let jetstream_configured = jetstream.is_some();
    let operation_attestation_key = cfg
        .operation_attestation_key
        .as_deref()
        .map(|key| Arc::<[u8]>::from(key.as_bytes()));
    let gateway = Arc::new(data_plane::TransportGateway::new(
        direct,
        cfg.api_url.clone(),
        tcp,
        jetstream,
        operation_attestation_key,
    )?);

    let state = state::AppState {
        shared_auth,
        gateway,
        direct_database_connected,
        stateless_http_configured,
        stateful_mtls_configured,
        jetstream_configured,
    };

    let app = routes::router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, service = %cfg.service_name, "act-web-server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("shutdown complete");
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
