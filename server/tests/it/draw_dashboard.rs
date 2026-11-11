//! Draw assignment API and dashboard group-card behavior (#47).

use crate::common::auth::html_contains;
use crate::common::draw::*;
use crate::common::groups::*;
use app::draw_api::DrawApiError;
use app::test_support as ts;
use sqlx::MySqlPool;

#[sqlx::test(migrations = "../migrations")]
async fn dashboard_lists_drawn_and_undrawn_groups_without_assignments(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let h = ts::create_group(&pool, d.alice, "Office").await;
    ts::add_member(&pool, h, d.bob).await;
    ts::add_member(&pool, h, d.carol).await;
    let k = ts::create_group(&pool, d.alice, "Undrawn").await;
    ts::add_member(&pool, k, d.bob).await;
    d.draw_and_deliver(d.g).await;
    d.draw_and_deliver(h).await;
    // H was drawn after G.
    set_drawn_at(&pool, d.g, "2026-11-01 10:00:00").await;
    set_drawn_at(&pool, h, "2026-11-02 10:00:00").await;

    let mut bob = d.bob().await;
    let list = list_mine(&mut bob).await;
    let names: Vec<&str> = list.iter().map(|s| s.group_name.as_str()).collect();
    assert_eq!(names, vec!["Office", GROUP_NAME]);

    let rows_g = assignments(&pool, d.g).await;
    let rows_h = assignments(&pool, h).await;
    let to_g = rows_g.iter().find(|r| r.0 == d.bob).unwrap().1;
    let to_h = rows_h.iter().find(|r| r.0 == d.bob).unwrap().1;
    assert_eq!((list[0].group_id, list[0].receiver_id), (h, to_h));
    assert_eq!((list[1].group_id, list[1].receiver_id), (d.g, to_g));
    assert_eq!(list[0].receiver_name, d.name_of(to_h));
    assert_eq!(list[1].receiver_name, d.name_of(to_g));

    let page = bob.get("/user/dashboard").await;
    assert_eq!(page.status, 200);
    assert_eq!(testid_count(&page.body, "dashboard-group"), 3);
    assert_eq!(testid_count(&page.body, "dashboard-assignments"), 0);
    assert!(!page.body.contains("receiver_name"));
    assert!(!page.body.contains("receiver_id"));
    for group in [d.g, h, k] {
        assert!(page.body.contains(&format!("href=\"/groups/{group}\"")));
    }
    assert!(
        !page
            .body
            .contains(&format!("/groups/{}/wishlist/{to_g}", d.g))
    );
    assert!(!page.body.contains(&format!("/groups/{h}/wishlist/{to_h}")));
    assert!(page.body.contains("Undrawn"));
    assert!(html_contains(&page.body, &en("group.status.drawn")));
    assert!(html_contains(&page.body, &en("group.status.open")));
}

#[sqlx::test(migrations = "../migrations")]
async fn dashboard_escapes_group_names_and_hides_nonmember_groups(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let group = ts::create_group(&pool, d.alice, "<script>group-marker</script>").await;
    ts::add_member(&pool, group, d.bob).await;
    let private = ts::create_group(&pool, d.alice, "Private group marker").await;
    let page = d.bob().await.get("/user/dashboard").await;
    assert_eq!(page.status, 200);
    assert!(
        page.body
            .contains("&lt;script&gt;group-marker&lt;/script&gt;")
    );
    assert!(!page.body.contains("<script>group-marker</script>"));
    assert!(!page.body.contains("Private group marker"));
    assert!(!page.body.contains(&format!("href=\"/groups/{private}\"")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_groups_drawn_in_the_same_second_are_ordered_by_group_id_descending(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let h = ts::create_group(&pool, d.alice, "Office").await;
    ts::add_member(&pool, h, d.bob).await;
    ts::add_member(&pool, h, d.carol).await;
    d.draw_and_deliver(d.g).await;
    d.draw_and_deliver(h).await;
    set_drawn_at(&pool, d.g, "2026-11-01 10:00:00").await;
    set_drawn_at(&pool, h, "2026-11-01 10:00:00").await;
    let list = list_mine(&mut d.bob().await).await;
    assert_eq!(
        list.iter().map(|s| s.group_id).collect::<Vec<_>>(),
        vec![h.max(d.g), h.min(d.g)]
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn a_user_without_groups_sees_the_empty_state(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut dave = d.dave().await;
    assert!(list_mine(&mut dave).await.is_empty());
    let page = dave.get("/user/dashboard").await;
    let section = text_of(block(&page.body, "dashboard-groups").expect("groups"));
    assert!(section.contains(&en("group.emptyMessage")), "{section}");
    assert_eq!(testid_count(&page.body, "dashboard-group"), 0);
    assert_eq!(testid_count(&page.body, "dashboard-assignments"), 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac32_the_list_needs_a_login(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    assert_eq!(
        list_mine_reply(&mut d.anon()).await.dr_err(),
        DrawApiError::Unauthorized
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac33_an_assignment_without_membership_is_not_shown(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    // A group Bob is not in (never was), with a row for him inserted directly.
    let other = ts::create_group(&pool, d.alice, "Elsewhere").await;
    ts::add_member(&pool, other, d.carol).await;
    ts::set_drawn(&pool, other).await;
    ts::add_assignment(&pool, other, d.bob, d.carol).await;
    ts::add_assignment(&pool, other, d.carol, d.alice).await;
    ts::add_assignment(&pool, other, d.alice, d.bob).await;

    let mut bob = d.bob().await;
    assert!(list_mine(&mut bob).await.is_empty());
    assert_eq!(
        my_assignment_reply(&mut bob, other).await.dr_err(),
        DrawApiError::NotFound
    );
    let page = bob.get("/user/dashboard").await;
    assert_eq!(testid_count(&page.body, "dashboard-assignment"), 0);
    assert!(!page.body.contains("Elsewhere"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac33_a_row_in_a_group_that_is_not_drawn_is_not_listed(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    // Bob is a member, a row exists, but the group is not marked drawn.
    ts::add_assignment(&pool, d.g, d.bob, d.carol).await;
    assert!(list_mine(&mut d.bob().await).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn a_member_who_left_no_longer_sees_the_assignment(pool: MySqlPool) {
    // Leaving a drawn group is refused (groups D4); if a row survives a removal made
    // directly in the DB, the card still hides it (S3).
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(d.g)
        .bind(d.bob)
        .execute(&pool)
        .await
        .unwrap();
    let mut bob = d.bob().await;
    assert!(list_mine(&mut bob).await.is_empty());
    assert_eq!(
        my_assignment_reply(&mut bob, d.g).await.dr_err(),
        DrawApiError::NotFound
    );
}
