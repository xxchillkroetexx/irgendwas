//! exclusions slice: the group row lock (AC23, AC24, D9). Writers take the group row
//! `FOR UPDATE`, then check membership, `is_drawn`, the cap and feasibility under it.

use std::time::Duration;

use crate::common::exclusions::*;
use crate::common::groups::*;
use app::exclusions_api::ExclusionError;
use app::server::membership::remove_member_data;
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

// ------------------------------------------------------------------------ AC23

#[sqlx::test(migrations = "../migrations")]
async fn ac23_two_rules_that_are_only_jointly_impossible_cannot_both_be_added(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.dave()).await;
    let mut bob = x.w.bob().await;
    let mut carol = x.w.carol().await;
    let tx = hold_lock(&pool, x.g()).await;
    let g = x.g();
    let dave = x.dave();
    let b = tokio::spawn(async move { add_ex(&mut bob, g, dave).await });
    let c = tokio::spawn(async move { add_ex(&mut carol, g, dave).await });
    assert_waiting(&b, "Bob's add").await;
    assert_waiting(&c, "Carol's add").await;
    tx.commit().await.unwrap();

    let results = [b.await.unwrap(), c.await.unwrap()];
    let ok = results.iter().filter(|r| r.status.is_success()).count();
    assert_eq!(ok, 1, "exactly one succeeds: {:?}", results.map(|r| r.body));
    let failed = results.iter().find(|r| !r.status.is_success()).unwrap();
    assert_eq!(failed.ex_err(), ExclusionError::WouldMakeDrawImpossible);
    let stored = rules(&pool, x.g()).await;
    assert_eq!(stored.len(), 2, "{stored:?}");
    assert!(stored.contains(&(x.alice(), x.dave())));
    assert_eq!(stored.iter().filter(|r| r.1 == x.dave()).count(), 2);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_a_waiting_add_sees_the_rule_committed_by_the_lock_holder(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.dave()).await;
    let mut carol = x.w.carol().await;
    let mut tx = hold_lock(&pool, x.g()).await;
    let g = x.g();
    let dave = x.dave();
    let add = tokio::spawn(async move { add_ex(&mut carol, g, dave).await });
    assert_waiting(&add, "Carol's add").await;

    // What Bob's call does under the lock: insert his rule, then commit.
    sqlx::query(
        "INSERT INTO exclusion_rules (group_id, user_id, excluded_user_id) VALUES (?, ?, ?)",
    )
    .bind(x.g())
    .bind(x.bob())
    .bind(x.dave())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(
        add.await.unwrap().ex_err(),
        ExclusionError::WouldMakeDrawImpossible
    );
    assert_eq!(
        rules(&pool, x.g()).await,
        [(x.alice(), x.dave()), (x.bob(), x.dave())]
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_the_cap_is_checked_under_the_lock_too(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let (g, extra) = x.g8(&pool).await;
    for target in &extra[..3] {
        ts::add_exclusion(&pool, g, x.bob(), *target).await;
    }
    // Two tabs of Bob add the 4th and 5th rule at once.
    let mut tab1 = x.w.bob().await;
    let mut tab2 = x.w.bob().await;
    let tx = hold_lock(&pool, g).await;
    let (t1, t2) = (extra[3], extra[4]);
    let a = tokio::spawn(async move { add_ex(&mut tab1, g, t1).await });
    let b = tokio::spawn(async move { add_ex(&mut tab2, g, t2).await });
    assert_waiting(&a, "tab 1").await;
    assert_waiting(&b, "tab 2").await;
    tx.commit().await.unwrap();
    let results = [a.await.unwrap(), b.await.unwrap()];
    assert_eq!(results.iter().filter(|r| r.status.is_success()).count(), 2);
    assert_eq!(
        rules(&pool, g)
            .await
            .iter()
            .filter(|r| r.0 == x.bob())
            .count(),
        5
    );
}

// ------------------------------------------------------------------------ AC23 (cap)

#[sqlx::test(migrations = "../migrations")]
async fn ac23_two_tabs_cannot_both_take_the_last_slot(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let (g, extra) = x.g8(&pool).await;
    for target in &extra[..4] {
        ts::add_exclusion(&pool, g, x.bob(), *target).await;
    }
    let mut tab1 = x.w.bob().await;
    let mut tab2 = x.w.bob().await;
    let tx = hold_lock(&pool, g).await;
    let (t1, t2) = (extra[4], extra[5]);
    let a = tokio::spawn(async move { add_ex(&mut tab1, g, t1).await });
    let b = tokio::spawn(async move { add_ex(&mut tab2, g, t2).await });
    assert_waiting(&a, "tab 1").await;
    assert_waiting(&b, "tab 2").await;
    tx.commit().await.unwrap();
    let results = [a.await.unwrap(), b.await.unwrap()];
    let ok = results.iter().filter(|r| r.status.is_success()).count();
    assert_eq!(ok, 1, "{:?}", results.map(|r| r.body));
    let failed = results.iter().find(|r| !r.status.is_success()).unwrap();
    assert_eq!(
        failed.ex_err(),
        ExclusionError::TooManyExclusions { max: 5 }
    );
    assert_eq!(
        rules(&pool, g)
            .await
            .iter()
            .filter(|r| r.0 == x.bob())
            .count(),
        5
    );
}

// ------------------------------------------------------------------------ AC24

#[sqlx::test(migrations = "../migrations")]
async fn ac24_add_and_remove_wait_for_a_draw_and_then_report_it(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let mut bob_add = x.w.bob().await;
    let mut bob_remove = x.w.bob().await;
    let mut tx = hold_lock(&pool, x.g()).await;
    let (g, dave, carol) = (x.g(), x.dave(), x.carol());
    let add = tokio::spawn(async move { add_ex(&mut bob_add, g, dave).await });
    let remove = tokio::spawn(async move { remove_ex(&mut bob_remove, g, carol).await });
    assert_waiting(&add, "add_exclusion").await;
    assert_waiting(&remove, "remove_exclusion").await;

    sqlx::query("UPDATE `groups` SET is_drawn = 1 WHERE id = ?")
        .bind(x.g())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(add.await.unwrap().ex_err(), ExclusionError::AlreadyDrawn);
    assert_eq!(remove.await.unwrap().ex_err(), ExclusionError::AlreadyDrawn);
    assert_eq!(rules(&pool, x.g()).await, [(x.bob(), x.carol())]);
}

// ------------------------------------------------------------ membership changes

/// What `remove_member` does under the lock.
async fn remove_under_lock(tx: &mut Transaction<'static, MySql>, group: i32, user: i32) {
    remove_member_data(tx, group, user)
        .await
        .expect("remove_member_data");
}

#[sqlx::test(migrations = "../migrations")]
async fn a_waiting_add_finds_its_caller_gone(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut carol = x.w.carol().await;
    let mut tx = hold_lock(&pool, x.g()).await;
    let (g, dave) = (x.g(), x.dave());
    let add = tokio::spawn(async move { add_ex(&mut carol, g, dave).await });
    assert_waiting(&add, "add_exclusion").await;
    remove_under_lock(&mut tx, x.g(), x.carol()).await;
    tx.commit().await.unwrap();
    assert_eq!(add.await.unwrap().ex_err(), ExclusionError::NotFound);
    assert!(all_rules(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn a_waiting_add_finds_its_target_gone(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    let mut tx = hold_lock(&pool, x.g()).await;
    let (g, carol) = (x.g(), x.carol());
    let add = tokio::spawn(async move { add_ex(&mut bob, g, carol).await });
    assert_waiting(&add, "add_exclusion").await;
    remove_under_lock(&mut tx, x.g(), x.carol()).await;
    tx.commit().await.unwrap();
    assert_eq!(add.await.unwrap().ex_err(), ExclusionError::TargetNotMember);
    assert!(all_rules(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn a_member_leaving_while_a_rule_is_added_leaves_no_orphan_rule(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    let mut dave = x.w.dave().await;
    let (g, d) = (x.g(), x.dave());
    let add = tokio::spawn(async move { add_ex(&mut bob, g, d).await });
    let leave = tokio::spawn(async move { leave_group(&mut dave, g).await });
    let (add, leave) = (add.await.unwrap(), leave.await.unwrap());
    leave.group_ok();
    // Either order is fine; afterwards no rule may point at Dave.
    assert!(
        add.status.is_success() || add.ex_err() == ExclusionError::TargetNotMember,
        "{}",
        add.body
    );
    assert!(
        rules(&pool, x.g())
            .await
            .iter()
            .all(|r| r.0 != x.dave() && r.1 != x.dave())
    );
    assert_eq!(ts::member_ids(&pool, x.g()).await.len(), 3);
}
