//! Postgres connectivity via sea-orm.
//!
//! Like the NATS bridge elsewhere in the platform, the database is treated as an
//! optional dependency at boot: an unreachable database logs a warning and the
//! service still serves health traffic instead of crash-looping the pod.

use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::time::Duration;

pub async fn connect(url: &str) -> Option<DatabaseConnection> {
    let mut opts = ConnectOptions::new(url.to_owned());
    opts.max_connections(10)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(5))
        .sqlx_logging(false);

    match Database::connect(opts).await {
        Ok(conn) => {
            tracing::info!("connected to Postgres");
            Some(conn)
        }
        Err(err) => {
            tracing::warn!(error = %err, "database unavailable; continuing without persistence");
            None
        }
    }
}
