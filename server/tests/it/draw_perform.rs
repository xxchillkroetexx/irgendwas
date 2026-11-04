//! draw slice: performing the draw (AC14-AC24). Fixture: G = Alice (admin), Bob, Carol; Dave
//! is outside.

use crate::common::auth::capture_logs;
use crate::common::draw::*;
use crate::common::exclusions::{add_ex, rules};
use crate::common::groups::*;
use app::draw_api::{DrawAdminStatus, DrawApiError, DrawBlocker};
use app::groups_api::GroupError;
use app::test_support::{self as ts, groups_snapshot, mailpit::unique_email};
use chrono::{Duration, Utc};
use sqlx::MySqlPool;

// ------------------------------------------------------------------------ AC14

#[sqlx::test(migrations = "../migrations")]
async fn ac14_the_admin_draws_and_every_member_gets_exactly_their_own_mail(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;

    perform(&mut alice, d.g).await.dr_ok();

    let rows = assignments(&pool, d.g).await;
    assert_eq!(rows.len(), 3);
    assert_valid(&rows, &[d.alice, d.bob, d.carol], &[]);
    assert!(is_drawn(&pool, d.g).await);
    assert!(drawn_at(&pool, d.g).await.is_some());
    // draw_success is flashed to Alice.
    let info = alice.session_info().await;
    assert!(
        info.flash
            .iter()
            .any(|m| m.key == "flash.success.draw_success"),
        "flash {:?}",
        info.flash
    );

    // Her page shows exactly one assignment, her own, linking to her receiver's wishlist.
    let my_receiver = rows.iter().find(|r| r.0 == d.alice).unwrap().1;
    let page = alice.get(&format!("/groups/{}", d.g)).await;
    assert_eq!(page.status, 200);
    assert_eq!(testid_count(&page.body, "my-assignment"), 1);
    let card = block(&page.body, "my-assignment").expect("assignment card");
    let link = open_tag(card, "assignment-wishlist").expect("wishlist button");
    assert_eq!(
        attr(link, "href"),
        Some(format!("/groups/{}/wishlist/{my_receiver}", d.g).as_str())
    );
    assert!(text_of(block(card, "assignment-receiver").unwrap()).contains(d.name_of(my_receiver)));

    // One mail per member, naming their own receiver, linking to the group, nobody else's.
    d.idle().await;
    for giver in [d.alice, d.bob, d.carol] {
        let receiver = rows.iter().find(|r| r.0 == giver).unwrap().1;
        let mails = d.draw_mails(giver, GROUP_NAME).await;
        assert_eq!(mails.len(), 1, "draw mails to {}", d.name_of(giver));
        let content = everything(&mails[0]);
        assert!(content.contains(d.name_of(receiver)), "names the receiver");
        assert!(
            content.contains(&format!("{APP_URL}/groups/{}", d.g)),
            "links to the group"
        );
        // The third member (neither giver nor receiver) does not appear at all.
        let third = [d.alice, d.bob, d.carol]
            .into_iter()
            .find(|u| *u != giver && *u != receiver)
            .unwrap();
        assert!(
            !content.contains(d.name_of(third)),
            "mail of {} mentions {}",
            d.name_of(giver),
            d.name_of(third)
        );
        assert_eq!(d.mail_count(giver).await, 1, "no other mails");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_the_draw_honours_the_members_exclusion_rules(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    // Bob must not draw Alice, so Bob -> Carol is forced.
    ts::add_exclusion(&pool, d.g, d.bob, d.alice).await;
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    let rows = assignments(&pool, d.g).await;
    assert_valid(&rows, &[d.alice, d.bob, d.carol], &[(d.bob, d.alice)]);
    assert!(rows.contains(&(d.bob, d.carol)));
    d.idle().await;
}

// ------------------------------------------------------------------------ AC15

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_group_of_two_is_drawn_after_the_small_group_hint(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let g2 = ts::create_group(&pool, d.alice, "Duo").await;
    ts::add_member(&pool, g2, d.bob).await;
    let mut alice = d.alice().await;

    let page = alice.get(&format!("/groups/{g2}")).await;
    let panel = block(&page.body, "draw-panel").expect("draw panel");
    assert!(text_of(panel).contains(&en("group.draw.smallGroupHint")));

    perform(&mut alice, g2).await.dr_ok();
    assert_eq!(
        assignments(&pool, g2).await,
        vec![(d.alice, d.bob), (d.bob, d.alice)]
    );
    d.idle().await;
}

// ------------------------------------------------------------------------ AC16

#[sqlx::test(migrations = "../migrations")]
async fn ac16_only_the_admin_may_draw_and_nothing_is_written_otherwise(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let before = groups_snapshot(&pool).await;

    assert_eq!(
        perform(&mut d.bob().await, d.g).await.dr_err(),
        DrawApiError::Forbidden
    );
    assert_eq!(
        perform(&mut d.dave().await, d.g).await.dr_err(),
        DrawApiError::NotFound
    );
    assert_eq!(
        perform(&mut d.anon(), d.g).await.dr_err(),
        DrawApiError::Unauthorized
    );
    // A group that does not exist looks the same as one the caller is not in.
    assert_eq!(
        perform(&mut d.alice().await, 999_999).await.dr_err(),
        DrawApiError::NotFound
    );

    d.idle().await;
    assert_eq!(groups_snapshot(&pool).await, before);
    for user in [d.alice, d.bob, d.carol, d.dave] {
        assert_eq!(d.mail_count(user).await, 0, "mail to {}", d.name_of(user));
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac16_a_member_cannot_read_the_admin_status_either(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    assert_eq!(
        status_reply(&mut d.bob().await, d.g).await.dr_err(),
        DrawApiError::Forbidden
    );
    assert_eq!(
        status_reply(&mut d.dave().await, d.g).await.dr_err(),
        DrawApiError::NotFound
    );
    assert_eq!(
        status_reply(&mut d.anon(), d.g).await.dr_err(),
        DrawApiError::Unauthorized
    );
}

// ------------------------------------------------------------------------ AC17

#[sqlx::test(migrations = "../migrations")]
async fn ac17_a_second_draw_is_refused_and_changes_nothing(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    let rows = assignments(&pool, d.g).await;
    let before = groups_snapshot(&pool).await;
    let drawn = drawn_at(&pool, d.g).await;

    let reply = perform(&mut d.alice().await, d.g).await;
    assert_eq!(reply.dr_err(), DrawApiError::AlreadyDrawn);
    d.idle().await;

    assert_eq!(assignments(&pool, d.g).await, rows);
    assert_eq!(groups_snapshot(&pool).await, before);
    assert_eq!(drawn_at(&pool, d.g).await, drawn);
    for user in [d.alice, d.bob, d.carol] {
        assert_eq!(
            d.mail_count(user).await,
            1,
            "no second mail to {}",
            d.name_of(user)
        );
    }
}

// ------------------------------------------------------------------------ AC18

#[sqlx::test(migrations = "../migrations")]
async fn ac18_a_group_with_only_the_admin_cannot_be_drawn(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let solo = ts::create_group(&pool, d.alice, "Solo").await;
    let mut alice = d.alice().await;

    let st = status(&mut alice, solo).await;
    assert_eq!(st.member_count, 1);
    assert_eq!(
        st.blocker,
        Some(DrawBlocker::TooFewMembers { count: 1, min: 2 })
    );

    let page = alice.get(&format!("/groups/{solo}")).await;
    let button = open_tag(&page.body, "draw-perform").expect("perform button");
    assert!(has_flag(button, "disabled"), "{button}");
    let blocker = text_of(block(&page.body, "draw-blocker").expect("blocker text"));
    assert_eq!(
        blocker,
        en_with(
            "group.draw.blocked.tooFewMembers",
            &[("count", "1"), ("min", "2")]
        )
    );

    assert_eq!(
        perform(&mut alice, solo).await.dr_err(),
        DrawApiError::TooFewMembers { count: 1, min: 2 }
    );
    assert!(assignments(&pool, solo).await.is_empty());
    assert!(!is_drawn(&pool, solo).await);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac18_more_than_200_members_cannot_be_drawn(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let big = ts::create_group(&pool, d.alice, "Big").await;
    for i in 0..200 {
        let id = ts::create_user(&pool, &format!("Member{i}"), &unique_email("big")).await;
        ts::add_member(&pool, big, id).await;
    }
    let mut alice = d.alice().await;
    let expected = DrawApiError::TooManyMembers {
        count: 201,
        max: 200,
    };
    assert_eq!(
        status(&mut alice, big).await.blocker,
        Some(DrawBlocker::TooManyMembers {
            count: 201,
            max: 200
        })
    );
    assert_eq!(perform(&mut alice, big).await.dr_err(), expected);
    assert!(assignments(&pool, big).await.is_empty());
}

// ------------------------------------------------------------------------ AC19

/// Dave joined, Bob excluded Alice and Carol (accepted while Dave was a member), Dave left:
/// Bob can draw nobody.
async fn blocked_world(pool: &MySqlPool) -> D {
    let d = draw_world(pool).await;
    ts::add_member(pool, d.g, d.dave).await;
    ts::add_exclusion(pool, d.g, d.bob, d.alice).await;
    ts::add_exclusion(pool, d.g, d.bob, d.carol).await;
    leave_group(&mut d.dave().await, d.g).await.group_ok();
    assert_eq!(
        ts::member_ids(pool, d.g).await,
        vec![d.alice, d.bob, d.carol]
    );
    d
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_an_impossible_draw_is_reported_without_naming_anyone(pool: MySqlPool) {
    let d = blocked_world(&pool).await;
    let mut alice = d.alice().await;
    let before = groups_snapshot(&pool).await;
    let (log, _guard) = capture_logs();

    // The status: a data-less blocker, and no member name anywhere in the response.
    let reply = status_reply(&mut alice, d.g).await;
    let status: DrawAdminStatus = reply.dr_value();
    assert_eq!(status.blocker, Some(DrawBlocker::DrawImpossible));
    let json: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(json["blocker"], serde_json::json!("DrawImpossible"));
    for name in ["Bob", "Carol", "Alice"] {
        assert!(
            !reply.body.contains(name),
            "status mentions {name}: {}",
            reply.body
        );
    }

    // The page.
    let page = alice.get(&format!("/groups/{}", d.g)).await;
    let panel = block(&page.body, "draw-panel").expect("panel");
    assert!(has_flag(
        open_tag(panel, "draw-perform").unwrap(),
        "disabled"
    ));
    let text = text_of(panel);
    assert!(text.contains(&en("exclusion.warning.drawImpossible")));
    assert!(text.contains(&en("group.draw.impossible.hint")));
    for name in ["Bob", "Carol"] {
        assert!(!text.contains(name), "panel mentions {name}: {text}");
    }

    // The draw itself: one generic error, nothing written.
    let err = perform(&mut alice, d.g).await;
    assert_eq!(err.dr_err(), DrawApiError::DrawImpossible);
    for name in ["Bob", "Carol"] {
        assert!(!err.body.contains(name), "{}", err.body);
    }
    d.idle().await;
    assert_eq!(groups_snapshot(&pool).await, before);

    // The log carries the kind, but no names, addresses or infeasibility ids.
    let log = log.contents();
    assert!(log.contains("no_receiver_for"), "log: {log}");
    for needle in [
        "Bob",
        "Carol",
        &d.emails[1],
        "NoReceiverFor",
        "Infeasib",
        "giver:",
    ] {
        assert!(!log.contains(needle), "log mentions {needle}: {log}");
    }
}

// ------------------------------------------------------------------------ AC20

#[sqlx::test(migrations = "../migrations")]
async fn ac20_drawing_before_the_deadline_closes_registration(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let deadline = Utc::now() + Duration::days(10);
    ts::set_group_dates(&pool, d.g, Some(deadline), None).await;
    let mut alice = d.alice().await;

    let st = status(&mut alice, d.g).await;
    let open_until = st.registration_open_until.expect("open until is set");
    assert!((open_until - deadline).num_seconds().abs() <= 1);

    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    assert_eq!(
        join_group(&mut d.dave().await, CODE).await.group_err(),
        GroupError::AlreadyDrawn
    );
    // After the draw there is no "open until" any more.
    assert_eq!(status(&mut alice, d.g).await.registration_open_until, None);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac20_without_a_future_deadline_there_is_no_warning(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    assert_eq!(status(&mut alice, d.g).await.registration_open_until, None);
    ts::set_group_dates(&pool, d.g, Some(Utc::now() - Duration::days(1)), None).await;
    assert_eq!(status(&mut alice, d.g).await.registration_open_until, None);
}

// ------------------------------------------------------------------------ AC21

#[sqlx::test(migrations = "../migrations")]
async fn ac21_a_plain_get_or_a_cross_origin_post_changes_nothing(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut alice = d.alice().await;

    // GET links are not an entry point.
    for path in ["perform_draw", "reset_draw"] {
        let reply = alice.get(&format!("/api/{path}?group_id={}", d.g)).await;
        assert!(
            !reply.status.is_success(),
            "GET /api/{path}: {}",
            reply.status
        );
    }
    // Cross-origin POSTs are refused.
    let mut evil = alice.fork().origin("https://evil.example");
    for name in ["perform_draw", "reset_draw"] {
        let reply = evil.call(name, &[("group_id", &d.g.to_string())]).await;
        assert_eq!(reply.status, 403, "{name}");
    }
    d.idle().await;
    assert_eq!(groups_snapshot(&pool).await, before);

    // The same for a drawn group (reset must not run either).
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    let drawn = groups_snapshot(&pool).await;
    for name in ["perform_draw", "reset_draw", "resend_draw_notifications"] {
        let reply = evil.call(name, &[("group_id", &d.g.to_string())]).await;
        assert_eq!(reply.status, 403, "{name}");
    }
    let reply = alice
        .get(&format!("/api/reset_draw?group_id={}", d.g))
        .await;
    assert!(!reply.status.is_success());
    assert_eq!(groups_snapshot(&pool).await, drawn);
}

// ------------------------------------------------------------------------ AC24

#[sqlx::test(migrations = "../migrations")]
async fn ac24_the_call_returns_before_any_mail_is_sent(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.block();
    let mut alice = d.alice().await;

    perform(&mut alice, d.g).await.dr_ok();
    // The draw is committed and answered while the mailer is stuck.
    assert!(is_drawn(&pool, d.g).await);
    for user in [d.alice, d.bob, d.carol] {
        assert_eq!(
            d.mail_count(user).await,
            0,
            "mail to {} too early",
            d.name_of(user)
        );
    }
    let st = status(&mut alice, d.g).await;
    let notes = st.notifications.expect("notification summary");
    assert_eq!(
        (notes.total, notes.sent, notes.pending, notes.failed),
        (3, 0, 3, 0)
    );

    d.transport.unblock();
    d.idle().await;
    for user in [d.alice, d.bob, d.carol] {
        assert_eq!(d.mail_count(user).await, 1);
    }
}

// ----------------------------------------------------- the rules a rule check relies on

#[sqlx::test(migrations = "../migrations")]
async fn the_draw_keeps_the_rules_it_used(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut bob = d.bob().await;
    add_ex(&mut bob, d.g, d.alice).await;
    perform(&mut d.alice().await, d.g).await.dr_ok();
    d.idle().await;
    assert_eq!(rules(&pool, d.g).await, vec![(d.bob, d.alice)]);
}
