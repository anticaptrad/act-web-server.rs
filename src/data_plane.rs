//! Live web-to-API adapters for the four reviewed interaction modes.

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use act_api_server::{
    transport_runtime::{
        AuthenticatedOperation, JETSTREAM_REQUEST_SUBJECT, MAX_AUTHORIZATION_BYTES,
        MAX_TRANSPORT_BYTES, OPERATION_TIMEOUT, OperationReply, sign_operation_attestation,
    },
    web_data_plane::{DataOperation, OperationEnvelope, WebApiMode},
};
use futures::{SinkExt, StreamExt};
use reqwest::Url;
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject},
};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::{
    net::TcpStream,
    sync::{Mutex, Semaphore},
};
use tokio_rustls::{TlsConnector, client::TlsStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::{config::MtlsConfig, db::DirectReadStore};

const MAX_HTTP_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_IN_FLIGHT_OPERATIONS: usize = 64;
const HTTP_TIMEOUT: Duration = Duration::from_secs(4);
static OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum GatewayReply {
    Projection {
        mode: WebApiMode,
        data: Value,
    },
    Accepted {
        operation_id: String,
        status: &'static str,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatewayError {
    Unauthorized,
    InvalidRequest,
    NotConfigured,
    Backpressure,
    Timeout,
    Upstream,
    InvalidResponse,
}

impl GatewayError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::InvalidRequest => "invalid_request",
            Self::NotConfigured => "transport_not_configured",
            Self::Backpressure => "backpressure",
            Self::Timeout => "upstream_timeout",
            Self::Upstream | Self::InvalidResponse => "upstream_unavailable",
        }
    }
}

pub struct PersistentMtlsClient {
    address: SocketAddr,
    server_name: String,
    connector: TlsConnector,
    connection: Mutex<Option<Framed<TlsStream<TcpStream>, LengthDelimitedCodec>>>,
}

impl PersistentMtlsClient {
    pub fn from_config(config: &MtlsConfig) -> anyhow::Result<Self> {
        let client_certificates = CertificateDer::pem_file_iter(&config.client_certificate_file)?
            .collect::<Result<Vec<_>, _>>()?;
        if client_certificates.is_empty() {
            anyhow::bail!("ACT_WEB_CLIENT_CERT_FILE contains no certificates");
        }
        let client_private_key = PrivateKeyDer::from_pem_file(&config.client_private_key_file)
            .map_err(|_| anyhow::anyhow!("ACT_WEB_CLIENT_KEY_FILE contains no usable key"))?;
        let server_certificates = CertificateDer::pem_file_iter(&config.server_ca_file)?
            .collect::<Result<Vec<_>, _>>()?;
        if server_certificates.is_empty() {
            anyhow::bail!("ACT_API_CA_FILE contains no certificates");
        }
        let mut roots = RootCertStore::empty();
        for certificate in server_certificates {
            roots.add(certificate)?;
        }
        let tls = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_client_auth_cert(client_certificates, client_private_key)?;
        Ok(Self {
            address: config.address.parse()?,
            server_name: config.server_name.clone(),
            connector: TlsConnector::from(Arc::new(tls)),
            connection: Mutex::new(None),
        })
    }

    async fn connect(
        &self,
    ) -> Result<Framed<TlsStream<TcpStream>, LengthDelimitedCodec>, GatewayError> {
        let tcp = tokio::time::timeout(OPERATION_TIMEOUT, TcpStream::connect(self.address))
            .await
            .map_err(|_| GatewayError::Timeout)?
            .map_err(|_| GatewayError::Upstream)?;
        let server_name = ServerName::try_from(self.server_name.clone())
            .map_err(|_| GatewayError::InvalidRequest)?;
        let tls = tokio::time::timeout(OPERATION_TIMEOUT, self.connector.connect(server_name, tcp))
            .await
            .map_err(|_| GatewayError::Timeout)?
            .map_err(|_| GatewayError::Upstream)?;
        Ok(Framed::new(tls, length_codec()))
    }

    async fn request(
        &self,
        request: &AuthenticatedOperation,
    ) -> Result<OperationReply, GatewayError> {
        let payload = serde_json::to_vec(request).map_err(|_| GatewayError::InvalidRequest)?;
        if payload.len() > MAX_TRANSPORT_BYTES {
            return Err(GatewayError::InvalidRequest);
        }
        let mut guard = self.connection.lock().await;
        let mut connection = match guard.take() {
            Some(connection) => connection,
            None => self.connect().await?,
        };
        let outcome = async {
            tokio::time::timeout(OPERATION_TIMEOUT, connection.send(payload.into()))
                .await
                .map_err(|_| GatewayError::Timeout)?
                .map_err(|_| GatewayError::Upstream)?;
            let frame = tokio::time::timeout(OPERATION_TIMEOUT, connection.next())
                .await
                .map_err(|_| GatewayError::Timeout)?
                .ok_or(GatewayError::Upstream)?
                .map_err(|_| GatewayError::InvalidResponse)?;
            serde_json::from_slice::<OperationReply>(&frame)
                .map_err(|_| GatewayError::InvalidResponse)
        }
        .await;
        if outcome.is_ok() {
            *guard = Some(connection);
        }
        outcome
    }
}

pub struct TransportGateway {
    direct: Option<DirectReadStore>,
    http: Option<reqwest::Client>,
    api_url: Option<Url>,
    tcp: Option<Arc<PersistentMtlsClient>>,
    jetstream: Option<async_nats::jetstream::Context>,
    operation_attestation_key: Option<Arc<[u8]>>,
    permits: Arc<Semaphore>,
}

impl TransportGateway {
    pub fn new(
        direct: Option<DirectReadStore>,
        api_url: Option<Url>,
        tcp: Option<Arc<PersistentMtlsClient>>,
        jetstream: Option<async_nats::jetstream::Context>,
        operation_attestation_key: Option<Arc<[u8]>>,
    ) -> anyhow::Result<Self> {
        let http = api_url
            .as_ref()
            .map(|url| {
                let loopback = url.scheme() == "http";
                reqwest::Client::builder()
                    .connect_timeout(Duration::from_secs(2))
                    .timeout(HTTP_TIMEOUT)
                    .redirect(reqwest::redirect::Policy::none())
                    .https_only(!loopback)
                    .build()
            })
            .transpose()?;
        Ok(Self {
            direct,
            http,
            api_url,
            tcp,
            jetstream,
            operation_attestation_key,
            permits: Arc::new(Semaphore::new(MAX_IN_FLIGHT_OPERATIONS)),
        })
    }

    pub async fn read(
        &self,
        mode: WebApiMode,
        subject: &str,
        authorization: &str,
    ) -> Result<GatewayReply, GatewayError> {
        validate_authorization(authorization)?;
        let _permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| GatewayError::Backpressure)?;
        match mode {
            WebApiMode::DirectReadOnlyDatabase => self.direct_read(subject).await,
            WebApiMode::StatelessHttp => self.http_read(authorization).await,
            WebApiMode::StatefulMtlsTcp => self.tcp_read(subject, authorization).await,
            WebApiMode::JetStreamAsync => self.jetstream_submit(subject).await,
        }
    }

    pub async fn status(
        &self,
        operation_id: &str,
        authorization: &str,
    ) -> Result<Value, GatewayError> {
        validate_identifier(operation_id, 128)?;
        let endpoint = self.api_endpoint(&format!("v1/operations/{operation_id}"))?;
        self.bounded_http_get(endpoint, authorization).await
    }

    async fn direct_read(&self, subject: &str) -> Result<GatewayReply, GatewayError> {
        let store = self.direct.as_ref().ok_or(GatewayError::NotConfigured)?;
        let events = store
            .events_for_subject(subject)
            .await
            .map_err(|_| GatewayError::Upstream)?;
        Ok(GatewayReply::Projection {
            mode: WebApiMode::DirectReadOnlyDatabase,
            data: serde_json::to_value(events).map_err(|_| GatewayError::InvalidResponse)?,
        })
    }

    async fn http_read(&self, authorization: &str) -> Result<GatewayReply, GatewayError> {
        let endpoint = self.api_endpoint("v1/youtube/status")?;
        let data = self.bounded_http_get(endpoint, authorization).await?;
        Ok(GatewayReply::Projection {
            mode: WebApiMode::StatelessHttp,
            data,
        })
    }

    async fn tcp_read(
        &self,
        subject: &str,
        authorization: &str,
    ) -> Result<GatewayReply, GatewayError> {
        let client = self.tcp.as_ref().ok_or(GatewayError::NotConfigured)?;
        let request = bearer_operation(subject, authorization, WebApiMode::StatefulMtlsTcp)?;
        let expected_id = request.envelope.operation_id.clone();
        let reply = client.request(&request).await?;
        if reply.operation_id.as_deref() != Some(expected_id.as_str()) {
            return Err(GatewayError::InvalidResponse);
        }
        if reply.error.is_some() {
            return Err(GatewayError::Upstream);
        }
        Ok(GatewayReply::Projection {
            mode: WebApiMode::StatefulMtlsTcp,
            data: reply.result.ok_or(GatewayError::InvalidResponse)?,
        })
    }

    async fn jetstream_submit(&self, subject: &str) -> Result<GatewayReply, GatewayError> {
        let context = self.jetstream.as_ref().ok_or(GatewayError::NotConfigured)?;
        let key = self
            .operation_attestation_key
            .as_deref()
            .ok_or(GatewayError::NotConfigured)?;
        let request = attested_operation(subject, key)?;
        let operation_id = request.envelope.operation_id.clone();
        let payload = serde_json::to_vec(&request).map_err(|_| GatewayError::InvalidRequest)?;
        if payload.len() > MAX_TRANSPORT_BYTES {
            return Err(GatewayError::InvalidRequest);
        }
        let mut headers = async_nats::HeaderMap::new();
        let dedupe_id = format!("act-operation:{operation_id}");
        headers.insert("Nats-Msg-Id", dedupe_id.as_str());
        let acknowledgement = tokio::time::timeout(
            OPERATION_TIMEOUT,
            context.publish_with_headers(
                JETSTREAM_REQUEST_SUBJECT.to_string(),
                headers,
                payload.into(),
            ),
        )
        .await
        .map_err(|_| GatewayError::Timeout)?
        .map_err(|_| GatewayError::Upstream)?;
        tokio::time::timeout(OPERATION_TIMEOUT, acknowledgement)
            .await
            .map_err(|_| GatewayError::Timeout)?
            .map_err(|_| GatewayError::Upstream)?;
        Ok(GatewayReply::Accepted {
            operation_id,
            status: "accepted",
        })
    }

    async fn bounded_http_get(
        &self,
        endpoint: Url,
        authorization: &str,
    ) -> Result<Value, GatewayError> {
        let http = self.http.as_ref().ok_or(GatewayError::NotConfigured)?;
        let response = http
            .get(endpoint)
            .header(reqwest::header::AUTHORIZATION, authorization)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    GatewayError::Timeout
                } else {
                    GatewayError::Upstream
                }
            })?;
        if !response.status().is_success() || response.status().is_redirection() {
            return Err(GatewayError::Upstream);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_HTTP_RESPONSE_BYTES as u64)
        {
            return Err(GatewayError::InvalidResponse);
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| GatewayError::Upstream)?;
            if body.len() + chunk.len() > MAX_HTTP_RESPONSE_BYTES {
                return Err(GatewayError::InvalidResponse);
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).map_err(|_| GatewayError::InvalidResponse)
    }

    fn api_endpoint(&self, path: &str) -> Result<Url, GatewayError> {
        self.api_url
            .as_ref()
            .ok_or(GatewayError::NotConfigured)?
            .join(path)
            .map_err(|_| GatewayError::InvalidRequest)
    }
}

fn bearer_operation(
    subject: &str,
    authorization: &str,
    mode: WebApiMode,
) -> Result<AuthenticatedOperation, GatewayError> {
    validate_authorization(authorization)?;
    if mode == WebApiMode::JetStreamAsync {
        return Err(GatewayError::InvalidRequest);
    }
    let envelope = operation_envelope(subject, mode)?;
    let request = AuthenticatedOperation {
        authorization: authorization.to_string(),
        envelope,
    };
    request
        .validate(mode)
        .map_err(|_| GatewayError::InvalidRequest)?;
    Ok(request)
}

fn attested_operation(
    subject: &str,
    operation_attestation_key: &[u8],
) -> Result<AuthenticatedOperation, GatewayError> {
    let envelope = operation_envelope(subject, WebApiMode::JetStreamAsync)?;
    let authorization = sign_operation_attestation(operation_attestation_key, &envelope)
        .map_err(|_| GatewayError::InvalidRequest)?;
    let request = AuthenticatedOperation {
        authorization,
        envelope,
    };
    request
        .validate(WebApiMode::JetStreamAsync)
        .map_err(|_| GatewayError::InvalidRequest)?;
    Ok(request)
}

fn operation_envelope(subject: &str, mode: WebApiMode) -> Result<OperationEnvelope, GatewayError> {
    validate_identifier(subject, 256)?;
    let now = now_unix_ms()?;
    let sequence = OPERATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let operation_id = format!("web-{now}-{sequence}");
    Ok(OperationEnvelope {
        version: 1,
        operation_id: operation_id.clone(),
        subject: subject.to_string(),
        resource: "youtube_status".to_string(),
        operation: DataOperation::Read,
        payload: json!({}),
        deadline_unix_ms: now + 10_000,
        dedupe_key: (mode == WebApiMode::JetStreamAsync).then_some(operation_id),
    })
}

fn validate_authorization(value: &str) -> Result<(), GatewayError> {
    let token = value
        .strip_prefix("Bearer ")
        .ok_or(GatewayError::Unauthorized)?;
    if value.len() > MAX_AUTHORIZATION_BYTES
        || token.is_empty()
        || token.trim() != token
        || token.chars().any(char::is_whitespace)
    {
        return Err(GatewayError::Unauthorized);
    }
    Ok(())
}

fn validate_identifier(value: &str, maximum: usize) -> Result<(), GatewayError> {
    if value.is_empty()
        || value.len() > maximum
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(GatewayError::InvalidRequest);
    }
    Ok(())
}

fn now_unix_ms() -> Result<u64, GatewayError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| GatewayError::InvalidRequest)
        .and_then(|duration| {
            u64::try_from(duration.as_millis()).map_err(|_| GatewayError::InvalidRequest)
        })
}

fn length_codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .length_field_length(4)
        .max_frame_length(MAX_TRANSPORT_BYTES)
        .new_codec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_envelope_is_strict_bounded_and_mode_scoped() {
        let tcp = bearer_operation("actor-1", "Bearer synthetic", WebApiMode::StatefulMtlsTcp)
            .expect("TCP operation");
        assert!(tcp.envelope.dedupe_key.is_none());
        let jetstream =
            attested_operation("actor-1", b"test-only-operation-attestation-key-32-bytes")
                .expect("JetStream operation");
        assert_eq!(
            jetstream.envelope.dedupe_key.as_deref(),
            Some(jetstream.envelope.operation_id.as_str())
        );
        let encoded = serde_json::to_vec(&jetstream).expect("json");
        assert!(encoded.len() <= MAX_TRANSPORT_BYTES);
        assert!(
            act_api_server::transport_runtime::verify_operation_attestation(
                b"test-only-operation-attestation-key-32-bytes",
                &jetstream.envelope,
                &jetstream.authorization,
            )
            .is_ok()
        );
        assert!(!encoded.windows(7).any(|window| window == b"Bearer "));
        assert!(
            !encoded
                .windows(b"synthetic".len())
                .any(|window| window == b"synthetic")
        );
    }

    #[test]
    fn authorization_and_identifier_validation_fail_closed() {
        assert_eq!(
            validate_authorization("Bearer token extra"),
            Err(GatewayError::Unauthorized)
        );
        assert_eq!(
            validate_identifier(" operation", 128),
            Err(GatewayError::InvalidRequest)
        );
    }
}
