//! draw slice: the panel on the group page, the dashboard card, plain (no-JS) form posts and
//! the catalogs (AC46, AC47, AC49, parts of AC14/AC15/AC18/AC19).

use crate::common::draw::*;
use crate::common::groups::*;
use app::test_support as ts;
use sqlx::MySqlPool;

fn group_path(g: i32) -> String {
    format!("/groups/{g}")
}

// ------------------------------------------------------------------------ AC46

#[sqlx::test(migrations = "../migrations")]
async fn ac46_before_the_draw_members_wait_and_the_admin_has_one_button(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let bob = d.bob().await.get(&group_path(d.g)).await;
    let panel = block(&bob.body, "draw-panel").expect("panel");
    assert!(text_of(panel).contains(&en("group.draw.pending")));
    for id in [
        "draw-perform",
        "draw-reset",
        "draw-resend",
        "draw-notifications",
    ] {
        assert_eq!(testid_count(&bob.body, id), 0, "Bob sees {id}");
    }
    assert!(!text_of(&bob.body).contains(&en("group.show.performDraw")));

    let alice = d.alice().await.get(&group_path(d.g)).await;
    assert_eq!(testid_count(&alice.body, "draw-perform"), 1);
    assert_eq!(
        count(&text_of(&alice.body), &en("group.show.performDraw")),
        1
    );
    assert_eq!(testid_count(&alice.body, "draw-reset"), 0);
    assert_eq!(testid_count(&alice.body, "draw-pending"), 0);
    let button = open_tag(&alice.body, "draw-perform").unwrap();
    assert!(!has_flag(button, "disabled"), "{button}");
    let panel = text_of(block(&alice.body, "draw-panel").unwrap());
    assert!(panel.contains(&en("group.draw.title")));
    assert!(
        panel.contains(&en_plural("group.draw.ready", 3, &[("count", "3")])),
        "{panel}"
    );
    assert!(panel.contains(&en("group.draw.smallGroupHint")));
    // The perform button lives in a POST form with the group id (works without JS).
    let panel_html = block(&alice.body, "draw-panel").unwrap();
    assert!(panel_html.contains("/api/perform_draw"));
    assert!(panel_html.contains("name=\"group_id\""));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac46_after_the_draw_everyone_sees_their_card_and_only_the_admin_the_controls(
    pool: MySqlPool,
) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;

    let bob = d.bob().await.get(&group_path(d.g)).await;
    assert_eq!(testid_count(&bob.body, "my-assignment"), 1);
    for id in [
        "draw-perform",
        "draw-reset",
        "draw-resend",
        "draw-notifications",
        "draw-pending",
    ] {
        assert_eq!(testid_count(&bob.body, id), 0, "Bob sees {id}");
    }
    let card = text_of(block(&bob.body, "my-assignment").unwrap());
    assert!(card.contains(&en("group.show.assignment")), "{card}");
    assert!(
        card.contains(&en("group.show.assignmentDescription")),
        "{card}"
    );
    assert!(card.contains(&en("group.show.viewWishlist")), "{card}");

    let alice = d.alice().await.get(&group_path(d.g)).await;
    assert_eq!(
        testid_count(&alice.body, "my-assignment"),
        1,
        "the admin's card is her own"
    );
    assert_eq!(testid_count(&alice.body, "draw-perform"), 0);
    assert_eq!(testid_count(&alice.body, "draw-reset"), 1);
    assert_eq!(testid_count(&alice.body, "draw-notifications"), 1);
    assert!(text_of(&alice.body).contains(&en("group.draw.reset")));
    let panel = block(&alice.body, "draw-panel").unwrap();
    assert!(panel.contains("/api/reset_draw"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac46_the_panel_is_not_shown_to_outsiders(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let page = d.dave().await.get(&group_path(d.g)).await;
    assert_eq!(testid_count(&page.body, "draw-panel"), 0);
    assert_eq!(testid_count(&page.body, "my-assignment"), 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_the_small_group_hint_is_for_groups_of_up_to_three(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    let hint = en("group.draw.smallGroupHint");
    assert!(text_of(&alice.get(&group_path(d.g)).await.body).contains(&hint));
    ts::add_member(&pool, d.g, d.dave).await;
    assert!(!text_of(&alice.get(&group_path(d.g)).await.body).contains(&hint));
}

// ------------------------------------------------------------------------ AC47

#[sqlx::test(migrations = "../migrations")]
async fn ac47_the_panel_and_the_dashboard_come_from_the_german_catalog(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.german("alice").await;
    let mut bob = d.german("bob").await;

    let page = alice.get(&group_path(d.g)).await;
    let panel = text_of(block(&page.body, "draw-panel").unwrap());
    assert!(panel.contains(&de("group.draw.title")), "{panel}");
    assert!(
        panel.contains(&de_plural("group.draw.ready", 3, &[("count", "3")])),
        "{panel}"
    );
    assert!(panel.contains(&de("group.draw.smallGroupHint")));
    assert!(panel.contains(&de("group.show.performDraw")));
    let bobs = text_of(block(&bob.get(&group_path(d.g)).await.body, "draw-panel").unwrap());
    assert!(bobs.contains(&de("group.draw.pending")), "{bobs}");

    // The blocker texts.
    let solo = ts::create_group(&pool, d.alice, "Solo").await;
    let page = alice.get(&group_path(solo)).await;
    assert_eq!(
        text_of(block(&page.body, "draw-blocker").unwrap()),
        de_with(
            "group.draw.blocked.tooFewMembers",
            &[("count", "1"), ("min", "2")]
        )
    );

    // The dashboard card, empty and with an assignment.
    let dash = bob.get("/user/dashboard").await;
    let card = text_of(block(&dash.body, "dashboard-assignments").unwrap());
    assert!(
        card.contains(&de("user.dashboard.assignments.title")),
        "{card}"
    );
    assert!(
        card.contains(&de("user.dashboard.assignments.empty")),
        "{card}"
    );

    d.draw_and_deliver(d.g).await;
    let page = bob.get(&group_path(d.g)).await;
    let card = text_of(block(&page.body, "my-assignment").unwrap());
    assert!(card.contains(&de("group.show.assignment")), "{card}");
    let admin = alice.get(&group_path(d.g)).await;
    let section = text_of(block(&admin.body, "draw-notifications").unwrap());
    assert!(
        section.contains(&de("group.draw.notifications.title")),
        "{section}"
    );
    assert!(section.contains(&de_plural(
        "group.draw.notifications.summary",
        3,
        &[("sent", "3"), ("total", "3")]
    )));
    assert!(text_of(&admin.body).contains(&de("group.draw.reset")));
}

#[test]
fn ac47_every_new_key_exists_in_both_catalogs_with_the_same_placeholders() {
    let plain = [
        "group.draw.title",
        "group.draw.pending",
        "group.draw.smallGroupHint",
        "group.draw.confirmRegistrationOpen",
        "group.draw.blocked.tooFewMembers",
        "group.draw.blocked.tooManyMembers",
        "group.draw.impossible.hint",
        "group.draw.drawnOn",
        "group.draw.notifications.title",
        "group.draw.notifications.failed",
        "group.draw.notifications.resend",
        "group.draw.reset",
        "group.draw.resetConfirm",
        "group.show.performDrawConfirm",
        "flash.success.draw_success",
        "flash.success.draw_reset",
        "flash.error.not_drawn",
        "flash.error.nothing_to_resend",
        "email.drawResult.subject",
        "email.drawResult.greeting",
        "email.drawResult.body",
        "email.drawResult.button",
        "email.drawResult.secret",
        "email.drawReset.subject",
        "email.drawReset.greeting",
        "email.drawReset.body",
        "email.drawReset.next",
    ];
    let plural = [
        "group.draw.ready",
        "group.draw.notifications.summary",
        "group.draw.notifications.pending",
        "flash.success.draw_notifications_requeued",
    ];
    let mut keys: Vec<String> = plain.iter().map(|k| k.to_string()).collect();
    for base in plural {
        keys.push(format!("{base}.one"));
        keys.push(format!("{base}.other"));
    }
    let placeholders = |text: &str| {
        let mut found: Vec<String> = text
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split('}').next())
            .map(str::to_string)
            .collect();
        found.sort();
        found.dedup();
        found
    };
    for key in keys {
        let (e, g) = (en(&key), de(&key));
        assert_eq!(
            placeholders(&e),
            placeholders(&g),
            "{key}: placeholders differ"
        );
    }
    // Removed keys.
    for gone in [
        "flash.error.draw_failed",
        "flash.warning.draw_partial_success",
        "flash.error.only_admin_draw",
    ] {
        assert_eq!(
            app::i18n::translate(app::i18n::Locale::En, gone),
            gone,
            "{gone} must be removed"
        );
    }
}

// ------------------------------------------------------------------------ AC49

#[sqlx::test(migrations = "../migrations")]
async fn ac49_the_drawn_date_is_formatted_in_the_readers_language(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    set_drawn_at(&pool, d.g, "2026-12-10 22:30:00").await;

    let en_page = d.bob().await.get(&group_path(d.g)).await;
    let line = text_of(block(&en_page.body, "assignment-drawn-on").expect("drawn on"));
    assert_eq!(
        line,
        en_with("group.draw.drawnOn", &[("date", "10 Dec 2026")])
    );
    let de_page = d.german("bob").await.get(&group_path(d.g)).await;
    let line = text_of(block(&de_page.body, "assignment-drawn-on").unwrap());
    assert_eq!(
        line,
        de_with("group.draw.drawnOn", &[("date", "10.12.2026")])
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac49_a_group_of_one_uses_the_singular_ready_form(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let solo = ts::create_group(&pool, d.alice, "Solo").await;
    let page = d.alice().await.get(&group_path(solo)).await;
    let line = text_of(block(&page.body, "draw-ready").expect("ready line"));
    assert_eq!(line, en_plural("group.draw.ready", 1, &[("count", "1")]));
    assert_ne!(line, en_plural("group.draw.ready", 2, &[("count", "1")]));
}

// ------------------------------------------------- no-JS form posts (302 + flash)

fn plain(c: &crate::common::auth::Client) -> crate::common::auth::Client {
    c.fork().header("accept", "text/html")
}

#[sqlx::test(migrations = "../migrations")]
async fn a_plain_form_post_draws_and_redirects_back_with_a_flash(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    let mut form = plain(&alice);

    let reply = perform(&mut form, d.g).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location().as_deref(), Some(group_path(d.g).as_str()));
    assert!(is_drawn(&pool, d.g).await);
    for name in ["Alice", "Bob", "Carol"] {
        assert!(!reply.body.contains(name), "redirect body mentions {name}");
    }
    let info = alice.session_info().await;
    assert!(
        info.flash
            .iter()
            .any(|m| m.key == "flash.success.draw_success"),
        "{:?}",
        info.flash
    );
    d.idle().await;

    // The reset form.
    let reply = reset(&mut form, d.g).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location().as_deref(), Some(group_path(d.g).as_str()));
    assert!(!is_drawn(&pool, d.g).await);
    let info = alice.session_info().await;
    assert!(
        info.flash
            .iter()
            .any(|m| m.key == "flash.success.draw_reset"),
        "{:?}",
        info.flash
    );
    d.idle().await;
}

#[sqlx::test(migrations = "../migrations")]
async fn a_plain_form_post_resends_failed_mails(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.fail(&d.emails[1]);
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    d.transport.heal(&d.emails[1]);

    let reply = resend(&mut plain(&alice), d.g).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location().as_deref(), Some(group_path(d.g).as_str()));
    let info = alice.session_info().await;
    assert!(
        info.flash
            .iter()
            .any(|m| m.key == "flash.success.draw_notifications_requeued"),
        "{:?}",
        info.flash
    );
    d.idle().await;
    assert_eq!(d.draw_mails(d.bob, GROUP_NAME).await.len(), 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_plain_form_post_error_comes_back_as_a_flash(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut bob = d.bob().await;
    let reply = perform(&mut plain(&bob), d.g).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location().as_deref(), Some(group_path(d.g).as_str()));
    let info = bob.session_info().await;
    assert!(
        info.flash
            .iter()
            .any(|m| m.key == "flash.error.forbidden_admin_only"),
        "{:?}",
        info.flash
    );
    assert!(!is_drawn(&pool, d.g).await);

    // Already drawn.
    d.draw_and_deliver(d.g).await;
    let mut alice = d.alice().await;
    let reply = perform(&mut plain(&alice), d.g).await;
    assert_eq!(reply.status, 302);
    let info = alice.session_info().await;
    assert!(
        info.flash
            .iter()
            .any(|m| m.key == "flash.error.already_drawn"),
        "{:?}",
        info.flash
    );
}
