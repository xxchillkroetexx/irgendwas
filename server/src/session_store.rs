//! MySQL-backed `tower_sessions::SessionStore` on our own sqlx pool.
//!
//! Table `sessions` is created by migration `0002_sessions.sql`. (The published
//! `tower-sessions-sqlx-store` is built against sqlx 0.8 / tower-sessions-core 0.14 and
//! creates its own schema, which needs CREATE SCHEMA rights.)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{MySqlConnection, MySqlPool};
use time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::{self, SessionStore};

#[derive(Debug, Clone)]
pub struct MySqlSessionStore {
    pool: MySqlPool,
}

impl MySqlSessionStore {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    /// Deletes expired sessions. Returns how many were removed.
    pub async fn delete_expired(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM sessions WHERE expiry_date < ?")
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Runs [`delete_expired`](Self::delete_expired) every `period` in the background.
    pub fn spawn_cleanup(self, period: std::time::Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(period);
            loop {
                interval.tick().await;
                match self.delete_expired().await {
                    Ok(0) => {}
                    Ok(n) => tracing::debug!(removed = n, "expired sessions deleted"),
                    Err(err) => tracing::warn!(error = %err, "deleting expired sessions failed"),
                }
            }
        })
    }

    async fn id_exists(conn: &mut MySqlConnection, id: &Id) -> session_store::Result<bool> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?)")
            .bind(id.to_string())
            .fetch_one(conn)
            .await
            .map_err(backend)
    }

    async fn save_with(conn: &mut MySqlConnection, record: &Record) -> session_store::Result<()> {
        let data =
            serde_json::to_vec(record).map_err(|e| session_store::Error::Encode(e.to_string()))?;
        sqlx::query(
            "INSERT INTO sessions (id, data, expiry_date) VALUES (?, ?, ?) \
             ON DUPLICATE KEY UPDATE data = VALUES(data), expiry_date = VALUES(expiry_date)",
        )
        .bind(record.id.to_string())
        .bind(data)
        .bind(to_chrono(record.expiry_date))
        .execute(conn)
        .await
        .map_err(backend)?;
        Ok(())
    }
}

fn backend(err: sqlx::Error) -> session_store::Error {
    session_store::Error::Backend(err.to_string())
}

fn to_chrono(t: OffsetDateTime) -> DateTime<Utc> {
    DateTime::from_timestamp(t.unix_timestamp(), t.nanosecond()).unwrap_or(DateTime::<Utc>::MIN_UTC)
}

#[async_trait]
impl SessionStore for MySqlSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        while Self::id_exists(&mut tx, &record.id).await? {
            record.id = Id::default();
        }
        Self::save_with(&mut tx, record).await?;
        tx.commit().await.map_err(backend)
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let mut conn = self.pool.acquire().await.map_err(backend)?;
        Self::save_with(&mut conn, record).await
    }

    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        let data: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT data FROM sessions WHERE id = ? AND expiry_date > ?")
                .bind(id.to_string())
                .bind(Utc::now())
                .fetch_optional(&self.pool)
                .await
                .map_err(backend)?;
        data.map(|bytes| {
            serde_json::from_slice(&bytes).map_err(|e| session_store::Error::Decode(e.to_string()))
        })
        .transpose()
    }

    async fn delete(&self, id: &Id) -> session_store::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }
}
