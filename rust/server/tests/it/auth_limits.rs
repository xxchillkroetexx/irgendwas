//! auth slice: rate limiting and client IP (AC11-AC16, AC32, AC51). Time moves through the
//! injectable limiter clock (`ManualClock`), never through sleeping.

use crate::common::auth::*;
use app::auth_api::AuthError;
use app::i18n::{I18n, Locale};
use app::test_support::{ALICE_PASSWORD, ManualClock, create_alice};
use chrono::Duration;
use sqlx::MySqlPool;

const ALICE: &str = "alice@example.com";

fn berlin() -> chrono_tz::Tz {
    chrono_tz::Europe::Berlin
}

async fn fail_logins(c: &mut Client, email: &str, n: usize) {
    for i in 0..n {
        let err = c.login(email, "definitely wrong").await.err();
        assert_eq!(err, AuthError::InvalidCredentials, "failure #{}", i + 1);
    }
}

fn minutes_of(err: AuthError) -> u32 {
    match err {
        AuthError::TooManyAttempts { minutes } => minutes,
        other => panic!("expected TooManyAttempts, got {other:?}"),
    }
}

// --------------------------------------------------------------------- AC11

#[sqlx::test(migrations = "../migrations")]
async fn ac11_eleventh_attempt_is_locked_even_with_the_right_password(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(&pool, &[], Opts::clock(&clock));
    let mut c = Client::new(&app);

    fail_logins(&mut c, ALICE, 10).await;
    let reply = c.login(ALICE, ALICE_PASSWORD).await;
    let minutes = minutes_of(reply.err());
    assert!((1..=15).contains(&minutes), "minutes = {minutes}");
    assert!(reply.location().is_none());
    assert_eq!(c.whoami().await, None, "no session was created");
}

// --------------------------------------------------------------------- AC12

#[sqlx::test(migrations = "../migrations")]
async fn ac12_unknown_emails_lock_out_with_the_same_response_shape(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(&pool, &[], Opts::clock(&clock));
    let mut c = Client::new(&app);

    fail_logins(&mut c, ALICE, 10).await;
    let real = c.login(ALICE, ALICE_PASSWORD).await;
    fail_logins(&mut c, "ghost@example.com", 10).await;
    let ghost = c.login("ghost@example.com", ALICE_PASSWORD).await;

    let minutes = minutes_of(ghost.err());
    assert!((1..=15).contains(&minutes), "minutes = {minutes}");
    assert_eq!(real.status, ghost.status);
    assert_eq!(real.body, ghost.body, "no enumeration via lockout");
}

// --------------------------------------------------------------------- AC13

#[sqlx::test(migrations = "../migrations")]
async fn ac13_lockout_ends_when_the_window_passes(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(&pool, &[], Opts::clock(&clock));
    let mut c = Client::new(&app);

    fail_logins(&mut c, ALICE, 10).await;
    minutes_of(c.login(ALICE, ALICE_PASSWORD).await.err());

    clock.advance(Duration::minutes(14));
    minutes_of(c.login(ALICE, ALICE_PASSWORD).await.err()); // still inside the window

    clock.advance(Duration::minutes(1) + Duration::seconds(5));
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    assert_eq!(c.whoami().await.as_deref(), Some(ALICE));
}

// --------------------------------------------------------------------- AC14

#[sqlx::test(migrations = "../migrations")]
async fn ac14_a_successful_login_resets_the_failure_counter(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(&pool, &[], Opts::clock(&clock));
    let mut c = Client::new(&app);

    fail_logins(&mut c, ALICE, 9).await;
    c.login(ALICE, ALICE_PASSWORD).await.ok();

    // 10 further failures are needed: all of them are still plain failures ...
    let mut c = Client::new(&app);
    fail_logins(&mut c, ALICE, 10).await;
    // ... and only now is the account locked.
    minutes_of(c.login(ALICE, ALICE_PASSWORD).await.err());
}

// --------------------------------------------------------------------- AC15

#[sqlx::test(migrations = "../migrations")]
async fn ac15_fifty_failures_from_one_ip_lock_that_ip_only(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(&pool, &[], Opts::clock(&clock));

    let mut noisy = Client::new(&app).peer("198.51.100.20");
    for i in 0..50 {
        let email = format!("user{i}@example.com");
        assert_eq!(
            noisy.login(&email, "wrong password").await.err(),
            AuthError::InvalidCredentials,
            "attempt #{}",
            i + 1
        );
    }
    let blocked = noisy.login(ALICE, ALICE_PASSWORD).await;
    minutes_of(blocked.err());
    assert_eq!(noisy.whoami().await, None);

    // The same attempt from another address proceeds normally.
    let mut other = Client::new(&app).peer("198.51.100.21");
    other.login(ALICE, ALICE_PASSWORD).await.ok();
    assert_eq!(other.whoami().await.as_deref(), Some(ALICE));

    // After the window the first IP is free again.
    clock.advance(Duration::minutes(16));
    noisy.login(ALICE, ALICE_PASSWORD).await.ok();
}

// --------------------------------------------------------------------- AC16

#[sqlx::test(migrations = "../migrations")]
async fn ac16_limiter_keys_on_the_client_behind_trusted_proxies(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    // Caddy (10.1.1.0/24) and an outer proxy (10.0.0.5) are trusted.
    let app = router(
        &pool,
        &[("TRUSTED_PROXIES_CIDR", "10.1.1.0/24 10.0.0.5/32")],
        Opts::clock(&clock),
    );

    // Client 203.0.113.9 reaches us once through Caddy alone ...
    let mut one_hop = Client::new(&app)
        .peer("10.1.1.1")
        .header("x-forwarded-for", "203.0.113.9, 10.1.1.1");
    for i in 0..25 {
        let email = format!("one{i}@example.com");
        one_hop.login(&email, "wrong password").await.err();
    }
    // ... and once through the outer proxy and Caddy, with a spoofed left part.
    for i in 0..25 {
        let mut two_hops = Client::new(&app).peer("10.1.1.2").header(
            "x-forwarded-for",
            &format!("198.51.100.{}, 203.0.113.9, 10.0.0.5", 50 + i),
        );
        let email = format!("two{i}@example.com");
        two_hops.login(&email, "wrong password").await.err();
    }
    // All 50 failures count against 203.0.113.9: the next attempt is blocked, whichever
    // way it arrives and whatever the spoofable left part says.
    minutes_of(one_hop.login(ALICE, ALICE_PASSWORD).await.err());
    let mut spoof = Client::new(&app)
        .peer("10.1.1.2")
        .header("x-forwarded-for", "198.51.100.7, 203.0.113.9, 10.0.0.5");
    minutes_of(spoof.login(ALICE, ALICE_PASSWORD).await.err());

    // A different client behind the same proxy is not affected.
    let mut other = Client::new(&app)
        .peer("10.1.1.1")
        .header("x-forwarded-for", "198.51.100.7, 10.1.1.1");
    other.login(ALICE, ALICE_PASSWORD).await.ok();
    assert_eq!(other.whoami().await.as_deref(), Some(ALICE));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac16_untrusted_peer_cannot_spoof_forwarded_for(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(
        &pool,
        &[("TRUSTED_PROXIES_CIDR", "10.1.1.0/24")],
        Opts::clock(&clock),
    );

    // 192.0.2.50 is not a proxy: a different forged client address per request does not
    // spread its failures over many keys.
    for i in 0..50 {
        let mut c = Client::new(&app)
            .peer("192.0.2.50")
            .header("x-forwarded-for", &format!("203.0.113.{}", i + 1));
        c.login(&format!("user{i}@example.com"), "wrong password")
            .await
            .err();
    }
    let mut c = Client::new(&app)
        .peer("192.0.2.50")
        .header("x-forwarded-for", "203.0.113.200");
    minutes_of(c.login(ALICE, ALICE_PASSWORD).await.err());
}

// --------------------------------------------------------------------- AC32

#[sqlx::test(migrations = "../migrations")]
async fn ac32_mail_endpoints_are_limited_per_ip_regardless_of_the_account(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(&pool, &[], Opts::clock(&clock));
    let mut c = Client::new(&app).peer("198.51.100.30");

    for i in 0..10 {
        c.register(&format!("new{i}@example.com"), None).await.ok();
        c.forgot(&format!("ghost{i}@example.com")).await.ok();
    }
    // The 21st call: too many, whether the address exists or not, on either endpoint.
    assert_eq!(
        c.forgot("ghost-final@example.com").await.err(),
        AuthError::TooManyRequests
    );
    assert_eq!(c.forgot(ALICE).await.err(), AuthError::TooManyRequests);
    assert_eq!(
        c.register("fresh@example.com", None).await.err(),
        AuthError::TooManyRequests
    );
    assert_eq!(
        c.register(ALICE, None).await.err(),
        AuthError::TooManyRequests
    );

    // Another address is not affected; the window ends after an hour.
    let mut other = Client::new(&app).peer("198.51.100.31");
    other.forgot(ALICE).await.ok();
    clock.advance(Duration::minutes(61));
    c.forgot("ghost-later@example.com").await.ok();
}

// --------------------------------------------------------------------- AC51

#[sqlx::test(migrations = "../migrations")]
async fn ac51_lockout_message_is_pluralised_in_both_languages(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(&pool, &[], Opts::clock(&clock));
    let mut c = Client::new(&app);
    fail_logins(&mut c, ALICE, 10).await;

    let en = I18n::new(Locale::En, berlin());
    let de = I18n::new(Locale::De, berlin());

    // 13 min 10 s in: 1 min 50 s left, rounded up to 2.
    clock.advance(Duration::minutes(13) + Duration::seconds(10));
    let err = c.login(ALICE, ALICE_PASSWORD).await.err();
    assert_eq!(minutes_of(err.clone()), 2);
    assert_eq!(
        err.text(&en),
        "Too many failed login attempts. Please try again in 2 minutes."
    );
    assert_eq!(
        err.text(&de),
        "Zu viele fehlgeschlagene Anmeldeversuche. Bitte versuchen Sie es in 2 Minuten erneut."
    );

    // 14 min 10 s in: 50 s left, rounded up to 1.
    clock.advance(Duration::minutes(1));
    let err = c.login(ALICE, ALICE_PASSWORD).await.err();
    assert_eq!(minutes_of(err.clone()), 1);
    assert_eq!(
        err.text(&en),
        "Too many failed login attempts. Please try again in 1 minute."
    );
    assert_eq!(
        err.text(&de),
        "Zu viele fehlgeschlagene Anmeldeversuche. Bitte versuchen Sie es in 1 Minute erneut."
    );
}
