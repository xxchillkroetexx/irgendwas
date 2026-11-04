//! draw slice: reset (AC40-AC43) and deleting a drawn group (AC45).

use crate::common::draw::*;
use crate::common::exclusions::{ExReply, add_ex, rules};
use crate::common::groups::*;
use crate::common::wishlists::*;
use app::draw_api::DrawApiError;
use app::test_support::{self as ts, groups_snapshot};
use sqlx::MySqlPool;

// ------------------------------------------------------------------------ AC40

#[sqlx::test(migrations = "../migrations")]
async fn ac40_a_reset_deletes_the_assignments_and_tells_the_givers(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    rename(&pool, d.alice, "AliceRs").await;
    rename(&pool, d.bob, "BobRs").await;
    rename(&pool, d.carol, "CarolRs").await;
    d.draw_and_deliver(d.g).await;
    let mut alice = d.alice().await;

    reset(&mut alice, d.g).await.dr_ok();
    d.idle().await;

    assert!(assignments(&pool, d.g).await.is_empty());
    assert!(!is_drawn(&pool, d.g).await);
    assert_eq!(drawn_at(&pool, d.g).await, None);
    let info = alice.session_info().await;
    assert!(
        info.flash
            .iter()
            .any(|m| m.key == "flash.success.draw_reset"),
        "{:?}",
        info.flash
    );

    // Bob and Carol: one reset mail each, without any receiver name. The admin's name is in
    // it (who reset), the third member's is not.
    for (user, third) in [(d.bob, "CarolRs"), (d.carol, "BobRs")] {
        let resets = d.reset_mails(user, GROUP_NAME).await;
        assert_eq!(resets.len(), 1, "{}", d.name_of(user));
        let content = everything(&resets[0]);
        assert!(content.contains("AliceRs"), "names the admin who reset");
        assert!(!content.contains(third), "reset mail names {third}");
        assert!(content.contains(&format!("{APP_URL}/groups/{}", d.g)));
    }
    // Alice reset it herself: no reset mail for her.
    assert_eq!(d.reset_mails(d.alice, GROUP_NAME).await.len(), 0);
    assert_eq!(d.draw_mails(d.alice, GROUP_NAME).await.len(), 1);

    for mut c in [d.alice().await, d.bob().await, d.carol().await] {
        assert_eq!(my_assignment(&mut c, d.g).await, None);
        assert!(list_mine(&mut c).await.is_empty());
    }
    let dash = d.bob().await.get("/user/dashboard").await;
    assert_eq!(testid_count(&dash.body, "dashboard-assignment"), 0);
    let page = d.bob().await.get(&format!("/groups/{}", d.g)).await;
    assert_eq!(testid_count(&page.body, "my-assignment"), 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_a_reset_keeps_rules_and_wishlists(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut bob = d.bob().await;
    add_ex(&mut bob, d.g, d.alice).await;
    add_item(&mut bob, d.g, "Socks", "", "").await;
    d.draw_and_deliver(d.g).await;
    let before = groups_snapshot(&pool).await;

    reset(&mut d.alice().await, d.g).await.dr_ok();
    d.idle().await;

    let after = groups_snapshot(&pool).await;
    assert_eq!(after.exclusions, before.exclusions);
    assert_eq!(after.wishlists, before.wishlists);
    assert_eq!(after.items, before.items);
    assert_eq!(after.members, before.members);
    assert_eq!(rules(&pool, d.g).await, vec![(d.bob, d.alice)]);
}

// ------------------------------------------------------------------------ AC41

#[sqlx::test(migrations = "../migrations")]
async fn ac41_after_a_reset_the_group_behaves_like_a_new_one(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    let mut alice = d.alice().await;
    reset(&mut alice, d.g).await.dr_ok();
    d.idle().await;

    // Exclusions can be edited again.
    let mut bob = d.bob().await;
    add_ex(&mut bob, d.g, d.carol).await.ex_ok();
    // Dave can join, Bob can leave, Alice can remove Carol.
    join_group(&mut d.dave().await, CODE).await.group_ok();
    leave_group(&mut bob, d.g).await.group_ok();
    remove_member(&mut alice, d.g, d.carol).await.group_ok();
    assert_eq!(ts::member_ids(&pool, d.g).await, vec![d.alice, d.dave]);

    // A new draw works and mails the current members.
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    assert_eq!(
        assignments(&pool, d.g).await,
        vec![(d.alice, d.dave), (d.dave, d.alice)]
    );
    assert_eq!(
        d.draw_mails(d.alice, GROUP_NAME).await.len(),
        2,
        "old and new"
    );
    assert_eq!(d.draw_mails(d.dave, GROUP_NAME).await.len(), 1);
    assert_eq!(
        d.draw_mails(d.bob, GROUP_NAME).await.len(),
        1,
        "only the old one"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac41_a_second_reset_needs_a_new_draw(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    let mut alice = d.alice().await;
    reset(&mut alice, d.g).await.dr_ok();
    assert_eq!(
        reset(&mut alice, d.g).await.dr_err(),
        DrawApiError::NotDrawn
    );
    d.draw_and_deliver(d.g).await;
    reset(&mut alice, d.g).await.dr_ok();
    d.idle().await;
}

// ------------------------------------------------------------------------ AC42

#[sqlx::test(migrations = "../migrations")]
async fn ac42_only_the_admin_may_reset_and_only_a_drawn_group(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    // Not drawn.
    assert_eq!(
        reset(&mut d.alice().await, d.g).await.dr_err(),
        DrawApiError::NotDrawn
    );
    d.draw_and_deliver(d.g).await;
    let before = groups_snapshot(&pool).await;
    assert_eq!(
        reset(&mut d.bob().await, d.g).await.dr_err(),
        DrawApiError::Forbidden
    );
    assert_eq!(
        reset(&mut d.dave().await, d.g).await.dr_err(),
        DrawApiError::NotFound
    );
    assert_eq!(
        reset(&mut d.anon(), d.g).await.dr_err(),
        DrawApiError::Unauthorized
    );
    d.idle().await;
    assert_eq!(groups_snapshot(&pool).await, before);
    for user in [d.bob, d.carol] {
        assert_eq!(d.reset_mails(user, GROUP_NAME).await.len(), 0);
    }
}

// ------------------------------------------------------------------------ AC43

/// The draw mails sit in the worker while the mailer is blocked; at most the one mail the
/// worker already had in flight (spec: "a draw mail may already be in flight while a reset
/// commits", accepted) may still be delivered, never one for a deleted row that the worker
/// had not started.
#[sqlx::test(migrations = "../migrations")]
async fn ac43_a_reset_drops_the_pending_draw_mails_and_sends_the_reset_mails(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.block();
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    tokio::task::yield_now().await;
    reset(&mut alice, d.g).await.dr_ok();
    assert!(assignments(&pool, d.g).await.is_empty());

    d.transport.unblock();
    d.idle().await;

    let mut draw_mails = 0;
    for user in [d.alice, d.bob, d.carol] {
        draw_mails += d.draw_mails(user, GROUP_NAME).await.len();
    }
    assert!(
        draw_mails <= 1,
        "{draw_mails} draw mails delivered for deleted rows"
    );
    assert!(
        d.transport.attempts_total() <= 3,
        "attempts: {}",
        d.transport.attempts_total()
    );
    assert_eq!(d.reset_mails(d.bob, GROUP_NAME).await.len(), 1);
    assert_eq!(d.reset_mails(d.carol, GROUP_NAME).await.len(), 1);
    assert_eq!(d.reset_mails(d.alice, GROUP_NAME).await.len(), 0);
}

// ------------------------------------------------------------------------ AC45

#[sqlx::test(migrations = "../migrations")]
async fn ac45_deleting_a_drawn_group_removes_its_assignments(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    delete_group(&mut d.alice().await, d.g).await.group_ok();
    assert!(assignments(&pool, d.g).await.is_empty());
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM gift_assignments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 0);
    assert!(list_mine(&mut d.bob().await).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac45_the_worker_does_not_send_the_pending_mails_of_a_deleted_group(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.block();
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    tokio::task::yield_now().await;
    delete_group(&mut alice, d.g).await.group_ok();

    d.transport.unblock();
    d.idle().await;
    let mut delivered = 0;
    for user in [d.alice, d.bob, d.carol] {
        delivered += d.draw_mails(user, GROUP_NAME).await.len();
    }
    // At most the one mail that was already in flight when the group was deleted.
    assert!(delivered <= 1, "{delivered} mails for a deleted group");
}
