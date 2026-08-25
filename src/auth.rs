//! Fail-closed Shared Auth middleware for the web boundary.
//!
//! The browser-supplied product bearer and the independently injected service
//! credential have separate lanes. The official client sends the user bearer
//! in the introspection body and uses the service credential only to authorize
//! that server-to-server request.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use shared_auth_client::{ClientError, SharedAuthClient};

use crate::state::AppState;

const REQUIRED_SCOPES: [&str; 1] = ["youtube:admin"];
const MAX_INTROSPECTION_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct SharedAuthVerifier {
    client: SharedAuthClient,
    audience: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedIdentity {
    pub subject: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthFailure {
    Missing,
    Invalid,
    Unavailable,
}

impl SharedAuthVerifier {
    pub fn new(
        base_url: impl Into<String>,
        service_credential: impl Into<String>,
        audience: impl Into<String>,
    ) -> Result<Self, ClientError> {
        let client = SharedAuthClient::try_new(base_url)?
            .with_service_credential(service_credential)
            .with_max_response_bytes(MAX_INTROSPECTION_RESPONSE_BYTES);
        Ok(Self {
            client,
            audience: audience.into(),
        })
    }

    pub async fn verify(&self, headers: &HeaderMap) -> Result<VerifiedIdentity, AuthFailure> {
        let token = bearer_token(headers)?;
        let claims = self
            .client
            .introspect_with_requirements(token, &self.audience, &REQUIRED_SCOPES)
            .await
            .map_err(map_client_error)?;
        if !claims.active || claims.aud.as_deref() != Some(self.audience.as_str()) {
            return Err(AuthFailure::Invalid);
        }
        let subject = claims
            .sub
            .filter(|subject| !subject.trim().is_empty())
            .ok_or(AuthFailure::Invalid)?;
        Ok(VerifiedIdentity { subject })
    }
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let verifier = state
        .shared_auth
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let identity = verifier
        .verify(request.headers())
        .await
        .map_err(|failure| match failure {
            AuthFailure::Missing | AuthFailure::Invalid => StatusCode::UNAUTHORIZED,
            AuthFailure::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        })?;
    request.extensions_mut().insert(identity);
    Ok(next.run(request).await)
}

pub fn bearer_token(headers: &HeaderMap) -> Result<&str, AuthFailure> {
    let raw = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthFailure::Missing)?;
    let (scheme, token) = raw.split_once(' ').ok_or(AuthFailure::Invalid)?;
    if !scheme.eq_ignore_ascii_case("Bearer")
        || token.is_empty()
        || token.trim() != token
        || token.chars().any(char::is_whitespace)
    {
        return Err(AuthFailure::Invalid);
    }
    Ok(token)
}

fn map_client_error(error: ClientError) -> AuthFailure {
    match error {
        ClientError::Unauthorized | ClientError::InvalidInput(_) => AuthFailure::Invalid,
        ClientError::MissingServiceCredential
        | ClientError::InvalidBaseUrl
        | ClientError::RequestTooLarge { .. }
        | ClientError::ResponseTooLarge { .. }
        | ClientError::Encode { .. }
        | ClientError::Decode { .. }
        | ClientError::Transport(_)
        | ClientError::Status(_)
        | ClientError::InsecureTransport(_) => AuthFailure::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header::AUTHORIZATION};

    use super::{AuthFailure, SharedAuthVerifier, bearer_token};

    #[test]
    fn bearer_parser_is_strict() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer synthetic"));
        assert_eq!(bearer_token(&headers), Ok("synthetic"));
        for value in [
            "Basic token",
            "Bearer",
            "Bearer token extra",
            "Bearer  token",
        ] {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(value).expect("header"));
            assert_eq!(bearer_token(&headers), Err(AuthFailure::Invalid));
        }
    }

    #[test]
    fn verifier_rejects_remote_cleartext_authority() {
        assert!(
            SharedAuthVerifier::new(
                "http://auth.example.test",
                "independent-service-credential",
                "act-web"
            )
            .is_err()
        );
    }
}
