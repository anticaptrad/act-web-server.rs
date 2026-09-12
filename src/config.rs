//! Strict environment-only configuration for the four web/API modes.

use anyhow::{Context, bail};
use reqwest::Url;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub service_name: String,
    pub shared_auth: Option<SharedAuthConfig>,
    pub direct_database: Option<DirectDatabaseConfig>,
    pub api_url: Option<Url>,
    pub mtls: Option<MtlsConfig>,
    pub nats_url: Option<String>,
    pub operation_attestation_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SharedAuthConfig {
    pub base_url: String,
    pub service_credential: String,
    pub audience: String,
}

#[derive(Debug, Clone)]
pub struct DirectDatabaseConfig {
    pub url: String,
    pub expected_role: String,
}

#[derive(Debug, Clone)]
pub struct MtlsConfig {
    pub address: String,
    pub server_name: String,
    pub client_certificate_file: String,
    pub client_private_key_file: String,
    pub server_ca_file: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let port = parse_env("PORT", 8080_u16)?;
        let service_name =
            env_non_empty("OTEL_SERVICE_NAME").unwrap_or_else(|| "act-web-server".to_string());

        let shared_auth_url = env_non_empty("SHARED_AUTH_URL");
        let shared_auth_service_credential = env_non_empty("SHARED_AUTH_SERVICE_CREDENTIAL");
        let shared_auth = match (shared_auth_url, shared_auth_service_credential) {
            (None, None) => None,
            (Some(base_url), Some(service_credential)) => {
                if service_credential.len() < 16 || service_credential.chars().any(char::is_control)
                {
                    bail!("SHARED_AUTH_SERVICE_CREDENTIAL is not a valid service bearer");
                }
                Some(SharedAuthConfig {
                    base_url,
                    service_credential,
                    // The web tier is a BFF for the API resource and forwards
                    // this same delegated product bearer on synchronous modes.
                    audience: "act-api".to_string(),
                })
            }
            _ => bail!("SHARED_AUTH_URL and SHARED_AUTH_SERVICE_CREDENTIAL are required together"),
        };

        let direct_url =
            env_non_empty("ACT_READONLY_DATABASE_URL").or_else(|| env_non_empty("DATABASE_URL"));
        let direct_role = env_non_empty("ACT_READONLY_DATABASE_ROLE");
        let direct_database = match (direct_url, direct_role) {
            (None, None) => None,
            (Some(url), Some(expected_role)) if expected_role.ends_with("_web_ro") => {
                Some(DirectDatabaseConfig { url, expected_role })
            }
            (Some(_), Some(_)) => bail!("ACT_READONLY_DATABASE_ROLE must end with _web_ro"),
            _ => {
                bail!("read-only database URL and ACT_READONLY_DATABASE_ROLE are required together")
            }
        };

        let api_url = env_non_empty("ACT_API_URL")
            .map(|value| validate_https_or_loopback(&value, "ACT_API_URL"))
            .transpose()?;
        let mtls = parse_mtls_config()?;
        let nats_url = env_non_empty("ACT_NATS_URL");
        if let Some(url) = nats_url.as_deref() {
            validate_nats_url(url)?;
        }
        let operation_attestation_key = env_non_empty("ACT_NATS_OPERATION_HMAC_KEY");
        validate_async_configuration(
            nats_url.is_some(),
            operation_attestation_key.as_deref(),
            api_url.is_some(),
        )?;
        if (api_url.is_some() || mtls.is_some() || nats_url.is_some()) && shared_auth.is_none() {
            bail!("Shared Auth is required whenever an API transport is configured");
        }

        Ok(Self {
            port,
            service_name,
            shared_auth,
            direct_database,
            api_url,
            mtls,
            nats_url,
            operation_attestation_key,
        })
    }
}

fn parse_mtls_config() -> anyhow::Result<Option<MtlsConfig>> {
    let values = [
        env_non_empty("ACT_API_MTLS_ADDR"),
        env_non_empty("ACT_API_TLS_SERVER_NAME"),
        env_non_empty("ACT_WEB_CLIENT_CERT_FILE"),
        env_non_empty("ACT_WEB_CLIENT_KEY_FILE"),
        env_non_empty("ACT_API_CA_FILE"),
    ];
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    let [
        Some(address),
        Some(server_name),
        Some(client_certificate_file),
        Some(client_private_key_file),
        Some(server_ca_file),
    ] = values
    else {
        bail!("all ACT_API_MTLS_* and ACT_* TLS file variables are required together")
    };
    address
        .parse::<std::net::SocketAddr>()
        .context("ACT_API_MTLS_ADDR must be an IP socket address")?;
    if server_name.is_empty() || server_name.chars().any(char::is_whitespace) {
        bail!("ACT_API_TLS_SERVER_NAME is invalid");
    }
    Ok(Some(MtlsConfig {
        address,
        server_name,
        client_certificate_file,
        client_private_key_file,
        server_ca_file,
    }))
}

fn validate_https_or_loopback(value: &str, name: &str) -> anyhow::Result<Url> {
    let url = Url::parse(value).with_context(|| format!("{name} is not a valid URL"))?;
    let loopback =
        url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if (url.scheme() != "https" && !loopback)
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        bail!("{name} requires HTTPS outside explicit loopback development");
    }
    Ok(url)
}

fn validate_nats_url(value: &str) -> anyhow::Result<()> {
    let allowed = value.starts_with("tls://")
        || value.starts_with("nats://127.0.0.1")
        || value.starts_with("nats://localhost")
        || value.starts_with("nats://[::1]");
    if !allowed {
        bail!("ACT_NATS_URL requires TLS outside explicit loopback development");
    }
    Ok(())
}

fn validate_async_configuration(
    nats_enabled: bool,
    attestation_key: Option<&str>,
    api_enabled: bool,
) -> anyhow::Result<()> {
    if nats_enabled && !api_enabled {
        bail!("ACT_API_URL is required with ACT_NATS_URL for owner-scoped operation status");
    }
    match (nats_enabled, attestation_key) {
        (false, None) => Ok(()),
        (true, None) => bail!("ACT_NATS_OPERATION_HMAC_KEY is required with ACT_NATS_URL"),
        (false, Some(_)) => bail!("ACT_NATS_URL is required with ACT_NATS_OPERATION_HMAC_KEY"),
        (true, Some(key)) => {
            if key.len() < 32 || key.chars().any(char::is_control) {
                bail!("ACT_NATS_OPERATION_HMAC_KEY must contain at least 32 non-control bytes");
            }
            Ok(())
        }
    }
}

fn env_non_empty(name: &str) -> Option<String> {
    crate::flags::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_env<T>(name: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match env_non_empty(name) {
        Some(value) => value
            .parse::<T>()
            .with_context(|| format!("invalid value for {name}")),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_async_configuration, validate_https_or_loopback, validate_nats_url};

    #[test]
    fn remote_cleartext_transports_are_rejected() {
        assert!(validate_https_or_loopback("https://api.example.test", "API").is_ok());
        assert!(validate_https_or_loopback("http://localhost:8081", "API").is_ok());
        assert!(validate_https_or_loopback("http://api.example.test", "API").is_err());
        assert!(validate_https_or_loopback("https://api.example.test/prefix", "API").is_err());
        assert!(validate_nats_url("tls://nats.example.test:4222").is_ok());
        assert!(validate_nats_url("nats://nats.example.test:4222").is_err());
    }

    #[test]
    fn async_mode_requires_a_separate_strong_attestation_key() {
        assert!(validate_async_configuration(false, None, false).is_ok());
        assert!(validate_async_configuration(true, None, true).is_err());
        assert!(validate_async_configuration(false, Some(&"k".repeat(32)), true).is_err());
        assert!(validate_async_configuration(true, Some("too-short"), true).is_err());
        assert!(validate_async_configuration(true, Some(&"k".repeat(32)), false).is_err());
        assert!(validate_async_configuration(true, Some(&"k".repeat(32)), true).is_ok());
    }
}
