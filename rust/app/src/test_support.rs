//! Fixtures for integration tests (feature `test-support`, enabled only through the
//! `server` crate's dev-dependencies). Use with `#[sqlx::test(migrations = "../migrations")]`,
//! which hands every test a fresh, migrated database.
//!
//! [`mailpit`]: read mails (and the links in them) back from Mailpit.

pub mod mailpit;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use chrono::{DateTime, Duration, TimeZone, Utc};
use sqlx::MySqlPool;

use crate::server::rate_limit::Clock;

use crate::server::password;

/// Password of every user created by [`create_user`].
pub const TEST_PASSWORD: &str = "correct horse battery staple";

/// argon2id hash of [`TEST_PASSWORD`], computed once per test binary (hashing is slow in
/// debug builds).
pub static TEST_PASSWORD_HASH: LazyLock<String> =
    LazyLock::new(|| password::hash(TEST_PASSWORD).expect("hash test password"));

static COUNTER: AtomicU32 = AtomicU32::new(1);

pub(crate) fn next() -> u32 {
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Inserts a user with password [`TEST_PASSWORD`] and returns its id.
pub async fn create_user(pool: &MySqlPool, name: &str, email: &str) -> i32 {
    let result = sqlx::query("INSERT INTO users (email, password, name) VALUES (?, ?, ?)")
        .bind(email)
        .bind(TEST_PASSWORD_HASH.as_str())
        .bind(name)
        .execute(pool)
        .await
        .expect("insert user");
    i32::try_from(result.last_insert_id()).expect("user id fits i32")
}

/// Sets `users.locale` (the account language) of `user_id`.
pub async fn set_user_locale(pool: &MySqlPool, user_id: i32, locale: Option<&str>) {
    sqlx::query("UPDATE users SET locale = ? WHERE id = ?")
        .bind(locale)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("set user locale");
}

/// Inserts a group administered by `admin_id` and adds the admin as a member
/// (as the PHP app does). Returns the group id.
pub async fn create_group(pool: &MySqlPool, admin_id: i32, name: &str) -> i32 {
    let code = format!("TEST{:08}{}", next(), admin_id);
    let result =
        sqlx::query("INSERT INTO `groups` (name, admin_id, invitation_code) VALUES (?, ?, ?)")
            .bind(name)
            .bind(admin_id)
            .bind(code)
            .execute(pool)
            .await
            .expect("insert group");
    let group_id = i32::try_from(result.last_insert_id()).expect("group id fits i32");
    add_member(pool, group_id, admin_id).await;
    group_id
}

/// Adds `user_id` to `group_id`.
pub async fn add_member(pool: &MySqlPool, group_id: i32, user_id: i32) {
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES (?, ?)")
        .bind(group_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("insert group member");
}

/// A [`Config`](crate::server::Config) for tests (no environment access).
pub fn test_config() -> crate::server::Config {
    crate::server::Config::from_lookup(|name| match name {
        "DB_PASSWORD" => Some(String::new()),
        _ => None,
    })
    .expect("test config")
}

// ------------------------------------------------------------------ auth fixtures

/// Password of the "Alice" fixture (auth spec).
pub const ALICE_PASSWORD: &str = "correct horse 1";
/// Password of the "Bob" fixture (auth spec).
pub const BOB_PASSWORD: &str = "bobpassword1";

static ALICE_HASH: LazyLock<String> =
    LazyLock::new(|| password::hash(ALICE_PASSWORD).expect("hash alice password"));
static BOB_HASH: LazyLock<String> =
    LazyLock::new(|| password::hash(BOB_PASSWORD).expect("hash bob password"));

/// Bob's stored `last_login`: 2026-12-01 10:00:00 UTC (11:00 in Europe/Berlin).
pub fn bob_last_login() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 12, 1, 10, 0, 0).unwrap()
}

/// User "Alice": password [`ALICE_PASSWORD`], `last_login` NULL, `locale` NULL.
/// `email` must be normalised (lowercase); pass a unique one in tests that read mail.
pub async fn create_alice(pool: &MySqlPool, email: &str) -> i32 {
    insert_user(pool, "Alice", email, &ALICE_HASH, None).await
}

/// User "Bob": password [`BOB_PASSWORD`], `last_login` = [`bob_last_login`], `locale` NULL.
pub async fn create_bob(pool: &MySqlPool, email: &str) -> i32 {
    insert_user(pool, "Bob", email, &BOB_HASH, Some(bob_last_login())).await
}

async fn insert_user(
    pool: &MySqlPool,
    name: &str,
    email: &str,
    hash: &str,
    last_login: Option<DateTime<Utc>>,
) -> i32 {
    let result =
        sqlx::query("INSERT INTO users (email, password, name, last_login) VALUES (?, ?, ?, ?)")
            .bind(email)
            .bind(hash)
            .bind(name)
            .bind(last_login)
            .execute(pool)
            .await
            .expect("insert user");
    i32::try_from(result.last_insert_id()).expect("user id fits i32")
}

/// Inserts a pending registration for `token` (stored as `SHA2(token, 256)`, lowercase hex,
/// like the app does), expiring `expires_in_secs` from now (negative: already expired).
pub async fn insert_pending_registration(
    pool: &MySqlPool,
    email: &str,
    token: &str,
    next_path: Option<&str>,
    expires_in_secs: i64,
) {
    sqlx::query(
        "INSERT INTO pending_registrations (email, token_hash, next_path, created_at, expires_at) \
         VALUES (?, SHA2(?, 256), ?, CURRENT_TIMESTAMP, DATE_ADD(CURRENT_TIMESTAMP, INTERVAL ? SECOND))",
    )
    .bind(email)
    .bind(token)
    .bind(next_path)
    .bind(expires_in_secs)
    .execute(pool)
    .await
    .expect("insert pending registration");
}

/// Gives `user_id` a reset token (stored as `SHA2(token, 256)`) expiring `expires_in_secs`
/// from now (negative: already expired).
pub async fn set_reset_token(pool: &MySqlPool, user_id: i32, token: &str, expires_in_secs: i64) {
    sqlx::query(
        "UPDATE users SET reset_token = SHA2(?, 256), \
         reset_token_expires = DATE_ADD(CURRENT_TIMESTAMP, INTERVAL ? SECOND) WHERE id = ?",
    )
    .bind(token)
    .bind(expires_in_secs)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("set reset token");
}

/// A [`Clock`] the test moves by hand (rate-limit windows).
#[derive(Debug)]
pub struct ManualClock(Mutex<DateTime<Utc>>);

impl ManualClock {
    /// Starts at the real current time.
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Utc::now())))
    }

    pub fn advance(&self, by: Duration) {
        let mut now = self.0.lock().expect("clock lock");
        *now += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock lock")
    }
}
