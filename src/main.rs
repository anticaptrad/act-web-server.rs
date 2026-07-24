use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "act_web_server=debug,info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // In a real app, this would extract the `Authorization` header, parse the JWT, 
    // and verify it against Supabase's JWKS endpoint.
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .layer(axum::middleware::from_fn(supabase_auth_middleware));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3002));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

use axum::{extract::Request, middleware::Next, response::Response, http::StatusCode};

async fn supabase_auth_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Placeholder logic for Supabase JWT verification
    let _auth_header = request.headers().get("Authorization");
    
    // For now, pass the request through.
    // In the future, decode and verify the JWT with jsonwebtoken crate.
    let response = next.run(request).await;
    Ok(response)
}
