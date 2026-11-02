//! draw slice: concurrency (AC22, AC23, D5). The group row is the lock: the draw starts with
//! `SELECT admin_id, is_drawn FROM `groups` WHERE id = ? FOR UPDATE`, re-checks the admin and
//! `is_drawn` under it and reads members and rules under it.

use std::time::Duration;

use crate::common::draw::*;
use crate::common::exclusions::{ExReply, add_ex};
use crate::common::groups::*;
use app::draw_api::DrawApiError;
use app::exclusions_api::ExclusionError;
use app::test_support as ts;
use sqlx::{MySql, MySqlPool, Transaction};
use tokio::task::JoinHandle;

async fn hold_lock(pool: &MySqlPool, group: i32) -> Transaction<'static, MySql> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM `groups` WHERE id = ? FOR UPDATE")
        .bind(group)
        .fetch_one(&mut *tx)
        .await
        .expect("lock the group row");
    tx
}

async fn assert_waiting<T>(handle: &JoinHandle<T>, what: &str) {
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !handle.is_finished(),
        "{what} must wait for the group row lock"
    );
}

// ------------------------------------------------------------------------ AC22

#[sqlx::test(migrations = "../migrations")]
async fn ac22_two_simultaneous_draws_give_one_draw_and_three_mails(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    for round in 0..5 {
        let g = if round == 0 {
            d.g
        } else {
            let g = ts::create_group(&pool, d.alice, &format!("Round{round}")).await;
            ts::add_member(&pool, g, d.bob).await;
            ts::add_member(&pool, g, d.carol).await;
            g
        };
        let mut first = d.alice().await;
        let mut second = first.fork();
        let (a, b) = tokio::join!(perform(&mut first, g), perform(&mut second, g));
        let results = [a.dr_result::<()>(), b.dr_result::<()>()];
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "round {round}: {results:?}"
        );
        assert!(
            results.contains(&Err(DrawApiError::AlreadyDrawn)),
            "round {round}: {results:?}"
        );
        assert_eq!(assignments(&pool, g).await.len(), 3);
    }
    d.idle().await;
    // Five groups, each drawn once: five mails per member and not one more.
    for user in [d.alice, d.bob, d.carol] {
        assert_eq!(d.mail_count(user).await, 5, "{}", d.name_of(user));
    }
}

// ------------------------------------------------------------------------ AC23

#[sqlx::test(migrations = "../migrations")]
async fn ac23_a_draw_waits_for_a_join_and_then_includes_the_new_member(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    let mut lock = hold_lock(&pool, d.g).await;

    let draw = tokio::spawn(async move { perform(&mut alice, d.g).await });
    assert_waiting(&draw, "perform_draw").await;
    assert!(!is_drawn(&pool, d.g).await);

    // A join as the groups slice does it, under the lock.
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES (?, ?)")
        .bind(d.g)
        .bind(d.dave)
        .execute(&mut *lock)
        .await
        .unwrap();
    lock.commit().await.unwrap();

    draw.await.unwrap().dr_ok();
    let rows = assignments(&pool, d.g).await;
    assert_eq!(rows.len(), 4);
    assert_valid(&rows, &[d.alice, d.bob, d.carol, d.dave], &[]);
    d.idle().await;
    assert_eq!(d.draw_mails(d.dave, GROUP_NAME).await.len(), 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_a_draw_waits_for_a_leave_and_draws_the_remaining_members(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    let mut lock = hold_lock(&pool, d.g).await;

    let draw = tokio::spawn(async move { perform(&mut alice, d.g).await });
    assert_waiting(&draw, "perform_draw").await;
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(d.g)
        .bind(d.carol)
        .execute(&mut *lock)
        .await
        .unwrap();
    lock.commit().await.unwrap();

    draw.await.unwrap().dr_ok();
    assert_eq!(
        assignments(&pool, d.g).await,
        vec![(d.alice, d.bob), (d.bob, d.alice)]
    );
    d.idle().await;
    assert_eq!(
        d.mail_count(d.carol).await,
        0,
        "the member who left gets no mail"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_a_draw_waits_for_a_leave_that_makes_it_too_small(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let g2 = ts::create_group(&pool, d.alice, "Duo").await;
    ts::add_member(&pool, g2, d.bob).await;
    let mut alice = d.alice().await;
    let mut lock = hold_lock(&pool, g2).await;

    let draw = tokio::spawn(async move { perform(&mut alice, g2).await });
    assert_waiting(&draw, "perform_draw").await;
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(g2)
        .bind(d.bob)
        .execute(&mut *lock)
        .await
        .unwrap();
    lock.commit().await.unwrap();

    assert_eq!(
        draw.await.unwrap().dr_err(),
        DrawApiError::TooFewMembers { count: 1, min: 2 }
    );
    assert!(assignments(&pool, g2).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn d5_the_admin_is_rechecked_under_the_lock(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    let mut lock = hold_lock(&pool, d.g).await;

    let draw = tokio::spawn(async move { perform(&mut alice, d.g).await });
    assert_waiting(&draw, "perform_draw").await;
    // The admin role was transferred meanwhile.
    sqlx::query("UPDATE `groups` SET admin_id = ? WHERE id = ?")
        .bind(d.bob)
        .bind(d.g)
        .execute(&mut *lock)
        .await
        .unwrap();
    lock.commit().await.unwrap();

    assert_eq!(draw.await.unwrap().dr_err(), DrawApiError::Forbidden);
    assert!(assignments(&pool, d.g).await.is_empty());
    assert!(!is_drawn(&pool, d.g).await);
}

#[sqlx::test(migrations = "../migrations")]
async fn d5_is_drawn_is_rechecked_under_the_lock(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    let mut lock = hold_lock(&pool, d.g).await;

    let draw = tokio::spawn(async move { perform(&mut alice, d.g).await });
    assert_waiting(&draw, "perform_draw").await;
    // Another draw committed while this one waited.
    for (giver, receiver) in [(d.alice, d.bob), (d.bob, d.carol), (d.carol, d.alice)] {
        sqlx::query(
            "INSERT INTO gift_assignments (group_id, giver_id, receiver_id) VALUES (?, ?, ?)",
        )
        .bind(d.g)
        .bind(giver)
        .bind(receiver)
        .execute(&mut *lock)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE `groups` SET is_drawn = 1, drawn_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(d.g)
        .execute(&mut *lock)
        .await
        .unwrap();
    lock.commit().await.unwrap();

    assert_eq!(draw.await.unwrap().dr_err(), DrawApiError::AlreadyDrawn);
    assert_eq!(
        assignments(&pool, d.g).await,
        vec![(d.alice, d.bob), (d.bob, d.carol), (d.carol, d.alice)],
        "the first draw's rows are untouched"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn d5_a_reset_waits_for_the_lock_too(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    let mut alice = d.alice().await;
    let lock = hold_lock(&pool, d.g).await;

    let reset_call = tokio::spawn(async move { reset(&mut alice, d.g).await });
    assert_waiting(&reset_call, "reset_draw").await;
    assert!(is_drawn(&pool, d.g).await);
    lock.commit().await.unwrap();
    reset_call.await.unwrap().dr_ok();
    assert!(!is_drawn(&pool, d.g).await);
    assert!(assignments(&pool, d.g).await.is_empty());
}

// ------------------------------------------------------- racing writes of other slices

#[sqlx::test(migrations = "../migrations")]
async fn a_rule_racing_with_the_draw_is_either_honoured_or_refused(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    for round in 0..12 {
        let g = ts::create_group(&pool, d.alice, &format!("Race{round}")).await;
        ts::add_member(&pool, g, d.bob).await;
        ts::add_member(&pool, g, d.carol).await;
        let mut alice = d.alice().await;
        let mut bob = d.bob().await;
        let (draw, rule) = tokio::join!(perform(&mut alice, g), add_ex(&mut bob, g, d.alice));
        draw.dr_ok();
        let rows = assignments(&pool, g).await;
        assert_eq!(rows.len(), 3);
        let rule_stored: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM exclusion_rules WHERE group_id = ? AND user_id = ? AND excluded_user_id = ?",
        )
        .bind(g)
        .bind(d.bob)
        .bind(d.alice)
        .fetch_one(&pool)
        .await
        .unwrap();
        if rule_stored == 1 {
            rule.ex_ok();
            assert!(
                !rows.contains(&(d.bob, d.alice)),
                "round {round}: a stored rule was ignored by the draw"
            );
        } else {
            assert_eq!(rule.ex_err(), ExclusionError::AlreadyDrawn, "round {round}");
        }
    }
    d.idle().await;
}

#[sqlx::test(migrations = "../migrations")]
async fn a_join_racing_with_the_draw_never_leaves_a_member_without_an_assignment(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    for round in 0..8 {
        let g = ts::create_group_with_code(
            &pool,
            d.alice,
            &format!("Join{round}"),
            &format!("JOIN{round:04}"),
        )
        .await;
        ts::add_member(&pool, g, d.bob).await;
        ts::add_member(&pool, g, d.carol).await;
        let mut alice = d.alice().await;
        let mut dave = d.dave().await;
        let code = ts::group_code(&pool, g).await;
        let (draw, join) = tokio::join!(perform(&mut alice, g), join_group(&mut dave, &code));
        draw.dr_ok();
        let members = ts::member_ids(&pool, g).await;
        let rows = assignments(&pool, g).await;
        assert_valid(&rows, &members, &[]);
        match join.group_result::<i32>() {
            Ok(_) => assert_eq!(members.len(), 4, "round {round}"),
            Err(e) => {
                assert_eq!(
                    e,
                    app::groups_api::GroupError::AlreadyDrawn,
                    "round {round}"
                );
                assert_eq!(members.len(), 3);
            }
        }
    }
    d.idle().await;
}

#[sqlx::test(migrations = "../migrations")]
async fn a_leave_racing_with_the_draw_keeps_the_invariant(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    for round in 0..8 {
        let g = ts::create_group(&pool, d.alice, &format!("Leave{round}")).await;
        ts::add_member(&pool, g, d.bob).await;
        ts::add_member(&pool, g, d.carol).await;
        let mut alice = d.alice().await;
        let mut carol = d.carol().await;
        let (draw, leave) = tokio::join!(perform(&mut alice, g), leave_group(&mut carol, g));
        draw.dr_ok();
        let members = ts::member_ids(&pool, g).await;
        assert_valid(&assignments(&pool, g).await, &members, &[]);
        match leave.group_result::<serde_json::Value>() {
            Ok(_) => assert_eq!(members.len(), 2, "round {round}"),
            Err(e) => assert_eq!(e, app::groups_api::GroupError::AlreadyDrawn),
        }
    }
    d.idle().await;
}

#[sqlx::test(migrations = "../migrations")]
async fn a_rejected_draw_holds_no_lock_afterwards(pool: MySqlPool) {
    // The lock is held only for the draw's transaction: after a refusal a join works at once.
    let d = draw_world(&pool).await;
    let solo = ts::create_group_with_code(&pool, d.alice, "Solo", "SOLO2345").await;
    let mut alice = d.alice().await;
    assert!(perform(&mut alice, solo).await.dr_result::<()>().is_err());
    let joined = tokio::time::timeout(
        Duration::from_secs(3),
        join_group(&mut d.dave().await, "SOLO2345"),
    )
    .await
    .expect("join must not wait for a lock");
    joined.group_ok();
}
