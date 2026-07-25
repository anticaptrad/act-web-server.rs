//! HTTP surface: public k8s probes plus Supabase-authenticated API routes.

use axum::{Extension, Json, Router, extract::State, middleware, routing::get};
use serde_json::{Value, json};

use crate::auth::{self, Claims};
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    // Routes behind Supabase JWT verification.
    let protected = Router::new()
        .route("/api/me", get(me))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .merge(protected)
        .with_state(state)
}

/// Liveness probe.
async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Readiness probe; reports database connectivity for observability.
async fn ready(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "ready": true,
        "database_connected": state.db.is_some(),
    }))
}

/// Returns the verified identity of the caller.
async fn me(Extension(claims): Extension<Claims>) -> Json<Value> {
    Json(json!({
        "sub": claims.sub,
        "email": claims.email,
        "role": claims.role,
    }))
}
