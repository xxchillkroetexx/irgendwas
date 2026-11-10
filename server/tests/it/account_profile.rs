//! account slice: name and password changes (AC10-AC19, AC37).

use crate::common::account::*;
use crate::common::auth::*;
use app::account_api::{AccountError, PasswordChanged};
use app::auth_api::AuthError;
use app::test_support::{ALICE_PASSWORD, set_pending_email, set_reset_token, user_row, user_rows};
use sqlx::MySqlPool;

const PROFILE: &str = "/user/profile";
const NEW: &str = "newpassword1";

// ----------------------------------------------------------------- name (AC10-13, AC37)

#[sqlx::test(migrations = "../migrations")]
async fn ac10_update_name_trims_and_the_current_user_shows_it(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    update_name(&mut w.c, "  Alice B  ").await.account_ok();
    assert_eq!(user_row(&pool, w.id).await.name, "Alice B");
    // The layout's current-user resource (nav dropdown) refetches this.
    let info = w.c.session_info().await;
    assert_eq!(info.user.expect("logged in").name, "Alice B");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_blank_name_is_required(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    for blank in ["   ", "", "\t \n"] {
        assert_eq!(
            update_name(&mut w.c, blank).await.account_err(),
            AccountError::NameRequired,
            "{blank:?}"
        );
    }
    assert_eq!(user_row(&pool, w.id).await.name, "Alice");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_name_length_is_counted_in_characters(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;

    assert_eq!(
        update_name(&mut w.c, &"x".repeat(101)).await.account_err(),
        AccountError::NameTooLong
    );
    assert_eq!(
        update_name(&mut w.c, &"ä".repeat(101)).await.account_err(),
        AccountError::NameTooLong
    );
    assert_eq!(user_row(&pool, w.id).await.name, "Alice", "unchanged");

    let hundred = "ä".repeat(100);
    update_name(&mut w.c, &hundred).await.account_ok();
    assert_eq!(user_row(&pool, w.id).await.name, hundred);

    // The limit applies after trimming.
    let padded = format!("  {}  ", "b".repeat(100));
    update_name(&mut w.c, &padded).await.account_ok();
    assert_eq!(user_row(&pool, w.id).await.name, "b".repeat(100));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_update_name_touches_only_the_name(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "new@example.com", &tok('p'), 3600).await;
    set_reset_token(&pool, w.id, &tok('r'), 1800).await;
    let before = user_row(&pool, w.id).await;

    update_name(&mut w.c, "Bob").await.account_ok();

    let after = user_row(&pool, w.id).await;
    assert_eq!(after.name, "Bob");
    assert_eq!(
        app::test_support::UserRow {
            name: "Bob".into(),
            ..before
        },
        after,
        "email, password, pending columns, reset token and epoch are unchanged"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac37_control_characters_in_the_name_are_rejected(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    for bad in ["A\u{7}B", "A\nB", "A\tB", "A\u{0}B", "A\u{7f}B"] {
        assert_eq!(
            update_name(&mut w.c, bad).await.account_err(),
            AccountError::NameInvalid,
            "{bad:?}"
        );
    }
    assert_eq!(user_row(&pool, w.id).await.name, "Alice");
}

// -------------------------------------------------------------- password (AC14-AC19)

#[sqlx::test(migrations = "../migrations")]
async fn ac14_change_password_replaces_the_argon2id_hash(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let before = user_row(&pool, w.id).await;

    change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
        .await
        .account_ok();

    let after = user_row(&pool, w.id).await;
    assert!(
        after.password.starts_with("$argon2id$"),
        "{}",
        after.password
    );
    assert_ne!(after.password, before.password);
    assert!(!after.password.contains(NEW), "no plaintext");

    let mut fresh = w.c.fresh();
    assert_eq!(
        fresh.login(&w.email, ALICE_PASSWORD).await.err(),
        AuthError::InvalidCredentials,
        "the old password no longer works"
    );
    fresh.login(&w.email, NEW).await.ok();
    assert_eq!(fresh.whoami().await.as_deref(), Some(w.email.as_str()));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_the_new_password_is_not_trimmed_and_may_equal_the_old_one(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;

    change_password(&mut w.c, ALICE_PASSWORD, ALICE_PASSWORD, ALICE_PASSWORD)
        .await
        .account_ok();
    let spaced = "  spaced password  ";
    change_password(&mut w.c, ALICE_PASSWORD, spaced, spaced)
        .await
        .account_ok();

    let mut fresh = w.c.fresh();
    assert_eq!(
        fresh.login(&w.email, spaced.trim()).await.err(),
        AuthError::InvalidCredentials,
        "the trimmed form is a different password"
    );
    fresh.login(&w.email, spaced).await.ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_wrong_current_password_changes_nothing(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let before = user_rows(&pool).await;

    for wrong in ["wrong", "", ALICE_PASSWORD.to_uppercase().as_str()] {
        assert_eq!(
            change_password(&mut w.c, wrong, NEW, NEW)
                .await
                .account_err(),
            AccountError::CurrentPasswordIncorrect,
            "{wrong:?}"
        );
    }
    assert_eq!(
        user_rows(&pool).await,
        before,
        "password and session_epoch unchanged"
    );
    assert_eq!(
        w.c.whoami().await.as_deref(),
        Some(w.email.as_str()),
        "still logged in"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac16_password_length_limits(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let before = user_rows(&pool).await;

    assert_eq!(
        change_password(&mut w.c, ALICE_PASSWORD, "short7c", "short7c")
            .await
            .account_err(),
        AccountError::PasswordTooShort
    );
    // 7 characters of 2 bytes each: counted in characters, not bytes.
    assert_eq!(
        change_password(&mut w.c, ALICE_PASSWORD, "äääääää", "äääääää")
            .await
            .account_err(),
        AccountError::PasswordTooShort
    );
    let long = "p".repeat(257);
    assert_eq!(
        change_password(&mut w.c, ALICE_PASSWORD, &long, &long)
            .await
            .account_err(),
        AccountError::PasswordTooLong
    );
    assert_eq!(user_rows(&pool).await, before, "nothing changed");

    // The boundaries themselves are fine: 8 and 256 characters.
    let eight = "ä".repeat(8);
    change_password(&mut w.c, ALICE_PASSWORD, &eight, &eight)
        .await
        .account_ok();
    let max = "q".repeat(256);
    change_password(&mut w.c, &eight, &max, &max)
        .await
        .account_ok();
    let mut fresh = w.c.fresh();
    fresh.login(&w.email, &max).await.ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_confirmation_must_match_exactly(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let before = user_rows(&pool).await;
    for confirm in ["newpassword2", "newpassword1 ", "NEWPASSWORD1", ""] {
        assert_eq!(
            change_password(&mut w.c, ALICE_PASSWORD, NEW, confirm)
                .await
                .account_err(),
            AccountError::PasswordsDontMatch,
            "{confirm:?}"
        );
    }
    assert_eq!(user_rows(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac16_ac17_checks_run_in_the_documented_order(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    // Length first, then the confirmation, then the current password.
    assert_eq!(
        change_password(&mut w.c, "wrong", "short", "different")
            .await
            .account_err(),
        AccountError::PasswordTooShort
    );
    assert_eq!(
        change_password(&mut w.c, "wrong", NEW, "different")
            .await
            .account_err(),
        AccountError::PasswordsDontMatch
    );
    assert_eq!(
        change_password(&mut w.c, "wrong", NEW, NEW)
            .await
            .account_err(),
        AccountError::CurrentPasswordIncorrect
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac18_changing_the_password_logs_out_every_other_session(pool: MySqlPool) {
    let w = alice_world_with(&pool, &[], Opts::default()).await;
    let mut a = w.c;
    let mut b = Client::new(&w.app);
    b.login(&w.email, ALICE_PASSWORD).await.ok();
    assert_eq!(b.get(PROFILE).await.status, 200, "B is logged in");
    let old_cookie = a.session_cookie().expect("A has a session").to_string();

    change_password(&mut a, ALICE_PASSWORD, NEW, NEW)
        .await
        .account_ok();

    // A keeps working, under a new session id.
    assert_eq!(a.get(PROFILE).await.status, 200);
    assert_eq!(a.whoami().await.as_deref(), Some(w.email.as_str()));
    let new_cookie = a.session_cookie().expect("A still has a session");
    assert_ne!(new_cookie, old_cookie, "the session id was cycled");
    let mut replay = Client::new(&w.app).with_cookie(server::SESSION_COOKIE, &old_cookie);
    assert_eq!(
        get_profile(&mut replay).await.account_err(),
        AccountError::Unauthorized,
        "the old session id is dead"
    );

    // B is out: server fns say Unauthorized, pages redirect to login.
    let mut b_fns = b.fork();
    assert_eq!(
        get_profile(&mut b_fns).await.account_err(),
        AccountError::Unauthorized
    );
    assert_eq!(
        update_name(&mut b_fns, "Hijack").await.account_err(),
        AccountError::Unauthorized
    );
    let res = b.get(PROFILE).await;
    assert!(res.status.is_redirection(), "got {}", res.status);
    assert!(res.location().unwrap().starts_with("/auth/login?next="));
    assert_eq!(user_row(&pool, w.id).await.name, "Alice");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_change_password_clears_reset_tokens_and_bumps_the_epoch(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_reset_token(&pool, w.id, &tok('r'), 1800).await;
    let before = user_row(&pool, w.id).await;
    assert!(before.reset_token.is_some() && before.reset_token_expires.is_some());

    change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
        .await
        .account_ok();

    let after = user_row(&pool, w.id).await;
    assert_eq!(after.reset_token, None);
    assert_eq!(after.reset_token_expires, None);
    assert_eq!(after.session_epoch, before.session_epoch + 1);

    // The mailed reset link is dead.
    let mut anon = w.c.fresh();
    assert_eq!(
        anon.call("check_reset_token", &[("token", &tok('r'))])
            .await
            .err(),
        AuthError::InvalidResetToken
    );
    assert_eq!(
        anon.reset(&tok('r'), "hijacked-pass-1", "hijacked-pass-1")
            .await
            .err(),
        AuthError::InvalidResetToken
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_every_successful_change_bumps_the_epoch_by_one(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
        .await
        .account_ok();
    change_password(&mut w.c, NEW, "another-pass-2", "another-pass-2")
        .await
        .account_ok();
    assert_eq!(user_row(&pool, w.id).await.session_epoch, 2);
    assert_eq!(
        w.c.whoami().await.as_deref(),
        Some(w.email.as_str()),
        "caller stays in"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn password_change_invalidates_pending_email_verification(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "new@example.com", &tok('v'), 3600).await;

    change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
        .await
        .account_ok();
    let after = user_row(&pool, w.id).await;
    assert!(after.pending_email.is_none());
    assert!(after.email_verification_token.is_none());
    assert!(after.email_verification_expires.is_none());
    assert_eq!(
        confirm_email_change(&mut w.c.fresh(), &tok('v'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(user_row(&pool, w.id).await.email, w.email);
}

#[sqlx::test(migrations = "../migrations")]
async fn password_change_reports_only_a_live_cancelled_email_change(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let first: PasswordChanged = change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
        .await
        .account_result()
        .unwrap();
    assert!(!first.pending_email_cancelled);
    set_pending_email(&pool, w.id, "expired@example.com", &tok('e'), -60).await;
    let second: PasswordChanged = change_password(&mut w.c, NEW, "newpassword2", "newpassword2")
        .await
        .account_result()
        .unwrap();
    assert!(!second.pending_email_cancelled);
    set_pending_email(&pool, w.id, "pending@example.com", &tok('p'), 3600).await;
    let third: PasswordChanged =
        change_password(&mut w.c, "newpassword2", "newpassword3", "newpassword3")
            .await
            .account_result()
            .unwrap();
    assert!(third.pending_email_cancelled);
    assert!(user_row(&pool, w.id).await.pending_email.is_none());
}

#[sqlx::test(migrations = "../migrations")]
async fn password_reset_invalidates_pending_email_verification(pool: MySqlPool) {
    let w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "new@example.com", &tok('v'), 3600).await;
    set_reset_token(&pool, w.id, &tok('r'), 1800).await;

    let mut anon = w.c.fresh();
    anon.reset(&tok('r'), NEW, NEW).await.ok();
    let after = user_row(&pool, w.id).await;
    assert!(after.pending_email.is_none());
    assert!(after.email_verification_token.is_none());
    assert!(after.email_verification_expires.is_none());
    assert_eq!(
        confirm_email_change(&mut anon, &tok('v'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(user_row(&pool, w.id).await.email, w.email);
}

#[sqlx::test(migrations = "../migrations")]
async fn verified_email_change_cannot_outlive_a_password_reset(pool: MySqlPool) {
    use std::time::Duration;

    let w = alice_world_with(&pool, &[], Opts::default()).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE id = ? FOR UPDATE")
        .bind(w.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let mut client = w.c.fork();
    let request = tokio::spawn(async move {
        request_email_change(&mut client, "new@example.com", ALICE_PASSWORD).await
    });

    // Wait for the SELECT FOR UPDATE, which follows verification of the old password.
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.PROCESSLIST \
                 WHERE DB = DATABASE() AND INFO LIKE 'SELECT pending_email%FOR UPDATE'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if count > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("email change never reached the locked row");
    // Simulate the atomic row update used by both password change and reset.
    sqlx::query(
        "UPDATE users SET session_epoch = session_epoch + 1, pending_email = NULL, \
        email_verification_token = NULL, email_verification_expires = NULL WHERE id = ?",
    )
    .bind(w.id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(
        request.await.unwrap().account_err(),
        AccountError::CurrentPasswordIncorrect
    );
    let after = user_row(&pool, w.id).await;
    assert!(after.pending_email.is_none());
    assert!(after.email_verification_token.is_none());
    assert!(after.email_verification_expires.is_none());
}
