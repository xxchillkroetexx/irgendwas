//! MySQL-backed `tower_sessions::SessionStore` on our own sqlx pool.
//!
//! Table `sessions` is created by migration `0002_sessions.sql`. (The published
//! `tower-sessions-sqlx-store` is built against sqlx 0.8 / tower-sessions-core 0.14 and
//! creates its own schema, which needs CREATE SCHEMA rights.)
//!
//! # Concurrent requests (#71)
//!
//! The session layer runs with `always_save`: at the end of every request that carries a
//! session cookie, tower-sessions hands the record it loaded at the start back to
//! [`SessionStore::save`] (records it never loaded or that it created in this request
//! go to [`SessionStore::create`]). Requests of one browser overlap all the time (a page's
//! resources, `session_info` after an action), so a plain "write the whole record" would
//! let a slow request
//!
//! - re-insert a session that a parallel logout deleted (the old cookie authenticated
//!   again), and
//! - write its stale snapshot over a parallel change (a taken flash message came back, a
//!   share draft or locale change got lost).
//!
//! Hence:
//!
//! - `create` is a plain `INSERT` under a freshly drawn id (another one on a collision),
//!   never the id handed in; `save` never inserts. A deleted (logged out, flushed, expired
//!   and cleaned up) id stays deleted.
//! - `load` remembers the loaded data under [`LOADED_KEY`] inside the record. That entry
//!   never reaches the database: every write strips it.
//! - `save` of an unchanged record only renews `expiry_date` (read-only requests cannot
//!   revert anything). A changed record is merged per key under a row lock: only the keys
//!   this request set or removed (compared with what it loaded) are applied to the row's
//!   current data. A key changed by two requests at once keeps the later write.

use std::collections::HashMap;

use app::server::db::{self, IsolationLevel};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{MySqlConnection, MySqlPool};
use time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::{self, SessionStore};

/// Record data key holding the data as loaded from the store (a JSON object). Only this
/// store reads it; it is removed before anything is written.
const LOADED_KEY: &str = "__irgendwas_session_store_loaded";

/// How many fresh ids `create` tries (2^128 ids: a collision in a row means something else
/// is wrong).
const CREATE_ATTEMPTS: usize = 4;

type Data = HashMap<String, Value>;

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

    /// Renews the expiry of an existing row; a missing row stays missing.
    async fn touch(&self, record: &Record) -> session_store::Result<()> {
        sqlx::query("UPDATE sessions SET expiry_date = ? WHERE id = ?")
            .bind(to_chrono(record.expiry_date))
            .bind(record.id.to_string())
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    /// Writes `data` (and the expiry) into the existing row `record.id`, merged with the
    /// row's current data as described in the module docs. `loaded == None` (the record
    /// was not loaded from the store, e.g. `Session::clear` before any read) replaces the
    /// data. A missing row stays missing.
    async fn update(
        &self,
        record: &Record,
        data: Data,
        loaded: Option<Data>,
    ) -> session_store::Result<()> {
        let mut conn = self.pool.acquire().await.map_err(backend)?;
        // READ COMMITTED: the locking read of a missing row takes no gap lock.
        let mut tx = db::begin_with(&mut conn, IsolationLevel::ReadCommitted)
            .await
            .map_err(backend)?;
        let current: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT data FROM sessions WHERE id = ? FOR UPDATE")
                .bind(record.id.to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(backend)?;
        let Some(current) = current else {
            // Logged out, flushed or expired and cleaned up meanwhile: keep it deleted.
            return tx.rollback().await.map_err(backend);
        };
        let data = match loaded {
            Some(loaded) => {
                let mut merged = match serde_json::from_slice::<Record>(&current) {
                    Ok(current) => strip(current.data),
                    Err(err) => {
                        tracing::warn!(error = %err, "unreadable stored session, replacing it");
                        Data::new()
                    }
                };
                apply_changes(&mut merged, &loaded, data);
                merged
            }
            None => data,
        };
        let stored = Record {
            id: record.id,
            data,
            expiry_date: record.expiry_date,
        };
        sqlx::query("UPDATE sessions SET data = ?, expiry_date = ? WHERE id = ?")
            .bind(encode(&stored)?)
            .bind(to_chrono(stored.expiry_date))
            .bind(stored.id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)
    }

    async fn insert(
        conn: &mut MySqlConnection,
        record: &Record,
        data: Vec<u8>,
    ) -> sqlx::Result<()> {
        sqlx::query("INSERT INTO sessions (id, data, expiry_date) VALUES (?, ?, ?)")
            .bind(record.id.to_string())
            .bind(data)
            .bind(to_chrono(record.expiry_date))
            .execute(conn)
            .await?;
        Ok(())
    }
}

/// Applies to `current` every key whose value in `ours` differs from `loaded`: set when
/// `ours` has it, removed when it does not.
fn apply_changes(current: &mut Data, loaded: &Data, mut ours: Data) {
    for key in loaded.keys() {
        if !ours.contains_key(key) {
            current.remove(key);
        }
    }
    for (key, value) in ours.drain() {
        if loaded.get(&key) != Some(&value) {
            current.insert(key, value);
        }
    }
}

/// `data` without the [`LOADED_KEY`] entry.
fn strip(mut data: Data) -> Data {
    data.remove(LOADED_KEY);
    data
}

/// Splits a record's data into the application's data and the snapshot taken at load.
fn split(record: &Record) -> (Data, Option<Data>) {
    let mut data = record.data.clone();
    let loaded = match data.remove(LOADED_KEY) {
        Some(Value::Object(map)) => Some(map.into_iter().collect()),
        _ => None,
    };
    (data, loaded)
}

fn encode(record: &Record) -> session_store::Result<Vec<u8>> {
    serde_json::to_vec(record).map_err(|e| session_store::Error::Encode(e.to_string()))
}

/// The store error for `err`, redacted ([`db::redacted`]): tower-sessions logs it, and a
/// database message can quote values such as the session id of a duplicate key.
fn backend(err: sqlx::Error) -> session_store::Error {
    session_store::Error::Backend(db::redacted(&err))
}

fn is_duplicate_key(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

fn to_chrono(t: OffsetDateTime) -> DateTime<Utc> {
    DateTime::from_timestamp(t.unix_timestamp(), t.nanosecond()).unwrap_or(DateTime::<Utc>::MIN_UTC)
}

/// Stores `data` as the loaded snapshot in `record`.
fn remember_loaded(record: &mut Record, data: Data) {
    record.data.insert(
        LOADED_KEY.to_string(),
        Value::Object(data.into_iter().collect()),
    );
}

#[async_trait]
impl SessionStore for MySqlSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        // A cycled id still carries the snapshot of the old id's data.
        let data = strip(std::mem::take(&mut record.data));
        // Always a fresh id, never the one handed in: after `Session::flush` the record
        // still carries the id that was just deleted (a stale login cleared by `load_user`),
        // and inserting it would bring the revoked cookie's session back. tower-sessions
        // takes the session id from `record.id` after `create`.
        let mut stored = Record {
            id: Id::default(),
            data,
            expiry_date: record.expiry_date,
        };
        let mut conn = self.pool.acquire().await.map_err(backend)?;
        let mut attempt = 1;
        loop {
            let bytes = encode(&stored)?;
            match Self::insert(&mut conn, &stored, bytes).await {
                Ok(()) => break,
                Err(err) if is_duplicate_key(&err) && attempt < CREATE_ATTEMPTS => {
                    attempt += 1;
                    stored.id = Id::default();
                }
                Err(err) => {
                    // Leave the caller's record as it was handed in (minus the snapshot).
                    record.data = stored.data;
                    return Err(backend(err));
                }
            }
        }
        record.id = stored.id;
        record.data = stored.data.clone();
        // Later saves in this request merge against what was just written.
        remember_loaded(record, stored.data);
        Ok(())
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let (data, loaded) = split(record);
        if loaded.as_ref() == Some(&data) {
            // Nothing changed in this request: renew the expiry, never rewrite the data.
            self.touch(record).await
        } else {
            self.update(record, data, loaded).await
        }
    }

    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        let data: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT data FROM sessions WHERE id = ? AND expiry_date > ?")
                .bind(id.to_string())
                .bind(Utc::now())
                .fetch_optional(&self.pool)
                .await
                .map_err(backend)?;
        let Some(bytes) = data else {
            return Ok(None);
        };
        let mut record: Record = serde_json::from_slice(&bytes)
            .map_err(|e| session_store::Error::Decode(e.to_string()))?;
        record.data = strip(std::mem::take(&mut record.data));
        let loaded = record.data.clone();
        remember_loaded(&mut record, loaded);
        Ok(Some(record))
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
