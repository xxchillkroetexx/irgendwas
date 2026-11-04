//! draw slice: the mail worker and the admin's delivery view (AC34-AC39, AC48, AC49 mails).
//! The transport fails or blocks per test; retry delays are zero.

use crate::common::draw::*;
use crate::common::groups::*;
use app::draw_api::DrawApiError;
use app::server::draw_worker::DrawWorker;
use app::server::mail::escape_html;
use app::test_support as ts;
use sqlx::MySqlPool;
use std::sync::Arc;
use std::time::Duration;

// ------------------------------------------------------------------------ AC34

#[sqlx::test(migrations = "../migrations")]
async fn ac34_a_failing_address_is_tried_three_times_and_shown_as_failed(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.fail(&d.emails[1]);
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;

    let state = delivery(&pool, d.g).await;
    assert_eq!(
        state,
        vec![(d.alice, true, 1), (d.bob, false, 3), (d.carol, true, 1)]
    );
    assert_eq!(
        d.transport.attempts_to(&d.emails[1]),
        3,
        "Bob: three attempts"
    );
    assert_eq!(d.transport.attempts_to(&d.emails[0]), 1);
    assert_eq!(d.transport.attempts_to(&d.emails[2]), 1);
    assert_eq!(d.mail_count(d.bob).await, 0);
    assert!(
        is_drawn(&pool, d.g).await,
        "a mail failure does not undo the draw"
    );
    assert_eq!(assignments(&pool, d.g).await.len(), 3);

    let st = status(&mut alice, d.g).await;
    let notes = st.notifications.expect("summary");
    assert_eq!(
        (notes.total, notes.sent, notes.pending, notes.failed),
        (3, 2, 0, 1)
    );
    assert_eq!(notes.failed_member_names, vec!["Bob".to_string()]);

    let page = alice.get(&format!("/groups/{}", d.g)).await;
    let section = block(&page.body, "draw-notifications").expect("admin section");
    let text = text_of(section);
    assert!(
        text.contains(&en_plural(
            "group.draw.notifications.summary",
            3,
            &[("sent", "2"), ("total", "3")]
        )),
        "{text}"
    );
    assert!(
        text.contains(&en_with(
            "group.draw.notifications.failed",
            &[("names", "Bob")]
        )),
        "{text}"
    );
    assert!(
        text.contains(&en("group.draw.notifications.resend")),
        "{text}"
    );
    assert_eq!(testid_count(section, "draw-resend"), 1);
    assert_eq!(testid_count(&page.body, "draw-reset"), 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac34_without_failures_there_is_no_failed_line_and_no_resend_button(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    let page = d.alice().await.get(&format!("/groups/{}", d.g)).await;
    let section = block(&page.body, "draw-notifications").expect("admin section");
    assert_eq!(testid_count(section, "draw-resend"), 0);
    assert_eq!(testid_count(section, "notifications-failed"), 0);
    assert_eq!(testid_count(section, "notifications-pending"), 0);
    assert!(text_of(section).contains(&en_plural(
        "group.draw.notifications.summary",
        3,
        &[("sent", "3"), ("total", "3")]
    )));
}

#[sqlx::test(migrations = "../migrations")]
async fn d8_members_do_not_see_the_delivery_section(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.draw_and_deliver(d.g).await;
    let page = d.bob().await.get(&format!("/groups/{}", d.g)).await;
    assert_eq!(testid_count(&page.body, "draw-notifications"), 0);
    assert_eq!(testid_count(&page.body, "draw-reset"), 0);
    assert_eq!(testid_count(&page.body, "draw-resend"), 0);
    assert_eq!(testid_count(&page.body, "draw-perform"), 0);
}

// ------------------------------------------------------------------------ AC35

#[sqlx::test(migrations = "../migrations")]
async fn ac35_resend_sends_only_the_failed_mail(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.fail(&d.emails[1]);
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    d.transport.heal(&d.emails[1]);

    let reply = resend(&mut alice, d.g).await;
    assert_eq!(reply.dr_value::<u32>(), 1);
    let info = alice.session_info().await;
    let flash = info
        .flash
        .iter()
        .find(|m| m.key == "flash.success.draw_notifications_requeued")
        .unwrap_or_else(|| panic!("flash {:?}", info.flash));
    assert_eq!(flash.plural, Some(1));
    d.idle().await;

    assert_eq!(d.draw_mails(d.bob, GROUP_NAME).await.len(), 1);
    assert_eq!(d.mail_count(d.bob).await, 1);
    assert_eq!(d.mail_count(d.alice).await, 1, "no second mail for Alice");
    assert_eq!(d.mail_count(d.carol).await, 1, "no second mail for Carol");
    let sent = delivery(&pool, d.g).await;
    assert!(sent.iter().all(|r| r.1), "every row is sent now: {sent:?}");
    let st = status(&mut alice, d.g).await.notifications.unwrap();
    assert_eq!((st.sent, st.failed, st.pending), (3, 0, 0));
}

// ------------------------------------------------------------------------ AC36

#[sqlx::test(migrations = "../migrations")]
async fn ac36_resend_errors(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    assert_eq!(
        resend(&mut alice, d.g).await.dr_err(),
        DrawApiError::NotDrawn
    );
    d.draw_and_deliver(d.g).await;
    assert_eq!(
        resend(&mut alice, d.g).await.dr_err(),
        DrawApiError::NothingToResend
    );
    assert_eq!(
        resend(&mut d.bob().await, d.g).await.dr_err(),
        DrawApiError::Forbidden
    );
    assert_eq!(
        resend(&mut d.dave().await, d.g).await.dr_err(),
        DrawApiError::NotFound
    );
    assert_eq!(
        resend(&mut d.anon(), d.g).await.dr_err(),
        DrawApiError::Unauthorized
    );
    d.idle().await;
    for user in [d.alice, d.bob, d.carol] {
        assert_eq!(d.mail_count(user).await, 1);
    }
}

// ------------------------------------------------------------------------ AC37

#[sqlx::test(migrations = "../migrations")]
async fn ac37_startup_recovery_sends_unsent_rows_below_the_attempt_limit(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    // A drawn group as a crashed server left it: Alice's mail never went out, Bob's has
    // used up its attempts, Carol's was sent.
    ts::set_drawn(&pool, d.g).await;
    set_drawn_at(&pool, d.g, "2026-11-01 10:00:00").await;
    ts::add_assignment(&pool, d.g, d.alice, d.bob).await;
    ts::add_assignment(&pool, d.g, d.bob, d.carol).await;
    ts::add_assignment(&pool, d.g, d.carol, d.alice).await;
    sqlx::query("UPDATE gift_assignments SET notification_attempts = 3 WHERE giver_id = ?")
        .bind(d.bob)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE gift_assignments SET notification_sent = 1, notification_attempts = 1 WHERE giver_id = ?",
    )
    .bind(d.carol)
    .execute(&pool)
    .await
    .unwrap();

    d.worker.recover().await.expect("recovery");
    d.idle().await;

    assert_eq!(d.draw_mails(d.alice, GROUP_NAME).await.len(), 1);
    assert_eq!(
        d.mail_count(d.bob).await,
        0,
        "attempts >= 3 are not retried"
    );
    assert_eq!(
        d.mail_count(d.carol).await,
        0,
        "sent rows are not sent again"
    );
    assert_eq!(
        delivery(&pool, d.g).await,
        vec![(d.alice, true, 1), (d.bob, false, 3), (d.carol, true, 1)]
    );

    // Recovery is idempotent: a second run finds nothing to do.
    d.worker.recover().await.expect("second recovery");
    d.idle().await;
    assert_eq!(d.mail_count(d.alice).await, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac37_recovery_ignores_groups_that_are_not_drawn(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    ts::add_assignment(&pool, d.g, d.alice, d.bob).await; // leftover row, is_drawn = 0
    d.worker.recover().await.expect("recovery");
    d.idle().await;
    assert_eq!(d.mail_count(d.bob).await, 0);
    assert_eq!(d.mail_count(d.alice).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac37_a_delay_between_attempts_is_waited_for(pool: MySqlPool) {
    // retry_delays[attempts - 1] is the wait before the next attempt; wait_idle also covers
    // a retry that is still waiting.
    let d = draw_world(&pool).await;
    let worker = DrawWorker::new(
        pool.clone(),
        Arc::new(crate::common::config_with(&[])),
        d.transport.clone(),
        vec![Duration::from_millis(300), Duration::from_millis(300)],
    );
    d.transport.fail(&d.emails[1]);
    ts::set_drawn(&pool, d.g).await;
    ts::add_assignment(&pool, d.g, d.alice, d.bob).await;
    ts::add_assignment(&pool, d.g, d.bob, d.carol).await;
    ts::add_assignment(&pool, d.g, d.carol, d.alice).await;
    let started = std::time::Instant::now();
    worker.recover().await.unwrap();
    worker.wait_idle().await;
    assert!(
        started.elapsed() >= Duration::from_millis(550),
        "two waits of 300 ms expected, took {:?}",
        started.elapsed()
    );
    assert_eq!(d.transport.attempts_to(&d.emails[1]), 3);
}

// ------------------------------------------------------------------------ AC38

#[sqlx::test(migrations = "../migrations")]
async fn ac38_mail_values_are_escaped_and_the_mail_is_html_plus_text(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let group = "<b>x</b>";
    let member = "<script>y</script>";
    rename_group(&pool, d.g, group).await;
    rename(&pool, d.bob, member).await;
    ts::set_user_locale(&pool, d.carol, Some("de")).await;
    d.draw_and_deliver(d.g).await;

    let rows = assignments(&pool, d.g).await;
    let giver_to_bob = rows.iter().find(|r| r.1 == d.bob).unwrap().0;
    let mail = &d.mails(giver_to_bob).await[0];
    assert!(
        mail.html.contains(&escape_html(group)),
        "group name escaped in: {}",
        mail.html
    );
    assert!(
        mail.html.contains(&escape_html(member)),
        "receiver name escaped in: {}",
        mail.html
    );
    for raw in ["<b>x</b>", "<script>y</script>"] {
        assert!(!mail.html.contains(raw), "raw {raw} in the HTML");
    }
    assert!(!mail.text.trim().is_empty(), "plain-text part present");
    assert!(
        mail.text.contains(member),
        "the text part is plain text, not escaped HTML"
    );

    // Bob's own mail greets him by his (escaped) name.
    let bobs = &d.mails(d.bob).await[0];
    assert!(bobs.html.contains(&escape_html(member)));
    assert!(!bobs.html.contains("<script>"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac38_a_giver_with_locale_de_gets_the_german_mail(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    ts::set_user_locale(&pool, d.carol, Some("de")).await;
    d.draw_and_deliver(d.g).await;
    let mail = &d.mails(d.carol).await[0];
    assert_eq!(
        mail.subject,
        de_with("email.drawResult.subject", &[("group", GROUP_NAME)])
    );
    assert!(mail.html.contains("lang=\"de\""), "{}", mail.html);
    assert!(
        mail.html
            .contains(&escape_html(&de("email.drawResult.secret")))
    );
    // Bob (no locale, APP_LOCALE default en) gets English.
    let bob = &d.mails(d.bob).await[0];
    assert_eq!(
        bob.subject,
        en_with("email.drawResult.subject", &[("group", GROUP_NAME)])
    );
    assert!(bob.html.contains("lang=\"en\""));
}

// ------------------------------------------------------------------------ AC39

#[sqlx::test(migrations = "../migrations")]
async fn ac39_while_mails_are_pending_the_panel_says_so(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.block();
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    let page = alice.get(&format!("/groups/{}", d.g)).await;
    let section = text_of(block(&page.body, "draw-notifications").expect("section"));
    assert!(
        section.contains(&en_plural(
            "group.draw.notifications.pending",
            3,
            &[("count", "3")]
        )),
        "{section}"
    );
    assert!(section.contains(&en_plural(
        "group.draw.notifications.summary",
        3,
        &[("sent", "0"), ("total", "3")]
    )));
    d.transport.unblock();
    d.idle().await;
}

// ------------------------------------------------------------------------ AC48

fn is_german(mail: &app::test_support::mailpit::Mail, subject_key: &str) -> bool {
    mail.subject == de_with(subject_key, &[("group", GROUP_NAME)])
        && mail.html.contains("lang=\"de\"")
}

fn is_english(mail: &app::test_support::mailpit::Mail, subject_key: &str) -> bool {
    mail.subject == en_with(subject_key, &[("group", GROUP_NAME)])
        && mail.html.contains("lang=\"en\"")
}

#[sqlx::test(migrations = "../migrations")]
async fn ac48_every_mail_uses_the_recipients_account_language(pool: MySqlPool) {
    let d = draw_world_with(&pool, &[("APP_LOCALE", "en")]).await;
    ts::set_user_locale(&pool, d.alice, Some("de")).await;
    ts::set_user_locale(&pool, d.bob, Some("en")).await;
    // Alice browses in English; her mail is German anyway.
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    assert!(is_german(
        &d.draw_mails(d.alice, GROUP_NAME).await[0],
        "email.drawResult.subject"
    ));
    assert!(is_english(
        &d.draw_mails(d.bob, GROUP_NAME).await[0],
        "email.drawResult.subject"
    ));
    assert!(is_english(
        &d.draw_mails(d.carol, GROUP_NAME).await[0],
        "email.drawResult.subject"
    ));

    reset(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    assert!(is_english(
        &d.reset_mails(d.bob, GROUP_NAME).await[0],
        "email.drawReset.subject"
    ));
    assert!(is_english(
        &d.reset_mails(d.carol, GROUP_NAME).await[0],
        "email.drawReset.subject"
    ));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac48_the_page_language_of_the_admin_does_not_matter(pool: MySqlPool) {
    let d = draw_world_with(&pool, &[("APP_LOCALE", "de")]).await;
    ts::set_user_locale(&pool, d.alice, Some("en")).await;
    let mut alice = d.german("alice").await; // German page, English account
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    assert!(is_english(
        &d.draw_mails(d.alice, GROUP_NAME).await[0],
        "email.drawResult.subject"
    ));
    // No stored locale: APP_LOCALE (de) applies.
    assert!(is_german(
        &d.draw_mails(d.carol, GROUP_NAME).await[0],
        "email.drawResult.subject"
    ));
    reset(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    assert!(is_german(
        &d.reset_mails(d.carol, GROUP_NAME).await[0],
        "email.drawReset.subject"
    ));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac48_an_unknown_stored_locale_falls_back_to_app_locale(pool: MySqlPool) {
    let d = draw_world_with(&pool, &[("APP_LOCALE", "de")]).await;
    ts::set_user_locale(&pool, d.carol, Some("fr")).await;
    d.draw_and_deliver(d.g).await;
    assert!(is_german(
        &d.draw_mails(d.carol, GROUP_NAME).await[0],
        "email.drawResult.subject"
    ));
    reset(&mut d.alice().await, d.g).await.dr_ok();
    d.idle().await;
    assert!(is_german(
        &d.reset_mails(d.carol, GROUP_NAME).await[0],
        "email.drawReset.subject"
    ));
}

// ------------------------------------------------------------------------ AC49

#[sqlx::test(migrations = "../migrations")]
async fn ac49_one_of_one_sent_and_one_pending_use_the_singular_forms(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let g2 = ts::create_group(&pool, d.alice, "Duo").await;
    ts::add_member(&pool, g2, d.bob).await;
    let mut alice = d.alice().await;
    // A group of two: 2 mails. Check pending (2) then sent (2): plural; the singular forms
    // need a count of 1, so drop one row to model "1 pending / 1 of 1 sent".
    d.transport.block();
    perform(&mut alice, g2).await.dr_ok();
    sqlx::query("DELETE FROM gift_assignments WHERE group_id = ? AND giver_id = ?")
        .bind(g2)
        .bind(d.bob)
        .execute(&pool)
        .await
        .unwrap();
    let page = alice.get(&format!("/groups/{g2}")).await;
    let section = text_of(block(&page.body, "draw-notifications").unwrap());
    assert!(
        section.contains(&en_plural(
            "group.draw.notifications.pending",
            1,
            &[("count", "1")]
        )),
        "{section}"
    );
    d.transport.unblock();
    d.idle().await;
    let page = alice.get(&format!("/groups/{g2}")).await;
    let section = text_of(block(&page.body, "draw-notifications").unwrap());
    assert!(
        section.contains(&en_plural(
            "group.draw.notifications.summary",
            1,
            &[("sent", "1"), ("total", "1")]
        )),
        "{section}"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac49_the_requeued_flash_reads_in_the_singular_for_one_mail(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    d.transport.fail(&d.emails[1]);
    let mut alice = d.alice().await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    d.transport.heal(&d.emails[1]);
    resend(&mut alice, d.g).await.dr_ok();
    let info = alice.session_info().await;
    let flash = info
        .flash
        .iter()
        .find(|m| m.key == "flash.success.draw_notifications_requeued")
        .expect("requeued flash");
    let en_text = flash.text(&app::i18n::I18n::new(app::i18n::Locale::En, chrono_tz::UTC));
    assert_eq!(en_text, "1 email is being sent again.");
    d.idle().await;
}
