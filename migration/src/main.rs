use sea_orm_migration::prelude::*;

#[tokio::main]
async fn main() {
    // Reads DATABASE_URL from the environment and applies pending migrations.
    // Usage: `cargo run -p migration -- up` (or `down`, `status`, `fresh`).
    cli::run_cli(migration::Migrator).await;
}
