//! MySQL-backed `tower_sessions::SessionStore` on our own sqlx pool.
//!
//! Table `sessions` is created by migration `0002_sessions.sql`. (The published
//! `tower-sessions-sqlx-store` is built against sqlx 0.8 / tower-sessions-core 0.14 and
//! creates its own schema, which needs CREATE SCHEMA rights.)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::MySqlPool;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::{self, SessionStore};

#[derive(Debug, Clone)]
pub struct MySqlSessionStore {
    pool: MySqlPool,
    /// Private to one HTTP request; store clones made inside that request share it.
    baselines: Arc<Mutex<HashMap<Id, Record>>>,
}

impl MySqlSessionStore {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            pool,
            baselines: Arc::new(Mutex::new(HashMap::new())),
        }
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
}

fn encode(record: &Record) -> session_store::Result<Vec<u8>> {
    serde_json::to_vec(record).map_err(|e| session_store::Error::Encode(e.to_string()))
}

fn backend(err: sqlx::Error) -> session_store::Error {
    session_store::Error::Backend(app::server::db::redacted(&err))
}

fn to_chrono(t: OffsetDateTime) -> DateTime<Utc> {
    DateTime::from_timestamp(t.unix_timestamp(), t.nanosecond()).unwrap_or(DateTime::<Utc>::MIN_UTC)
}

fn conflict() -> session_store::Error {
    session_store::Error::Backend("concurrent session update conflict".into())
}

/// Flash operations only append messages or take the complete queue. Rebase
/// additions and consumption separately so a concurrent reader cannot discard
/// new messages, and concurrent additions do not lose either message.
fn merge_flash(
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
    current: Option<&serde_json::Value>,
) -> session_store::Result<Option<serde_json::Value>> {
    use serde_json::Value;
    let base = match before {
        Some(Value::Array(values)) => values.as_slice(),
        None => &[],
        _ => return Err(conflict()),
    };
    let diff = |value: Option<&Value>, latest: bool| -> session_store::Result<(bool, Vec<Value>)> {
        match value {
            None if before.is_some() => Ok((true, Vec::new())),
            None => Ok((false, Vec::new())),
            Some(Value::Array(values)) if values.starts_with(base) => {
                Ok((false, values[base.len()..].to_vec()))
            }
            // Another request consumed the old queue and subsequently appended
            // fresh messages, all of which belong to the new queue.
            Some(Value::Array(values)) if latest => Ok((true, values.clone())),
            _ => Err(conflict()),
        }
    };
    let (ours_removed, ours_added) = diff(after, false)?;
    let (theirs_removed, theirs_added) = diff(current, true)?;
    if ours_removed && theirs_removed {
        return Err(conflict());
    }
    let mut result = if ours_removed || theirs_removed {
        Vec::new()
    } else {
        base.to_vec()
    };
    result.extend(theirs_added);
    result.extend(ours_added);
    Ok((!result.is_empty()).then_some(Value::Array(result)))
}

/// Share drafts have stable random IDs. Merge additions and removals for
/// different drafts rather than treating the entire list as a single value.
fn merge_drafts(
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
    current: Option<&serde_json::Value>,
    claimed: &HashSet<String>,
) -> session_store::Result<Option<serde_json::Value>> {
    use serde_json::Value;
    fn entries(value: Option<&Value>) -> session_store::Result<Vec<(String, Value)>> {
        let values = match value {
            Some(Value::Array(values)) => values.as_slice(),
            None => &[],
            _ => return Err(conflict()),
        };
        let mut seen = HashSet::new();
        let mut result = Vec::with_capacity(values.len());
        for value in values {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(conflict)?;
            if !seen.insert(id.to_owned()) {
                return Err(conflict());
            }
            result.push((id.to_owned(), value.clone()));
        }
        Ok(result)
    }
    let base = entries(before)?;
    let desired = entries(after)?;
    let mut latest = entries(current)?;
    // A successful wishlist transaction is authoritative even if the response
    // has not yet persisted its in-memory session snapshot.
    latest.retain(|(id, _)| !claimed.contains(id));
    for (id, old) in &base {
        if claimed.contains(id) {
            continue;
        }
        let wanted = desired.iter().find(|(key, _)| key == id).map(|(_, v)| v);
        if wanted == Some(old) {
            continue;
        }
        let existing = latest.iter().position(|(key, _)| key == id);
        match (wanted, existing) {
            (None, None) => (), // Both removed the same draft.
            (None, Some(pos)) if latest[pos].1 == *old => {
                latest.remove(pos);
            }
            (Some(value), Some(pos)) if latest[pos].1 == *old || latest[pos].1 == *value => {
                latest[pos].1 = value.clone();
            }
            _ => return Err(conflict()),
        }
    }
    for (id, value) in &desired {
        if claimed.contains(id) {
            continue;
        }
        if base.iter().any(|(key, _)| key == id) {
            continue;
        }
        match latest.iter().find(|(key, _)| key == id) {
            Some((_, existing)) if existing != value => return Err(conflict()),
            Some(_) => (),
            None => latest.push((id.clone(), value.clone())),
        }
    }
    // Concurrent additions must obey the same oldest-first bound as normal intake.
    if latest.len() > app::server::share::MAX_PENDING {
        latest.drain(..latest.len() - app::server::share::MAX_PENDING);
    }
    Ok(Some(Value::Array(
        latest.into_iter().map(|(_, value)| value).collect(),
    )))
}

/// Apply only changes made by this request to the latest committed snapshot.
/// A conflicting same-key change is rejected instead of reporting success while
/// discarding a concurrent write. In particular, an unchanged read can never
/// restore an old flash, locale, or share draft.
fn merge(
    base: &Record,
    desired: &Record,
    mut latest: Record,
    claimed_drafts: &HashSet<String>,
) -> session_store::Result<Record> {
    let keys: HashSet<&String> = base.data.keys().chain(desired.data.keys()).collect();
    for key in keys {
        let before = base.data.get(key);
        let after = desired.data.get(key);
        if key == app::server::share::DRAFTS_KEY && !claimed_drafts.is_empty() {
            match merge_drafts(before, after, latest.data.get(key), claimed_drafts)? {
                Some(value) => latest.data.insert(key.clone(), value),
                None => latest.data.remove(key),
            };
            continue;
        }
        if before == after {
            continue;
        }
        let current = latest.data.get(key);
        if key == app::server::flash::FLASH_KEY && current != before {
            match merge_flash(before, after, current)? {
                Some(value) => latest.data.insert(key.clone(), value),
                None => latest.data.remove(key),
            };
            continue;
        }
        if key == app::server::share::DRAFTS_KEY && current != before {
            match merge_drafts(before, after, current, claimed_drafts)? {
                Some(value) => latest.data.insert(key.clone(), value),
                None => latest.data.remove(key),
            };
            continue;
        }
        if current != before {
            // Removing the same pending value twice (notably flash messages)
            // must not return the value successfully to two consumers.
            if current != after || after.is_none() {
                return Err(conflict());
            }
        }
        if let Some(value) = after {
            latest.data.insert(key.clone(), value.clone());
        } else {
            latest.data.remove(key);
        }
    }
    // Preserve a newer rolling expiry set by another request, while allowing an
    // uncontended explicit expiry change (including shortening) to take effect.
    latest.expiry_date = if latest.expiry_date == base.expiry_date {
        desired.expiry_date
    } else {
        latest.expiry_date.max(desired.expiry_date)
    };
    Ok(latest)
}

#[async_trait]
impl SessionStore for MySqlSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        // flush() clears the session ID but retains the old ID in its cached
        // Record. Never let a subsequent write revive the revoked cookie.
        record.id = Id::default();
        loop {
            let result =
                sqlx::query("INSERT INTO sessions (id, data, expiry_date) VALUES (?, ?, ?)")
                    .bind(record.id.to_string())
                    .bind(encode(record)?)
                    .bind(to_chrono(record.expiry_date))
                    .execute(&self.pool)
                    .await;
            match result {
                Ok(_) => {
                    self.baselines
                        .lock()
                        .unwrap()
                        .insert(record.id, record.clone());
                    return Ok(());
                }
                Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
                    record.id = Id::default();
                }
                Err(err) => return Err(backend(err)),
            }
        }
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let base = self
            .baselines
            .lock()
            .unwrap()
            .get(&record.id)
            .cloned()
            .ok_or_else(conflict)?;
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let bytes: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT data FROM sessions WHERE id = ? AND expiry_date > ? FOR UPDATE",
        )
        .bind(record.id.to_string())
        .bind(Utc::now())
        .fetch_optional(&mut *tx)
        .await
        .map_err(backend)?;
        let bytes = bytes
            .ok_or_else(|| session_store::Error::Backend("session no longer exists".into()))?;
        let latest: Record = serde_json::from_slice(&bytes)
            .map_err(|e| session_store::Error::Decode(e.to_string()))?;
        let mut ids = HashSet::new();
        for snapshot in [&base, record, &latest] {
            if let Some(value) = snapshot.data.get(app::server::share::DRAFTS_KEY) {
                let values = value.as_array().ok_or_else(conflict)?;
                for value in values {
                    ids.insert(
                        value
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .ok_or_else(conflict)?
                            .to_owned(),
                    );
                }
            }
        }
        let mut claimed = HashSet::new();
        for id in ids {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM consumed_share_drafts WHERE draft_hash = ?)",
            )
            .bind(app::server::token::hash(&id))
            .fetch_one(&mut *tx)
            .await
            .map_err(backend)?;
            if exists {
                claimed.insert(id);
            }
        }
        let merged = merge(&base, record, latest, &claimed)?;
        sqlx::query("UPDATE sessions SET data = ?, expiry_date = ? WHERE id = ?")
            .bind(encode(&merged)?)
            .bind(to_chrono(merged.expiry_date))
            .bind(record.id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        self.baselines.lock().unwrap().insert(record.id, merged);
        Ok(())
    }

    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        let data: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT data FROM sessions WHERE id = ? AND expiry_date > ?")
                .bind(id.to_string())
                .bind(Utc::now())
                .fetch_optional(&self.pool)
                .await
                .map_err(backend)?;
        let record: Option<Record> = data
            .map(|bytes| {
                serde_json::from_slice(&bytes)
                    .map_err(|e| session_store::Error::Decode(e.to_string()))
            })
            .transpose()?;
        if let Some(record) = &record {
            self.baselines.lock().unwrap().insert(*id, record.clone());
        } else {
            self.baselines.lock().unwrap().remove(id);
        }
        Ok(record)
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
