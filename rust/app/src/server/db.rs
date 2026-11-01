//! Database pool and migrations.
//!
//! Every connection runs with `time_zone='+00:00'`, so `TIMESTAMP` columns map 1:1 to
//! `chrono::DateTime<Utc>`. Production MariaDB runs in Europe/Berlin, but `TIMESTAMP`
//! values are stored as UTC internally, so existing data reads back correctly.
//!
//! Queries use the runtime API (`sqlx::query_as::<_, T>(..)`), not the compile-time
//! macros, so builds never need a database. Never use `SELECT *`: list the columns.

use std::time::Duration;

use sqlx::MySqlPool;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};

use super::config::DbConfig;

/// All migrations in `rust/migrations`, embedded at compile time.
pub static MIGRATOR: Migrator = sqlx::migrate!("../migrations");

/// Connection options for `db`, pinned to UTC and utf8mb4.
pub fn connect_options(db: &DbConfig) -> MySqlConnectOptions {
    MySqlConnectOptions::new()
        .host(&db.host)
        .port(db.port)
        .username(&db.username)
        .password(&db.password)
        .database(&db.database)
        .charset("utf8mb4")
        .timezone(Some(String::from("+00:00")))
}

/// Opens the pool. Retries for up to `wait` while the database is starting.
pub async fn connect(db: &DbConfig, wait: Duration) -> Result<MySqlPool, sqlx::Error> {
    let deadline = tokio::time::Instant::now() + wait;
    let mut delay = Duration::from_millis(250);
    loop {
        let result = MySqlPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(connect_options(db))
            .await;
        match result {
            Ok(pool) => return Ok(pool),
            Err(err) if tokio::time::Instant::now() + delay < deadline => {
                tracing::warn!(error = %err, "database not reachable yet, retrying");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(5));
            }
            Err(err) => return Err(err),
        }
    }
}

/// Applies all pending migrations.
pub async fn migrate(pool: &MySqlPool) -> Result<(), MigrateError> {
    MIGRATOR.run(pool).await
}

/// Cheap liveness probe used by `GET /health`.
pub async fn ping(pool: &MySqlPool) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .map(|_| ())
}
