//! Shared application state passed to handlers and middleware.

use std::sync::Arc;

use crate::{auth::SharedAuthVerifier, data_plane::TransportGateway};

#[derive(Clone)]
pub struct AppState {
    pub shared_auth: Option<SharedAuthVerifier>,
    pub gateway: Arc<TransportGateway>,
    pub direct_database_connected: bool,
    pub stateless_http_configured: bool,
    pub stateful_mtls_configured: bool,
    pub jetstream_configured: bool,
}
