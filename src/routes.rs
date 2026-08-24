//! HTTP surface: public k8s probes plus Supabase-authenticated API routes.

use axum::{Extension, Json, Router, extract::State, middleware, routing::get};
use serde_json::{Value, json};

use crate::auth::{self, Claims};
use crate::state::AppState;
use crate::ui;

pub fn router(state: AppState) -> Router {
    // Routes behind Supabase JWT verification.
    let protected =
        Router::new()
            .route("/api/me", get(me))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                auth::require_auth,
            ));

    let public = Router::new()
        // Operator UI. Public like the probes: it exposes no data of its own and
        // reads /api/me only with a token the operator supplies in the browser.
        .route("/", get(ui::index))
        .route("/health", get(health))
        .route("/ready", get(ready));

    #[cfg(feature = "ui-leptos")]
    let public = public.route("/ui/leptos", get(crate::ui_leptos::index));

    #[cfg(feature = "ui-dioxus")]
    let public = public.route("/ui/dioxus", get(crate::ui_dioxus::index));

    public.merge(protected).with_state(state)
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
