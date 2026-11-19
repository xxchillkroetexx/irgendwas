//! The dashboard's group cards (spec: dashboard, #47; draw AC31, AC32 as amended) and the
//! `list_my_assignments` API, which no page uses since #47 (draw S1–S3, AC33).

use crate::common::auth::html_contains;
use crate::common::draw::*;
use crate::common::groups::*;
use app::draw_api::DrawApiError;
use app::test_support as ts;
use sqlx::MySqlPool;

// ------------------------------------------------------------ dashboard cards

/// The card of group `g` on a dashboard (panics if there is none).
fn card_of(body: &str, g: i32) -> &str {
    let href = format!("href=\"/groups/{g}\"");
    let cards: Vec<&str> = blocks(body, "dashboard-group")
        .into_iter()
        .filter(|card| card.contains(&href))
        .collect();
    assert_eq!(cards.len(), 1, "one card for group {g}");
    cards[0]
}

fn status_of(card: &str) -> String {
    text_of(block(card, "group-status").expect("status badge"))
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_every_group_is_a_card_with_its_status_and_the_admin_badge(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    rename(&pool, d.alice, "AliceDb9").await;
    rename(&pool, d.carol, "CarolDb9").await;
    let h = ts::create_group(&pool, d.alice, "Office").await;
    ts::add_member(&pool, h, d.bob).await;
    ts::add_member(&pool, h, d.carol).await;
    let k = ts::create_group(&pool, d.alice, "Undrawn").await;
    ts::add_member(&pool, k, d.bob).await;
    let own = ts::create_group(&pool, d.bob, "Bobs Own").await;
    sqlx::query("UPDATE `groups` SET description = 'DescriptionMarker' WHERE id = ?")
        .bind(h)
        .execute(&pool)
        .await
        .unwrap();
    d.draw_and_deliver(d.g).await;
    d.draw_and_deliver(h).await;
    let to_g = assignments(&pool, d.g).await;
    let to_g = to_g.iter().find(|r| r.0 == d.bob).unwrap().1;
    let to_h = assignments(&pool, h).await;
    let to_h = to_h.iter().find(|r| r.0 == d.bob).unwrap().1;

    let page = d.bob().await.get("/user/dashboard").await;
    assert_eq!(page.status, 200);
    // Newest group first, like `/groups`.
    assert_eq!(blocks(&page.body, "dashboard-group").len(), 4);
    let hrefs: Vec<usize> = [own, k, h, d.g]
        .iter()
        .map(|g| page.body.find(&format!("href=\"/groups/{g}\"")).unwrap())
        .collect();
    assert!(hrefs.windows(2).all(|w| w[0] < w[1]), "newest first");

    for (g, name, status, admin) in [
        (own, "Bobs Own", "group.status.open", true),
        (k, "Undrawn", "group.status.open", false),
        (h, "Office", "group.status.drawn", false),
        (d.g, GROUP_NAME, "group.status.drawn", false),
    ] {
        let card = card_of(&page.body, g);
        let text = text_of(card);
        assert!(text.contains(name), "{text}");
        assert_eq!(status_of(card), en(status), "{name}");
        assert_eq!(text.contains(&en("group.admin")), admin, "{name}: {text}");
    }

    // No assignment data and nothing the cards do not show (admin names, descriptions),
    // in the HTML and in the hydration data.
    for (what, needle) in [
        ("receiver_name", "receiver_name".to_string()),
        ("receiver_id", "receiver_id".to_string()),
        ("a wishlist link", "/wishlist/".to_string()),
        ("Alice (admin, maybe a receiver)", "AliceDb9".to_string()),
        ("Carol (maybe a receiver)", "CarolDb9".to_string()),
        ("a description", "DescriptionMarker".to_string()),
        (
            "G's receiver link",
            format!("/groups/{}/wishlist/{to_g}", d.g),
        ),
        ("H's receiver link", format!("/groups/{h}/wishlist/{to_h}")),
    ] {
        assert!(!page.body.contains(&needle), "dashboard carries {what}");
    }
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
    assert_eq!(testid_count(&page.body, "dashboard-group"), 2);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac32_a_user_without_groups_sees_the_empty_state(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let page = d.dave().await.get("/user/dashboard").await;
    assert_eq!(page.status, 200);
    let section = text_of(block(&page.body, "dashboard-groups").expect("groups"));
    assert!(section.contains(&en("group.title")), "{section}");
    let empty = text_of(block(&page.body, "dashboard-groups-empty").expect("empty state"));
    assert!(empty.contains(&en("group.emptyMessage")), "{empty}");
    assert!(empty.contains(&en("group.emptyMessageAction")), "{empty}");
    assert_eq!(testid_count(&page.body, "dashboard-group"), 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac32_the_german_dashboard_has_german_cards_and_empty_state(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    ts::create_group(&pool, d.bob, "Bobs Own").await;
    d.draw_and_deliver(d.g).await;

    let page = d.german("bob").await.get("/user/dashboard").await;
    let section = text_of(block(&page.body, "dashboard-groups").expect("groups"));
    assert!(section.contains(&de("group.title")), "{section}");
    assert_eq!(
        status_of(card_of(&page.body, d.g)),
        de("group.status.drawn")
    );
    let mut own = blocks(&page.body, "dashboard-group");
    own.retain(|card| card.contains("Bobs Own"));
    assert_eq!(status_of(own[0]), de("group.status.open"));
    assert!(text_of(own[0]).contains(&de("group.admin")));

    let empty = d.german("dave").await.get("/user/dashboard").await;
    let text = text_of(block(&empty.body, "dashboard-groups-empty").expect("empty state"));
    assert!(text.contains(&de("group.emptyMessage")), "{text}");
    assert!(text.contains(&de("group.emptyMessageAction")), "{text}");
}

/// The default page after login: a failing groups query leaves the section out, the page
/// keeps its status and the rest of its content (review of #86, finding 1).
#[sqlx::test(migrations = "../migrations")]
async fn a_failing_groups_query_leaves_the_section_out_and_keeps_the_page(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut bob = d.bob().await;
    sqlx::query("RENAME TABLE group_members TO group_members_unavailable")
        .execute(&pool)
        .await
        .expect("rename");
    let page = bob.get("/user/dashboard").await;
    sqlx::query("RENAME TABLE group_members_unavailable TO group_members")
        .execute(&pool)
        .await
        .expect("rename back");

    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("user.dashboard.tips.title")));
    assert!(html_contains(&page.body, &en("user.dashboard.subtitle")));
    assert_eq!(testid_count(&page.body, "dashboard-groups"), 0);
    assert_eq!(testid_count(&page.body, "dashboard-group"), 0);
    assert!(!html_contains(&page.body, &en("error.generic.title")));
}

// -------------------------------------------- list_my_assignments (no page uses it)

#[sqlx::test(migrations = "../migrations")]
async fn list_my_assignments_returns_own_receivers_newest_draw_first(pool: MySqlPool) {
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

    let list = list_mine(&mut d.bob().await).await;
    let rows_g = assignments(&pool, d.g).await;
    let rows_h = assignments(&pool, h).await;
    let to_g = rows_g.iter().find(|r| r.0 == d.bob).unwrap().1;
    let to_h = rows_h.iter().find(|r| r.0 == d.bob).unwrap().1;
    let got: Vec<(i32, &str, i32, &str)> = list
        .iter()
        .map(|s| {
            (
                s.group_id,
                s.group_name.as_str(),
                s.receiver_id,
                s.receiver_name.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (h, "Office", to_h, d.name_of(to_h)),
            (d.g, GROUP_NAME, to_g, d.name_of(to_g)),
        ]
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn list_my_assignments_orders_draws_in_the_same_second_by_group_id_descending(
    pool: MySqlPool,
) {
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
async fn list_my_assignments_is_empty_without_groups_and_needs_a_login(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    assert!(list_mine(&mut d.dave().await).await.is_empty());
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
    assert!(!page.body.contains("Elsewhere"));
    assert!(!page.body.contains(&format!("href=\"/groups/{other}\"")));
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
    // directly in the DB, the API still hides it (S3).
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
