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

/// Inserts a group administered by `admin_id` and adds the admin as a member.
/// Returns the group id.
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

// --------------------------------------------------------------- account fixtures

/// Every column of `users` the account slice reads or writes (not `updated_at`, which
/// changes on every write).
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct UserRow {
    pub id: i32,
    pub name: String,
    pub email: String,
    pub password: String,
    pub pending_email: Option<String>,
    pub email_verification_token: Option<String>,
    pub email_verification_expires: Option<DateTime<Utc>>,
    pub reset_token: Option<String>,
    pub reset_token_expires: Option<DateTime<Utc>>,
    pub session_epoch: u32,
    pub locale: Option<String>,
}

/// The `users` row of `user_id`.
pub async fn user_row(pool: &MySqlPool, user_id: i32) -> UserRow {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, name, email, password, pending_email, email_verification_token, \
         email_verification_expires, reset_token, reset_token_expires, session_epoch, locale \
         FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("select user")
}

/// All `users` rows, ordered by id (to assert that nothing changed anywhere).
pub async fn user_rows(pool: &MySqlPool) -> Vec<UserRow> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, name, email, password, pending_email, email_verification_token, \
         email_verification_expires, reset_token, reset_token_expires, session_epoch, locale \
         FROM users ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("select users")
}

/// Starts a pending email change for `user_id`: `pending_email`, the token stored as
/// `SHA2(token, 256)` (like the app does) and an expiry `expires_in_secs` from now
/// (negative: already expired). `pending_email` is stored as given.
pub async fn set_pending_email(
    pool: &MySqlPool,
    user_id: i32,
    pending_email: &str,
    token: &str,
    expires_in_secs: i64,
) {
    sqlx::query(
        "UPDATE users SET pending_email = ?, email_verification_token = SHA2(?, 256), \
         email_verification_expires = DATE_ADD(CURRENT_TIMESTAMP, INTERVAL ? SECOND) \
         WHERE id = ?",
    )
    .bind(pending_email)
    .bind(token)
    .bind(expires_in_secs)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("set pending email");
}

/// Like [`set_pending_email`] with a fixed expiry instant.
pub async fn set_pending_email_at(
    pool: &MySqlPool,
    user_id: i32,
    pending_email: &str,
    token: &str,
    expires_at: DateTime<Utc>,
) {
    sqlx::query(
        "UPDATE users SET pending_email = ?, email_verification_token = SHA2(?, 256), \
         email_verification_expires = ? WHERE id = ?",
    )
    .bind(pending_email)
    .bind(token)
    .bind(expires_at)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("set pending email");
}

/// Moves the expiry of `user_id`'s pending change to `secs_from_now` (a request issued
/// `24h - secs_from_now` ago).
pub async fn set_pending_expiry(pool: &MySqlPool, user_id: i32, secs_from_now: i64) {
    sqlx::query(
        "UPDATE users SET email_verification_expires = \
         DATE_ADD(CURRENT_TIMESTAMP, INTERVAL ? SECOND) WHERE id = ?",
    )
    .bind(secs_from_now)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("set pending expiry");
}

// ---------------------------------------------------------------- groups fixtures

/// Password of the "Carol" fixture (groups spec).
pub const CAROL_PASSWORD: &str = "carolpassword1";
/// Password of the "Dave" fixture (groups spec).
pub const DAVE_PASSWORD: &str = "davepassword12";

static CAROL_HASH: LazyLock<String> =
    LazyLock::new(|| password::hash(CAROL_PASSWORD).expect("hash carol password"));
static DAVE_HASH: LazyLock<String> =
    LazyLock::new(|| password::hash(DAVE_PASSWORD).expect("hash dave password"));

/// User "Carol": password [`CAROL_PASSWORD`].
pub async fn create_carol(pool: &MySqlPool, email: &str) -> i32 {
    insert_user(pool, "Carol", email, &CAROL_HASH, None).await
}

/// User "Dave": password [`DAVE_PASSWORD`].
pub async fn create_dave(pool: &MySqlPool, email: &str) -> i32 {
    insert_user(pool, "Dave", email, &DAVE_HASH, None).await
}

/// Password of the "Eve" fixture (exclusions spec: a user in none of the groups).
pub const EVE_PASSWORD: &str = "evepassword123";

static EVE_HASH: LazyLock<String> =
    LazyLock::new(|| password::hash(EVE_PASSWORD).expect("hash eve password"));

/// User "Eve": password [`EVE_PASSWORD`].
pub async fn create_eve(pool: &MySqlPool, email: &str) -> i32 {
    insert_user(pool, "Eve", email, &EVE_HASH, None).await
}

/// Like [`create_group`] with a chosen invitation code (a valid one is 8 characters of
/// `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`).
pub async fn create_group_with_code(
    pool: &MySqlPool,
    admin_id: i32,
    name: &str,
    code: &str,
) -> i32 {
    let group_id = create_group(pool, admin_id, name).await;
    set_invitation_code(pool, group_id, code).await;
    group_id
}

/// Overwrites the invitation code as given (no normalisation).
pub async fn set_invitation_code(pool: &MySqlPool, group_id: i32, code: &str) {
    sqlx::query("UPDATE `groups` SET invitation_code = ? WHERE id = ?")
        .bind(code)
        .bind(group_id)
        .execute(pool)
        .await
        .expect("set invitation code");
}

/// The stored invitation code.
pub async fn group_code(pool: &MySqlPool, group_id: i32) -> String {
    sqlx::query_scalar("SELECT invitation_code FROM `groups` WHERE id = ?")
        .bind(group_id)
        .fetch_one(pool)
        .await
        .expect("select invitation code")
}

/// Sets the registration deadline and the draw date (UTC instants, `None` = NULL).
pub async fn set_group_dates(
    pool: &MySqlPool,
    group_id: i32,
    registration_deadline: Option<DateTime<Utc>>,
    draw_date: Option<DateTime<Utc>>,
) {
    sqlx::query("UPDATE `groups` SET registration_deadline = ?, draw_date = ? WHERE id = ?")
        .bind(registration_deadline)
        .bind(draw_date)
        .bind(group_id)
        .execute(pool)
        .await
        .expect("set group dates");
}

/// Marks the group as drawn (no assignments are created; see [`add_assignment`]).
pub async fn set_drawn(pool: &MySqlPool, group_id: i32) {
    sqlx::query("UPDATE `groups` SET is_drawn = 1 WHERE id = ?")
        .bind(group_id)
        .execute(pool)
        .await
        .expect("set drawn");
}

/// Sets `groups.created_at`.
pub async fn set_group_created_at(pool: &MySqlPool, group_id: i32, at: DateTime<Utc>) {
    sqlx::query("UPDATE `groups` SET created_at = ? WHERE id = ?")
        .bind(at)
        .bind(group_id)
        .execute(pool)
        .await
        .expect("set created_at");
}

/// A `groups` row as the groups slice stores it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GroupRow {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub admin_id: i32,
    pub invitation_code: String,
    pub registration_deadline: Option<DateTime<Utc>>,
    pub draw_date: Option<DateTime<Utc>>,
    pub is_drawn: Option<bool>,
}

/// The `groups` row, if it exists.
pub async fn find_group(pool: &MySqlPool, group_id: i32) -> Option<GroupRow> {
    sqlx::query_as::<_, GroupRow>(
        "SELECT id, name, description, admin_id, invitation_code, registration_deadline, \
         draw_date, is_drawn FROM `groups` WHERE id = ?",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await
    .expect("select group")
}

/// The `groups` row (panics if missing).
pub async fn group_row(pool: &MySqlPool, group_id: i32) -> GroupRow {
    find_group(pool, group_id).await.expect("group exists")
}

/// User ids of the members of `group_id`, ordered by id.
pub async fn member_ids(pool: &MySqlPool, group_id: i32) -> Vec<i32> {
    sqlx::query_scalar("SELECT user_id FROM group_members WHERE group_id = ? ORDER BY user_id")
        .bind(group_id)
        .fetch_all(pool)
        .await
        .expect("select members")
}

/// Creates the user's wishlist for the group with one item per title. Returns its id.
pub async fn create_wishlist(
    pool: &MySqlPool,
    group_id: i32,
    user_id: i32,
    titles: &[&str],
) -> i32 {
    let result = sqlx::query("INSERT INTO wishlists (user_id, group_id) VALUES (?, ?)")
        .bind(user_id)
        .bind(group_id)
        .execute(pool)
        .await
        .expect("insert wishlist");
    let id = i32::try_from(result.last_insert_id()).expect("wishlist id fits i32");
    for title in titles {
        sqlx::query("INSERT INTO wishlist_items (wishlist_id, title) VALUES (?, ?)")
            .bind(id)
            .bind(title)
            .execute(pool)
            .await
            .expect("insert wishlist item");
    }
    id
}

/// Inserts the rule "`user_id` must not give to `excluded_user_id`".
pub async fn add_exclusion(pool: &MySqlPool, group_id: i32, user_id: i32, excluded_user_id: i32) {
    sqlx::query(
        "INSERT INTO exclusion_rules (group_id, user_id, excluded_user_id) VALUES (?, ?, ?)",
    )
    .bind(group_id)
    .bind(user_id)
    .bind(excluded_user_id)
    .execute(pool)
    .await
    .expect("insert exclusion");
}

/// Inserts a gift assignment row (does not touch `is_drawn`).
pub async fn add_assignment(pool: &MySqlPool, group_id: i32, giver_id: i32, receiver_id: i32) {
    sqlx::query("INSERT INTO gift_assignments (group_id, giver_id, receiver_id) VALUES (?, ?, ?)")
        .bind(group_id)
        .bind(giver_id)
        .bind(receiver_id)
        .execute(pool)
        .await
        .expect("insert assignment");
}

/// Everything the groups slice reads or writes, for "nothing changed" assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupsSnapshot {
    pub groups: Vec<GroupRow>,
    pub members: Vec<(i32, i32)>,
    pub wishlists: Vec<(i32, i32, i32)>,
    pub items: Vec<(i32, i32, String)>,
    pub exclusions: Vec<(i32, i32, i32)>,
    pub assignments: Vec<(i32, i32, i32)>,
}

/// The whole groups-related state of the database.
pub async fn groups_snapshot(pool: &MySqlPool) -> GroupsSnapshot {
    let groups = sqlx::query_as::<_, GroupRow>(
        "SELECT id, name, description, admin_id, invitation_code, registration_deadline, \
         draw_date, is_drawn FROM `groups` ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("snapshot groups");
    let members = sqlx::query_as::<_, (i32, i32)>(
        "SELECT group_id, user_id FROM group_members ORDER BY group_id, user_id",
    )
    .fetch_all(pool)
    .await
    .expect("snapshot members");
    let wishlists = sqlx::query_as::<_, (i32, i32, i32)>(
        "SELECT id, group_id, user_id FROM wishlists ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("snapshot wishlists");
    let items = sqlx::query_as::<_, (i32, i32, String)>(
        "SELECT id, wishlist_id, title FROM wishlist_items ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("snapshot items");
    let exclusions = sqlx::query_as::<_, (i32, i32, i32)>(
        "SELECT group_id, user_id, excluded_user_id FROM exclusion_rules \
         ORDER BY group_id, user_id, excluded_user_id",
    )
    .fetch_all(pool)
    .await
    .expect("snapshot exclusions");
    let assignments = sqlx::query_as::<_, (i32, i32, i32)>(
        "SELECT group_id, giver_id, receiver_id FROM gift_assignments \
         ORDER BY group_id, giver_id",
    )
    .fetch_all(pool)
    .await
    .expect("snapshot assignments");
    GroupsSnapshot {
        groups,
        members,
        wishlists,
        items,
        exclusions,
        assignments,
    }
}
