//! account slice: security properties that cut across the ACs (lookalike addresses, tokens
//! in logs, single use under concurrency, POST-only) and the i18n contract (error keys,
//! catalog entries).

use crate::common::account::*;
use crate::common::auth::*;
use app::account_api::{AccountError, ConfirmResult};
use app::auth_api::AuthError;
use app::i18n::{Locale, catalog};
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, create_bob, set_pending_email, user_row, user_rows,
};
use sqlx::MySqlPool;

// ------------------------------------------------------ lookalike and variant addresses

#[sqlx::test(migrations = "../migrations")]
async fn a_case_variant_of_another_account_is_that_account(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let bob_email = unique_email("bob");
    let bob_id = create_bob(&pool, &bob_email).await;
    let bob_before = user_row(&pool, bob_id).await;

    // Normalised before storing: the "different" spelling is Bob's address.
    request_email_change(
        &mut w.c,
        &format!(" {} ", bob_email.to_uppercase()),
        ALICE_PASSWORD,
    )
    .await
    .account_ok();
    assert_eq!(
        user_row(&pool, w.id).await.pending_email.as_deref(),
        Some(bob_email.as_str())
    );
    let token = token_from_mail(&mailpit, &bob_email).await;

    let mut anon = Client::new(&w.app);
    assert_eq!(
        confirm_email_change(&mut anon, &token).await.account_err(),
        AccountError::EmailTaken
    );
    assert_eq!(user_row(&pool, w.id).await.email, w.email);
    assert_eq!(user_row(&pool, bob_id).await, bob_before);
}

#[sqlx::test(migrations = "../migrations")]
async fn an_accent_variant_of_another_account_is_a_different_address(pool: MySqlPool) {
    let w = alice_world(&pool).await;
    let bob_email = unique_email("bob");
    let bob_id = create_bob(&pool, &bob_email).await;
    let bob_before = user_row(&pool, bob_id).await;
    // utf8mb4_unicode_ci would treat 'ó' and 'o' as equal; the stored addresses are
    // compared byte for byte.
    let lookalike = bob_email.replacen("bob", "bób", 1);
    assert_ne!(lookalike, bob_email);
    set_pending_email(&pool, w.id, &lookalike, &tok('v'), 3600).await;
    let mut anon = Client::new(&w.app);

    confirm_email_change(&mut anon, &tok('v'))
        .await
        .account_ok();

    assert_eq!(
        user_row(&pool, w.id).await.email,
        lookalike,
        "Alice got the lookalike"
    );
    assert_eq!(
        user_row(&pool, bob_id).await,
        bob_before,
        "Bob is untouched"
    );

    // Each address logs into its own account only.
    let mut as_bob = Client::new(&w.app);
    as_bob.login(&bob_email, BOB_PASSWORD).await.ok();
    assert_eq!(as_bob.whoami().await.as_deref(), Some(bob_email.as_str()));
    let mut as_alice = Client::new(&w.app);
    as_alice.login(&lookalike, ALICE_PASSWORD).await.ok();
    assert_eq!(as_alice.whoami().await.as_deref(), Some(lookalike.as_str()));
    let mut mixed = Client::new(&w.app);
    assert_eq!(
        mixed.login(&bob_email, ALICE_PASSWORD).await.err(),
        AuthError::InvalidCredentials
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn an_existing_lookalike_account_does_not_block_the_plain_address(pool: MySqlPool) {
    let w = alice_world(&pool).await;
    let bob_email = unique_email("bob");
    let lookalike = bob_email.replacen("bob", "bób", 1);
    let carol = app::test_support::create_user(&pool, "Carol", &lookalike).await;
    let carol_before = user_row(&pool, carol).await;
    set_pending_email(&pool, w.id, &bob_email, &tok('v'), 3600).await;
    let mut anon = Client::new(&w.app);

    confirm_email_change(&mut anon, &tok('v'))
        .await
        .account_ok();

    assert_eq!(user_row(&pool, w.id).await.email, bob_email);
    assert_eq!(user_row(&pool, carol).await, carol_before);
}

// ---------------------------------------------------------------------- logging

#[sqlx::test(migrations = "../migrations")]
async fn tokens_links_and_addresses_never_reach_the_logs(pool: MySqlPool) {
    let (logs, _guard) = capture_logs();
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let new = unique_email("alice-new");

    request_email_change(&mut w.c, &new, ALICE_PASSWORD)
        .await
        .account_ok();
    let token = token_from_mail(&mailpit, &new).await;
    // Failing calls log too: wrong password, bad token.
    request_email_change(&mut w.c, &new, "wrong")
        .await
        .account_err();
    let mut anon = Client::new(&w.app);
    confirm_email_change(&mut anon, "not-a-token")
        .await
        .account_err();
    confirm_email_change(&mut anon, &token).await.account_ok();
    mailpit.wait_for_count(&w.email, 1).await;
    settle().await;

    let log = logs.contents();
    assert!(!log.is_empty(), "the capture works");
    assert!(!log.contains(&token), "raw token in the log");
    assert!(
        !log.contains(&sha256_hex(&pool, &token).await),
        "token hash in the log"
    );
    assert!(!log.contains(VERIFY), "verification link in the log");
    assert!(!log.contains(&new), "new address in the log");
    assert!(!log.contains(&w.email), "old address in the log");
    assert!(!log.contains(ALICE_PASSWORD), "password in the log");
}

// ----------------------------------------------------------------- single use

#[sqlx::test(migrations = "../migrations")]
async fn concurrent_confirms_apply_the_change_exactly_once(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let w = alice_world(&pool).await;
    let new = unique_email("alice-new");
    set_pending_email(&pool, w.id, &new, &tok('v'), 3600).await;
    let mut one = Client::new(&w.app);
    let mut two = Client::new(&w.app);
    let mut three = Client::new(&w.app);

    let token = tok('v');
    let (a, b, c) = tokio::join!(
        confirm_email_change(&mut one, &token),
        confirm_email_change(&mut two, &token),
        confirm_email_change(&mut three, &token),
    );

    let results: Vec<Result<ConfirmResult, AccountError>> = [a, b, c]
        .iter()
        .map(|r| r.account_result::<ConfirmResult>())
        .collect();
    let wins = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(wins, 1, "exactly one confirm wins: {results:?}");
    for failed in results.iter().filter_map(|r| r.as_ref().err()) {
        assert_eq!(*failed, AccountError::InvalidOrExpiredToken);
    }
    assert_eq!(user_row(&pool, w.id).await.email, new);
    assert_eq!(
        mails_after_settle(&mailpit, &w.email).await,
        1,
        "one notice only"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn confirming_does_not_log_anyone_in(pool: MySqlPool) {
    let w = alice_world(&pool).await;
    let new = unique_email("alice-new");
    set_pending_email(&pool, w.id, &new, &tok('v'), 3600).await;
    let mut anon = Client::new(&w.app);

    confirm_email_change(&mut anon, &tok('v'))
        .await
        .account_ok();

    assert_eq!(
        anon.whoami().await,
        None,
        "the token proves the mailbox, not a login"
    );
}

// --------------------------------------------------------------------- POST only

#[sqlx::test(migrations = "../migrations")]
async fn mutations_cannot_be_triggered_by_get(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "pending@mail.test", &tok('v'), 3600).await;
    let before = user_rows(&pool).await;

    for uri in [
        "/api/update_name?name=Hacked",
        "/api/cancel_email_change",
        "/api/request_email_change?new_email=evil%40mail.test&current_password=x",
        "/api/change_password?current_password=a&new_password=b&new_password_confirm=b",
        &format!("/api/confirm_email_change?token={}", tok('v')),
    ] {
        let res = w.c.get(uri).await;
        assert!(
            !res.status.is_success(),
            "GET {uri} answered {}",
            res.status
        );
    }
    assert_eq!(user_rows(&pool).await, before);
}

// ----------------------------------------------------------------------- i18n

#[test]
fn every_error_maps_to_its_spec_key() {
    let table = [
        (AccountError::NameRequired, "flash.error.name_required"),
        (AccountError::NameInvalid, "flash.error.name_invalid"),
        (AccountError::NameTooLong, "flash.error.name_too_long"),
        (
            AccountError::PasswordTooShort,
            "flash.error.password_too_short",
        ),
        (
            AccountError::PasswordTooLong,
            "validation.password_too_long",
        ),
        (
            AccountError::PasswordsDontMatch,
            "flash.error.passwords_dont_match",
        ),
        (
            AccountError::CurrentPasswordIncorrect,
            "flash.error.current_password_incorrect",
        ),
        (AccountError::InvalidEmail, "flash.error.invalid_email"),
        (AccountError::EmailUnchanged, "flash.error.email_unchanged"),
        (
            AccountError::EmailChangeTooSoon,
            "flash.error.email_change_too_soon",
        ),
        (AccountError::MailSendFailed, "flash.error.mail_send_failed"),
        (
            AccountError::InvalidOrExpiredToken,
            "flash.error.email_verification_failed",
        ),
        (AccountError::EmailTaken, "flash.error.email_already_in_use"),
        (AccountError::Internal, "error.generic.message"),
    ];
    for (error, key) in table {
        assert_eq!(error.message_key(), key, "{error:?}");
    }
}

/// Keys the account slice uses (spec: "i18n keys"), reused and added.
const ACCOUNT_KEYS: &[&str] = &[
    "nav.dashboard",
    "nav.profile",
    "user.dashboard.welcome",
    "user.dashboard.subtitle",
    "user.dashboard.tips.title",
    "user.dashboard.tips.createWishlist",
    "user.dashboard.tips.inviteFriends",
    "user.dashboard.tips.setExclusions",
    "user.profile.name",
    "user.profile.email",
    "user.profile.emailChangeNotice",
    "user.profile.emailPendingVerification",
    "user.profile.emailPendingExpires",
    "user.profile.updateProfile",
    "user.profile.backToDashboard",
    "user.profile.changeEmail.title",
    "user.profile.changeEmail.new",
    "user.profile.changeEmail.currentPassword",
    "user.profile.changeEmail.submit",
    "user.profile.changeEmail.cancel",
    "user.profile.changePassword.title",
    "user.profile.changePassword.current",
    "user.profile.changePassword.new",
    "user.profile.changePassword.confirm",
    "user.profile.changePassword.submit",
    "user.profile.changePassword.otherSessionsNotice",
    "user.verifyEmail.title",
    "user.verifyEmail.description",
    "user.verifyEmail.submit",
    "user.verifyEmail.goToProfile",
    "user.verifyEmail.goToLogin",
    "flash.success.profile_updated",
    "flash.success.email_verification_sent",
    "flash.success.email_verified",
    "flash.success.password_changed",
    "flash.success.email_change_cancelled",
    "flash.error.name_required",
    "flash.error.name_too_long",
    "flash.error.name_invalid",
    "flash.error.invalid_email",
    "flash.error.email_unchanged",
    "flash.error.current_password_incorrect",
    "flash.error.email_change_too_soon",
    "flash.error.mail_send_failed",
    "flash.error.email_verification_failed",
    "flash.error.email_already_in_use",
    "flash.error.passwords_dont_match",
    "flash.error.password_too_short",
    "validation.password_too_long",
    "email.verifyEmail.subject",
    "email.verifyEmail.greeting",
    "email.verifyEmail.body",
    "email.verifyEmail.button",
    "email.verifyEmail.linkHint",
    "email.verifyEmail.expiry",
    "email.verifyEmail.ignore",
    "email.emailChanged.subject",
    "email.emailChanged.body",
];

/// Placeholders the spec's templates need.
const PLACEHOLDERS: &[(&str, &str)] = &[
    ("user.dashboard.welcome", "{name}"),
    ("user.profile.emailPendingVerification", "{email}"),
    ("user.profile.emailPendingExpires", "{date}"),
    ("email.verifyEmail.greeting", "{name}"),
    ("email.emailChanged.body", "{email}"),
];

#[test]
fn the_account_keys_exist_in_both_catalogs() {
    for locale in [Locale::En, Locale::De] {
        let cat = catalog(locale);
        for key in ACCOUNT_KEYS {
            let text = cat.get(*key);
            assert!(
                text.is_some_and(|t| !t.trim().is_empty()),
                "{locale}: key {key} is missing"
            );
        }
        for (key, placeholder) in PLACEHOLDERS {
            assert!(
                cat.get(*key).is_some_and(|t| t.contains(placeholder)),
                "{locale}: {key} must contain {placeholder}"
            );
        }
    }
}

#[test]
fn keys_the_spec_drops_are_not_ported() {
    for locale in [Locale::En, Locale::De] {
        let cat = catalog(locale);
        for key in cat.keys() {
            assert!(
                !key.starts_with("user.dashboard.quickActions."),
                "{locale}: {key} is unused"
            );
        }
        for key in ["user.profile.emailNotice", "user.dashboard.groups.viewAll"] {
            assert!(!cat.contains_key(key), "{locale}: {key} must not be ported");
        }
    }
}
