//! Shared application state passed to handlers and middleware.

use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct AppState {
    /// Postgres pool. `None` when `DATABASE_URL` is unset or unreachable at boot.
    pub db: Option<DatabaseConnection>,
    /// Supabase HS256 signing secret. `None` disables protected routes (fail closed).
    pub jwt_secret: Option<String>,
    /// Expected JWT audience claim.
    pub jwt_aud: String,
}
