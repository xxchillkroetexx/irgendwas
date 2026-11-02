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
