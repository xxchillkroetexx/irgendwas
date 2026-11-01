//! auth slice: forgot/reset password, mail content and language, logging, error hiding
//! (AC33-AC42, AC44, AC45, AC50). Tests that need a mail read it from Mailpit (unique
//! recipient addresses); the others seed the reset token directly.

mod common;

use std::time::{Duration, Instant};

use app::auth_api::{AuthError, Field, FieldError};
use app::flash::FlashMessage;
use app::i18n::{Locale, translate};
use app::server::password;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, ManualClock, create_alice, create_bob, create_user,
    insert_pending_registration, set_reset_token, set_user_locale,
};
use common::auth::*;
use sqlx::MySqlPool;

const RESET: &str = "/auth/reset-password/";
const ALICE: &str = "alice@example.com";
const NEW_PASSWORD: &str = "newpassword1";

fn tok(c: char) -> String {
    c.to_string().repeat(43)
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(800)).await;
}

async fn hash_of(pool: &MySqlPool, email: &str) -> String {
    sqlx::query_scalar("SELECT password FROM users WHERE email = ?")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn sha256_hex(pool: &MySqlPool, s: &str) -> String {
    sqlx::query_scalar("SELECT SHA2(?, 256)")
        .bind(s)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn validation(err: &AuthError, field: Field, error: FieldError) {
    match err {
        AuthError::Validation(list) => assert!(
            list.contains(&(field, error)),
            "expected {field:?}/{error:?} in {list:?}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}

// --------------------------------------------------------------------- AC33

#[sqlx::test(migrations = "../migrations")]
async fn ac33_reset_request_stores_a_hash_and_mails_the_link(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("alice");
    create_alice(&pool, &email).await;
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);

    c.forgot(&format!(" {} ", email.to_uppercase())).await.ok();

    let (stored, ttl): (String, i64) = sqlx::query_as(
        "SELECT reset_token, TIMESTAMPDIFF(SECOND, CURRENT_TIMESTAMP, reset_token_expires) \
         FROM users WHERE email = ?",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(is_sha256_hex(&stored), "{stored}");
    assert!(
        (3_500..=3_600).contains(&ttl),
        "valid for about an hour, got {ttl}s"
    );

    let mail = mailpit.wait_for_latest(&email).await;
    assert_eq!(
        mail.subject,
        translate(Locale::En, "email.passwordReset.subject")
    );
    let link = mail.first_url_with_path(RESET).expect("reset link");
    assert!(
        link.starts_with(&format!("http://localhost:3000{RESET}")),
        "{link}"
    );
    let token = token_in(&link, RESET);
    assert!(is_token(&token), "{token:?}");
    assert_ne!(token, stored);
    assert_eq!(sha256_hex(&pool, &token).await, stored);
    assert!(
        mail.html.contains("Alice") && mail.text.contains("Alice"),
        "greets by name"
    );
}

// --------------------------------------------------------------------- AC34

#[sqlx::test(migrations = "../migrations")]
async fn ac34_unknown_email_gets_the_same_response_and_no_mail(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("alice");
    let ghost = unique_email("ghost");
    create_alice(&pool, &email).await;
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);

    let known = c.forgot(&email).await;
    let unknown = c.forgot(&ghost).await;
    known.ok();
    unknown.ok();
    assert_eq!(known.status, unknown.status);
    assert_eq!(known.body, unknown.body, "byte-identical response");

    mailpit.wait_for_latest(&email).await;
    settle().await;
    assert!(
        mailpit.messages_for(&ghost).await.is_empty(),
        "no mail to an unknown address"
    );
}

// --------------------------------------------------------------------- AC35

#[sqlx::test(migrations = "../migrations")]
async fn ac35_an_unreachable_smtp_server_does_not_delay_the_response(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    // A server that accepts connections and never says a word.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    let app = router(
        &pool,
        &[
            ("MAIL_HOST", "127.0.0.1"),
            ("MAIL_PORT", &port),
            ("MAIL_ENCRYPTION", "none"),
        ],
        Opts::default(),
    );
    let mut c = Client::new(&app);
    // Warm up (first request builds the route list etc.), then measure.
    c.forgot("warmup@example.com").await.ok();

    let started = Instant::now();
    let reply = c.forgot(ALICE).await;
    let took = started.elapsed();
    reply.ok();
    assert!(took < Duration::from_secs(1), "returned after {took:?}");

    let unknown = c.forgot("ghost@example.com").await;
    assert_eq!(reply.body, unknown.body);
}

// --------------------------------------------------------------------- AC36

#[sqlx::test(migrations = "../migrations")]
async fn ac36_reset_sets_the_new_password_and_consumes_the_token(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    // The reset page renders for a valid link (and does not consume it).
    let page = c.get(&format!("{RESET}{}", tok('r'))).await;
    assert_eq!(page.status, 200);
    assert_eq!(
        page.header("referrer-policy").as_deref(),
        Some("same-origin")
    );
    assert!(
        page.header("cache-control")
            .unwrap_or_default()
            .contains("no-store")
    );
    c.call("check_reset_token", &[("token", &tok('r'))])
        .await
        .ok();

    let reply = c.reset(&tok('r'), NEW_PASSWORD, NEW_PASSWORD).await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some("/auth/login"));
    let info = c.session_info().await;
    assert_flash(
        &info,
        &FlashMessage::success("flash.success.password_reset"),
    );
    assert!(info.user.is_none(), "resetting does not log in");

    let (token, expires, hash): (
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
        String,
    ) = sqlx::query_as(
        "SELECT reset_token, reset_token_expires, password FROM users WHERE email = ?",
    )
    .bind(ALICE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(token.is_none() && expires.is_none());
    assert!(hash.starts_with("$argon2id$"));

    let mut login = Client::new(&app);
    assert_eq!(
        login.login(ALICE, ALICE_PASSWORD).await.err(),
        AuthError::InvalidCredentials
    );
    login.login(ALICE, NEW_PASSWORD).await.ok();
    assert_eq!(login.whoami().await.as_deref(), Some(ALICE));
}

// --------------------------------------------------------------------- AC37

#[sqlx::test(migrations = "../migrations")]
async fn ac37_a_password_reset_ends_every_session_of_that_user(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut browser_a = Client::new(&app);
    browser_a.login(ALICE, ALICE_PASSWORD).await.ok();
    assert_eq!(browser_a.get("/user/dashboard").await.status, 200);

    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    let mut browser_b = Client::new(&app);
    browser_b
        .reset(&tok('r'), NEW_PASSWORD, NEW_PASSWORD)
        .await
        .ok();

    let res = browser_a.get("/user/dashboard").await;
    assert!(res.status.is_redirection(), "got {}", res.status);
    assert!(res.location().unwrap().starts_with("/auth/login?next="));
    assert_eq!(browser_a.whoami().await, None);
    let epoch: u32 = sqlx::query_scalar("SELECT session_epoch FROM users WHERE id = ?")
        .bind(alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(epoch, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac37_the_caller_own_session_dies_too(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    set_reset_token(&pool, alice, &tok('r'), 1800).await;

    c.reset(&tok('r'), NEW_PASSWORD, NEW_PASSWORD).await.ok();
    assert_eq!(c.whoami().await, None);
}

// --------------------------------------------------------------------- AC38

#[sqlx::test(migrations = "../migrations")]
async fn ac38_used_expired_and_unknown_links_are_rejected(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    let bob = create_bob(&pool, "bob@example.com").await;
    set_reset_token(&pool, alice, &tok('u'), 1800).await;
    set_reset_token(&pool, bob, &tok('e'), -1).await; // expired a second ago
    let app = router(&pool, &[], Opts::default());

    Client::new(&app)
        .reset(&tok('u'), NEW_PASSWORD, NEW_PASSWORD)
        .await
        .ok(); // now used up
    let alice_hash = hash_of(&pool, ALICE).await;
    let bob_hash = hash_of(&pool, "bob@example.com").await;

    for token in [tok('u'), tok('e'), tok('z'), "short".to_string()] {
        let mut c = Client::new(&app);
        let page = c.get(&format!("{RESET}{token}")).await;
        assert!(page.status.is_redirection(), "status {}", page.status);
        assert_eq!(page.location().as_deref(), Some("/auth/forgot-password"));
        assert_flash(
            &c.session_info().await,
            &FlashMessage::error("flash.error.reset_token_expired"),
        );
        assert_eq!(
            c.call("check_reset_token", &[("token", &token)])
                .await
                .err(),
            AuthError::InvalidResetToken
        );
        assert_eq!(
            c.reset(&token, "anotherpass1", "anotherpass1").await.err(),
            AuthError::InvalidResetToken
        );
    }
    assert_eq!(
        hash_of(&pool, ALICE).await,
        alice_hash,
        "password unchanged"
    );
    assert_eq!(
        hash_of(&pool, "bob@example.com").await,
        bob_hash,
        "password unchanged"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac38_a_newer_reset_request_supersedes_the_older_link(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("alice");
    create_alice(&pool, &email).await;
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);

    c.forgot(&email).await.ok();
    mailpit.wait_for_count(&email, 1).await;
    c.forgot(&email).await.ok();
    let mails = mailpit.wait_for_count(&email, 2).await;

    let stored: String = sqlx::query_scalar("SELECT reset_token FROM users WHERE email = ?")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (mut current, mut stale) = (None, None);
    for mail in &mails {
        let token = token_in(&mail.first_url_with_path(RESET).unwrap(), RESET);
        if sha256_hex(&pool, &token).await == stored {
            current = Some(token);
        } else {
            stale = Some(token);
        }
    }
    let (current, stale) = (current.expect("current link"), stale.expect("older link"));

    let before = hash_of(&pool, &email).await;
    let page = c.get(&format!("{RESET}{stale}")).await;
    assert_eq!(page.location().as_deref(), Some("/auth/forgot-password"));
    assert_eq!(
        c.reset(&stale, NEW_PASSWORD, NEW_PASSWORD).await.err(),
        AuthError::InvalidResetToken
    );
    assert_eq!(hash_of(&pool, &email).await, before);
    c.reset(&current, NEW_PASSWORD, NEW_PASSWORD).await.ok();
}

// --------------------------------------------------------------------- AC39

#[sqlx::test(migrations = "../migrations")]
async fn ac39_invalid_new_passwords_are_rejected_and_the_link_survives(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    let before = hash_of(&pool, ALICE).await;
    let long = "p".repeat(257);

    validation(
        &c.reset(&tok('r'), "short12", "short12").await.err(),
        Field::Password,
        FieldError::PasswordTooShort,
    );
    validation(
        &c.reset(&tok('r'), NEW_PASSWORD, "different123").await.err(),
        Field::PasswordConfirm,
        FieldError::PasswordsDontMatch,
    );
    validation(
        &c.reset(&tok('r'), &long, &long).await.err(),
        Field::Password,
        FieldError::PasswordTooLong,
    );

    assert_eq!(hash_of(&pool, ALICE).await, before);
    c.call("check_reset_token", &[("token", &tok('r'))])
        .await
        .ok();
    c.reset(&tok('r'), NEW_PASSWORD, NEW_PASSWORD).await.ok();
}

// --------------------------------------------------------------------- AC40

#[sqlx::test(migrations = "../migrations")]
async fn ac40_a_reset_lifts_the_login_lockout(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    let clock = ManualClock::new();
    let app = router(&pool, &[], Opts::clock(&clock));
    let mut c = Client::new(&app);
    for _ in 0..10 {
        assert_eq!(
            c.login(ALICE, "wrong password").await.err(),
            AuthError::InvalidCredentials
        );
    }
    assert!(matches!(
        c.login(ALICE, ALICE_PASSWORD).await.err(),
        AuthError::TooManyAttempts { .. }
    ));

    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    c.reset(&tok('r'), NEW_PASSWORD, NEW_PASSWORD).await.ok();

    c.login(ALICE, NEW_PASSWORD).await.ok();
    assert_eq!(c.whoami().await.as_deref(), Some(ALICE));
}

// --------------------------------------------------------------------- AC41

#[sqlx::test(migrations = "../migrations")]
async fn ac41_concurrent_resets_with_one_token_succeed_exactly_once(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    let app = router(&pool, &[], Opts::default());
    let mut first = Client::new(&app);
    let mut second = Client::new(&app);

    let token = tok('r');
    let (a, b) = tokio::join!(
        first.reset(&token, "firstpassword1", "firstpassword1"),
        second.reset(&token, "secondpassword2", "secondpassword2"),
    );
    let results = [
        a.result::<serde_json::Value>(),
        b.result::<serde_json::Value>(),
    ];
    let oks = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(oks, 1, "exactly one succeeds: {results:?}");
    assert!(results.contains(&Err(AuthError::InvalidResetToken)));

    // The winner's password is the stored one.
    let hash = hash_of(&pool, ALICE).await;
    let winner = if results[0].is_ok() {
        "firstpassword1"
    } else {
        "secondpassword2"
    };
    assert!(password::verify(winner, &hash));
}

// --------------------------------------------------------------------- AC42

#[sqlx::test(migrations = "../migrations")]
async fn ac42_names_are_html_escaped_in_the_reset_mail(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("eve");
    create_user(&pool, "<b>Eve</b>", &email).await;
    let app = router(&pool, &[], Opts::mail());
    Client::new(&app).forgot(&email).await.ok();

    let mail = mailpit.wait_for_latest(&email).await;
    assert!(
        mail.html.contains("&lt;b&gt;Eve&lt;/b&gt;"),
        "{}",
        mail.html
    );
    assert!(
        !mail.html.contains("<b>Eve"),
        "no tag from the name: {}",
        mail.html
    );
}

// --------------------------------------------------------------------- AC44

#[sqlx::test(migrations = "../migrations")]
async fn ac44_tokens_links_and_passwords_never_reach_the_log(pool: MySqlPool) {
    let (logs, _guard) = capture_logs();
    let mailpit = Mailpit::from_env();
    let app = router(&pool, &[], Opts::mail());
    let alice = unique_email("alice");
    create_alice(&pool, &alice).await;
    let carol = unique_email("carol");
    let mut c = Client::new(&app);

    // AC21: registration.
    c.register(&carol, None).await.ok();
    let mail = mailpit.wait_for_latest(&carol).await;
    let reg_link = mail.first_url_with_path("/auth/register/confirm/").unwrap();
    let reg_token = token_in(&reg_link, "/auth/register/confirm/");
    // Rendering the token pages (valid link, then used-up link) must not log the token either.
    assert_eq!(
        c.get(&format!("/auth/register/confirm/{reg_token}"))
            .await
            .status,
        200
    );
    c.complete(&reg_token, "Carol", "carol-secret-pw1", "carol-secret-pw1")
        .await
        .ok();

    // AC33: reset request, AC36: reset.
    c.forgot(&alice).await.ok();
    let mail = mailpit.wait_for_latest(&alice).await;
    let reset_link = mail.first_url_with_path(RESET).unwrap();
    let reset_token = token_in(&reset_link, RESET);
    assert_eq!(c.get(&format!("{RESET}{reset_token}")).await.status, 200);
    c.reset(&reset_token, "alice-new-secret-pw2", "alice-new-secret-pw2")
        .await
        .ok();
    // The same pages with the now-dead tokens (redirect branch).
    c.get(&format!("/auth/register/confirm/{reg_token}")).await;
    c.get(&format!("{RESET}{reset_token}")).await;
    settle().await;

    let log = logs.contents();
    assert!(
        !log.is_empty(),
        "the capture works (the app logs at debug level)"
    );
    for secret in [
        reg_token.as_str(),
        reg_link.as_str(),
        reset_token.as_str(),
        reset_link.as_str(),
        "carol-secret-pw1",
        "alice-new-secret-pw2",
    ] {
        assert!(!log.contains(secret), "log leaks {secret:?}");
    }
}

// --------------------------------------------------------------------- AC45

#[sqlx::test(migrations = "../migrations")]
async fn ac45_database_errors_surface_as_internal_without_details(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    let assert_internal = |what: &str, reply: &Reply| {
        assert_eq!(reply.err(), AuthError::Internal, "{what}");
        let lower = reply.body.to_lowercase();
        for leak in [
            "sql",
            "table",
            "users",
            "pending_registrations",
            "doesn't exist",
            "stack",
        ] {
            assert!(
                !lower.contains(leak),
                "{what}: body leaks {leak:?}: {}",
                reply.body
            );
        }
    };

    // The user table is gone.
    sqlx::query("RENAME TABLE users TO users_gone")
        .execute(&pool)
        .await
        .unwrap();
    assert_internal("login", &c.login(ALICE, ALICE_PASSWORD).await);
    assert_internal("register", &c.register("new@example.com", None).await);
    assert_internal("request_password_reset", &c.forgot("new@example.com").await);
    assert_internal(
        "check_reset_token",
        &c.call("check_reset_token", &[("token", &tok('r'))]).await,
    );
    assert_internal(
        "reset_password",
        &c.reset(&tok('r'), NEW_PASSWORD, NEW_PASSWORD).await,
    );
    sqlx::query("RENAME TABLE users_gone TO users")
        .execute(&pool)
        .await
        .unwrap();

    // The pending-registration table is gone.
    insert_pending_registration(&pool, "carol@example.com", &tok('a'), None, 3600).await;
    sqlx::query("RENAME TABLE pending_registrations TO pending_gone")
        .execute(&pool)
        .await
        .unwrap();
    assert_internal(
        "get_pending_registration",
        &c.call("get_pending_registration", &[("token", &tok('a'))])
            .await,
    );
    assert_internal(
        "complete_registration",
        &c.complete(&tok('a'), "Carol", "carolpass1", "carolpass1")
            .await,
    );
    assert_internal("register", &c.register("new@example.com", None).await);
}

// --------------------------------------------------------------------- AC50

#[sqlx::test(migrations = "../migrations")]
async fn ac50_reset_mail_follows_the_account_language(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let de_subject = translate(Locale::De, "email.passwordReset.subject");
    let en_subject = translate(Locale::En, "email.passwordReset.subject");
    assert_ne!(de_subject, en_subject, "the catalogs differ");

    // German account, English page: German mail.
    let alice_mail = unique_email("alice");
    let alice = create_alice(&pool, &alice_mail).await;
    set_user_locale(&pool, alice, Some("de")).await;
    // Account without a language (APP_LOCALE=en), German page: English mail.
    let bob_mail = unique_email("bob");
    create_bob(&pool, &bob_mail).await;
    let app = router(&pool, &[("APP_LOCALE", "en")], Opts::mail());
    Client::new(&app)
        .with_cookie("locale", "en")
        .forgot(&alice_mail)
        .await
        .ok();
    Client::new(&app)
        .with_cookie("locale", "de")
        .forgot(&bob_mail)
        .await
        .ok();

    let mail = mailpit.wait_for_latest(&alice_mail).await;
    assert_eq!(mail.subject, de_subject);
    assert!(mail.html.contains("lang=\"de\""), "{}", mail.html);
    let mail = mailpit.wait_for_latest(&bob_mail).await;
    assert_eq!(mail.subject, en_subject);
    assert!(mail.html.contains("lang=\"en\""), "{}", mail.html);

    // An unknown stored language falls back to APP_LOCALE (de here).
    let carl_mail = unique_email("carl");
    let carl = create_alice(&pool, &carl_mail).await;
    sqlx::query("UPDATE users SET locale = 'fr' WHERE id = ?")
        .bind(carl)
        .execute(&pool)
        .await
        .unwrap();
    let app = router(&pool, &[("APP_LOCALE", "de")], Opts::mail());
    Client::new(&app)
        .with_cookie("locale", "en")
        .forgot(&carl_mail)
        .await
        .ok();
    let mail = mailpit.wait_for_latest(&carl_mail).await;
    assert_eq!(mail.subject, de_subject);
    assert!(mail.html.contains("lang=\"de\""), "{}", mail.html);
}
