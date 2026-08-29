//! SeaORM connections with separate read-only and durable-operation lanes.

use std::time::Duration;

use sea_orm::{
    AccessMode, ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend,
    Statement, TransactionTrait,
};
use serde::Serialize;
use serde_json::Value;

use crate::config::DirectDatabaseConfig;

const DIRECT_READ_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_DIRECT_ROWS: usize = 100;
const READ_EVENTS_SQL: &str = "SELECT id::text AS id, payload::text AS payload_json, created_at::text AS created_at FROM events WHERE subject = $1 ORDER BY created_at DESC LIMIT 100";

#[derive(Clone)]
pub struct DirectReadStore {
    database: DatabaseConnection,
    expected_role: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventProjection {
    pub id: String,
    pub payload: Value,
    pub created_at: String,
}

impl DirectReadStore {
    pub async fn connect(config: &DirectDatabaseConfig) -> anyhow::Result<Self> {
        let database = connect(&config.url).await?;
        let role = database
            .query_one(Statement::from_string(
                DbBackend::Postgres,
                "SELECT current_user AS role_name",
            ))
            .await?
            .ok_or_else(|| anyhow::anyhow!("database role query returned no row"))?
            .try_get::<String>("", "role_name")?;
        if role != config.expected_role || !role.ends_with("_web_ro") {
            anyhow::bail!("database connection is not using the reviewed web read-only role");
        }
        Ok(Self {
            database,
            expected_role: config.expected_role.clone(),
        })
    }

    pub async fn events_for_subject(&self, subject: &str) -> anyhow::Result<Vec<EventProjection>> {
        if subject.is_empty()
            || subject.len() > 256
            || subject.trim() != subject
            || subject.chars().any(char::is_control)
        {
            anyhow::bail!("invalid actor subject");
        }
        if !self.expected_role.ends_with("_web_ro") {
            anyhow::bail!("read-only role proof is invalid");
        }
        let transaction = self
            .database
            .begin_with_config(None, Some(AccessMode::ReadOnly))
            .await?;
        let rows = tokio::time::timeout(
            DIRECT_READ_TIMEOUT,
            transaction.query_all(Statement::from_sql_and_values(
                DbBackend::Postgres,
                READ_EVENTS_SQL,
                vec![subject.into()],
            )),
        )
        .await
        .map_err(|_| anyhow::anyhow!("direct read timed out"))??;
        if rows.len() > MAX_DIRECT_ROWS {
            anyhow::bail!("direct read exceeded the reviewed row bound");
        }
        let projections = rows
            .into_iter()
            .map(|row| {
                Ok(EventProjection {
                    id: row.try_get("", "id")?,
                    payload: serde_json::from_str(&row.try_get::<String>("", "payload_json")?)
                        .map_err(|error| sea_orm::DbErr::Json(error.to_string()))?,
                    created_at: row.try_get("", "created_at")?,
                })
            })
            .collect::<Result<Vec<_>, sea_orm::DbErr>>()?;
        transaction.commit().await?;
        Ok(projections)
    }
}

async fn connect(url: &str) -> anyhow::Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(url.to_owned());
    options
        .max_connections(10)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(5))
        .sqlx_logging(false);
    Ok(Database::connect(options).await?)
}

#[cfg(test)]
mod tests {
    use super::READ_EVENTS_SQL;

    #[test]
    fn direct_query_is_fixed_actor_scoped_and_select_only() {
        let query = READ_EVENTS_SQL.to_ascii_uppercase();
        assert!(query.trim_start().starts_with("SELECT "));
        assert!(query.contains("WHERE SUBJECT = $1"));
        assert!(query.contains("LIMIT 100"));
        for prohibited in ["INSERT ", "UPDATE ", "DELETE ", "DROP ", "TRUNCATE "] {
            assert!(!query.contains(prohibited), "found {prohibited}");
        }
    }
}
