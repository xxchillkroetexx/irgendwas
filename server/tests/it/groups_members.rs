//! groups slice: leaving, removing a member, transferring the admin role and the shared
//! `remove_member_data` (AC35-AC43, D5).

use crate::common::auth::*;
use crate::common::groups::*;
use app::groups_api::GroupError;
use app::server::membership::remove_member_data;
use app::test_support::{self as ts, groups_snapshot};
use sqlx::MySqlPool;

async fn count(pool: &MySqlPool, sql: &'static str, a: i32, b: i32) -> i64 {
    sqlx::query_scalar(sql)
        .bind(a)
        .bind(b)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn wishlists_of(pool: &MySqlPool, group: i32, user: i32) -> i64 {
    count(
        pool,
        "SELECT COUNT(*) FROM wishlists WHERE group_id = ? AND user_id = ?",
        group,
        user,
    )
    .await
}

async fn items_of(pool: &MySqlPool, group: i32, user: i32) -> i64 {
    count(
        pool,
        "SELECT COUNT(*) FROM wishlist_items i JOIN wishlists w ON w.id = i.wishlist_id \
         WHERE w.group_id = ? AND w.user_id = ?",
        group,
        user,
    )
    .await
}

/// G: Alice (admin), Bob and Carol; wishlists for Alice and Bob; rules Bob->Alice,
/// Alice->Bob and Carol->Alice. Second group H with Alice and Bob and data of its own.
struct Seeded {
    w: W,
    h: i32,
}

async fn seed(pool: &MySqlPool) -> Seeded {
    let w = world(pool).await;
    ts::add_member(pool, w.g, w.carol).await;
    ts::create_wishlist(pool, w.g, w.alice, &["Alice socks", "Alice book"]).await;
    ts::create_wishlist(pool, w.g, w.bob, &["Bob tea", "Bob mug"]).await;
    ts::add_exclusion(pool, w.g, w.bob, w.alice).await;
    ts::add_exclusion(pool, w.g, w.alice, w.bob).await;
    ts::add_exclusion(pool, w.g, w.carol, w.alice).await;
    let h = ts::create_group_with_code(pool, w.alice, "Other", "OTHR2345").await;
    ts::add_member(pool, h, w.bob).await;
    ts::create_wishlist(pool, h, w.bob, &["Bob in H"]).await;
    ts::add_exclusion(pool, h, w.bob, w.alice).await;
    ts::add_exclusion(pool, h, w.alice, w.bob).await;
    Seeded { w, h }
}

/// Bob's data in G is gone, everyone else's and all of H is untouched.
async fn assert_bob_gone_from_g(pool: &MySqlPool, s: &Seeded) {
    let w = &s.w;
    assert_eq!(ts::member_ids(pool, w.g).await, vec![w.alice, w.carol]);
    assert_eq!(wishlists_of(pool, w.g, w.bob).await, 0, "Bob's wishlist");
    assert_eq!(items_of(pool, w.g, w.bob).await, 0, "Bob's items");
    let rules: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT user_id, excluded_user_id FROM exclusion_rules WHERE group_id = ? ORDER BY 1, 2",
    )
    .bind(w.g)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        rules,
        vec![(w.carol, w.alice)],
        "both rules with Bob are gone"
    );
    // Alice's own data stays.
    assert_eq!(wishlists_of(pool, w.g, w.alice).await, 1);
    assert_eq!(items_of(pool, w.g, w.alice).await, 2);
    // The other group is untouched.
    assert_eq!(ts::member_ids(pool, s.h).await, vec![w.alice, w.bob]);
    assert_eq!(wishlists_of(pool, s.h, w.bob).await, 1);
    assert_eq!(items_of(pool, s.h, w.bob).await, 1);
    let h_rules: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM exclusion_rules WHERE group_id = ?")
            .bind(s.h)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(h_rules, 2);
}

// ------------------------------------------------------------------------- AC35

#[sqlx::test(migrations = "../migrations")]
async fn ac35_leaving_deletes_the_members_data_in_that_group_only(pool: MySqlPool) {
    let s = seed(&pool).await;
    let mut bob = s.w.bob().await;
    assert_eq!(wishlists_of(&pool, s.w.g, s.w.bob).await, 1);
    assert_eq!(rules_involving_both(&pool, s.w.g, s.w.bob).await, 2);

    leave_group(&mut bob, s.w.g).await.group_ok();

    assert_bob_gone_from_g(&pool, &s).await;
    assert!(
        summaries(&mut bob).await.iter().all(|g| g.id != s.w.g),
        "G is no longer in his list"
    );
    assert_eq!(
        get_group(&mut bob, s.w.g).await.group_err(),
        GroupError::NotFound
    );
    assert_eq!(bob.get(&format!("/groups/{}", s.w.g)).await.status, 404);
}

async fn rules_involving_both(pool: &MySqlPool, group: i32, user: i32) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM exclusion_rules WHERE group_id = ? \
         AND (user_id = ? OR excluded_user_id = ?)",
    )
    .bind(group)
    .bind(user)
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn ac35_rejoining_revives_nothing(pool: MySqlPool) {
    let s = seed(&pool).await;
    let mut bob = s.w.bob().await;
    leave_group(&mut bob, s.w.g).await.group_ok();
    join_group(&mut bob, CODE).await.group_ok();

    assert_eq!(wishlists_of(&pool, s.w.g, s.w.bob).await, 0);
    assert_eq!(rules_involving_both(&pool, s.w.g, s.w.bob).await, 0);
    assert_eq!(items_of(&pool, s.w.g, s.w.bob).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac35_a_plain_form_post_ends_on_the_list_with_the_flash(pool: MySqlPool) {
    let s = seed(&pool).await;
    let mut bob = s.w.bob().await;
    bob.headers.push(("accept".into(), "text/html".into()));

    let reply = leave_group(&mut bob, s.w.g).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location().as_deref(), Some("/groups"));
    let page = bob.get("/groups").await;
    assert!(html_contains(&page.body, &en("flash.success.left_group")));
    assert!(!page.body.contains("Family"));
}

// ------------------------------------------------------------------------- AC36

#[sqlx::test(migrations = "../migrations")]
async fn ac36_the_admin_cannot_leave(pool: MySqlPool) {
    let s = seed(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut alice = s.w.alice().await;

    let reply = leave_group(&mut alice, s.w.g).await;
    assert!(reply.status.is_client_error(), "{}", reply.status);
    assert_eq!(reply.group_err(), GroupError::AdminCannotLeave);
    assert_eq!(groups_snapshot(&pool).await, before);
    assert!(!detail(&mut alice, s.w.g).await.can_leave);
}

// ------------------------------------------------------------------------- AC37

#[sqlx::test(migrations = "../migrations")]
async fn ac37_after_the_draw_nobody_leaves(pool: MySqlPool) {
    let s = seed(&pool).await;
    let w = &s.w;
    ts::add_assignment(&pool, w.g, w.alice, w.bob).await;
    ts::add_assignment(&pool, w.g, w.bob, w.carol).await;
    ts::add_assignment(&pool, w.g, w.carol, w.alice).await;
    ts::set_drawn(&pool, w.g).await;
    let before = groups_snapshot(&pool).await;
    let mut bob = w.bob().await;

    let reply = leave_group(&mut bob, w.g).await;
    assert_eq!(reply.group_err(), GroupError::AlreadyDrawn);
    assert_eq!(groups_snapshot(&pool).await, before);

    let d = detail(&mut bob, w.g).await;
    assert!(!d.can_leave);
    let page = bob.get(&format!("/groups/{}", w.g)).await;
    assert!(
        !html_contains(&page.body, &en("group.show.leaveGroup")),
        "no Leave button"
    );
    assert!(html_contains(
        &page.body,
        &en("group.show.cannotLeaveAfterDraw")
    ));
}

// ------------------------------------------------------------------------- AC38

#[sqlx::test(migrations = "../migrations")]
async fn ac38_a_non_member_cannot_leave(pool: MySqlPool) {
    let s = seed(&pool).await;
    let before = groups_snapshot(&pool).await;
    let reply = leave_group(&mut s.w.dave().await, s.w.g).await;
    assert_eq!(reply.status, 404);
    assert_eq!(reply.group_err(), GroupError::NotFound);
    let reply = leave_group(&mut s.w.dave().await, 999_999).await;
    assert_eq!(reply.group_err(), GroupError::NotFound);
    assert_eq!(groups_snapshot(&pool).await, before);
}

// ------------------------------------------------------------------------- AC39

#[sqlx::test(migrations = "../migrations")]
async fn ac39_removing_a_member_deletes_the_same_data_as_leaving(pool: MySqlPool) {
    let s = seed(&pool).await;
    // Defensive: assignment rows of the member in an undrawn group go too.
    ts::add_assignment(&pool, s.w.g, s.w.bob, s.w.carol).await;
    ts::add_assignment(&pool, s.w.g, s.w.alice, s.w.bob).await;
    let mut alice = s.w.alice().await;

    remove_member(&mut alice, s.w.g, s.w.bob).await.group_ok();

    assert_bob_gone_from_g(&pool, &s).await;
    let leftovers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM gift_assignments WHERE group_id = ? AND (giver_id = ? OR receiver_id = ?)",
    )
    .bind(s.w.g)
    .bind(s.w.bob)
    .bind(s.w.bob)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(leftovers, 0);

    let mut bob = s.w.bob().await;
    assert_eq!(bob.get(&format!("/groups/{}", s.w.g)).await.status, 404);
    assert_eq!(
        get_group(&mut bob, s.w.g).await.group_err(),
        GroupError::NotFound
    );

    let d = detail(&mut alice, s.w.g).await;
    assert_eq!(d.members.len(), 2);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac39_a_removed_member_can_rejoin_with_the_code(pool: MySqlPool) {
    let s = seed(&pool).await;
    remove_member(&mut s.w.alice().await, s.w.g, s.w.bob)
        .await
        .group_ok();
    // D12: removal and the code are separate; a leaked code needs a regeneration.
    assert_eq!(
        join_group(&mut s.w.bob().await, CODE).await.value::<i32>(),
        s.w.g
    );
}

// ------------------------------------------------------------------------- AC40

#[sqlx::test(migrations = "../migrations")]
async fn ac40_remove_member_errors_change_nothing(pool: MySqlPool) {
    let s = seed(&pool).await;
    let w = &s.w;
    let before = groups_snapshot(&pool).await;
    let mut alice = w.alice().await;

    assert_eq!(
        remove_member(&mut alice, w.g, w.alice).await.group_err(),
        GroupError::CannotRemoveSelf
    );
    assert_eq!(
        remove_member(&mut alice, w.g, w.dave).await.group_err(),
        GroupError::TargetNotMember
    );
    assert_eq!(
        remove_member(&mut alice, w.g, 999_999).await.group_err(),
        GroupError::TargetNotMember
    );
    let reply = remove_member(&mut w.bob().await, w.g, w.alice).await;
    assert_eq!(reply.status, 403);
    assert_eq!(reply.group_err(), GroupError::Forbidden);
    assert_eq!(
        remove_member(&mut w.bob().await, w.g, w.carol)
            .await
            .group_err(),
        GroupError::Forbidden
    );
    let reply = remove_member(&mut w.dave().await, w.g, w.bob).await;
    assert_eq!(reply.status, 404);
    assert_eq!(reply.group_err(), GroupError::NotFound);

    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_after_the_draw_nobody_is_removed(pool: MySqlPool) {
    let s = seed(&pool).await;
    let w = &s.w;
    ts::add_assignment(&pool, w.g, w.alice, w.bob).await;
    ts::add_assignment(&pool, w.g, w.bob, w.alice).await;
    ts::set_drawn(&pool, w.g).await;
    let before = groups_snapshot(&pool).await;

    assert_eq!(
        remove_member(&mut w.alice().await, w.g, w.bob)
            .await
            .group_err(),
        GroupError::AlreadyDrawn
    );
    assert_eq!(groups_snapshot(&pool).await, before);
    let page = w.alice().await.get(&format!("/groups/{}", w.g)).await;
    assert!(
        !html_contains(&page.body, &en("group.show.removeMember")),
        "no Remove button"
    );
    assert!(
        html_contains(&page.body, &en("group.show.makeAdmin")),
        "transfer still offered"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_the_target_is_checked_against_the_group_in_the_request(pool: MySqlPool) {
    // Carol is admin of H; Alice is only a member of G. Neither the group id nor the user
    // id of another group may be used to reach into H.
    let w = world(&pool).await;
    let h = ts::create_group_with_code(&pool, w.carol, "Carols", "CARL2345").await;
    ts::add_member(&pool, h, w.dave).await;
    ts::create_wishlist(&pool, h, w.dave, &["Dave item"]).await;
    let before = groups_snapshot(&pool).await;
    let mut alice = w.alice().await;

    // Dave is a member of H, not of G.
    assert_eq!(
        remove_member(&mut alice, w.g, w.dave).await.group_err(),
        GroupError::TargetNotMember
    );
    // Alice is no member of H at all.
    assert_eq!(
        remove_member(&mut alice, h, w.dave).await.group_err(),
        GroupError::NotFound
    );
    // Carol administers H but not G.
    let mut carol = w.carol().await;
    assert_eq!(
        remove_member(&mut carol, w.g, w.bob).await.group_err(),
        GroupError::NotFound
    );
    assert_eq!(
        transfer_admin(&mut carol, w.g, w.carol).await.group_err(),
        GroupError::NotFound
    );
    assert_eq!(groups_snapshot(&pool).await, before);
}

// ------------------------------------------------------------------------- AC41

#[sqlx::test(migrations = "../migrations")]
async fn ac41_transferring_hands_the_admin_role_to_the_target_and_keeps_the_old_admin(
    pool: MySqlPool,
) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let mut bob = w.bob().await;

    transfer_admin(&mut alice, w.g, w.bob).await.group_ok();

    assert_eq!(ts::group_row(&pool, w.g).await.admin_id, w.bob);
    assert_eq!(ts::member_ids(&pool, w.g).await, vec![w.alice, w.bob]);

    let b = detail(&mut bob, w.g).await;
    assert!(b.is_admin && !b.can_leave);
    assert_eq!(b.admin.name, "Bob");
    let inv = b.invitation.expect("the new admin sees the invitation");
    assert_eq!(inv.code, CODE);
    assert!(
        b.members.iter().all(|m| m.email.is_some()),
        "and the emails"
    );

    let a = detail(&mut alice, w.g).await;
    assert!(!a.is_admin && a.can_leave);
    assert!(a.invitation.is_none());
    assert!(a.members.iter().all(|m| m.email.is_none()));

    let page = alice.get(&format!("/groups/{}", w.g)).await;
    assert!(html_contains(&page.body, &en("group.show.leaveGroup")));
    assert!(!page.body.contains(CODE));
    assert!(
        !page.body.contains(&w.emails[1]),
        "Bob's email is hidden from Alice now"
    );
    let page = bob.get(&format!("/groups/{}", w.g)).await;
    assert!(page.body.contains(CODE));
    assert!(html_contains(&page.body, &en("group.show.editButton")));
    assert!(!html_contains(&page.body, &en("group.show.leaveGroup")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac41_the_old_admin_loses_the_admin_rights_at_once(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    transfer_admin(&mut alice, w.g, w.bob).await.group_ok();

    for reply in [
        update_group(&mut alice, w.g, "Mine still", "", "", "").await,
        delete_group(&mut alice, w.g).await,
        regenerate_invitation(&mut alice, w.g).await,
        remove_member(&mut alice, w.g, w.bob).await,
        transfer_admin(&mut alice, w.g, w.alice).await,
    ] {
        assert_eq!(reply.group_err(), GroupError::Forbidden);
    }
    assert_eq!(
        alice.get(&format!("/groups/{}/edit", w.g)).await.status,
        403
    );
    // And can leave like any member.
    leave_group(&mut alice, w.g).await.group_ok();
    assert_eq!(ts::member_ids(&pool, w.g).await, vec![w.bob]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac41_the_admin_can_be_handed_on_repeatedly(pool: MySqlPool) {
    let w = world(&pool).await;
    transfer_admin(&mut w.alice().await, w.g, w.bob)
        .await
        .group_ok();
    transfer_admin(&mut w.bob().await, w.g, w.alice)
        .await
        .group_ok();
    assert_eq!(ts::group_row(&pool, w.g).await.admin_id, w.alice);
}

// ------------------------------------------------------------------------- AC42

#[sqlx::test(migrations = "../migrations")]
async fn ac42_transfer_works_after_the_draw_and_reveals_no_assignment(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_member(&pool, w.g, w.carol).await;
    ts::add_assignment(&pool, w.g, w.alice, w.bob).await;
    ts::add_assignment(&pool, w.g, w.bob, w.carol).await;
    ts::add_assignment(&pool, w.g, w.carol, w.alice).await;
    ts::set_drawn(&pool, w.g).await;
    let assignments_before = groups_snapshot(&pool).await.assignments;
    let mut alice = w.alice().await;
    let mut bob = w.bob().await;

    transfer_admin(&mut alice, w.g, w.bob).await.group_ok();

    let after = groups_snapshot(&pool).await;
    assert_eq!(after.groups[0].admin_id, w.bob);
    assert_eq!(
        after.assignments, assignments_before,
        "assignments untouched"
    );
    for c in [&mut alice, &mut bob] {
        let reply = get_group(c, w.g).await;
        let d: app::groups_api::GroupDetail = reply.value();
        assert!(d.is_drawn);
        let json = reply.body.to_lowercase();
        for word in ["assign", "giver", "receiver"] {
            assert!(!json.contains(word), "{word}");
        }
    }
}

// ------------------------------------------------------------------------- AC43

#[sqlx::test(migrations = "../migrations")]
async fn ac43_transfer_errors_change_nothing(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut alice = w.alice().await;

    assert_eq!(
        transfer_admin(&mut alice, w.g, w.alice).await.group_err(),
        GroupError::TargetIsSelf
    );
    assert_eq!(
        transfer_admin(&mut alice, w.g, w.dave).await.group_err(),
        GroupError::TargetNotMember
    );
    assert_eq!(
        transfer_admin(&mut alice, w.g, 999_999).await.group_err(),
        GroupError::TargetNotMember
    );
    let reply = transfer_admin(&mut w.bob().await, w.g, w.bob).await;
    assert_eq!(reply.status, 403);
    assert_eq!(reply.group_err(), GroupError::Forbidden);
    let reply = transfer_admin(&mut w.dave().await, w.g, w.dave).await;
    assert_eq!(reply.status, 404);
    assert_eq!(reply.group_err(), GroupError::NotFound);

    assert_eq!(groups_snapshot(&pool).await, before);
}

// ------------------------------------------------------- remove_member_data (D5)

#[sqlx::test(migrations = "../migrations")]
async fn d5_remove_member_data_deletes_everything_of_the_user_in_that_group_only(pool: MySqlPool) {
    let s = seed(&pool).await;
    let w = &s.w;
    ts::add_assignment(&pool, w.g, w.alice, w.bob).await;
    ts::add_assignment(&pool, w.g, w.bob, w.carol).await;
    ts::add_assignment(&pool, w.g, w.carol, w.alice).await;
    ts::add_assignment(&pool, s.h, w.alice, w.bob).await;
    ts::add_assignment(&pool, s.h, w.bob, w.alice).await;
    let before = groups_snapshot(&pool).await;

    let mut tx = pool.begin().await.unwrap();
    remove_member_data(&mut tx, w.g, w.bob).await.unwrap();
    tx.commit().await.unwrap();

    assert_bob_gone_from_g(&pool, &s).await;
    let after = groups_snapshot(&pool).await;
    assert_eq!(
        after.assignments,
        vec![
            (w.g, w.carol, w.alice),
            (s.h, w.alice, w.bob),
            (s.h, w.bob, w.alice)
        ]
    );
    assert_eq!(
        after.groups, before.groups,
        "the group rows themselves are untouched"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn d5_the_deletion_is_part_of_the_callers_transaction(pool: MySqlPool) {
    let s = seed(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    remove_member_data(&mut tx, s.w.g, s.w.bob).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(groups_snapshot(&pool).await, before);
}
