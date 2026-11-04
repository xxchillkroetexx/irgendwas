//! account slice: requesting and cancelling an email change, and the verification mail
//! (AC20-AC28, AC38). Mails are read from Mailpit; every address is unique per test.

use crate::common::account::*;
use crate::common::auth::*;
use app::account_api::{AccountError, ConfirmResult};
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, TEST_PASSWORD, create_bob, create_user, set_pending_email,
    set_pending_expiry, set_user_locale, user_row, user_rows,
};
use chrono::{Duration, Utc};
use sqlx::MySqlPool;

const PROFILE: &str = "/user/profile";
const DAY: i64 = 86_400;

/// `address` as a user might type it: shouting, with spaces around.
fn typed(address: &str) -> String {
    format!("  {} ", address.to_uppercase())
}

// -------------------------------------------------------------------------- AC20

#[sqlx::test(migrations = "../migrations")]
async fn ac20_request_stores_a_hash_and_mails_the_link_to_the_new_address(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let new = unique_email("alice-new");
    let before = user_row(&pool, w.id).await;

    request_email_change(&mut w.c, &typed(&new), ALICE_PASSWORD)
        .await
        .account_ok();

    let row = user_row(&pool, w.id).await;
    assert_eq!(
        row.pending_email.as_deref(),
        Some(new.as_str()),
        "normalised"
    );
    assert_eq!(row.email, w.email, "the current address is unchanged");
    assert_eq!(row.password, before.password);
    assert_eq!(
        row.session_epoch, before.session_epoch,
        "sessions are not touched"
    );
    let expires = row.email_verification_expires.expect("expiry");
    let ttl = expires - Utc::now();
    assert!(
        (ttl - Duration::hours(24)).num_seconds().abs() <= 60,
        "valid for 24 h, got {ttl}"
    );

    // Exactly one mail, to the stored pending address, with a 43-character token link.
    let mails = mailpit.wait_for_count(&new, 1).await;
    assert_eq!(
        mails_after_settle(&mailpit, &new).await,
        1,
        "exactly one mail"
    );
    let mail = &mails[0];
    assert_eq!(
        mail.to
            .iter()
            .map(|a| a.address.as_str())
            .collect::<Vec<_>>(),
        [new.as_str()]
    );
    assert_eq!(mail.subject, en("email.verifyEmail.subject"));
    let link = mail.first_url_with_path(VERIFY).expect("verification link");
    assert!(link.starts_with(&format!("{APP_URL}{VERIFY}")), "{link}");
    let token = token_in(&link, VERIFY);
    assert!(is_token(&token), "{token:?}");

    // Only the SHA-256 of the token is stored.
    let stored = row.email_verification_token.expect("token hash");
    assert!(is_sha256_hex(&stored), "{stored}");
    assert_ne!(stored, token);
    assert_eq!(sha256_hex(&pool, &token).await, stored);

    // Greeting with the name and the new address, a plain-text alternative.
    assert!(mail.html.contains("Alice") && mail.html.contains(&new));
    assert!(mail.text.contains(&new) && mail.text.contains(&link));
    assert!(mail.html.contains("lang=\"en\""));

    // The profile now shows the pending box.
    let page = w.c.get(PROFILE).await;
    let pending = en_with("user.profile.emailPendingVerification", &[("email", &new)]);
    assert!(html_contains(&page.body, &pending), "{pending}");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac20_every_mail_carries_a_fresh_token(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let first = unique_email("alice-a");
    let second = unique_email("alice-b");

    request_email_change(&mut w.c, &first, ALICE_PASSWORD)
        .await
        .account_ok();
    set_pending_expiry(&pool, w.id, DAY - 120).await;
    request_email_change(&mut w.c, &second, ALICE_PASSWORD)
        .await
        .account_ok();

    let a = token_from_mail(&mailpit, &first).await;
    let b = token_from_mail(&mailpit, &second).await;
    assert_ne!(a, b);
}

// -------------------------------------------------------------------------- AC21

#[sqlx::test(migrations = "../migrations")]
async fn ac21_invalid_addresses_are_rejected(pool: MySqlPool) {
    let mut w = alice_world(&pool).await;
    set_pending_email(&pool, w.id, "prior@mail.test", &tok('p'), DAY - 3600).await;
    let before = user_rows(&pool).await;
    let too_long = format!("{}@example.com", "a".repeat(250));

    for bad in [
        "not-an-email",
        "a@b",
        "@example.com",
        "a@",
        "a@@example.com",
        "a@b@example.com",
        "a b@example.com",
        "a@exa mple.com",
        "a@.example.com",
        "a@example.com.",
        "a\u{7}@example.com",
        too_long.as_str(),
    ] {
        assert_eq!(
            request_email_change(&mut w.c, bad, ALICE_PASSWORD)
                .await
                .account_err(),
            AccountError::InvalidEmail,
            "{bad:?}"
        );
    }
    assert_eq!(
        user_rows(&pool).await,
        before,
        "the pending columns are unchanged"
    );
}

// -------------------------------------------------------------------------- AC22

#[sqlx::test(migrations = "../migrations")]
async fn ac22_the_current_address_in_any_spelling_is_unchanged(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let before = user_rows(&pool).await;

    for same in [typed(&w.email), w.email.to_uppercase(), w.email.clone()] {
        assert_eq!(
            request_email_change(&mut w.c, &same, ALICE_PASSWORD)
                .await
                .account_err(),
            AccountError::EmailUnchanged,
            "{same:?}"
        );
    }
    assert_eq!(user_rows(&pool).await, before);
    assert_eq!(mails_after_settle(&mailpit, &w.email).await, 0);
}

// -------------------------------------------------------------------------- AC23

#[sqlx::test(migrations = "../migrations")]
async fn ac23_the_current_password_is_required(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let new = unique_email("alice-new");
    let before = user_rows(&pool).await;

    for wrong in ["wrong", "", ALICE_PASSWORD.to_uppercase().as_str(), " "] {
        assert_eq!(
            request_email_change(&mut w.c, &new, wrong)
                .await
                .account_err(),
            AccountError::CurrentPasswordIncorrect,
            "{wrong:?}"
        );
    }
    assert_eq!(user_rows(&pool).await, before);
    assert_eq!(mails_after_settle(&mailpit, &new).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_the_password_field_is_not_optional(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let new = unique_email("alice-new");
    let before = user_rows(&pool).await;

    let reply =
        w.c.call("request_email_change", &[("new_email", &new)])
            .await;
    assert!(
        !reply.status.is_success(),
        "a missing password is not a success"
    );
    assert_eq!(user_rows(&pool).await, before);
    assert_eq!(mails_after_settle(&mailpit, &new).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac21_ac22_ac23_ac25_errors_are_checked_in_the_documented_order(pool: MySqlPool) {
    let mut w = alice_world(&pool).await;
    let new = unique_email("alice-new");

    // Email validity, then unchanged, then password.
    assert_eq!(
        request_email_change(&mut w.c, "nope", "wrong")
            .await
            .account_err(),
        AccountError::InvalidEmail
    );
    assert_eq!(
        request_email_change(&mut w.c, &w.email, "wrong")
            .await
            .account_err(),
        AccountError::EmailUnchanged
    );
    // The password is checked before the throttle (a wrong password reveals nothing).
    request_email_change(&mut w.c, &new, ALICE_PASSWORD)
        .await
        .account_ok();
    assert_eq!(
        request_email_change(&mut w.c, &unique_email("other"), "wrong")
            .await
            .account_err(),
        AccountError::CurrentPasswordIncorrect
    );
    assert_eq!(
        request_email_change(&mut w.c, &unique_email("other"), ALICE_PASSWORD)
            .await
            .account_err(),
        AccountError::EmailChangeTooSoon
    );
}

// -------------------------------------------------------------------------- AC24

#[sqlx::test(migrations = "../migrations")]
async fn ac24_taken_addresses_are_indistinguishable_from_free_ones(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut alice = alice_world(&pool).await;
    let bob_email = unique_email("bob");
    let bob_id = create_bob(&pool, &bob_email).await;
    let dave_email = unique_email("dave");
    create_user(&pool, "Dave", &dave_email).await;
    let erin_email = unique_email("erin");
    create_user(&pool, "Erin", &erin_email).await;
    let carol_email = unique_email("carol");
    let carol_id = create_user(&pool, "Carol", &carol_email).await;
    let x = unique_email("x");
    set_pending_email(&pool, carol_id, &x, &tok('c'), DAY - 3600).await;
    let free = unique_email("free");
    let bob_before = user_row(&pool, bob_id).await;
    let carol_before = user_row(&pool, carol_id).await;

    let mut dave = Client::new(&alice.app);
    dave.login(&dave_email, TEST_PASSWORD).await.ok();
    let mut erin = Client::new(&alice.app);
    erin.login(&erin_email, TEST_PASSWORD).await.ok();

    // The same call, once for a free address and once for Bob's: identical answers.
    let for_free = request_email_change(&mut dave, &free, TEST_PASSWORD).await;
    let for_bob = request_email_change(&mut alice.c, &bob_email, ALICE_PASSWORD).await;
    for_free.account_ok();
    for_bob.account_ok();
    assert_eq!(for_free.status, for_bob.status);
    assert_eq!(for_free.body, for_bob.body, "byte-identical response");

    // Pending set, mail sent to Bob's address (his mailbox proves control, not the app).
    let row = user_row(&pool, alice.id).await;
    assert_eq!(row.pending_email.as_deref(), Some(bob_email.as_str()));
    assert!(row.email_verification_token.is_some());
    mailpit.wait_for_count(&bob_email, 1).await;
    mailpit.wait_for_count(&free, 1).await;
    assert_eq!(
        user_row(&pool, bob_id).await,
        bob_before,
        "Bob's row is untouched"
    );

    // Another user's pending address is no obstacle either.
    request_email_change(&mut erin, &x, TEST_PASSWORD)
        .await
        .account_ok();
    mailpit.wait_for_count(&x, 1).await;
    assert_eq!(
        user_row(&pool, carol_id).await,
        carol_before,
        "Carol keeps her pending change"
    );
}

// -------------------------------------------------------------------------- AC25

#[sqlx::test(migrations = "../migrations")]
async fn ac25_a_second_request_within_a_minute_is_throttled(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let first = unique_email("alice-first");
    let second = unique_email("alice-second");

    request_email_change(&mut w.c, &first, ALICE_PASSWORD)
        .await
        .account_ok();
    let token = token_from_mail(&mailpit, &first).await;
    let pending = user_row(&pool, w.id).await;

    assert_eq!(
        request_email_change(&mut w.c, &second, ALICE_PASSWORD)
            .await
            .account_err(),
        AccountError::EmailChangeTooSoon
    );
    assert_eq!(
        user_row(&pool, w.id).await,
        pending,
        "the pending change is not modified"
    );
    assert_eq!(mails_after_settle(&mailpit, &second).await, 0);

    // Still too soon 30 s in, and the first link still works.
    set_pending_expiry(&pool, w.id, DAY - 30).await;
    assert_eq!(
        request_email_change(&mut w.c, &second, ALICE_PASSWORD)
            .await
            .account_err(),
        AccountError::EmailChangeTooSoon
    );
    let mut anon = Client::new(&w.app);
    confirm_email_change(&mut anon, &token).await.account_ok();
    assert_eq!(user_row(&pool, w.id).await.email, first);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac25_after_a_minute_a_new_request_replaces_the_old_one(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let first = unique_email("alice-first");
    let second = unique_email("alice-other");

    request_email_change(&mut w.c, &first, ALICE_PASSWORD)
        .await
        .account_ok();
    let first_token = token_from_mail(&mailpit, &first).await;
    set_pending_expiry(&pool, w.id, DAY - 61).await;

    request_email_change(&mut w.c, &second, ALICE_PASSWORD)
        .await
        .account_ok();
    let second_token = token_from_mail(&mailpit, &second).await;
    assert_eq!(
        user_row(&pool, w.id).await.pending_email.as_deref(),
        Some(second.as_str())
    );

    let mut anon = Client::new(&w.app);
    assert_eq!(
        confirm_email_change(&mut anon, &first_token)
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken,
        "the first link is dead"
    );
    confirm_email_change(&mut anon, &second_token)
        .await
        .account_ok();
    assert_eq!(user_row(&pool, w.id).await.email, second);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac25_an_expired_pending_change_does_not_throttle(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let new = unique_email("alice-new");
    set_pending_email(&pool, w.id, "stale@mail.test", &tok('s'), -10).await;

    request_email_change(&mut w.c, &new, ALICE_PASSWORD)
        .await
        .account_ok();
    mailpit.wait_for_count(&new, 1).await;
    assert_eq!(
        user_row(&pool, w.id).await.pending_email.as_deref(),
        Some(new.as_str())
    );
}

// -------------------------------------------------------------------------- AC26

#[sqlx::test(migrations = "../migrations")]
async fn ac26_a_failing_mail_transport_restores_the_pending_columns(pool: MySqlPool) {
    let port = closed_port();
    let vars = [
        ("MAIL_HOST", "127.0.0.1"),
        ("MAIL_PORT", port.as_str()),
        ("MAIL_ENCRYPTION", "none"),
    ];

    // Nothing pending before: nothing pending after.
    let mut w = alice_world_with(&pool, &vars, Opts::default()).await;
    let before = user_row(&pool, w.id).await;
    assert_eq!(
        request_email_change(&mut w.c, &unique_email("new"), ALICE_PASSWORD)
            .await
            .account_err(),
        AccountError::MailSendFailed
    );
    assert_eq!(user_row(&pool, w.id).await, before);

    // An earlier (old enough) pending change comes back exactly as it was.
    set_pending_email(&pool, w.id, "prior@mail.test", &tok('p'), DAY - 3600).await;
    let prior = user_row(&pool, w.id).await;
    assert_eq!(
        request_email_change(&mut w.c, &unique_email("new"), ALICE_PASSWORD)
            .await
            .account_err(),
        AccountError::MailSendFailed
    );
    assert_eq!(
        user_row(&pool, w.id).await,
        prior,
        "token hash and expiry restored too"
    );
}

// -------------------------------------------------------------------------- AC27

#[sqlx::test(migrations = "../migrations")]
async fn ac27_the_mail_is_escaped_and_in_the_account_language(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    // The page is English (no locale yet); the account is German from now on.
    sqlx::query("UPDATE users SET name = ? WHERE id = ?")
        .bind("<script>x</script>")
        .bind(w.id)
        .execute(&pool)
        .await
        .unwrap();
    set_user_locale(&pool, w.id, Some("de")).await;
    let new = unique_email("alice-new");

    request_email_change(&mut w.c, &new, ALICE_PASSWORD)
        .await
        .account_ok();

    let mail = mailpit.wait_for_latest(&new).await;
    assert_ne!(
        de("email.verifyEmail.subject"),
        en("email.verifyEmail.subject")
    );
    assert_eq!(mail.subject, de("email.verifyEmail.subject"));
    assert!(mail.html.contains("&lt;script&gt;"), "the name is escaped");
    assert!(
        !mail.html.contains("<script"),
        "no script element in the mail"
    );
    assert!(mail.html.contains("lang=\"de\""));
    assert!(!mail.text.trim().is_empty(), "plain-text part");
    assert!(mail.first_url_with_path(VERIFY).is_some());
}

// -------------------------------------------------------------------------- AC28

#[sqlx::test(migrations = "../migrations")]
async fn ac28_cancel_clears_the_pending_change_and_kills_the_link(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "new@example.com", &tok('v'), 3600).await;
    set_reset_token_for(&pool, w.id).await;
    let before = user_row(&pool, w.id).await;

    cancel_email_change(&mut w.c).await.account_ok();

    let after = user_row(&pool, w.id).await;
    assert_eq!(after.pending_email, None);
    assert_eq!(after.email_verification_token, None);
    assert_eq!(after.email_verification_expires, None);
    assert_eq!(
        app::test_support::UserRow {
            pending_email: None,
            email_verification_token: None,
            email_verification_expires: None,
            ..before
        },
        after,
        "only the three pending columns changed"
    );

    let page = w.c.get(PROFILE).await;
    assert!(!page.body.contains("new@example.com"), "no pending box");
    let mut anon = Client::new(&w.app);
    assert_eq!(
        confirm_email_change(&mut anon, &tok('v'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(user_row(&pool, w.id).await.email, w.email);

    // Idempotent.
    cancel_email_change(&mut w.c).await.account_ok();
    assert_eq!(user_row(&pool, w.id).await, after);
}

async fn set_reset_token_for(pool: &MySqlPool, id: i32) {
    app::test_support::set_reset_token(pool, id, &tok('r'), 1800).await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac28_cancel_with_nothing_pending_succeeds(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let before = user_row(&pool, w.id).await;
    cancel_email_change(&mut w.c).await.account_ok();
    cancel_email_change(&mut w.c).await.account_ok();
    assert_eq!(user_row(&pool, w.id).await, before);
}

// -------------------------------------------------------------------------- AC38

#[sqlx::test(migrations = "../migrations")]
async fn ac38_both_mails_use_the_account_language_of_the_token_owner(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    set_user_locale(&pool, w.id, Some("de")).await;
    let new = unique_email("alice-new");
    assert_eq!(
        lang_of(&w.c.get(PROFILE).await.body),
        "en",
        "Alice is on an English page"
    );

    request_email_change(&mut w.c, &new, ALICE_PASSWORD)
        .await
        .account_ok();
    let verify = mailpit.wait_for_latest(&new).await;
    assert_ne!(
        de("email.verifyEmail.subject"),
        en("email.verifyEmail.subject")
    );
    assert_eq!(verify.subject, de("email.verifyEmail.subject"));
    assert!(verify.html.contains("lang=\"de\""));
    let token = token_in(&verify.first_url_with_path(VERIFY).unwrap(), VERIFY);

    // The link is confirmed from an English browser without a session.
    let mut english = Client::new(&w.app).with_cookie("locale", "en");
    let result = confirm_email_change(&mut english, &token)
        .await
        .account_result::<ConfirmResult>()
        .expect("confirmed");
    assert!(!result.logged_in);

    assert_ne!(
        de("email.emailChanged.subject"),
        en("email.emailChanged.subject")
    );
    let notice = mailpit.wait_for_latest(&w.email).await;
    assert_eq!(notice.subject, de("email.emailChanged.subject"));
    assert!(notice.html.contains("lang=\"de\""));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac38_without_an_account_language_the_default_locale_is_used(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let bob_email = unique_email("bob");
    let bob_id = create_bob(&pool, &bob_email).await;
    assert_eq!(user_row(&pool, bob_id).await.locale, None);
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app).with_cookie("locale", "de");
    c.login(&bob_email, BOB_PASSWORD).await.ok();
    assert_eq!(
        lang_of(&c.get(PROFILE).await.body),
        "de",
        "Bob is on a German page"
    );
    let new = unique_email("bob-new");

    request_email_change(&mut c, &new, BOB_PASSWORD)
        .await
        .account_ok();

    let mail = mailpit.wait_for_latest(&new).await;
    assert_eq!(
        mail.subject,
        en("email.verifyEmail.subject"),
        "APP_LOCALE=en"
    );
    assert!(mail.html.contains("lang=\"en\""));
}
