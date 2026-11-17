//! The migrations build the expected schema, and timestamps round-trip as UTC.

use chrono::{DateTime, Utc};
use sqlx::MySqlPool;

#[sqlx::test(migrations = "../migrations")]
async fn migrations_create_schema_and_store_utc(pool: MySqlPool) {
    let mut tables: Vec<String> = sqlx::query_scalar(
        "SELECT CAST(table_name AS CHAR) FROM information_schema.tables \
         WHERE table_schema = DATABASE() AND table_name <> '_sqlx_migrations' ORDER BY table_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    tables.sort();
    assert_eq!(
        tables,
        [
            "consumed_share_drafts",
            "draw_reset_notifications",
            "exclusion_rules",
            "gift_assignments",
            "group_members",
            "groups",
            "pending_registrations",
            "sessions",
            "users",
            "wishlist_items",
            "wishlists",
        ]
    );

    // Every migration was recorded and running them again is a no-op.
    app::server::db::migrate(&pool)
        .await
        .expect("re-running migrations");
    let applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(applied, app::server::db::MIGRATOR.iter().count() as i64);

    // The connection runs in UTC even though the server default is Europe/Berlin.
    let tz: String = sqlx::query_scalar("SELECT CAST(@@session.time_zone AS CHAR)")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tz, "+00:00");

    // A TIMESTAMP written via the DB default reads back as the current UTC instant.
    sqlx::query("INSERT INTO users (email, password, name) VALUES ('tz@example.com', 'x', 'TZ')")
        .execute(&pool)
        .await
        .unwrap();
    let created: DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM users WHERE email = 'tz@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let drift = (Utc::now() - created).num_seconds().abs();
    assert!(
        drift < 60,
        "created_at {created} is not the current UTC time"
    );

    // An explicit UTC value survives a round trip unchanged.
    let deadline = DateTime::parse_from_rfc3339("2026-12-24T17:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    sqlx::query("UPDATE users SET last_login = ? WHERE email = 'tz@example.com'")
        .bind(deadline)
        .execute(&pool)
        .await
        .unwrap();
    let read: DateTime<Utc> =
        sqlx::query_scalar("SELECT last_login FROM users WHERE email = 'tz@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(read, deadline);
}

#[test]
fn connect_options_pin_utc() {
    let config = app::test_support::test_config();
    let options = app::server::db::connect_options(&config.db);
    // MySqlConnectOptions has no getter for the time zone; its Debug output shows it.
    assert!(format!("{options:?}").contains("timezone: Some(\"+00:00\")"));
}

#[sqlx::test(migrations = "../migrations")]
async fn widening_session_data_preserves_existing_rows(pool: MySqlPool) {
    sqlx::query("ALTER TABLE sessions MODIFY COLUMN data BLOB NOT NULL")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions (id, data, expiry_date) VALUES (?, ?, ?)")
        .bind("0123456789012345678901")
        .bind(b"existing session".as_slice())
        .bind(
            DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../../../migrations/0010_session_data_mediumblob.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let (data, expiry): (Vec<u8>, DateTime<Utc>) =
        sqlx::query_as("SELECT data, expiry_date FROM sessions WHERE id = ?")
            .bind("0123456789012345678901")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(data, b"existing session");
    assert_eq!(expiry.to_rfc3339(), "2030-01-01T00:00:00+00:00");
}

#[sqlx::test(migrations = "../migrations")]
async fn durable_draw_outbox_upgrade_preserves_populated_groups_and_assignments(pool: MySqlPool) {
    use app::test_support as ts;

    let alice = ts::create_alice(&pool, "migration-alice@example.test").await;
    let bob = ts::create_bob(&pool, "migration-bob@example.test").await;
    let group = ts::create_group(&pool, alice, "Existing draw").await;
    ts::add_member(&pool, group, bob).await;
    ts::add_assignment(&pool, group, alice, bob).await;
    ts::add_assignment(&pool, group, bob, alice).await;
    sqlx::query(
        "UPDATE `groups` SET is_drawn = TRUE, drawn_at = '2026-11-01 12:00:00' WHERE id = ?",
    )
    .bind(group)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE gift_assignments SET notification_sent = TRUE, notification_attempts = 2, notification_last_attempt_at = '2026-11-01 12:01:00' WHERE group_id = ? AND giver_id = ?")
        .bind(group).bind(alice).execute(&pool).await.unwrap();
    sqlx::query("UPDATE gift_assignments SET notification_attempts = 3, notification_last_attempt_at = '2026-11-01 12:02:00' WHERE group_id = ? AND giver_id = ?")
        .bind(group).bind(bob).execute(&pool).await.unwrap();

    // Reconstruct the exact pre-0011 schema in this isolated SQLx database, with
    // existing rows representing both already-sent and failed notifications.
    sqlx::query("DROP TABLE draw_reset_notifications")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "ALTER TABLE `groups` DROP COLUMN draw_mail_claim, DROP COLUMN draw_mail_claim_until",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE gift_assignments DROP COLUMN notification_claim, DROP COLUMN notification_claim_until, DROP COLUMN notification_next_attempt_at")
        .execute(&pool).await.unwrap();

    type Assignment = (i32, i32, i32, i32, bool, u8, Option<DateTime<Utc>>);
    let before_groups = ts::groups_snapshot(&pool).await;
    let before: Vec<Assignment> = sqlx::query_as("SELECT id, group_id, giver_id, receiver_id, notification_sent, notification_attempts, notification_last_attempt_at FROM gift_assignments ORDER BY id")
        .fetch_all(&pool).await.unwrap();
    let drawn_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT drawn_at FROM `groups` WHERE id = ?")
            .bind(group)
            .fetch_one(&pool)
            .await
            .unwrap();

    sqlx::raw_sql(include_str!(
        "../../../migrations/0011_durable_draw_notifications.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let after: Vec<Assignment> = sqlx::query_as("SELECT id, group_id, giver_id, receiver_id, notification_sent, notification_attempts, notification_last_attempt_at FROM gift_assignments ORDER BY id")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(after, before);
    assert_eq!(ts::groups_snapshot(&pool).await, before_groups);
    type GroupMigrationState = (Option<DateTime<Utc>>, Option<String>, Option<DateTime<Utc>>);
    let (after_drawn_at, group_claim, group_claim_until): GroupMigrationState = sqlx::query_as(
        "SELECT drawn_at, draw_mail_claim, draw_mail_claim_until FROM `groups` WHERE id = ?",
    )
    .bind(group)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_drawn_at, drawn_at);
    assert!(group_claim.is_none());
    assert!(group_claim_until.is_none());
    let nonempty_new_columns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM gift_assignments WHERE notification_claim IS NOT NULL OR notification_claim_until IS NOT NULL OR notification_next_attempt_at IS NOT NULL")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(nonempty_new_columns, 0);
    let reset_notices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM draw_reset_notifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reset_notices, 0);
}

/// The migrations directory, read at runtime (no compile-time `migrate!`).
async fn migrator() -> sqlx::migrate::Migrator {
    sqlx::migrate::Migrator::new(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../migrations"
    )))
    .await
    .expect("read the migrations")
}

/// Numbers are unique and gapless: a renumbered or duplicated file shows up here, not
/// in production (`dev` and #84 both added a 0009 before #84 was renumbered).
#[tokio::test]
async fn migration_numbers_are_unique_and_without_gaps() {
    let versions: Vec<i64> = migrator().await.iter().map(|m| m.version).collect();
    let expected: Vec<i64> = (1..=i64::try_from(versions.len()).unwrap()).collect();
    assert_eq!(versions, expected);
    assert!(versions.len() >= 12, "0001-0012 at least: {versions:?}");
}

/// The whole chain after 0008 (the production schema before the archive and #84) applies
/// to a database that already holds rows of every kind, and keeps them.
#[sqlx::test(migrations = false)]
async fn migrations_after_0008_apply_to_a_populated_database_and_keep_its_rows(pool: MySqlPool) {
    let migrator = migrator().await;
    migrator.run_to(8, &pool).await.expect("0001-0008");

    // A production-like state: a migrated bcrypt account, a drawn group with sent and
    // failed mails, an undrawn group with a rule, a scheme-less PHP link and a session.
    for (id, email) in [
        (1, "anna@example.test"),
        (2, "ben@example.test"),
        (3, "cleo@example.test"),
    ] {
        sqlx::query("INSERT INTO users (id, email, password, name) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(email)
            .bind("$2y$10$abcdefghijklmnopqrstuuJ3jVvHq3d0w3jX8o6bF1pS4cQ0r8m5K")
            .bind(format!("User {id}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query(
        "INSERT INTO `groups` (id, name, admin_id, invitation_code, is_drawn, drawn_at, updated_at) \
         VALUES (1, 'Drawn 2025', 1, 'ABCD2345', TRUE, '2025-12-01 18:00:00', '2025-12-01 18:00:00'), \
                (2, 'Open 2026', 2, 'EFGH6789', FALSE, NULL, '2025-12-25 10:00:00')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO group_members (group_id, user_id) VALUES (1, 1), (1, 2), (1, 3), (2, 2), (2, 3)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO gift_assignments (group_id, giver_id, receiver_id, notification_sent, notification_attempts) \
         VALUES (1, 1, 2, TRUE, 1), (1, 2, 3, FALSE, 3), (1, 3, 1, FALSE, 0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO exclusion_rules (group_id, user_id, excluded_user_id) VALUES (2, 2, 3)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO wishlists (id, user_id, group_id) VALUES (1, 2, 1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO wishlist_items (wishlist_id, title, link, position) \
         VALUES (1, 'Book', 'www.amazon.de/dp/123', 1), (1, 'Socks', NULL, 2)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, data, expiry_date) VALUES (?, ?, '2030-01-01 00:00:00')",
    )
    .bind("0123456789012345678901")
    .bind(b"existing session".as_slice())
    .execute(&pool)
    .await
    .unwrap();

    type Snapshot = (
        Vec<(i32, String, String)>,
        Vec<(
            i32,
            String,
            String,
            bool,
            Option<DateTime<Utc>>,
            DateTime<Utc>,
        )>,
        Vec<(i32, i32, i32, bool, u8)>,
        Vec<(i32, i32)>,
        Vec<(i32, i32, i32)>,
        Vec<(String, Option<String>, i32)>,
        Vec<(String, Vec<u8>)>,
    );
    async fn snapshot(pool: &MySqlPool) -> Snapshot {
        (
            sqlx::query_as("SELECT id, name, password FROM users ORDER BY id")
                .fetch_all(pool)
                .await
                .unwrap(),
            sqlx::query_as(
                "SELECT id, name, invitation_code, is_drawn, drawn_at, updated_at \
                 FROM `groups` ORDER BY id",
            )
            .fetch_all(pool)
            .await
            .unwrap(),
            sqlx::query_as(
                "SELECT group_id, giver_id, receiver_id, notification_sent, notification_attempts \
                 FROM gift_assignments ORDER BY id",
            )
            .fetch_all(pool)
            .await
            .unwrap(),
            sqlx::query_as(
                "SELECT group_id, user_id FROM group_members ORDER BY group_id, user_id",
            )
            .fetch_all(pool)
            .await
            .unwrap(),
            sqlx::query_as(
                "SELECT group_id, user_id, excluded_user_id FROM exclusion_rules ORDER BY id",
            )
            .fetch_all(pool)
            .await
            .unwrap(),
            sqlx::query_as("SELECT title, link, position FROM wishlist_items ORDER BY id")
                .fetch_all(pool)
                .await
                .unwrap(),
            sqlx::query_as("SELECT id, data FROM sessions ORDER BY id")
                .fetch_all(pool)
                .await
                .unwrap(),
        )
    }
    let before = snapshot(&pool).await;

    migrator
        .run(&pool)
        .await
        .expect("0009-0012 on populated data");

    assert_eq!(snapshot(&pool).await, before);
    let applied: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(applied >= 12);
    // 0009: nothing archived or warned, the undrawn group's clock restarts.
    let archive: Vec<(i32, bool, bool, bool)> = sqlx::query_as(
        "SELECT id, archived_at IS NOT NULL, archive_warned_at IS NOT NULL, \
         kept_active_at IS NOT NULL FROM `groups` ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        archive,
        vec![(1, false, false, false), (2, false, false, true)]
    );
    // 0010: the session column is wider.
    let session_type: String = sqlx::query_scalar(
        "SELECT CAST(data_type AS CHAR) FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = 'sessions' AND column_name = 'data'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(session_type, "mediumblob");
    // 0011: no claims, no retry deadlines, no reset notices.
    let claims: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM gift_assignments WHERE notification_claim IS NOT NULL \
                 OR notification_claim_until IS NOT NULL OR notification_next_attempt_at IS NOT NULL) \
              + (SELECT COUNT(*) FROM `groups` WHERE draw_mail_claim IS NOT NULL \
                 OR draw_mail_claim_until IS NOT NULL) \
              + (SELECT COUNT(*) FROM draw_reset_notifications)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(claims, 0);
    // 0012: no draft is consumed yet.
    let consumed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM consumed_share_drafts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(consumed, 0);
}
