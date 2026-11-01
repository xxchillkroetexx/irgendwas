//! account slice: rate limits. AC40 (current-password attempts, `account:pw:<user_id>`,
//! 10 per 15 min) and AC43 (per-recipient mail limit `mail:email:<new email>`, 3 per hour,
//! shared with auth's register/forgot-password). Time moves through `ManualClock`.

use std::sync::Arc;

use crate::common::account::*;
use crate::common::auth::*;
use app::account_api::AccountError;
use app::auth_api::AuthError;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, ManualClock, TEST_PASSWORD, create_bob, create_user, user_row,
    user_rows,
};
use chrono::Duration;
use sqlx::MySqlPool;

const NEW: &str = "newpassword1";

fn mail_clock(clock: &Arc<ManualClock>) -> Opts {
    Opts {
        mail: true,
        clock: Some(clock.clone()),
    }
}

fn minutes_of(err: AccountError) -> u32 {
    match err {
        AccountError::TooManyAttempts { minutes } => minutes,
        other => panic!("expected TooManyAttempts, got {other:?}"),
    }
}

/// `n` wrong-password attempts, alternating between the two fns when `mix`.
async fn fail_attempts(c: &mut Client, n: usize, mix: bool) {
    for i in 0..n {
        let reply = if mix && i % 2 == 1 {
            request_email_change(c, &unique_email("never-mailed"), "definitely wrong").await
        } else {
            change_password(c, "definitely wrong", NEW, NEW).await
        };
        assert_eq!(
            reply.account_err(),
            AccountError::CurrentPasswordIncorrect,
            "failure #{}",
            i + 1
        );
    }
}

// -------------------------------------------------------------------------- AC40

#[sqlx::test(migrations = "../migrations")]
async fn ac40_eleventh_attempt_is_blocked_even_with_the_correct_password(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let clock = ManualClock::new();
    let mut w = alice_world_with(&pool, &[], mail_clock(&clock)).await;
    let new = unique_email("alice-new");
    fail_attempts(&mut w.c, 10, false).await;
    let before = user_rows(&pool).await;

    let reply = change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW).await;
    let minutes = minutes_of(reply.account_err());
    assert!((1..=15).contains(&minutes), "minutes = {minutes}");
    // The limiter is shared: the other fn is blocked too, with the correct password.
    let reply = request_email_change(&mut w.c, &new, ALICE_PASSWORD).await;
    assert!((1..=15).contains(&minutes_of(reply.account_err())));

    assert_eq!(user_rows(&pool).await, before, "nothing changed");
    assert_eq!(mails_after_settle(&mailpit, &new).await, 0, "no mail");
    // The old password still works: the blocked call did not change it.
    let mut fresh = w.c.fresh();
    fresh.login(&w.email, ALICE_PASSWORD).await.ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_both_fns_count_against_the_same_budget(pool: MySqlPool) {
    let clock = ManualClock::new();
    for first_is_password in [true, false] {
        let mut w = alice_world_with(&pool, &[], mail_clock(&clock)).await;
        // 5 + 5 failures in either order.
        for i in 0..10 {
            let use_password_fn = (i < 5) == first_is_password;
            let reply = if use_password_fn {
                change_password(&mut w.c, "wrong", NEW, NEW).await
            } else {
                request_email_change(&mut w.c, &unique_email("never-mailed"), "wrong").await
            };
            assert_eq!(reply.account_err(), AccountError::CurrentPasswordIncorrect);
        }
        // Either fn is now blocked.
        minutes_of(
            change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
                .await
                .account_err(),
        );
        minutes_of(
            request_email_change(&mut w.c, &unique_email("never-mailed"), ALICE_PASSWORD)
                .await
                .account_err(),
        );
        // The next round uses a new account, hence a new key.
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_validation_failures_are_not_counted(pool: MySqlPool) {
    let clock = ManualClock::new();
    let mut w = alice_world_with(&pool, &[], mail_clock(&clock)).await;

    for _ in 0..15 {
        assert_eq!(
            change_password(&mut w.c, "wrong", "short", "short")
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
            request_email_change(&mut w.c, "not-an-email", "wrong")
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
    }
    // The full budget is still there: 10 verified failures, then the block.
    fail_attempts(&mut w.c, 10, true).await;
    minutes_of(
        change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
            .await
            .account_err(),
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_a_successful_verification_clears_the_counter(pool: MySqlPool) {
    let clock = ManualClock::new();
    let mut w = alice_world_with(&pool, &[], mail_clock(&clock)).await;

    fail_attempts(&mut w.c, 9, true).await;
    change_password(&mut w.c, ALICE_PASSWORD, ALICE_PASSWORD, ALICE_PASSWORD)
        .await
        .account_ok();
    // Counter is back at zero: ten more failures are verified, the 11th is blocked.
    for i in 0..10 {
        assert_eq!(
            change_password(&mut w.c, "wrong", NEW, NEW)
                .await
                .account_err(),
            AccountError::CurrentPasswordIncorrect,
            "failure #{}",
            i + 1
        );
    }
    minutes_of(
        change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
            .await
            .account_err(),
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_the_block_ends_with_the_window(pool: MySqlPool) {
    let clock = ManualClock::new();
    let mut w = alice_world_with(&pool, &[], mail_clock(&clock)).await;
    fail_attempts(&mut w.c, 10, false).await;
    minutes_of(
        change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
            .await
            .account_err(),
    );

    clock.advance(Duration::minutes(14));
    minutes_of(
        change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
            .await
            .account_err(),
    );
    clock.advance(Duration::minutes(2));
    change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
        .await
        .account_ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_the_limit_is_per_user_and_independent_of_the_login_limiter(pool: MySqlPool) {
    let clock = ManualClock::new();
    let mut w = alice_world_with(&pool, &[], mail_clock(&clock)).await;
    let bob_email = unique_email("bob");
    create_bob(&pool, &bob_email).await;
    let mut bob = Client::new(&w.app);
    bob.login(&bob_email, BOB_PASSWORD).await.ok();

    // account:pw blocked for Alice: she can still log in, Bob is unaffected.
    fail_attempts(&mut w.c, 10, false).await;
    minutes_of(
        change_password(&mut w.c, ALICE_PASSWORD, NEW, NEW)
            .await
            .account_err(),
    );
    let mut other_browser = Client::new(&w.app);
    other_browser.login(&w.email, ALICE_PASSWORD).await.ok();
    assert_eq!(
        other_browser.whoami().await.as_deref(),
        Some(w.email.as_str())
    );
    assert_eq!(
        change_password(&mut bob, "wrong", NEW, NEW)
            .await
            .account_err(),
        AccountError::CurrentPasswordIncorrect
    );

    // And the other way round: a locked-out login does not block change_password.
    let carol_email = unique_email("carol");
    create_user(&pool, "Carol", &carol_email).await;
    let mut carol = Client::new(&w.app);
    carol.login(&carol_email, TEST_PASSWORD).await.ok();
    let mut attacker = Client::new(&w.app);
    for _ in 0..10 {
        assert_eq!(
            attacker.login(&carol_email, "wrong").await.err(),
            AuthError::InvalidCredentials
        );
    }
    assert!(matches!(
        attacker.login(&carol_email, TEST_PASSWORD).await.err(),
        AuthError::TooManyAttempts { .. }
    ));
    change_password(&mut carol, TEST_PASSWORD, NEW, NEW)
        .await
        .account_ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac40_the_limiter_holds_under_a_concurrent_burst(pool: MySqlPool) {
    let clock = ManualClock::new();
    let w = alice_world_with(&pool, &[], mail_clock(&clock)).await;

    let mut set = tokio::task::JoinSet::new();
    for i in 0..30 {
        let mut c = w.c.fork();
        set.spawn(async move {
            if i % 2 == 0 {
                change_password(&mut c, "wrong password", NEW, NEW)
                    .await
                    .account_err()
            } else {
                request_email_change(&mut c, &unique_email("never-mailed"), "wrong password")
                    .await
                    .account_err()
            }
        });
    }
    let (mut verified, mut limited) = (0, 0);
    while let Some(result) = set.join_next().await {
        match result.expect("task") {
            AccountError::CurrentPasswordIncorrect => verified += 1,
            AccountError::TooManyAttempts { .. } => limited += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(verified + limited, 30);
    assert!(
        verified <= 10,
        "{verified} concurrent attempts reached password verification (limit 10)"
    );
}

// -------------------------------------------------------------------------- AC43

/// `n` more accounts on `w`'s app, each logged in.
async fn extra_users(pool: &MySqlPool, w: &World, n: usize) -> Vec<Client> {
    let mut clients = Vec::new();
    for i in 0..n {
        let email = unique_email(&format!("extra{i}"));
        create_user(pool, "Extra", &email).await;
        let mut c = Client::new(&w.app);
        c.login(&email, TEST_PASSWORD).await.ok();
        clients.push(c);
    }
    clients
}

#[sqlx::test(migrations = "../migrations")]
async fn ac43_the_fourth_mail_to_one_recipient_is_not_sent(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let mut others = extra_users(&pool, &w, 2).await;
    let target = unique_email("target");

    // Three different accounts (so the 60 s per-account throttle is not in play).
    request_email_change(&mut w.c, &target, ALICE_PASSWORD)
        .await
        .account_ok();
    request_email_change(&mut others[0], &target, TEST_PASSWORD)
        .await
        .account_ok();
    request_email_change(&mut others[1], &target, TEST_PASSWORD)
        .await
        .account_ok();
    mailpit.wait_for_count(&target, 3).await;

    let mut fourth = extra_users(&pool, &w, 1).await.remove(0);
    let fourth_id: i32 = sqlx::query_scalar("SELECT MAX(id) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    let before = user_row(&pool, fourth_id).await;
    assert_eq!(
        request_email_change(&mut fourth, &target, TEST_PASSWORD)
            .await
            .account_err(),
        AccountError::EmailChangeTooSoon
    );
    assert_eq!(
        user_row(&pool, fourth_id).await,
        before,
        "pending columns unchanged"
    );
    assert_eq!(
        mails_after_settle(&mailpit, &target).await,
        3,
        "no fourth mail"
    );

    // The failed request did not use up the account's own throttle.
    let elsewhere = unique_email("elsewhere");
    request_email_change(&mut fourth, &elsewhere, TEST_PASSWORD)
        .await
        .account_ok();
    mailpit.wait_for_count(&elsewhere, 1).await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac43_the_budget_is_shared_with_registration_mails(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let mut w = alice_world(&pool).await;
    let target = unique_email("target");
    let mut anon = Client::new(&w.app);
    for _ in 0..3 {
        anon.register(&target, None).await.ok();
    }
    mailpit.wait_for_count(&target, 3).await;
    let before = user_row(&pool, w.id).await;

    assert_eq!(
        request_email_change(&mut w.c, &target, ALICE_PASSWORD)
            .await
            .account_err(),
        AccountError::EmailChangeTooSoon
    );
    assert_eq!(user_row(&pool, w.id).await, before);
    assert_eq!(mails_after_settle(&mailpit, &target).await, 3);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac43_the_recipient_budget_comes_back_after_an_hour(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let clock = ManualClock::new();
    let mut w = alice_world_with(&pool, &[], mail_clock(&clock)).await;
    let mut others = extra_users(&pool, &w, 3).await;
    let target = unique_email("target");
    request_email_change(&mut w.c, &target, ALICE_PASSWORD)
        .await
        .account_ok();
    request_email_change(&mut others[0], &target, TEST_PASSWORD)
        .await
        .account_ok();
    request_email_change(&mut others[1], &target, TEST_PASSWORD)
        .await
        .account_ok();
    assert_eq!(
        request_email_change(&mut others[2], &target, TEST_PASSWORD)
            .await
            .account_err(),
        AccountError::EmailChangeTooSoon
    );

    clock.advance(Duration::minutes(61));
    request_email_change(&mut others[2], &target, TEST_PASSWORD)
        .await
        .account_ok();
    mailpit.wait_for_count(&target, 4).await;
}
