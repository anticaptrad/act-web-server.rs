//! Supabase JWT verification middleware.
//!
//! Supabase issues HS256-signed access tokens. This middleware extracts the
//! `Authorization: Bearer <jwt>` header, verifies the signature against the
//! configured secret, and validates the `exp` and `aud` claims. Verified claims
//! are stored in request extensions for downstream handlers.
//!
//! The middleware fails closed: a missing/invalid token is rejected, and if no
//! signing secret is configured every protected request is denied.

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the Supabase user id.
    pub sub: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    /// Expiry (seconds since epoch); validated by jsonwebtoken.
    pub exp: usize,
    /// Audience. Required rather than optional on purpose: jsonwebtoken skips
    /// audience matching entirely when the claim is absent, so an optional
    /// field would let a token minted for another service through. Typed as a
    /// `Value` because the JWT spec allows either a string or an array.
    pub aud: serde_json::Value,
}

pub async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let secret = state.jwt_secret.as_ref().ok_or_else(|| {
        tracing::error!("SUPABASE_JWT_SECRET not configured; rejecting protected request");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let token = bearer_token(&request).ok_or(StatusCode::UNAUTHORIZED)?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[state.jwt_aud.as_str()]);
    // Not validated by default, which would accept a not-yet-valid token.
    validation.validate_nbf = true;
    // The 60s default is wider than we need; keep just enough for clock skew.
    validation.leeway = state.jwt_leeway_secs;
    if let Some(issuer) = state.jwt_issuer.as_ref() {
        validation.set_issuer(&[issuer.as_str()]);
    }

    let claims = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|err| {
        tracing::debug!(error = %err, "JWT verification failed");
        StatusCode::UNAUTHORIZED
    })?
    .claims;

    let mut request = request;
    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

fn bearer_token(request: &Request) -> Option<String> {
    request
        .headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::to_owned)
}
