//! exclusions slice: privacy (AC4, AC5, D4), authorization (AC19, AC20) and transport
//! (AC21). Nobody sees another member's rules; no payload carries emails or assignments.

use crate::common::auth::*;
use crate::common::exclusions::*;
use crate::common::groups::*;
use crate::common::{TEST_ORIGIN, post_from, send};
use app::exclusions_api::ExclusionError;
use app::test_support as ts;
use axum::http::StatusCode;
use sqlx::MySqlPool;

// ------------------------------------------------------------------------ AC5

#[sqlx::test(migrations = "../migrations")]
async fn ac5_the_admin_sees_only_her_own_rules(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::add_exclusion(&pool, x.g(), x.carol(), x.dave()).await;
    let mut alice = x.w.alice().await;
    let p = page(&mut alice, x.g()).await;
    assert!(p.my_rules.is_empty());
    assert_eq!(
        p.candidates.iter().map(|m| m.user_id).collect::<Vec<_>>(),
        [x.bob(), x.carol(), x.dave()],
        "everyone is still a candidate for Alice"
    );
    assert!(p.notice.is_none());
    let html = alice.get(&ex_path(x.g())).await;
    assert!(html_contains(&html.body, &en("exclusion.current.empty")));
}

/// Every payload and page that other slices serve must be byte-identical (apart from the
/// nonce) with and without rules between other members: rule data is never part of them.
#[sqlx::test(migrations = "../migrations")]
async fn ac5_no_other_page_or_fn_carries_rule_data(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let g = x.g();
    let uris = [
        format!("/groups/{g}"),
        "/groups".to_string(),
        "/user/dashboard".to_string(),
        format!("/groups/{g}/wishlist"),
        format!("/groups/{g}/wishlist/{}", x.bob()),
    ];
    let calls = ["list_my_groups"];

    async fn snapshot(
        x: &X,
        uris: &[String],
        calls: &[&str],
        g: i32,
    ) -> Vec<(String, String, String)> {
        let mut out = vec![];
        for (who, mut c) in [
            ("alice", x.w.alice().await),
            ("bob", x.w.bob().await),
            ("carol", x.w.carol().await),
            ("dave", x.w.dave().await),
        ] {
            c.session_info().await; // drain the login flash
            for uri in uris {
                let r = c.get(uri).await;
                out.push((who.to_string(), uri.clone(), without_nonce(&r.body)));
            }
            for call in calls {
                let r = c.call(call, &[]).await;
                out.push((who.to_string(), call.to_string(), r.body));
            }
            let r = get_group(&mut c, g).await;
            out.push((who.to_string(), "get_group".to_string(), r.body));
            let r = c
                .call("get_wishlist_panel", &[("group_id", &g.to_string())])
                .await;
            out.push((who.to_string(), "get_wishlist_panel".to_string(), r.body));
        }
        out
    }

    let before = snapshot(&x, &uris, &calls, g).await;
    ts::add_exclusion(&pool, g, x.bob(), x.carol()).await;
    ts::add_exclusion(&pool, g, x.carol(), x.dave()).await;
    ts::add_exclusion(&pool, g, x.dave(), x.alice()).await;
    let after = snapshot(&x, &uris, &calls, g).await;
    assert_eq!(before.len(), after.len());
    for (b, a) in before.iter().zip(after.iter()) {
        assert_eq!((&b.0, &b.1), (&a.0, &a.1));
        assert_eq!(
            b.2, a.2,
            "{} / {} changed with other members' rules",
            b.0, b.1
        );
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_the_exclusions_page_of_somebody_else_is_unaffected_by_foreign_rules(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let g = x.g();
    async fn pages(x: &X, g: i32) -> Vec<String> {
        let mut out = vec![];
        for mut c in [x.w.alice().await, x.w.dave().await] {
            c.session_info().await;
            out.push(without_nonce(&c.get(&ex_path(g)).await.body));
            out.push(get_ex(&mut c, g).await.body);
        }
        out
    }
    let before = pages(&x, g).await;
    // Bob->Carol and Carol->Dave: neither is Alice's nor Dave's own rule.
    ts::add_exclusion(&pool, g, x.bob(), x.carol()).await;
    ts::add_exclusion(&pool, g, x.carol(), x.alice()).await;
    assert_eq!(pages(&x, g).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_a_rule_pointing_at_somebody_does_not_show_for_them(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.carol(), x.dave()).await;
    let mut dave = x.w.dave().await;
    let p = page(&mut dave, x.g()).await;
    assert!(p.my_rules.is_empty());
    assert_eq!(p.candidates.len(), 3);
}

// ----------------------------------------------------------- payload hygiene

#[sqlx::test(migrations = "../migrations")]
async fn payloads_and_pages_carry_no_emails_and_no_assignments(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.dave()).await;
    // A drawn group with real assignments: Bob gives to Carol.
    ts::add_assignment(&pool, x.g(), x.bob(), x.carol()).await;
    ts::add_assignment(&pool, x.g(), x.carol(), x.alice()).await;
    ts::add_assignment(&pool, x.g(), x.alice(), x.dave()).await;
    ts::add_assignment(&pool, x.g(), x.dave(), x.bob()).await;
    ts::set_drawn(&pool, x.g()).await;
    for (who, mut c) in [("alice", x.w.alice().await), ("bob", x.w.bob().await)] {
        let json = get_ex(&mut c, x.g()).await.body;
        let html = c.get(&ex_path(x.g())).await.body;
        for email in &x.w.emails {
            assert!(!json.contains(email.as_str()), "{who}: email in JSON");
            assert!(!html.contains(email.as_str()), "{who}: email in HTML");
        }
        for fragment in ["gift", "assign", "receiver", "giver"] {
            assert!(
                !json.to_lowercase().contains(fragment),
                "{who}: {fragment} in JSON: {json}"
            );
        }
    }
    // Bob's own rule is the only name besides the group on his drawn page.
    let mut bob = x.w.bob().await;
    let html = bob.get(&ex_path(x.g())).await.body;
    assert!(html.contains("Dave"));
    assert!(!html.contains("Carol"), "his receiver is not on the page");
}

// ------------------------------------------------------------------------ AC19

#[sqlx::test(migrations = "../migrations")]
async fn ac19_a_non_member_and_a_missing_group_are_indistinguishable_and_change_nothing(
    pool: MySqlPool,
) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let before = all_rules(&pool).await;
    let mut eve = x.eve_client().await;
    let expected = serde_json::to_string(&ExclusionError::NotFound).unwrap();
    for (name, group) in [("foreign", x.g()), ("missing", 999_999)] {
        let replies = [
            get_ex(&mut eve, group).await,
            add_ex(&mut eve, group, x.bob()).await,
            add_ex(&mut eve, group, x.eve).await,
            add_ex(&mut eve, group, 0).await,
            remove_ex(&mut eve, group, x.carol()).await,
        ];
        for r in replies {
            assert_eq!(r.status.as_u16(), 404, "{name}");
            assert_eq!(r.ex_err(), ExclusionError::NotFound, "{name}");
            assert_eq!(r.body, expected, "{name}");
        }
    }
    // Anybody on a missing group.
    for mut c in [x.w.bob().await, x.w.alice().await] {
        assert_eq!(
            get_ex(&mut c, 999_999).await.ex_err(),
            ExclusionError::NotFound
        );
        assert_eq!(
            add_ex(&mut c, 999_999, x.carol()).await.ex_err(),
            ExclusionError::NotFound
        );
        assert_eq!(
            remove_ex(&mut c, 999_999, x.carol()).await.ex_err(),
            ExclusionError::NotFound
        );
    }
    assert_eq!(all_rules(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_a_former_member_is_a_non_member(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut dave = x.w.dave().await;
    leave_group(&mut dave, x.g()).await.group_ok();
    assert_eq!(
        get_ex(&mut dave, x.g()).await.ex_err(),
        ExclusionError::NotFound
    );
    assert_eq!(
        add_ex(&mut dave, x.g(), x.bob()).await.ex_err(),
        ExclusionError::NotFound
    );
    assert!(all_rules(&pool).await.is_empty());
}

// ------------------------------------------------------------------------ AC20

#[sqlx::test(migrations = "../migrations")]
async fn ac20_every_fn_without_a_session_is_unauthorized_and_changes_nothing(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let before = all_rules(&pool).await;
    let mut anon = x.w.anon();
    for (name, reply) in [
        ("get_exclusions", get_ex(&mut anon, x.g()).await),
        ("add_exclusion", add_ex(&mut anon, x.g(), x.dave()).await),
        (
            "remove_exclusion",
            remove_ex(&mut anon, x.g(), x.carol()).await,
        ),
        ("get missing", get_ex(&mut anon, 999_999).await),
    ] {
        assert_eq!(reply.status.as_u16(), 401, "{name}");
        assert_eq!(reply.ex_err(), ExclusionError::Unauthorized, "{name}");
    }
    assert_eq!(all_rules(&pool).await, before);
}

// ------------------------------------------------------------------------ AC21

#[sqlx::test(migrations = "../migrations")]
async fn ac21_old_get_links_and_get_requests_change_nothing(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let before = all_rules(&pool).await;
    let mut bob = x.w.bob().await;
    let (g, c, d) = (x.g(), x.carol(), x.dave());
    for uri in [
        format!("/exclusions/{g}/remove/{c}"),
        format!("/exclusions/{g}"),
        format!("/api/remove_exclusion?group_id={g}&excluded_user_id={c}"),
        format!("/api/add_exclusion?group_id={g}&excluded_user_id={d}"),
        format!("/groups/{g}/exclusions?remove={c}"),
        format!("/groups/{g}/exclusions/remove/{c}"),
    ] {
        let reply = bob.get(&uri).await;
        assert!(!reply.status.is_server_error(), "{uri}: {}", reply.status);
        assert!(
            !reply.status.is_redirection() || reply.location().is_some(),
            "{uri}"
        );
        assert_eq!(all_rules(&pool).await, before, "{uri}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac21_a_cross_origin_or_origin_less_post_is_refused_and_changes_nothing(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let before = all_rules(&pool).await;
    let bob = x.w.bob().await;
    let cookie = format!(
        "{}={}",
        server::SESSION_COOKIE,
        bob.session_cookie().unwrap()
    );
    let (g, c, d) = (x.g(), x.carol(), x.dave());
    let calls = [
        (
            "add_exclusion",
            format!("group_id={g}&excluded_user_id={d}"),
        ),
        (
            "remove_exclusion",
            format!("group_id={g}&excluded_user_id={c}"),
        ),
    ];
    for (name, body) in &calls {
        for origin in [Some("https://evil.example"), Some("null"), None] {
            let req = post_from(origin, &format!("/api/{name}"), Some(&cookie), body);
            let res = send(&x.w.app, req).await;
            assert_eq!(
                res.status(),
                StatusCode::FORBIDDEN,
                "{name} origin {origin:?}"
            );
        }
    }
    assert_eq!(all_rules(&pool).await, before);
    // The same request from the right origin is honoured.
    let req = post_from(
        Some(TEST_ORIGIN),
        "/api/add_exclusion",
        Some(&cookie),
        &calls[0].1,
    );
    assert!(send(&x.w.app, req).await.status().is_success());
    assert!(rules(&pool, g).await.contains(&(x.bob(), d)));
}
