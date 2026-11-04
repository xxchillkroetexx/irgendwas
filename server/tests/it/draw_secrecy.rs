//! draw slice: secrecy (AC25-AC30, S1-S9). Nobody learns another member's receiver: not a
//! member, not the admin; not through a server fn, a page, the hydration payload, a log
//! line or a mail.

use std::collections::BTreeSet;

use crate::common::auth::capture_logs;
use crate::common::draw::*;
use crate::common::groups::*;
use app::draw_api::{
    DrawAdminStatus, DrawApiError, DrawBlocker, MyAssignment, MyAssignmentSummary,
    NotificationSummary,
};
use app::test_support as ts;
use chrono::Utc;
use serde_json::Value;
use sqlx::MySqlPool;

/// The world with names that appear nowhere else in a page.
async fn marked(pool: &MySqlPool) -> D {
    let d = draw_world(pool).await;
    rename(pool, d.alice, "AliceMk7").await;
    rename(pool, d.bob, "BobMk7").await;
    rename(pool, d.carol, "CarolMk7").await;
    d
}

fn marker(d: &D, user: i32) -> String {
    format!("{}Mk7", d.name_of(user))
}

// ------------------------------------------------------------------------ AC25

#[sqlx::test(migrations = "../migrations")]
async fn ac25_a_member_gets_exactly_their_own_receiver(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    // Undrawn: Ok(None) for members, NotFound for outsiders.
    for mut c in [d.alice().await, d.bob().await, d.carol().await] {
        assert_eq!(my_assignment(&mut c, d.g).await, None);
    }
    assert_eq!(
        my_assignment_reply(&mut d.dave().await, d.g).await.dr_err(),
        DrawApiError::NotFound
    );

    d.draw_and_deliver(d.g).await;
    let rows = assignments(&pool, d.g).await;
    for (user, mut c) in [
        (d.alice, d.alice().await),
        (d.bob, d.bob().await),
        (d.carol, d.carol().await),
    ] {
        let expected = rows.iter().find(|r| r.0 == user).unwrap().1;
        let got: MyAssignment = my_assignment(&mut c, d.g).await.expect("assigned");
        assert_eq!(got.receiver_id, expected, "{}", d.name_of(user));
        assert_eq!(got.receiver_name, d.name_of(expected));
        assert_eq!(Some(got.drawn_at), drawn_at(&pool, d.g).await);
    }
    assert_eq!(
        my_assignment_reply(&mut d.dave().await, d.g).await.dr_err(),
        DrawApiError::NotFound
    );
    assert_eq!(
        my_assignment_reply(&mut d.anon(), d.g).await.dr_err(),
        DrawApiError::Unauthorized
    );
    assert_eq!(
        my_assignment_reply(&mut d.alice().await, 999_999)
            .await
            .dr_err(),
        DrawApiError::NotFound
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac25_the_fns_ignore_a_user_id_argument(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    let rows = assignments(&pool, d.g).await;
    let alices = rows.iter().find(|r| r.0 == d.alice).unwrap().1;
    let mut alice = d.alice().await;
    for extra in ["user_id", "giver_id", "receiver_id"] {
        let reply = alice
            .call(
                "get_my_assignment",
                &[("group_id", &d.g.to_string()), (extra, &d.bob.to_string())],
            )
            .await;
        let got: Option<MyAssignment> = reply.dr_value();
        assert_eq!(got.unwrap().receiver_id, alices, "extra argument {extra}");
    }
    let reply = alice
        .call("list_my_assignments", &[("user_id", &d.bob.to_string())])
        .await;
    let list: Vec<MyAssignmentSummary> = reply.dr_value();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].receiver_id, alices);
}

// ------------------------------------------------------------------------ AC26

fn keys(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                out.insert(k.clone());
                keys(v, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| keys(v, out)),
        _ => {}
    }
}

#[test]
fn ac26_the_admin_types_have_no_field_about_receivers() {
    let status = DrawAdminStatus {
        is_drawn: true,
        drawn_at: Some(Utc::now()),
        member_count: 3,
        blocker: Some(DrawBlocker::DrawImpossible),
        registration_open_until: Some(Utc::now()),
        notifications: Some(NotificationSummary {
            total: 3,
            sent: 1,
            pending: 1,
            failed: 1,
            failed_member_names: vec!["Bob".into()],
        }),
    };
    let mut found = BTreeSet::new();
    keys(&serde_json::to_value(&status).unwrap(), &mut found);
    let expected: BTreeSet<String> = [
        "is_drawn",
        "drawn_at",
        "member_count",
        "blocker",
        "registration_open_until",
        "notifications",
        "total",
        "sent",
        "pending",
        "failed",
        "failed_member_names",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(found, expected);
    for key in &found {
        assert!(
            !["receiver", "giver", "assign"]
                .iter()
                .any(|w| key.contains(w)),
            "field {key} refers to the pairing"
        );
    }
    // The blocker variants carry counts or nothing, never ids or names.
    for blocker in [
        DrawBlocker::TooFewMembers { count: 1, min: 2 },
        DrawBlocker::TooManyMembers {
            count: 201,
            max: 200,
        },
        DrawBlocker::DrawImpossible,
    ] {
        let mut k = BTreeSet::new();
        keys(&serde_json::to_value(&blocker).unwrap(), &mut k);
        assert!(
            k.is_subset(
                &["TooFewMembers", "TooManyMembers", "count", "min", "max"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            )
        );
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac26_the_admin_status_names_only_the_member_whose_mail_failed(pool: MySqlPool) {
    let d = marked(&pool).await;
    d.transport.fail(&d.emails[1]);
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;

    let reply = status_reply(&mut alice, d.g).await;
    let st: DrawAdminStatus = reply.dr_value();
    let notes = st.notifications.expect("summary");
    assert_eq!(notes.failed_member_names, vec!["BobMk7".to_string()]);
    let json: Value = serde_json::from_str(&reply.body).unwrap();
    let mut found = BTreeSet::new();
    keys(&json, &mut found);
    assert!(
        !found
            .iter()
            .any(|k| k.contains("receiver") || k.contains("giver"))
    );
    for name in ["AliceMk7", "CarolMk7", GROUP_NAME] {
        assert!(
            !reply.body.contains(name),
            "status mentions {name}: {}",
            reply.body
        );
    }
    assert_eq!(reply.body.matches("BobMk7").count(), 1);

    // The other admin fns answer without any member data either.
    let resend_reply = resend(&mut alice, d.g).await;
    d.transport.heal(&d.emails[1]);
    assert_eq!(resend_reply.dr_value::<u32>(), 1);
    for name in ["AliceMk7", "BobMk7", "CarolMk7"] {
        assert!(!resend_reply.body.contains(name));
    }
    d.idle().await;
    let reset_reply = reset(&mut alice, d.g).await;
    assert_eq!(reset_reply.dr_value::<Value>(), Value::Null);
    let again = perform(&mut alice, d.g).await;
    assert_eq!(again.dr_value::<Value>(), Value::Null);
    d.idle().await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac26_no_other_server_fn_response_carries_assignment_data(pool: MySqlPool) {
    let d = marked(&pool).await;
    d.draw_and_deliver(d.g).await;
    let g = d.g.to_string();
    for mut c in [d.alice().await, d.bob().await] {
        for (name, args) in [
            ("get_group", vec![("group_id", g.as_str())]),
            ("list_my_groups", vec![]),
            ("get_wishlist_panel", vec![("group_id", g.as_str())]),
            ("get_exclusions", vec![("group_id", g.as_str())]),
        ] {
            let reply = c.call(name, &args).await;
            assert!(reply.status.is_success(), "{name}: {}", reply.body);
            for needle in ["receiver", "giver", "assignment"] {
                assert!(
                    !reply.body.contains(needle),
                    "{name} mentions {needle}: {}",
                    reply.body
                );
            }
        }
        let info = c.session_info().await;
        let flat = serde_json::to_string(&info).unwrap();
        assert!(!flat.contains("receiver"));
    }
}

// ------------------------------------------------------------------------ AC27

/// Where `receiver_name` / `receiver_id` occur in a response body: exactly once each, and
/// the value next to the name is the receiver's.
fn assert_one_serialized_assignment(body: &str, own_receiver: &str, others: &[String], what: &str) {
    assert_eq!(count(body, "receiver_name"), 1, "{what}: receiver_name");
    assert_eq!(count(body, "receiver_id"), 1, "{what}: receiver_id");
    let at = body.find("receiver_name").unwrap();
    let window = &body[at..(at + 80).min(body.len())];
    assert!(window.contains(own_receiver), "{what}: {window}");
    for other in others {
        assert!(!window.contains(other.as_str()), "{what}: {window}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac27_the_group_page_and_the_dashboard_show_only_the_callers_receiver(pool: MySqlPool) {
    let d = marked(&pool).await;
    d.draw_and_deliver(d.g).await;
    let rows = assignments(&pool, d.g).await;
    for (user, mut c) in [
        (d.alice, d.alice().await),
        (d.bob, d.bob().await),
        (d.carol, d.carol().await),
    ] {
        let receiver = rows.iter().find(|r| r.0 == user).unwrap().1;
        let own = marker(&d, receiver);
        let others: Vec<String> = [d.alice, d.bob, d.carol]
            .into_iter()
            .filter(|u| *u != receiver)
            .map(|u| marker(&d, u))
            .collect();
        let who = d.name_of(user);

        let page = c.get(&format!("/groups/{}", d.g)).await;
        assert_eq!(page.status, 200);
        assert_eq!(testid_count(&page.body, "my-assignment"), 1, "{who}");
        let card = text_of(block(&page.body, "my-assignment").unwrap());
        assert!(card.contains(&own), "{who}: {card}");
        for other in &others {
            assert!(
                !card.contains(other.as_str()),
                "{who}'s card shows {other}: {card}"
            );
        }
        // The whole draw panel (the admin's has more in it) names nobody else either.
        let panel = text_of(block(&page.body, "draw-panel").unwrap());
        for other in &others {
            assert!(
                !panel.contains(other.as_str()),
                "{who}'s panel shows {other}"
            );
        }
        assert_one_serialized_assignment(&page.body, &own, &others, &format!("{who} group page"));

        let dash = c.get("/user/dashboard").await;
        assert_eq!(dash.status, 200);
        assert_eq!(testid_count(&dash.body, "dashboard-assignment"), 1, "{who}");
        let row = text_of(block(&dash.body, "dashboard-assignment").unwrap());
        assert!(row.contains(&own), "{who}: {row}");
        for other in &others {
            assert!(
                !row.contains(other.as_str()),
                "{who}'s dashboard row shows {other}"
            );
            assert!(
                !text_of(block(&dash.body, "dashboard-assignments").unwrap())
                    .contains(other.as_str())
            );
        }
        assert_one_serialized_assignment(&dash.body, &own, &others, &format!("{who} dashboard"));
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac27_an_undrawn_group_page_carries_no_assignment(pool: MySqlPool) {
    let d = marked(&pool).await;
    for mut c in [d.alice().await, d.bob().await] {
        let page = c.get(&format!("/groups/{}", d.g)).await;
        assert_eq!(testid_count(&page.body, "my-assignment"), 0);
        assert_eq!(count(&page.body, "receiver_name"), 0);
        let dash = c.get("/user/dashboard").await;
        assert_eq!(testid_count(&dash.body, "dashboard-assignment"), 0);
        assert_eq!(count(&dash.body, "receiver_name"), 0);
    }
}

// ------------------------------------------------------------------------ AC28

#[sqlx::test(migrations = "../migrations")]
async fn ac28_the_group_page_and_the_dashboard_are_not_cacheable(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let check = |what: &str, cache: Option<String>| {
        assert!(
            cache.clone().unwrap_or_default().contains("no-store"),
            "{what}: {cache:?}"
        );
    };
    for drawn in [false, true] {
        if drawn {
            d.draw_and_deliver(d.g).await;
        }
        for (who, mut c) in [
            ("alice", d.alice().await),
            ("bob", d.bob().await),
            ("dave", d.dave().await),
        ] {
            let page = c.get(&format!("/groups/{}", d.g)).await;
            check(
                &format!("{who} group page drawn={drawn}"),
                page.header("cache-control"),
            );
            let dash = c.get("/user/dashboard").await;
            check(
                &format!("{who} dashboard drawn={drawn}"),
                dash.header("cache-control"),
            );
        }
    }
}

// ------------------------------------------------------------------------ AC29

#[sqlx::test(migrations = "../migrations")]
async fn ac29_the_logs_never_show_receivers_addresses_or_mail_bodies(pool: MySqlPool) {
    let d = marked(&pool).await;
    d.transport.fail(&d.emails[1]); // one injected delivery failure
    let (log, _guard) = capture_logs();

    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    for mut c in [d.alice().await, d.bob().await, d.carol().await] {
        my_assignment(&mut c, d.g).await;
        list_mine(&mut c).await;
        c.get(&format!("/groups/{}", d.g)).await;
        c.get("/user/dashboard").await;
    }
    status(&mut alice, d.g).await;
    d.transport.heal(&d.emails[1]);
    resend(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    reset(&mut alice, d.g).await.dr_ok();
    d.idle().await;

    let log = log.contents();
    assert!(!log.is_empty(), "nothing was captured");
    let mut forbidden: Vec<String> = vec![
        "AliceMk7".into(),
        "BobMk7".into(),
        "CarolMk7".into(),
        en("email.drawResult.secret"),
        en("email.drawReset.next"),
        "seed".into(),
    ];
    for email in &d.emails {
        forbidden.push(email.clone());
        forbidden.push(email.split('@').next().unwrap().to_string());
    }
    for needle in &forbidden {
        assert!(
            !log.contains(needle.as_str()),
            "log contains {needle:?}:\n{log}"
        );
    }
    // The failed delivery was logged (with ids and counts only).
    assert!(
        log.to_lowercase().contains("warn"),
        "no warning for the failed mail:\n{log}"
    );
}

// ------------------------------------------------------------------------ AC30

#[test]
fn ac30_only_the_listed_server_fns_deal_with_assignments() {
    let paths: BTreeSet<String> = leptos::server_fn::axum::server_fn_paths()
        .map(|(path, _)| path.to_string())
        .collect();
    let related: BTreeSet<&str> = paths
        .iter()
        .map(String::as_str)
        .filter(|p| {
            ["assign", "draw", "receiver", "giver"]
                .iter()
                .any(|w| p.contains(w))
        })
        .collect();
    let expected: BTreeSet<&str> = [
        "/api/get_my_assignment",
        "/api/list_my_assignments",
        "/api/get_draw_admin_status",
        "/api/perform_draw",
        "/api/reset_draw",
        "/api/resend_draw_notifications",
    ]
    .into_iter()
    .collect();
    assert_eq!(related, expected);
}

// ------------------------------------------------------------------ S4 / S8 / D6 data

#[sqlx::test(migrations = "../migrations")]
async fn s8_a_member_without_an_assignment_and_an_undrawn_group_look_the_same(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    // Drawn flag set but Dave (a member) has no row: still Ok(None), never an error.
    ts::add_member(&pool, d.g, d.dave).await;
    ts::set_drawn(&pool, d.g).await;
    ts::add_assignment(&pool, d.g, d.alice, d.bob).await;
    ts::add_assignment(&pool, d.g, d.bob, d.carol).await;
    ts::add_assignment(&pool, d.g, d.carol, d.alice).await;
    let undrawn = ts::create_group(&pool, d.alice, "Other").await;
    ts::add_member(&pool, undrawn, d.dave).await;
    let mut dave = d.dave().await;
    let in_drawn = my_assignment_reply(&mut dave, d.g).await;
    let in_undrawn = my_assignment_reply(&mut dave, undrawn).await;
    assert_eq!(in_drawn.status, in_undrawn.status);
    assert_eq!(in_drawn.body, in_undrawn.body);
    assert_eq!(in_drawn.dr_value::<Option<MyAssignment>>(), None);
}
