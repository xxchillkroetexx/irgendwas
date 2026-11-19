//! Database pool and migrations.
//!
//! Every connection runs with `time_zone='+00:00'`, so `TIMESTAMP` columns map 1:1 to
//! `chrono::DateTime<Utc>`. Production MariaDB runs in Europe/Berlin, but `TIMESTAMP`
//! values are stored as UTC internally, so existing data reads back correctly.
//!
//! Queries use the runtime API (`sqlx::query_as::<_, T>(..)`), not the compile-time
//! macros, so builds never need a database. Never use `SELECT *`: list the columns.

use std::time::Duration;

use sqlx::migrate::{MigrateError, Migrator};
use sqlx::mysql::{MySqlConnectOptions, MySqlConnection, MySqlPoolOptions};
use sqlx::pool::PoolConnection;
use sqlx::{MySql, MySqlPool, Transaction};

use super::config::DbConfig;

/// All migrations in `migrations/`, embedded at compile time.
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
        let result = pool_options().connect_with(connect_options(db)).await;
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

/// The pool settings [`connect`] uses: 10 connections, 5 s acquire timeout, and
/// [`roll_back_leftover_transaction`] on every connection that comes back to the pool.
pub fn pool_options() -> MySqlPoolOptions {
    MySqlPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .after_release(|conn, _meta| Box::pin(roll_back_leftover_transaction(conn)))
}

/// Ends a transaction the server still has open on a connection that is going back to
/// the pool (#99). A request dropped while its `BEGIN` was in flight leaves the server
/// inside a transaction that sqlx never recorded, so sqlx neither rolls it back nor
/// knows about it; the next request on that connection then failed with 1568 at its own
/// `SET TRANSACTION`, or read from the stale snapshot (1020). Transactions sqlx does know
/// about are already rolled back by the time this runs: their `ROLLBACK` is queued on
/// drop and sent before this query.
pub async fn roll_back_leftover_transaction(
    conn: &mut MySqlConnection,
) -> Result<bool, sqlx::Error> {
    let open: i64 = sqlx::query_scalar("SELECT CAST(@@in_transaction AS SIGNED)")
        .fetch_one(&mut *conn)
        .await?;
    if open != 0 {
        tracing::warn!("pooled connection came back inside a transaction; rolling it back");
        sqlx::raw_sql("ROLLBACK").execute(&mut *conn).await?;
    }
    Ok(true)
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

/// The isolation level of one transaction (see [`begin_with`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// Every statement reads what committed before it; searches take no gap locks.
    ReadCommitted,
    /// One snapshot for the whole transaction (taken at its first consistent read).
    RepeatableRead,
}

impl IsolationLevel {
    /// `SET TRANSACTION` and `START TRANSACTION` in one statement batch (see [`begin_with`]).
    fn begin_statement(self) -> &'static str {
        match self {
            IsolationLevel::ReadCommitted => {
                "SET TRANSACTION ISOLATION LEVEL READ COMMITTED; START TRANSACTION"
            }
            IsolationLevel::RepeatableRead => {
                "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ; START TRANSACTION"
            }
        }
    }
}

/// Starts a transaction with an explicit isolation level on `conn`. `SET TRANSACTION`
/// (without `SESSION`) only applies to the next transaction, so the pooled connection
/// keeps its default level afterwards. Both statements go to the server in one packet:
/// a request dropped in between can no longer leave the pending level on the connection
/// for its next transaction. A request dropped after the packet was sent can still leave
/// the server inside a transaction sqlx never recorded; the pool's
/// [`roll_back_leftover_transaction`] ends it when the connection is released (#99).
pub async fn begin_with(
    conn: &mut PoolConnection<MySql>,
    level: IsolationLevel,
) -> Result<Transaction<'_, MySql>, sqlx::Error> {
    sqlx::Connection::begin_with(&mut **conn, level.begin_statement()).await
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
