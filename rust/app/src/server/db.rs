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

/// A description of `err` that is safe to log: the error kind and, for database errors,
/// the SQLSTATE and server error number. Never the message, which can quote values
/// (e.g. `Duplicate entry 'alice@example.com' for key 'email'`).
pub fn redacted(err: &sqlx::Error) -> String {
    match err {
        sqlx::Error::Database(db) => {
            let number = db
                .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                .map(|e| e.number().to_string())
                .unwrap_or_else(|| "?".to_string());
            format!(
                "database error (sqlstate {}, number {number}, kind {:?})",
                db.code().as_deref().unwrap_or("?"),
                db.kind()
            )
        }
        sqlx::Error::RowNotFound => "row not found".to_string(),
        sqlx::Error::PoolTimedOut => "pool timed out".to_string(),
        sqlx::Error::PoolClosed => "pool closed".to_string(),
        sqlx::Error::Io(e) => format!("io error ({:?})", e.kind()),
        sqlx::Error::Tls(_) => "tls error".to_string(),
        sqlx::Error::Protocol(_) => "protocol error".to_string(),
        sqlx::Error::ColumnNotFound(_) | sqlx::Error::ColumnIndexOutOfBounds { .. } => {
            "column error".to_string()
        }
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_) => "decode error".to_string(),
        _ => "other database error".to_string(),
    }
}
