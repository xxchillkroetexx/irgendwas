//! Fixtures for integration tests (feature `test-support`, enabled only through the
//! `server` crate's dev-dependencies). Use with `#[sqlx::test(migrations = "../migrations")]`,
//! which hands every test a fresh, migrated database.
//!
//! [`mailpit`]: read mails (and the links in them) back from Mailpit.

pub mod mailpit;

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU32, Ordering};

use sqlx::MySqlPool;

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
