//! draw slice: the schema (AC44) and the invariant D6.

use crate::common::draw::*;
use app::test_support as ts;
use sqlx::MySqlPool;

async fn column(
    pool: &MySqlPool,
    table: &str,
    column: &str,
) -> Option<(String, String, Option<String>)> {
    sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT CAST(data_type AS CHAR), CAST(is_nullable AS CHAR), CAST(column_default AS CHAR) \
         FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?",
    )
    .bind(table)
    .bind(column)
    .fetch_optional(pool)
    .await
    .expect("read column")
}

#[sqlx::test(migrations = "../migrations")]
async fn ac44_email_templates_is_gone(pool: MySqlPool) {
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_schema = DATABASE() AND table_name = 'email_templates'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tables, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac44_the_new_and_tightened_columns(pool: MySqlPool) {
    let (_, nullable, default) = column(&pool, "groups", "is_drawn").await.expect("is_drawn");
    assert_eq!(nullable, "NO");
    assert!(
        matches!(default.as_deref(), Some("0" | "false" | "FALSE")),
        "default {default:?}"
    );
    let (kind, nullable, default) = column(&pool, "groups", "drawn_at").await.expect("drawn_at");
    assert_eq!((kind.as_str(), nullable.as_str()), ("timestamp", "YES"));
    assert!(
        matches!(default.as_deref(), None | Some("NULL")),
        "default {default:?}"
    );

    let (_, nullable, default) = column(&pool, "gift_assignments", "notification_sent")
        .await
        .expect("notification_sent");
    assert_eq!(nullable, "NO");
    assert!(matches!(default.as_deref(), Some("0" | "false" | "FALSE")));
    let (kind, nullable, default) = column(&pool, "gift_assignments", "notification_attempts")
        .await
        .expect("notification_attempts");
    assert_eq!((kind.as_str(), nullable.as_str()), ("tinyint", "NO"));
    assert_eq!(default.as_deref(), Some("0"));
    let (kind, nullable, _) = column(&pool, "gift_assignments", "notification_last_attempt_at")
        .await
        .expect("notification_last_attempt_at");
    assert_eq!((kind.as_str(), nullable.as_str()), ("timestamp", "YES"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac44_a_group_cannot_have_a_null_is_drawn(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let result = sqlx::query("UPDATE `groups` SET is_drawn = NULL WHERE id = ?")
        .bind(d.g)
        .execute(&pool)
        .await;
    assert!(result.is_err(), "is_drawn must be NOT NULL");
    // A new group starts undrawn, with no drawn_at.
    assert!(!is_drawn(&pool, d.g).await);
    assert_eq!(drawn_at(&pool, d.g).await, None);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac44_giving_to_yourself_fails_the_check_constraint(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let err = sqlx::query(
        "INSERT INTO gift_assignments (group_id, giver_id, receiver_id) VALUES (?, ?, ?)",
    )
    .bind(d.g)
    .bind(d.alice)
    .bind(d.alice)
    .execute(&pool)
    .await
    .expect_err("self assignment must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("chk_gift_assignments_not_self"),
        "not the CHECK constraint: {message}"
    );
    assert!(assignments(&pool, d.g).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac44_a_giver_and_a_receiver_occur_once_per_group(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    ts::add_assignment(&pool, d.g, d.alice, d.bob).await;
    for (giver, receiver, what) in [
        (d.alice, d.carol, "same giver twice"),
        (d.carol, d.bob, "same receiver twice"),
    ] {
        let err = sqlx::query(
            "INSERT INTO gift_assignments (group_id, giver_id, receiver_id) VALUES (?, ?, ?)",
        )
        .bind(d.g)
        .bind(giver)
        .bind(receiver)
        .execute(&pool)
        .await
        .expect_err(what);
        assert!(
            err.as_database_error()
                .is_some_and(|e| e.is_unique_violation()),
            "{what}: {err}"
        );
    }
    assert_eq!(assignments(&pool, d.g).await, vec![(d.alice, d.bob)]);
}

// ----------------------------------------------------------------------------- D6

#[sqlx::test(migrations = "../migrations")]
async fn d6_drawn_flag_drawn_at_and_rows_agree_after_a_draw_and_a_reset(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let invariant = |expect_drawn: bool| {
        let pool = pool.clone();
        let g = d.g;
        async move {
            let drawn = is_drawn(&pool, g).await;
            assert_eq!(drawn, expect_drawn);
            assert_eq!(drawn_at(&pool, g).await.is_some(), expect_drawn);
            let members = ts::member_ids(&pool, g).await;
            let rows = assignments(&pool, g).await;
            if expect_drawn {
                assert_valid(&rows, &members, &[]);
            } else {
                assert!(rows.is_empty());
            }
        }
    };
    invariant(false).await;
    d.draw_and_deliver(d.g).await;
    invariant(true).await;
    reset(&mut d.alice().await, d.g).await.dr_ok();
    d.idle().await;
    invariant(false).await;
    d.draw_and_deliver(d.g).await;
    invariant(true).await;
}

#[sqlx::test(migrations = "../migrations")]
async fn d8_delivery_columns_start_clean_and_record_the_attempt(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.block();
    perform(&mut d.alice().await, d.g).await.dr_ok();
    let fresh: Vec<(i32, bool, u8, bool)> = sqlx::query_as(
        "SELECT giver_id, notification_sent, notification_attempts, \
         notification_last_attempt_at IS NULL FROM gift_assignments WHERE group_id = ? ORDER BY giver_id",
    )
    .bind(d.g)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        fresh.iter().all(|r| !r.1 && r.2 == 0 && r.3),
        "fresh rows: {fresh:?}"
    );
    d.transport.unblock();
    d.idle().await;
    let done: Vec<(bool, bool)> = sqlx::query_as(
        "SELECT notification_sent, notification_last_attempt_at IS NOT NULL FROM gift_assignments WHERE group_id = ?",
    )
    .bind(d.g)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(done.len(), 3);
    assert!(done.iter().all(|r| r.0 && r.1), "{done:?}");
}
