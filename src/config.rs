//! Runtime configuration sourced from the environment (no `.env` — `dotenv` is
//! blacklisted, see `agents.md`). Secrets are injected by the k8s deployment.

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub service_name: String,
    /// Postgres connection string (e.g. Supabase). Optional at boot.
    pub database_url: Option<String>,
    /// Supabase JWT signing secret (HS256). When absent, protected routes fail closed.
    pub supabase_jwt_secret: Option<String>,
    /// Expected `aud` claim; Supabase issues "authenticated" for signed-in users.
    pub supabase_jwt_aud: String,
    /// Expected `iss` claim. When set, tokens from another issuer are rejected.
    pub supabase_jwt_iss: Option<String>,
    /// Clock-skew tolerance for `exp`/`nbf`, in seconds.
    pub supabase_jwt_leeway_secs: u64,
}

impl Config {
    pub fn from_env() -> Self {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8080);

        let service_name =
            std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "act-web-server".to_string());

        let database_url = non_empty(std::env::var("DATABASE_URL").ok());
        let supabase_jwt_secret = non_empty(std::env::var("SUPABASE_JWT_SECRET").ok());
        let supabase_jwt_aud =
            std::env::var("SUPABASE_JWT_AUD").unwrap_or_else(|_| "authenticated".to_string());

        Self {
            port,
            service_name,
            database_url,
            supabase_jwt_secret,
            supabase_jwt_aud,
        }
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.is_empty())
}
