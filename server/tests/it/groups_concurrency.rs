//! groups slice: concurrency (AC44, D16). The group row is the lock: every membership
//! change runs in a transaction that starts with `SELECT ... FROM `groups` WHERE id = ?
//! FOR UPDATE` and decides under that lock (plan: "group row first"). The tests hold that
//! lock in a transaction of their own, start the call, show that it waits, and then let it
//! go after changing the world the way a draw or a delete would.

use std::time::Duration;

use crate::common::groups::*;
use app::groups_api::GroupError;
use app::test_support::{self as ts, groups_snapshot};
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

/// Lets the spawned call run until it blocks, and checks that it is still waiting.
async fn assert_waiting<T>(handle: &JoinHandle<T>, what: &str) {
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !handle.is_finished(),
        "{what} must wait for the group row lock"
    );
}

/// A draw as the draw slice will do it: under the lock, assign every member, mark drawn.
async fn simulate_draw(tx: &mut Transaction<'static, MySql>, group: i32) {
    let members: Vec<i32> =
        sqlx::query_scalar("SELECT user_id FROM group_members WHERE group_id = ? ORDER BY user_id")
            .bind(group)
            .fetch_all(&mut **tx)
            .await
            .unwrap();
    for (i, giver) in members.iter().enumerate() {
        let receiver = members[(i + 1) % members.len()];
        sqlx::query(
            "INSERT INTO gift_assignments (group_id, giver_id, receiver_id) VALUES (?, ?, ?)",
        )
        .bind(group)
        .bind(giver)
        .bind(receiver)
        .execute(&mut **tx)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE `groups` SET is_drawn = 1 WHERE id = ?")
        .bind(group)
        .execute(&mut **tx)
        .await
        .unwrap();
}

async fn assert_every_member_has_an_assignment(pool: &MySqlPool, group: i32) {
    let drawn: Option<bool> = sqlx::query_scalar("SELECT is_drawn FROM `groups` WHERE id = ?")
        .bind(group)
        .fetch_one(pool)
        .await
        .unwrap();
    if drawn == Some(true) {
        let without: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM group_members m WHERE m.group_id = ? AND NOT EXISTS \
             (SELECT 1 FROM gift_assignments a WHERE a.group_id = m.group_id AND a.giver_id = m.user_id)",
        )
        .bind(group)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            without, 0,
            "a member without an assignment in a drawn group"
        );
    }
}

// ------------------------------------------------------------------------- AC44

#[sqlx::test(migrations = "../migrations")]
async fn ac44_a_join_waits_for_a_running_draw_and_is_then_rejected(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    let mut draw = hold_lock(&pool, w.g).await;

    let join = tokio::spawn(async move { join_group(&mut carol, CODE).await });
    assert_waiting(&join, "join_group").await;

    simulate_draw(&mut draw, w.g).await;
    draw.commit().await.unwrap();

    let reply = join.await.unwrap();
    assert_eq!(reply.group_err(), GroupError::AlreadyDrawn);
    assert_eq!(ts::member_ids(&pool, w.g).await, vec![w.alice, w.bob]);
    assert_every_member_has_an_assignment(&pool, w.g).await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac44_a_join_that_gets_the_lock_first_is_a_member_before_the_draw(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    let lock = hold_lock(&pool, w.g).await;

    let join = tokio::spawn(async move { join_group(&mut carol, CODE).await });
    assert_waiting(&join, "join_group").await;
    lock.commit().await.unwrap(); // nothing was drawn meanwhile

    assert_eq!(join.await.unwrap().value::<i32>(), w.g);
    // A draw that starts afterwards sees Carol as a member and assigns her.
    let mut draw = hold_lock(&pool, w.g).await;
    simulate_draw(&mut draw, w.g).await;
    draw.commit().await.unwrap();
    assert_every_member_has_an_assignment(&pool, w.g).await;
    let assigned: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM gift_assignments WHERE group_id = ?")
            .bind(w.g)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(assigned, 3);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac44_leave_remove_and_the_page_wait_for_a_running_draw(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_member(&pool, w.g, w.carol).await;
    let mut bob = w.bob().await;
    let mut alice = w.alice().await;
    let mut draw = hold_lock(&pool, w.g).await;

    let leave = tokio::spawn(async move { leave_group(&mut bob, w.g).await });
    let (g, carol_id) = (w.g, w.carol);
    let remove = tokio::spawn(async move { remove_member(&mut alice, g, carol_id).await });
    assert_waiting(&leave, "leave_group").await;
    assert_waiting(&remove, "remove_member").await;

    simulate_draw(&mut draw, w.g).await;
    draw.commit().await.unwrap();

    assert_eq!(leave.await.unwrap().group_err(), GroupError::AlreadyDrawn);
    assert_eq!(remove.await.unwrap().group_err(), GroupError::AlreadyDrawn);
    assert_eq!(
        ts::member_ids(&pool, w.g).await,
        vec![w.alice, w.bob, w.carol]
    );
    assert_every_member_has_an_assignment(&pool, w.g).await;
}

// ------------------------------------------------------------- join / leave vs delete

#[sqlx::test(migrations = "../migrations")]
async fn a_join_that_races_with_a_delete_fails_cleanly(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    let mut deleter = hold_lock(&pool, w.g).await;

    let join = tokio::spawn(async move { join_group(&mut carol, CODE).await });
    assert_waiting(&join, "join_group").await;

    sqlx::query("DELETE FROM `groups` WHERE id = ?")
        .bind(w.g)
        .execute(&mut *deleter)
        .await
        .unwrap();
    deleter.commit().await.unwrap();

    let reply = join.await.unwrap();
    assert_ne!(reply.status, 500, "{}", reply.body);
    assert!(
        matches!(
            reply.group_err(),
            GroupError::InvalidCode | GroupError::NotFound
        ),
        "{:?}",
        reply.body
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = ?")
        .bind(w.g)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0, "no membership for a deleted group");
}

#[sqlx::test(migrations = "../migrations")]
async fn a_leave_that_races_with_a_delete_fails_cleanly(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut bob = w.bob().await;
    let mut deleter = hold_lock(&pool, w.g).await;

    let leave = tokio::spawn(async move { leave_group(&mut bob, w.g).await });
    assert_waiting(&leave, "leave_group").await;
    sqlx::query("DELETE FROM `groups` WHERE id = ?")
        .bind(w.g)
        .execute(&mut *deleter)
        .await
        .unwrap();
    deleter.commit().await.unwrap();

    let reply = leave.await.unwrap();
    assert_ne!(reply.status, 500, "{}", reply.body);
    assert_eq!(reply.group_err(), GroupError::NotFound);
}

#[sqlx::test(migrations = "../migrations")]
async fn delete_group_does_not_deadlock_with_a_running_join(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    let mut alice = w.alice().await;
    let g = w.g;
    let join = tokio::spawn(async move { join_group(&mut carol, CODE).await });
    let delete = tokio::spawn(async move { delete_group(&mut alice, g).await });

    let (j, d) = tokio::time::timeout(Duration::from_secs(20), async {
        (join.await.unwrap(), delete.await.unwrap())
    })
    .await
    .expect("no deadlock");
    d.group_ok();
    assert_ne!(j.status, 500, "{}", j.body);
    assert!(ts::find_group(&pool, w.g).await.is_none());
    let orphans: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = ?")
        .bind(w.g)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(orphans, 0);
}

// ------------------------------------------------------------ concurrent admin changes

#[sqlx::test(migrations = "../migrations")]
async fn two_concurrent_transfers_by_the_admin_leave_exactly_one_winner(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_member(&pool, w.g, w.carol).await;
    let mut a1 = w.alice().await;
    let mut a2 = w.alice().await;
    let lock = hold_lock(&pool, w.g).await;

    let (g, bob, carol) = (w.g, w.bob, w.carol);
    let to_bob = tokio::spawn(async move { transfer_admin(&mut a1, g, bob).await });
    let to_carol = tokio::spawn(async move { transfer_admin(&mut a2, g, carol).await });
    assert_waiting(&to_bob, "transfer_admin").await;
    assert_waiting(&to_carol, "transfer_admin").await;
    lock.commit().await.unwrap();

    let results = [
        ("bob", to_bob.await.unwrap()),
        ("carol", to_carol.await.unwrap()),
    ];
    let winners: Vec<&str> = results
        .iter()
        .filter(|(_, r)| r.status.is_success())
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(winners.len(), 1, "exactly one transfer wins");
    for (name, reply) in &results {
        if !reply.status.is_success() {
            assert_eq!(
                reply.group_err(),
                GroupError::Forbidden,
                "the loser (to {name}) is no longer admin"
            );
        }
    }
    let admin = ts::group_row(&pool, w.g).await.admin_id;
    assert_eq!(admin, if winners[0] == "bob" { w.bob } else { w.carol });
    assert_eq!(
        ts::member_ids(&pool, w.g).await.len(),
        3,
        "everybody is still a member"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn removing_a_member_who_leaves_at_the_same_time_succeeds_once(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let mut bob = w.bob().await;
    let lock = hold_lock(&pool, w.g).await;

    let (g, bob_id) = (w.g, w.bob);
    let remove = tokio::spawn(async move { remove_member(&mut alice, g, bob_id).await });
    let leave = tokio::spawn(async move { leave_group(&mut bob, g).await });
    assert_waiting(&remove, "remove_member").await;
    lock.commit().await.unwrap();

    let (removed, left) = (remove.await.unwrap(), leave.await.unwrap());
    let oks = [removed.status.is_success(), left.status.is_success()];
    assert_eq!(oks.iter().filter(|ok| **ok).count(), 1, "one of them wins");
    if !removed.status.is_success() {
        assert_eq!(removed.group_err(), GroupError::TargetNotMember);
    }
    if !left.status.is_success() {
        assert_eq!(left.group_err(), GroupError::NotFound);
    }
    assert_eq!(ts::member_ids(&pool, w.g).await, vec![w.alice]);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_transfer_to_a_member_who_leaves_at_the_same_time_never_makes_a_non_member_admin(
    pool: MySqlPool,
) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let mut bob = w.bob().await;
    let lock = hold_lock(&pool, w.g).await;

    let (g, bob_id) = (w.g, w.bob);
    let transfer = tokio::spawn(async move { transfer_admin(&mut alice, g, bob_id).await });
    let leave = tokio::spawn(async move { leave_group(&mut bob, g).await });
    assert_waiting(&transfer, "transfer_admin").await;
    lock.commit().await.unwrap();
    let (t, l) = (transfer.await.unwrap(), leave.await.unwrap());

    let row = ts::group_row(&pool, w.g).await;
    let members = ts::member_ids(&pool, w.g).await;
    assert!(
        members.contains(&row.admin_id),
        "the admin is always a member (D1)"
    );
    // Either order is fine, but not both ways at once.
    assert!(
        t.status.is_success() || l.status.is_success(),
        "{} / {}",
        t.body,
        l.body
    );
}

// ------------------------------------------------------------------- mixed storm

#[sqlx::test(migrations = "../migrations")]
async fn a_storm_of_mixed_calls_never_deadlocks_or_breaks_the_invariants(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_member(&pool, w.g, w.carol).await;
    let mut tasks: Vec<(&str, JoinHandle<crate::common::auth::Reply>)> = Vec::new();
    let (g, bob, carol, dave) = (w.g, w.bob, w.carol, w.dave);

    let mut c = w.dave().await;
    tasks.push((
        "join",
        tokio::spawn(async move { join_group(&mut c, CODE).await }),
    ));
    let mut c = w.bob().await;
    tasks.push((
        "leave",
        tokio::spawn(async move { leave_group(&mut c, g).await }),
    ));
    let mut c = w.alice().await;
    tasks.push((
        "update",
        tokio::spawn(async move { update_group(&mut c, g, "Renamed", "", "2030-01-01", "").await }),
    ));
    let mut c = w.alice().await;
    tasks.push((
        "regenerate",
        tokio::spawn(async move { regenerate_invitation(&mut c, g).await }),
    ));
    let mut c = w.alice().await;
    tasks.push((
        "remove",
        tokio::spawn(async move { remove_member(&mut c, g, carol).await }),
    ));
    let mut c = w.alice().await;
    tasks.push((
        "transfer",
        tokio::spawn(async move { transfer_admin(&mut c, g, bob).await }),
    ));
    let mut c = w.carol().await;
    tasks.push((
        "list",
        tokio::spawn(async move { list_my_groups(&mut c).await }),
    ));
    let mut c = w.bob().await;
    tasks.push((
        "get",
        tokio::spawn(async move { get_group(&mut c, g).await }),
    ));
    let _ = dave;

    let all = tokio::time::timeout(Duration::from_secs(30), async {
        let mut out = Vec::new();
        for (name, handle) in tasks {
            out.push((name, handle.await.unwrap()));
        }
        out
    })
    .await
    .expect("no deadlock");
    for (name, reply) in &all {
        assert_ne!(reply.status, 500, "{name}: {}", reply.body);
    }

    let row = ts::group_row(&pool, w.g).await;
    let members = ts::member_ids(&pool, w.g).await;
    assert!(
        members.contains(&row.admin_id),
        "the admin is a member: {members:?}"
    );
    let mut unique = members.clone();
    unique.dedup();
    assert_eq!(unique, members);
    let _ = groups_snapshot(&pool).await;
}
