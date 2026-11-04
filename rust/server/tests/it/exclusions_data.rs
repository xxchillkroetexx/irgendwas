//! exclusions slice: the data contract for the draw (`load_rules`, AC30) and the composite
//! member foreign keys (AC22).

use crate::common::exclusions::*;
use app::server::exclusions::load_rules;
use app::test_support as ts;
use sqlx::MySqlPool;

// ------------------------------------------------------------------------ AC30

#[sqlx::test(migrations = "../migrations")]
async fn ac30_load_rules_returns_the_groups_pairs_in_order(pool: MySqlPool) {
    let x = x_world(&pool).await;
    // Inserted out of order, into two groups.
    let other = x
        .group_of("Other", &[x.bob(), x.carol(), x.eve], &pool)
        .await;
    ts::add_exclusion(&pool, x.g(), x.dave(), x.alice()).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.dave()).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.alice()).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.carol()).await;
    ts::add_exclusion(&pool, other, x.carol(), x.eve).await;
    ts::add_exclusion(&pool, other, x.bob(), x.carol()).await;

    let mut tx = pool.begin().await.unwrap();
    let main = load_rules(&mut tx, x.g()).await.unwrap();
    let side = load_rules(&mut tx, other).await.unwrap();
    let none = load_rules(&mut tx, 999_999).await.unwrap();
    tx.rollback().await.unwrap();

    assert_eq!(
        main,
        [
            (x.alice(), x.carol()),
            (x.bob(), x.alice()),
            (x.bob(), x.dave()),
            (x.dave(), x.alice()),
        ]
    );
    assert_eq!(side, [(x.bob(), x.carol()), (x.carol(), x.eve)]);
    assert!(none.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac30_load_rules_of_an_empty_group_is_empty(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    assert!(load_rules(&mut tx, x.g()).await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac30_load_rules_holds_only_pairs_between_current_members(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::add_exclusion(&pool, x.g(), x.carol(), x.dave()).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.dave()).await;
    // Carol leaves through the shared helper (groups D5).
    let mut tx = pool.begin().await.unwrap();
    app::server::membership::remove_member_data(&mut tx, x.g(), x.carol())
        .await
        .unwrap();
    let rules = load_rules(&mut tx, x.g()).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(rules, [(x.alice(), x.dave())]);
}

// ------------------------------------------------------------------------ AC22

#[sqlx::test(migrations = "../migrations")]
async fn ac22_deleting_a_membership_row_removes_the_rules_in_both_directions(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::add_exclusion(&pool, x.g(), x.carol(), x.bob()).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.bob()).await;
    let other = x.group_of("Other", &[x.bob(), x.carol()], &pool).await;
    ts::add_exclusion(&pool, other, x.bob(), x.carol()).await;

    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(x.g())
        .bind(x.carol())
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(rules(&pool, x.g()).await, [(x.alice(), x.bob())]);
    assert_eq!(
        rules(&pool, other).await,
        [(x.bob(), x.carol())],
        "other groups keep their rules"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_a_rule_needs_two_current_members_of_the_group(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let insert = |giver: i32, excluded: i32| {
        let pool = pool.clone();
        let g = x.g();
        async move {
            sqlx::query(
                "INSERT INTO exclusion_rules (group_id, user_id, excluded_user_id) VALUES (?, ?, ?)",
            )
            .bind(g)
            .bind(giver)
            .bind(excluded)
            .execute(&pool)
            .await
        }
    };
    // The excluded user is no member of G (Eve exists as a user).
    let err = insert(x.bob(), x.eve)
        .await
        .expect_err("excluded not a member");
    assert!(
        err.as_database_error()
            .is_some_and(|e| e.is_foreign_key_violation()),
        "{err}"
    );
    // The giver is no member either.
    let err = insert(x.eve, x.bob())
        .await
        .expect_err("giver not a member");
    assert!(
        err.as_database_error()
            .is_some_and(|e| e.is_foreign_key_violation()),
        "{err}"
    );
    // A member of another group only does not count.
    let other = x.group_of("Other", &[x.eve], &pool).await;
    assert!(other > 0);
    assert!(insert(x.bob(), x.eve).await.is_err());
    // Control: two members work.
    insert(x.bob(), x.carol()).await.expect("valid rule");
    assert_eq!(rules(&pool, x.g()).await, [(x.bob(), x.carol())]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_the_existing_cascades_and_unique_key_stay(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let dup = sqlx::query(
        "INSERT INTO exclusion_rules (group_id, user_id, excluded_user_id) VALUES (?, ?, ?)",
    )
    .bind(x.g())
    .bind(x.bob())
    .bind(x.carol())
    .execute(&pool)
    .await
    .expect_err("duplicate");
    assert!(
        dup.as_database_error()
            .is_some_and(|e| e.is_unique_violation()),
        "{dup}"
    );
    // Deleting the group removes its rules.
    sqlx::query("DELETE FROM `groups` WHERE id = ?")
        .bind(x.g())
        .execute(&pool)
        .await
        .unwrap();
    assert!(all_rules(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_leaving_through_the_app_keeps_working_with_the_new_keys(pool: MySqlPool) {
    use crate::common::groups::*;
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::add_exclusion(&pool, x.g(), x.carol(), x.dave()).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.dave()).await;
    let mut carol = x.w.carol().await;
    leave_group(&mut carol, x.g()).await.group_ok();
    assert_eq!(rules(&pool, x.g()).await, [(x.alice(), x.dave())]);
    // Remove (admin) and rejoin: the old rules do not come back.
    let mut alice = x.w.alice().await;
    remove_member(&mut alice, x.g(), x.bob()).await.group_ok();
    assert_eq!(rules(&pool, x.g()).await, [(x.alice(), x.dave())]);
}
