//! Public probes/UI and Shared Auth protected four-mode gateway routes.

use act_api_server::web_data_plane::WebApiMode;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    auth::{self, VerifiedIdentity},
    data_plane::GatewayError,
    state::AppState,
    ui,
};

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/me", get(me))
        .route("/api/data/:mode", get(read_data))
        .route("/api/operations/:operation_id", get(operation_status))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    let public = Router::new()
        .route("/", get(ui::index))
        .route("/health", get(health))
        .route("/ready", get(ready));

    #[cfg(feature = "ui-leptos")]
    let public = public.route("/ui/leptos", get(crate::ui_leptos::index));

    #[cfg(feature = "ui-dioxus")]
    let public = public.route("/ui/dioxus", get(crate::ui_dioxus::index));

    public.merge(protected).with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn ready(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "ready": true,
        "shared_auth_configured": state.shared_auth.is_some(),
        "modes": {
            "direct_read_only_database": state.direct_database_connected,
            "stateless_http": state.stateless_http_configured,
            "stateful_mtls_tcp": state.stateful_mtls_configured,
            "jet_stream_async": state.jetstream_configured,
        }
    }))
}

async fn me(Extension(identity): Extension<VerifiedIdentity>) -> impl IntoResponse {
    private_json(json!({"sub": identity.subject}))
}

async fn read_data(
    State(state): State<AppState>,
    Extension(identity): Extension<VerifiedIdentity>,
    Path(mode): Path<WebApiMode>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let authorization = authorization(&headers)?;
    let reply = state
        .gateway
        .read(mode, &identity.subject, authorization)
        .await
        .map_err(ApiError::from_gateway)?;
    Ok(private_json(json!(reply)))
}

async fn operation_status(
    State(state): State<AppState>,
    Extension(_identity): Extension<VerifiedIdentity>,
    Path(operation_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let authorization = authorization(&headers)?;
    let reply = state
        .gateway
        .status(&operation_id, authorization)
        .await
        .map_err(ApiError::from_gateway)?;
    Ok(private_json(reply))
}

fn authorization(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::unauthorized())
}

fn private_json(value: Value) -> impl IntoResponse {
    (
        [
            ("cache-control", "private, no-store"),
            ("vary", "authorization"),
        ],
        Json(value),
    )
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
}

impl ApiError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
        }
    }

    fn from_gateway(error: GatewayError) -> Self {
        let status = match error {
            GatewayError::Unauthorized => StatusCode::UNAUTHORIZED,
            GatewayError::InvalidRequest => StatusCode::BAD_REQUEST,
            GatewayError::NotConfigured
            | GatewayError::Backpressure
            | GatewayError::Timeout
            | GatewayError::Upstream
            | GatewayError::InvalidResponse => StatusCode::SERVICE_UNAVAILABLE,
        };
        Self {
            status,
            code: error.code(),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            [("cache-control", "no-store")],
            Json(ErrorBody { error: self.code }),
        )
            .into_response()
    }
}
