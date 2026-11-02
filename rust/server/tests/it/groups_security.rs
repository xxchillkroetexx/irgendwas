//! groups slice: security properties across all functions and pages. No existence leak,
//! admin-only functions, client IDs never trusted, no other user's email or assignment in
//! any response, CSRF.

use crate::common::auth::*;
use crate::common::groups::*;
use crate::common::{post_from, send};
use app::groups_api::{GroupDetail, GroupError};
use app::test_support::{self as ts, groups_snapshot};
use sqlx::MySqlPool;

/// The admin-only fns, called as `who` for group `g` (target `user`).
async fn admin_calls(c: &mut Client, g: i32, user: i32) -> Vec<(&'static str, Reply)> {
    vec![
        ("update_group", update_group(c, g, "X", "", "", "").await),
        ("delete_group", delete_group(c, g).await),
        ("regenerate_invitation", regenerate_invitation(c, g).await),
        ("remove_member", remove_member(c, g, user).await),
        ("transfer_admin", transfer_admin(c, g, user).await),
    ]
}

/// The member-level fns, called as `c` for group `g`.
async fn member_calls(c: &mut Client, g: i32) -> Vec<(&'static str, Reply)> {
    vec![
        ("get_group", get_group(c, g).await),
        ("leave_group", leave_group(c, g).await),
    ]
}

#[sqlx::test(migrations = "../migrations")]
async fn a_non_member_cannot_tell_a_foreign_group_from_a_missing_one(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut dave = w.dave().await;
    dave.session_info().await; // takes the "logged in" flash, so both pages render alike
    let missing = 999_999;

    let mut foreign = admin_calls(&mut dave, w.g, w.bob).await;
    foreign.extend(member_calls(&mut dave, w.g).await);
    let mut absent = admin_calls(&mut dave, missing, w.bob).await;
    absent.extend(member_calls(&mut dave, missing).await);

    for ((name, f), (_, m)) in foreign.iter().zip(absent.iter()) {
        assert_eq!(f.status, 404, "{name}");
        assert_eq!(f.group_err(), GroupError::NotFound, "{name}");
        assert_eq!(f.status, m.status, "{name}: same status");
        assert_eq!(f.body, m.body, "{name}: same body");
    }
    for path in ["", "/edit"] {
        let a = dave.get(&format!("/groups/{}{path}", w.g)).await;
        let b = dave.get(&format!("/groups/{missing}{path}")).await;
        assert_eq!(
            (a.status, without_nonce(&a.body)),
            (b.status, without_nonce(&b.body)),
            "page {path:?}"
        );
        assert_eq!(a.status, 404);
    }
    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_member_gets_forbidden_from_every_admin_only_fn(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut bob = w.bob().await;
    for (name, reply) in admin_calls(&mut bob, w.g, w.alice).await {
        assert_eq!(reply.status, 403, "{name}");
        assert_eq!(reply.group_err(), GroupError::Forbidden, "{name}");
    }
    assert_eq!(bob.get(&format!("/groups/{}/edit", w.g)).await.status, 403);
    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn extra_form_fields_cannot_change_ownership_code_or_state(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let bob = w.bob.to_string();

    // create_group
    let reply = alice
        .call(
            "create_group",
            &[
                ("name", "Mass"),
                ("description", ""),
                ("registration_deadline", ""),
                ("draw_date", ""),
                ("admin_id", &bob),
                ("invitation_code", "HACKED22"),
                ("is_drawn", "1"),
                ("id", &w.g.to_string()),
            ],
        )
        .await;
    let id: i32 = reply.value();
    assert_ne!(id, w.g, "an existing id is never overwritten");
    let row = ts::group_row(&pool, id).await;
    assert_eq!(row.admin_id, w.alice);
    assert_ne!(row.invitation_code, "HACKED22");
    assert_ne!(row.is_drawn, Some(true));
    assert_eq!(ts::member_ids(&pool, id).await, vec![w.alice]);

    // update_group
    let reply = alice
        .call(
            "update_group",
            &[
                ("group_id", &w.g.to_string()),
                ("name", "Renamed"),
                ("description", ""),
                ("registration_deadline", ""),
                ("draw_date", ""),
                ("admin_id", &bob),
                ("invitation_code", "HACKED22"),
                ("is_drawn", "1"),
            ],
        )
        .await;
    reply.group_ok();
    let row = ts::group_row(&pool, w.g).await;
    assert_eq!(row.name, "Renamed");
    assert_eq!(row.admin_id, w.alice);
    assert_eq!(row.invitation_code, CODE);
    assert_ne!(row.is_drawn, Some(true));
}

#[sqlx::test(migrations = "../migrations")]
async fn the_acting_user_always_comes_from_the_session(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut dave = w.dave().await;
    let (alice, g) = (w.alice.to_string(), w.g.to_string());

    // Passing someone else's id as "user" or "admin" changes who is acting: nobody.
    for (name, extra) in [
        (
            "delete_group",
            [("user_id", alice.as_str()), ("admin_id", alice.as_str())],
        ),
        (
            "regenerate_invitation",
            [("user_id", alice.as_str()), ("admin_id", alice.as_str())],
        ),
        (
            "leave_group",
            [("user_id", alice.as_str()), ("member_id", alice.as_str())],
        ),
    ] {
        let reply = dave
            .call(name, &[("group_id", g.as_str()), extra[0], extra[1]])
            .await;
        assert_eq!(reply.group_err(), GroupError::NotFound, "{name}");
    }
    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn no_other_users_email_appears_in_group_pages_or_their_payloads(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_member(&pool, w.g, w.carol).await;
    ts::add_member(&pool, w.g, w.dave).await;
    let mut bob = w.bob().await;

    // Bob is a plain member: he may see names, nobody's email (not even in the serialized
    // resources inside the page) except his own address in his own navbar/profile.
    for path in [format!("/groups/{}", w.g), "/groups".to_string()] {
        let page = bob.get(&path).await;
        assert_eq!(page.status, 200, "{path}");
        for (i, email) in w.emails.iter().enumerate() {
            if i != 1 {
                assert!(!page.body.contains(email.as_str()), "{path} leaks {email}");
            }
        }
        assert!(!page.body.contains(CODE), "{path} leaks the code");
    }
    let reply = get_group(&mut bob, w.g).await;
    for (i, email) in w.emails.iter().enumerate() {
        if i != 1 {
            assert!(
                !reply.body.contains(email.as_str()),
                "get_group leaks {email}"
            );
        }
    }
    assert!(!reply.body.contains(CODE));
    let list = list_my_groups(&mut bob).await;
    assert!(!list.body.contains('@'), "list_my_groups carries no email");
    assert!(!list.body.contains(CODE), "nor a code");
}

#[sqlx::test(migrations = "../migrations")]
async fn the_invitation_preview_and_the_enter_code_pages_show_no_member_data(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;

    let reply = preview_invitation(&mut carol, CODE).await;
    reply.group_ok();
    assert!(!reply.body.contains('@'));
    assert!(!reply.body.contains(&w.alice.to_string()) || reply.body.contains("group_id"));
    let page = carol.get(&format!("/join/{CODE}")).await;
    for email in &w.emails {
        assert!(!page.body.contains(email.as_str()));
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn the_preview_escapes_group_and_admin_names(pool: MySqlPool) {
    let w = world(&pool).await;
    sqlx::query("UPDATE `groups` SET name = '<script>alert(1)</script>' WHERE id = ?")
        .bind(w.g)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET name = '<b>Boss</b>' WHERE id = ?")
        .bind(w.alice)
        .execute(&pool)
        .await
        .unwrap();
    let mut carol = w.carol().await;
    let page = carol.get(&format!("/join/{CODE}")).await;
    assert!(!page.body.contains("<script>alert(1)</script>"));
    assert!(!page.body.contains("<b>Boss</b>"));
    assert!(html_contains(&page.body, "<b>Boss</b>"));
}

#[sqlx::test(migrations = "../migrations")]
async fn no_assignment_is_reachable_through_any_groups_fn_even_for_the_admin(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_member(&pool, w.g, w.carol).await;
    ts::add_assignment(&pool, w.g, w.alice, w.carol).await;
    ts::add_assignment(&pool, w.g, w.bob, w.alice).await;
    ts::add_assignment(&pool, w.g, w.carol, w.bob).await;
    ts::set_drawn(&pool, w.g).await;
    let mut alice = w.alice().await;

    let mut bodies = vec![
        get_group(&mut alice, w.g).await.body,
        list_my_groups(&mut alice).await.body,
        alice.get(&format!("/groups/{}", w.g)).await.body,
        alice.get("/groups").await.body,
        alice.get(&format!("/groups/{}/edit", w.g)).await.body,
    ];
    let d: GroupDetail = get_group(&mut alice, w.g).await.value();
    assert!(d.is_drawn);
    for body in bodies.drain(..) {
        let lower = body.to_lowercase();
        for word in ["giver_id", "receiver_id", "\"giver\"", "\"receiver\""] {
            assert!(!lower.contains(word), "{word}");
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn every_mutating_fn_rejects_a_cross_origin_post(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let alice = w.alice().await;
    let cookie = format!(
        "{}={}",
        server::SESSION_COOKIE,
        alice.session_cookie().unwrap()
    );
    let g = w.g.to_string();
    let bob = w.bob.to_string();
    let bodies: Vec<(&str, String)> = vec![
        (
            "create_group",
            form(&[
                ("name", "X"),
                ("description", ""),
                ("registration_deadline", ""),
                ("draw_date", ""),
            ]),
        ),
        (
            "update_group",
            form(&[
                ("group_id", &g),
                ("name", "X"),
                ("description", ""),
                ("registration_deadline", ""),
                ("draw_date", ""),
            ]),
        ),
        ("delete_group", form(&[("group_id", &g)])),
        ("regenerate_invitation", form(&[("group_id", &g)])),
        ("join_group", form(&[("code", CODE)])),
        ("leave_group", form(&[("group_id", &g)])),
        (
            "remove_member",
            form(&[("group_id", &g), ("user_id", &bob)]),
        ),
        (
            "transfer_admin",
            form(&[("group_id", &g), ("user_id", &bob)]),
        ),
    ];
    for (name, body) in bodies {
        let req = post_from(
            Some("https://evil.example"),
            &format!("/api/{name}"),
            Some(&cookie),
            &body,
        );
        let res = send(&w.app, req).await;
        assert_eq!(res.status(), 403, "{name}");
    }
    assert_eq!(groups_snapshot(&pool).await, before);
}
