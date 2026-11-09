//! Issue #71: races between credential changes (password reset/change, email change) and
//! the flows that read credentials first and write later (login rehash, email-change
//! request, reset request, registration).
//!
//! Interleavings are made deterministic like the other `*_concurrency` tests: the test
//! holds `SELECT ... FOR UPDATE` on the user's row, starts the call, checks that it waits
//! at its write, changes the row the way the concurrent flow does, commits and lets the
//! call go on. The registration race has no row to wait on (its upsert and re-read are
//! separate autocommit statements), so a trigger stands in for the concurrent upsert.

use std::time::Duration;

use crate::common::account::*;
use crate::common::auth::*;
use app::account_api::{AccountError, PasswordChanged};
use app::auth_api::AuthError;
use app::server::password;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, create_alice, set_pending_email, set_pending_expiry, set_reset_token, user_row,
};
use sqlx::{MySql, MySqlPool, Transaction};
use tokio::task::JoinHandle;

const NEW_PASSWORD: &str = "a brand new password";

async fn hold_user_row(pool: &MySqlPool, user_id: i32) -> Transaction<'static, MySql> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE id = ? FOR UPDATE")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .expect("lock the user row");
    tx
}

/// Lets the spawned call run until it blocks, and checks that it is still waiting.
async fn assert_waiting<T>(handle: &JoinHandle<T>, what: &str) {
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !handle.is_finished(),
        "{what} must wait for the user row lock"
    );
}

/// What a password reset or change writes: a new hash and the next epoch, the reset token
/// and any pending email change cleared.
async fn simulate_password_change(tx: &mut Transaction<'static, MySql>, user_id: i32) -> String {
    let hash = password::hash(NEW_PASSWORD).unwrap();
    sqlx::query(
        "UPDATE users SET password = ?, session_epoch = session_epoch + 1, \
         reset_token = NULL, reset_token_expires = NULL, pending_email = NULL, \
         email_verification_token = NULL, email_verification_expires = NULL WHERE id = ?",
    )
    .bind(&hash)
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .unwrap();
    hash
}

fn assert_no_pending_change(row: &app::test_support::UserRow) {
    assert_eq!(row.pending_email, None, "pending_email");
    assert_eq!(
        row.email_verification_token, None,
        "email_verification_token"
    );
    assert_eq!(
        row.email_verification_expires, None,
        "email_verification_expires"
    );
}

/// An argon2id hash of `password` with weaker parameters than the current ones, so a
/// successful login rehashes it.
fn outdated_hash(password: &str) -> String {
    use argon2::password_hash::PasswordHasher;
    use argon2::{Algorithm, Argon2, Params, Version};
    let weak = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(4096, 1, 1, None).unwrap(),
    );
    weak.hash_password(password.as_bytes()).unwrap().to_string()
}

// ------------------------------------- 1: a pending email change dies with the password

#[sqlx::test(migrations = "../migrations")]
async fn a_password_reset_voids_a_pending_email_change(pool: MySqlPool) {
    let w = alice_world(&pool).await;
    let attacker = unique_email("attacker");
    set_pending_email(&pool, w.id, &attacker, &tok('v'), 3600).await;
    set_reset_token(&pool, w.id, &tok('r'), 1800).await;

    let mut owner = Client::new(&w.app);
    owner
        .reset(&tok('r'), NEW_PASSWORD, NEW_PASSWORD)
        .await
        .ok();

    let row = user_row(&pool, w.id).await;
    assert_no_pending_change(&row);
    let mut anon = Client::new(&w.app);
    assert_eq!(
        confirm_email_change(&mut anon, &tok('v'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(user_row(&pool, w.id).await.email, w.email);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_password_change_voids_a_pending_email_change(pool: MySqlPool) {
    let mut w = alice_world(&pool).await;
    let attacker = unique_email("attacker");
    set_pending_email(&pool, w.id, &attacker, &tok('v'), 3600).await;

    let changed = change_password(&mut w.c, ALICE_PASSWORD, NEW_PASSWORD, NEW_PASSWORD)
        .await
        .account_result::<PasswordChanged>();
    assert_eq!(
        changed,
        Ok(PasswordChanged {
            pending_email_cancelled: true
        }),
        "the page is told that the pending change is gone"
    );

    let row = user_row(&pool, w.id).await;
    assert_no_pending_change(&row);
    let mut anon = Client::new(&w.app);
    assert_eq!(
        confirm_email_change(&mut anon, &tok('v'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(user_row(&pool, w.id).await.email, w.email);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_password_change_reports_a_cancelled_change_only_if_one_was_live(pool: MySqlPool) {
    let mut w = alice_world(&pool).await;
    // Nothing pending.
    let changed = change_password(&mut w.c, ALICE_PASSWORD, NEW_PASSWORD, NEW_PASSWORD)
        .await
        .account_result::<PasswordChanged>();
    assert_eq!(
        changed,
        Ok(PasswordChanged {
            pending_email_cancelled: false
        })
    );
    // An expired change is no change (the profile does not show it either).
    set_pending_email(&pool, w.id, &unique_email("old"), &tok('x'), 3600).await;
    set_pending_expiry(&pool, w.id, -1).await;
    let changed = change_password(&mut w.c, NEW_PASSWORD, ALICE_PASSWORD, ALICE_PASSWORD)
        .await
        .account_result::<PasswordChanged>();
    assert_eq!(
        changed,
        Ok(PasswordChanged {
            pending_email_cancelled: false
        })
    );
    assert_no_pending_change(&user_row(&pool, w.id).await);
}

#[sqlx::test(migrations = "../migrations")]
async fn without_javascript_the_password_flash_says_the_email_change_was_cancelled(
    pool: MySqlPool,
) {
    let w = alice_world(&pool).await;
    let mut plain = w.c.fork().header("accept", "text/html");
    set_pending_email(&pool, w.id, &unique_email("alice-new"), &tok('v'), 3600).await;

    let reply = change_password(&mut plain, ALICE_PASSWORD, NEW_PASSWORD, NEW_PASSWORD).await;

    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location().as_deref(), Some("/user/profile"));
    // Read the flash like the next page load does (a server fn call without `Accept: text/html`).
    let mut browser = plain.fork();
    browser.headers.clear();
    let info = browser.session_info().await;
    assert!(
        has_flash_key(
            &info,
            "flash.success.password_changed_email_change_cancelled"
        ),
        "{:?}",
        info.flash
    );
    assert!(!has_flash_key(&info, "flash.success.password_changed"));
    assert_no_pending_change(&user_row(&pool, w.id).await);
}

#[sqlx::test(migrations = "../migrations")]
async fn an_email_change_verified_before_a_reset_is_not_written_after_it(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let w = alice_world(&pool).await;
    let attacker = unique_email("attacker");
    let mut lock = hold_user_row(&pool, w.id).await;

    // The password check passes against the old hash; the write waits for the row.
    let mut tab = w.c.fork();
    let target = attacker.clone();
    let request =
        tokio::spawn(async move { request_email_change(&mut tab, &target, ALICE_PASSWORD).await });
    assert_waiting(&request, "request_email_change").await;
    simulate_password_change(&mut lock, w.id).await;
    lock.commit().await.unwrap();

    assert_eq!(
        request.await.unwrap().account_err(),
        AccountError::Unauthorized
    );
    assert_no_pending_change(&user_row(&pool, w.id).await);
    assert_eq!(
        mails_after_settle(&mailpit, &attacker).await,
        0,
        "no verification mail for a change made with the old password"
    );
}

// ------------------------------ 2: no reset link for an address the account just left

#[sqlx::test(migrations = "../migrations")]
async fn a_reset_request_racing_an_email_confirmation_mails_no_link(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let former = unique_email("alice-former");
    let new = unique_email("alice-new");
    let alice = create_alice(&pool, &former).await;
    set_pending_email(&pool, alice, &new, &tok('v'), 3600).await;
    let app = router(&pool, &[], Opts::mail());
    let mut lock = hold_user_row(&pool, alice).await;

    // The request reads the account under the former address; its token write waits.
    let mut c = Client::new(&app);
    let typed = former.clone();
    let forgot = tokio::spawn(async move {
        let reply = c.forgot(&typed).await;
        let info = c.session_info().await;
        (reply, info)
    });
    assert_waiting(&forgot, "request_password_reset").await;
    // confirm_email_change's statement: the address moves, reset tokens are cleared.
    sqlx::query(
        "UPDATE users SET email = pending_email, pending_email = NULL, \
         email_verification_token = NULL, email_verification_expires = NULL, \
         reset_token = NULL, reset_token_expires = NULL WHERE id = ?",
    )
    .bind(alice)
    .execute(&mut *lock)
    .await
    .unwrap();
    lock.commit().await.unwrap();

    // The same answer as always (no enumeration) ...
    let (reply, info) = forgot.await.unwrap();
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some("/auth/login"));
    assert!(has_flash_key(&info, "flash.success.reset_instructions"));
    // ... but no token and no mail for the former address.
    let row = user_row(&pool, alice).await;
    assert_eq!(row.email, new);
    assert_eq!(
        row.reset_token, None,
        "no reset token written after the move"
    );
    assert_eq!(row.reset_token_expires, None);
    assert_eq!(mails_after_settle(&mailpit, &former).await, 0);
    assert_eq!(mailpit.messages_for(&new).await.len(), 0);
}

// ------------------------------------ 3: a login rehash never undoes a password change

#[sqlx::test(migrations = "../migrations")]
async fn a_login_rehash_does_not_undo_a_concurrent_password_change(pool: MySqlPool) {
    let email = unique_email("alice");
    let alice = create_alice(&pool, &email).await;
    let old_hash = outdated_hash(ALICE_PASSWORD);
    assert!(
        password::needs_rehash(&old_hash),
        "fixture must be rehashed"
    );
    sqlx::query("UPDATE users SET password = ? WHERE id = ?")
        .bind(&old_hash)
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    let app = router(&pool, &[], Opts::default());
    let mut lock = hold_user_row(&pool, alice).await;

    // The old password verifies against the read hash; the rehash write waits.
    let mut racer = Client::new(&app);
    let typed = email.clone();
    let login = tokio::spawn(async move {
        let reply = racer.login(&typed, ALICE_PASSWORD).await;
        (reply, racer.whoami().await)
    });
    assert_waiting(&login, "login (rehash)").await;
    let new_hash = simulate_password_change(&mut lock, alice).await;
    lock.commit().await.unwrap();

    let (reply, who) = login.await.unwrap();
    assert_eq!(reply.err(), AuthError::InvalidCredentials);
    assert_eq!(who, None, "no session from stale credentials");
    assert_eq!(user_row(&pool, alice).await.password, new_hash);

    let mut c = Client::new(&app);
    c.login(&email, NEW_PASSWORD).await.ok();
    assert_eq!(c.whoami().await.as_deref(), Some(email.as_str()));
    let mut d = Client::new(&app);
    assert_eq!(
        d.login(&email, ALICE_PASSWORD).await.err(),
        AuthError::InvalidCredentials
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn a_login_verified_before_a_password_reset_is_refused(pool: MySqlPool) {
    let email = unique_email("alice");
    let alice = create_alice(&pool, &email).await;
    let app = router(&pool, &[], Opts::default());
    let mut lock = hold_user_row(&pool, alice).await;

    let mut racer = Client::new(&app);
    let typed = email.clone();
    let login = tokio::spawn(async move {
        let reply = racer.login(&typed, ALICE_PASSWORD).await;
        (reply, racer.whoami().await)
    });
    assert_waiting(&login, "login").await;
    simulate_password_change(&mut lock, alice).await;
    lock.commit().await.unwrap();

    let (reply, who) = login.await.unwrap();
    assert_eq!(reply.err(), AuthError::InvalidCredentials);
    assert_eq!(who, None);
}

// ------------------------------------------ 4: concurrent registrations never 500

#[sqlx::test(migrations = "../migrations")]
async fn a_registration_whose_token_was_replaced_meanwhile_answers_success_without_mail(
    pool: MySqlPool,
) {
    let mailpit = Mailpit::from_env();
    // Stands in for a concurrent request's upsert landing between this request's upsert
    // and its re-read: the stored token is not the one this request generated.
    sqlx::raw_sql(
        "CREATE TRIGGER superseded BEFORE INSERT ON pending_registrations FOR EACH ROW \
         SET NEW.token_hash = SHA2(CONCAT('other request ', NEW.token_hash), 256)",
    )
    .execute(&pool)
    .await
    .expect("create trigger");
    let email = unique_email("carol");
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);

    let reply = c.register(&email, None).await;
    assert!(
        reply.status.is_success(),
        "{}: {}",
        reply.status,
        reply.body
    );
    reply.ok();
    assert_eq!(
        mails_after_settle(&mailpit, &email).await,
        0,
        "no mail with a link that is known to be dead"
    );
}

/// Smoke test with real concurrent requests. It is probabilistic (it hit the 500 on the
/// old code, but needs no particular interleaving); the deterministic regression test is
/// the trigger-based one above.
#[sqlx::test(migrations = "../migrations")]
async fn simultaneous_registrations_for_a_new_address_both_succeed(pool: MySqlPool) {
    let app = router(
        &pool,
        &[("RATE_LIMIT_MAIL_PER_IP", "1000")],
        Opts::default(),
    );
    for round in 0..15 {
        let email = unique_email("carol");
        let mut one = Client::new(&app);
        let mut two = Client::new(&app).peer("192.0.2.2");
        let (a, b) = tokio::join!(one.register(&email, None), two.register(&email, None));
        for reply in [&a, &b] {
            assert!(
                reply.status.is_success(),
                "round {round}: {} {}",
                reply.status,
                reply.body
            );
        }
        assert_eq!(a.body, b.body, "round {round}: identical answers");
        let rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pending_registrations WHERE email = ?")
                .bind(&email)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rows, 1, "round {round}: one pending registration");
    }
}

/// Smoke test: the existing-account branch was not affected by the race; this pins that
/// both answers stay successful and identical (non-enumeration).
#[sqlx::test(migrations = "../migrations")]
async fn simultaneous_registrations_for_an_existing_address_both_succeed(pool: MySqlPool) {
    let email = unique_email("alice");
    create_alice(&pool, &email).await;
    let app = router(
        &pool,
        &[("RATE_LIMIT_MAIL_PER_IP", "1000")],
        Opts::default(),
    );
    let mut one = Client::new(&app);
    let mut two = Client::new(&app).peer("192.0.2.2");

    let (a, b) = tokio::join!(one.register(&email, None), two.register(&email, None));

    for reply in [&a, &b] {
        assert!(reply.status.is_success(), "{} {}", reply.status, reply.body);
    }
    assert_eq!(a.body, b.body, "identical answers");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_registrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0, "no pending registration for an existing account");
}
