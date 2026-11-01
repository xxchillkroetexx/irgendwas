//! In-memory rate limiter with an injectable clock (spec: auth, "Rate limiting").
//!
//! Fixed windows per key: the first hit opens a window of [`Limit::window`]; up to
//! [`Limit::max`] hits count inside it; the window closes on its own. Counters live in this
//! process only (lost on restart, fine for a single container). Expired entries are purged
//! opportunistically. Tests swap `AppState::rate_limiter` for
//! `RateLimiter::new(Arc::new(ManualClock))` to move time.

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, TimeDelta, Utc};

/// Source of "now" for the limiter windows.
pub trait Clock: Send + Sync + Debug {
    fn now(&self) -> DateTime<Utc>;
}

/// The real clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// At most `max` counted events per fixed `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub max: u32,
    pub window: TimeDelta,
}

/// Failed logins per normalised email (`login:email:<email>`): 10 per 15 minutes.
pub const LOGIN_PER_EMAIL: Limit = Limit {
    max: 10,
    window: TimeDelta::minutes(15),
};
/// Failed logins per client IP (`login:ip:<ip>`): 50 per 15 minutes.
pub const LOGIN_PER_IP: Limit = Limit {
    max: 50,
    window: TimeDelta::minutes(15),
};
/// Mails per address from register + forgot-password (`mail:email:<email>`): 3 per hour.
pub const MAIL_PER_EMAIL: Limit = Limit {
    max: 3,
    window: TimeDelta::minutes(60),
};
/// Register + forgot-password calls per client IP (`mail:ip:<ip>`): 20 per hour.
pub const MAIL_PER_IP: Limit = Limit {
    max: 20,
    window: TimeDelta::minutes(60),
};

/// Current-password checks in change_password + request_email_change
/// (`account:pw:<user_id>`): 10 per 15 minutes (spec: account AC40).
pub const ACCOUNT_PASSWORD: Limit = Limit {
    max: 10,
    window: TimeDelta::minutes(15),
};

/// Purge expired entries once the map holds this many keys.
const PURGE_THRESHOLD: usize = 1024;

#[derive(Debug, Clone, Copy)]
struct Entry {
    count: u32,
    window_end: DateTime<Utc>,
}

/// Key -> (count, window end), behind a mutex.
#[derive(Debug)]
pub struct RateLimiter {
    clock: Arc<dyn Clock>,
    entries: Mutex<HashMap<String, Entry>>,
}

impl RateLimiter {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// A limiter on the system clock.
    pub fn system() -> Self {
        Self::new(Arc::new(SystemClock))
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        // A panic while holding the lock leaves only counters behind: keep using them.
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The time left in the current window if `key` has used up `limit`, else `None`.
    pub fn blocked(&self, key: &str, limit: Limit) -> Option<TimeDelta> {
        let now = self.clock.now();
        let entries = self.lock();
        let entry = entries.get(key)?;
        (entry.window_end > now && entry.count >= limit.max).then(|| entry.window_end - now)
    }

    /// Counts one event for `key` (opening a new window if none is running).
    pub fn record(&self, key: &str, limit: Limit) {
        let now = self.clock.now();
        let mut entries = self.lock();
        Self::bump(&mut entries, key, limit, now);
    }

    /// Counts one event for `key` if `limit` allows it; `false` (nothing counted) if the
    /// limit is used up.
    pub fn try_acquire(&self, key: &str, limit: Limit) -> bool {
        let now = self.clock.now();
        let mut entries = self.lock();
        if let Some(entry) = entries.get(key)
            && entry.window_end > now
            && entry.count >= limit.max
        {
            return false;
        }
        Self::bump(&mut entries, key, limit, now);
        true
    }

    /// Atomically counts one event for every key, unless one of them has used up its limit:
    /// then nothing is counted and the longest remaining window is returned. Lets a caller
    /// reserve an attempt *before* doing the expensive check (no race between check and
    /// count under concurrency).
    pub fn try_acquire_all(&self, keys: &[(&str, Limit)]) -> Result<(), TimeDelta> {
        let now = self.clock.now();
        let mut entries = self.lock();
        let wait = keys
            .iter()
            .filter_map(|(key, limit)| {
                let entry = entries.get(*key)?;
                (entry.window_end > now && entry.count >= limit.max).then(|| entry.window_end - now)
            })
            .max();
        if let Some(wait) = wait {
            return Err(wait);
        }
        for (key, limit) in keys {
            Self::bump(&mut entries, key, *limit, now);
        }
        Ok(())
    }

    /// Gives back one event counted for `key` in its current window (a reserved attempt
    /// that turned out not to count, e.g. a successful login).
    pub fn release(&self, key: &str) {
        let now = self.clock.now();
        let mut entries = self.lock();
        if let Some(entry) = entries.get_mut(key)
            && entry.window_end > now
        {
            entry.count = entry.count.saturating_sub(1);
        }
    }

    /// Forgets `key` (e.g. the failure counter after a successful login).
    pub fn clear(&self, key: &str) {
        self.lock().remove(key);
    }

    fn bump(entries: &mut HashMap<String, Entry>, key: &str, limit: Limit, now: DateTime<Utc>) {
        if entries.len() >= PURGE_THRESHOLD {
            entries.retain(|_, entry| entry.window_end > now);
        }
        match entries.get_mut(key) {
            Some(entry) if entry.window_end > now => {
                entry.count = entry.count.saturating_add(1);
            }
            _ => {
                entries.insert(
                    key.to_string(),
                    Entry {
                        count: 1,
                        window_end: now + limit.window,
                    },
                );
            }
        }
    }
}

/// Whole minutes in `left`, rounded up, at least 1 (for `TooManyAttempts { minutes }`).
pub fn minutes_rounded_up(left: TimeDelta) -> u32 {
    let millis = left.num_milliseconds().max(1);
    let minutes = (millis + 59_999) / 60_000;
    u32::try_from(minutes).unwrap_or(u32::MAX).max(1)
}

/// Keys carry a SHA-256 of the email / address, so their size is bounded whatever the
/// input and the limiter's memory holds no plain addresses.
fn key(prefix: &str, value: &str) -> String {
    format!("{prefix}{}", super::token::hash(value))
}

/// `login:email:<sha256(email)>` (email already normalised).
pub fn login_email_key(email: &str) -> String {
    key("login:email:", email)
}

/// `login:ip:<sha256(ip)>`.
pub fn login_ip_key(ip: &str) -> String {
    key("login:ip:", ip)
}

/// `mail:email:<sha256(email)>` (email already normalised).
pub fn mail_email_key(email: &str) -> String {
    key("mail:email:", email)
}

/// `mail:ip:<sha256(ip)>`.
pub fn mail_ip_key(ip: &str) -> String {
    key("mail:ip:", ip)
}

/// `account:pw:<user_id>` (a user id is no secret: not hashed).
pub fn account_password_key(user_id: i32) -> String {
    format!("account:pw:{user_id}")
}
