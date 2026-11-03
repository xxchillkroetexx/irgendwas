//! auth slice: login, logout, session lifecycle, account language at login
//! (AC1-AC10, AC17-AC20, AC47). Named after the acceptance criteria of
//! docs/rewrite/specs/auth.md. Server fns are POSTed through the real router.
//!
//! The protected page used throughout is the dashboard (`/user/dashboard`): the groups
//! pages of AC6 do not exist yet, so the guard contract is checked on the one page every
//! auth redirect already depends on.

use crate::common::account::tok;
use crate::common::auth::*;
use app::auth_api::AuthError;
use app::flash::FlashMessage;
use app::i18n::{I18n, Locale, translate};
use app::paths::DASHBOARD_PATH;
use app::server::i18n::locale_cookie;
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, bob_last_login, create_alice, create_bob, set_reset_token,
    set_user_locale,
};
use sqlx::MySqlPool;

const ALICE: &str = "alice@example.com";
const BOB: &str = "bob@example.com";

fn berlin() -> chrono_tz::Tz {
    chrono_tz::Europe::Berlin
}

fn assert_redirect_to(reply: &Reply, location: &str) {
    assert!(
        reply.status.is_redirection(),
        "expected a redirect, got {} {:?}",
        reply.status,
        reply.body.chars().take(200).collect::<String>()
    );
    assert_eq!(reply.location().as_deref(), Some(location));
}

// ---------------------------------------------------------------------- AC1

#[sqlx::test(migrations = "../migrations")]
async fn ac1_login_normalises_email_and_lands_on_dashboard(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    let reply = c.login("ALICE@Example.com ", ALICE_PASSWORD).await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some(DASHBOARD_PATH));

    // The dashboard shows the success flash (last_login was NULL: the plain message).
    let page = c.get(DASHBOARD_PATH).await;
    assert_eq!(page.status, 200, "dashboard for a logged-in user");
    assert!(
        html_contains(
            &page.body,
            &translate(Locale::En, "flash.success.logged_in")
        ),
        "flash.success.logged_in expected on the dashboard"
    );
    assert_eq!(c.whoami().await.as_deref(), Some(ALICE));
}

// ---------------------------------------------------------------------- AC2

#[sqlx::test(migrations = "../migrations")]
async fn ac2_login_flash_shows_previous_last_login_in_page_locale_and_timezone(pool: MySqlPool) {
    create_bob(&pool, BOB).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    c.login(BOB, BOB_PASSWORD).await.ok();
    let page = c.get(DASHBOARD_PATH).await;
    assert!(
        html_contains(&page.body, "1 Dec 2026, 11:00"),
        "English page shows the previous login in Europe/Berlin"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac2_flash_message_is_typed_with_the_previous_login(pool: MySqlPool) {
    create_bob(&pool, BOB).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    c.login(BOB, BOB_PASSWORD).await.ok();
    let info = c.session_info().await;
    let expected = FlashMessage::success("flash.success.logged_in_last_login")
        .with_datetime("date", bob_last_login());
    assert_flash(&info, &expected);
    assert!(!has_flash_key(&info, "flash.success.logged_in"));

    assert!(
        expected
            .text(&I18n::new(Locale::En, berlin()))
            .contains("1 Dec 2026, 11:00")
    );
    assert!(
        expected
            .text(&I18n::new(Locale::De, berlin()))
            .contains("01.12.2026 11:00")
    );

    // The previous value is read before it is overwritten: last_login is now.
    let age: i64 = sqlx::query_scalar(
        "SELECT TIMESTAMPDIFF(SECOND, last_login, CURRENT_TIMESTAMP) FROM users WHERE email = ?",
    )
    .bind(BOB)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((0..60).contains(&age), "last_login updated, {age}s old");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac2_german_account_gets_german_dashboard_and_date(pool: MySqlPool) {
    let bob = create_bob(&pool, BOB).await;
    set_user_locale(&pool, bob, Some("de")).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    c.login(BOB, BOB_PASSWORD).await.ok();
    let page = c.get(DASHBOARD_PATH).await;
    assert_eq!(lang_of(&page.body), "de", "the dashboard is German");
    assert!(
        html_contains(&page.body, "01.12.2026 11:00"),
        "German date format of the previous login"
    );
}

// ---------------------------------------------------------------------- AC3

#[sqlx::test(migrations = "../migrations")]
async fn ac3_unknown_email_and_wrong_password_look_identical(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());

    let mut unknown = Client::new(&app);
    let a = unknown
        .login("nobody@example.com", "whatever password")
        .await;
    let mut wrong = Client::new(&app);
    let b = wrong.login(ALICE, "not the password").await;

    assert_eq!(a.err(), AuthError::InvalidCredentials);
    assert_eq!(b.err(), AuthError::InvalidCredentials);
    assert_eq!(a.status, b.status);
    assert_eq!(a.body, b.body, "identical serialized response");
    assert!(a.location().is_none() && b.location().is_none());

    // No login happened in either session.
    assert_eq!(unknown.whoami().await, None);
    assert_eq!(wrong.whoami().await, None);
    let sessions_with_user: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE data LIKE '%user_id%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sessions_with_user, 0);
}

/// Accounts from the PHP data migration keep their bcrypt hash: the old password is
/// rejected exactly like a wrong one (no error page, no tell), and a password reset makes
/// the account usable. Timing: `password::foreign_hashes_cost_as_much_as_a_real_verification`.
#[sqlx::test(migrations = "../migrations")]
async fn ac3_migrated_php_account_is_rejected_like_a_wrong_password_until_reset(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    sqlx::query("UPDATE users SET password = ? WHERE id = ?")
        .bind("$2y$10$abcdefghijklmnopqrstuuJ5Z5nZQm2c1yS1V5gW8kq3yJ4bq7tWe")
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    let app = router(&pool, &[], Opts::default());

    let mut unknown = Client::new(&app);
    let a = unknown.login("nobody@example.com", ALICE_PASSWORD).await;
    let mut migrated = Client::new(&app);
    let b = migrated.login(ALICE, ALICE_PASSWORD).await;
    assert_eq!(b.err(), AuthError::InvalidCredentials);
    assert_eq!(a.status, b.status);
    assert_eq!(a.body, b.body, "identical serialized response");
    assert_eq!(migrated.whoami().await, None);

    set_reset_token(&pool, alice, &tok('m'), 1800).await;
    let new_password = "a brand new password";
    migrated
        .reset(&tok('m'), new_password, new_password)
        .await
        .ok();
    migrated.login(ALICE, new_password).await.ok();
    assert_eq!(migrated.whoami().await.as_deref(), Some(ALICE));
    let stored: String = sqlx::query_scalar("SELECT password FROM users WHERE id = ?")
        .bind(alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(stored.starts_with("$argon2id$"), "{stored}");
}

// ---------------------------------------------------------------------- AC4

#[sqlx::test(migrations = "../migrations")]
async fn ac4_missing_credentials_are_reported_and_not_counted(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    assert_eq!(
        c.login("", ALICE_PASSWORD).await.err(),
        AuthError::MissingCredentials
    );
    assert_eq!(
        c.login(ALICE, "").await.err(),
        AuthError::MissingCredentials
    );
    assert_eq!(c.login("", "").await.err(), AuthError::MissingCredentials);
    // Far more than the 10 allowed failures: none of them counted.
    for _ in 0..12 {
        assert_eq!(
            c.login(ALICE, "").await.err(),
            AuthError::MissingCredentials
        );
    }
    let reply = c.login(ALICE, ALICE_PASSWORD).await;
    reply.ok();
    assert_eq!(c.whoami().await.as_deref(), Some(ALICE));
}

// ---------------------------------------------------------------------- AC5

#[sqlx::test(migrations = "../migrations")]
async fn ac5_login_sets_last_login_and_cycles_the_session_id(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());

    // An attacker plants a session id (the victim's browser already holds an anonymous
    // session with data).
    let mut c = Client::new(&app);
    c.get("/test/touch").await;
    let planted = c
        .session_cookie()
        .expect("anonymous session cookie")
        .to_string();

    let before: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_login FROM users WHERE email = ?")
            .bind(ALICE)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(before.is_none());

    c.login(ALICE, ALICE_PASSWORD).await.ok();
    let after = c
        .session_cookie()
        .expect("session cookie after login")
        .to_string();
    assert_ne!(planted, after, "session id must change at login");

    // The planted id is useless.
    let mut attacker = Client::new(&app).with_cookie(server::SESSION_COOKIE, &planted);
    assert_eq!(attacker.whoami().await, None);
    assert_eq!(c.whoami().await.as_deref(), Some(ALICE));

    let age: i64 = sqlx::query_scalar(
        "SELECT TIMESTAMPDIFF(SECOND, last_login, CURRENT_TIMESTAMP) FROM users WHERE email = ?",
    )
    .bind(ALICE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (0..60).contains(&age),
        "last_login is the current time, {age}s old"
    );
}

// ---------------------------------------------------------------------- AC6

#[sqlx::test(migrations = "../migrations")]
async fn ac6_anonymous_get_of_a_protected_page_redirects_to_login_with_next(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    let res = c.get("/user/dashboard?x=1").await;
    assert!(res.status.is_redirection(), "got {}", res.status);
    let location = res.location().expect("Location");
    assert!(
        location.starts_with("/auth/login?next=%2Fuser%2Fdashboard"),
        "path and query are percent-encoded into next: {location}"
    );
    let next = location.strip_prefix("/auth/login?next=").unwrap();
    assert_eq!(urldecode(next), "/user/dashboard?x=1");

    // The login page explains why.
    let page = c.get(&location).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(
        &page.body,
        &translate(Locale::En, "auth.login.required")
    ));
    // ... but not when there is no (valid) next.
    for uri in ["/auth/login", "/auth/login?next=%2F%2Fevil.example"] {
        let plain = c.get(uri).await;
        assert!(
            !html_contains(&plain.body, &translate(Locale::En, "auth.login.required")),
            "{uri} must not show the notice"
        );
    }

    // After logging in with that next, the user is sent back there.
    let reply = c
        .login_next(ALICE, ALICE_PASSWORD, "/user/dashboard?x=1")
        .await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some("/user/dashboard?x=1"));
}

// ---------------------------------------------------------------------- AC7

#[sqlx::test(migrations = "../migrations")]
async fn ac7_unsafe_next_values_fall_back_to_the_dashboard(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let long = format!("/{}", "a".repeat(599));
    let bad = [
        "//evil.example/x",
        "/\\evil.example",
        "https://evil.example",
        "javascript:alert(1)",
        "/\r\nSet-Cookie:x",
        "/auth/login",
        "/api/login",
        long.as_str(),
    ];
    for next in bad {
        let mut c = Client::new(&app);
        let reply = c.login_next(ALICE, ALICE_PASSWORD, next).await;
        reply.ok();
        assert_eq!(
            reply.location().as_deref(),
            Some(DASHBOARD_PATH),
            "next={next:?}"
        );
    }

    let mut c = Client::new(&app);
    let reply = c
        .login_next(ALICE, ALICE_PASSWORD, "/groups/3?tab=wishlist")
        .await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some("/groups/3?tab=wishlist"));
}

// ---------------------------------------------------------------------- AC8

#[sqlx::test(migrations = "../migrations")]
async fn ac8_logged_in_visitors_skip_the_login_and_register_pages(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();

    assert_redirect_to(&c.get("/auth/login").await, DASHBOARD_PATH);
    assert_redirect_to(&c.get("/auth/register").await, DASHBOARD_PATH);
    // Login honours a valid next, ignores an invalid one.
    assert_redirect_to(
        &c.get("/auth/login?next=%2Fgroups%2F3%3Ftab%3Dwishlist")
            .await,
        "/groups/3?tab=wishlist",
    );
    assert_redirect_to(
        &c.get("/auth/login?next=%2F%2Fevil.example").await,
        DASHBOARD_PATH,
    );
    // The recovery pages stay reachable.
    assert_eq!(c.get("/auth/forgot-password").await.status, 200);
}

// ---------------------------------------------------------------------- AC9

#[sqlx::test(migrations = "../migrations")]
async fn ac9_login_from_a_foreign_origin_is_rejected(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app).origin("https://evil.example");

    let reply = c.login(ALICE, ALICE_PASSWORD).await;
    assert_eq!(reply.status, 403);
    assert!(reply.set_cookie(server::SESSION_COOKIE).is_none());
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 0, "no session created");
    c.origin = crate::common::TEST_ORIGIN.to_string();
    assert_eq!(c.whoami().await, None);
}

// --------------------------------------------------------------------- AC10

#[sqlx::test(migrations = "../migrations")]
async fn ac10_session_cookie_flags_and_no_remember_me(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;

    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    let reply = c.login(ALICE, ALICE_PASSWORD).await;
    let cookie = reply
        .set_cookie(server::SESSION_COOKIE)
        .expect("session cookie");
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(cookie.contains("SameSite=Lax"), "{cookie}");
    assert!(!cookie.contains("Secure"), "{cookie}");

    let https = "https://wichteln.example";
    let app = router(&pool, &[("APP_URL", https)], Opts::default());
    let mut c = Client::new(&app).origin(https);
    let reply = c.login(ALICE, ALICE_PASSWORD).await;
    let cookie = reply
        .set_cookie(server::SESSION_COOKIE)
        .expect("session cookie");
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"),
        "{cookie}"
    );
    assert!(cookie.contains("Secure"), "{cookie}");

    let page = Client::new(&app).get("/auth/login").await;
    assert_eq!(page.status, 200);
    let lower = page.body.to_lowercase();
    assert!(!lower.contains("remember"), "no remember-me control");
    assert!(
        !lower.contains("type=\"checkbox\""),
        "no checkbox on the login form"
    );
}

// --------------------------------------------------------------------- AC17

#[sqlx::test(migrations = "../migrations")]
async fn ac17_logout_ends_the_session_and_flashes_in_a_fresh_one(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    c.session_info().await; // take the login flash
    let old = c.session_cookie().unwrap().to_string();

    let reply = c.call("logout", &[]).await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some("/"));

    let info = c.session_info().await;
    assert!(info.user.is_none());
    assert_flash(&info, &FlashMessage::success("flash.success.logged_out"));

    // Replaying the old cookie is anonymous: a protected page sends it to login.
    let mut replay = Client::new(&app).with_cookie(server::SESSION_COOKIE, &old);
    assert_eq!(replay.whoami().await, None);
    let res = replay.get(DASHBOARD_PATH).await;
    assert!(res.status.is_redirection());
    assert!(res.location().unwrap().starts_with("/auth/login?next="));

    // Logout is idempotent and works anonymously.
    let mut anon = Client::new(&app);
    let reply = anon.call("logout", &[]).await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some("/"));
}

// --------------------------------------------------------------------- AC18

#[sqlx::test(migrations = "../migrations")]
async fn ac18_get_logout_is_not_routed_and_changes_nothing(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();

    let res = c.get("/auth/logout").await;
    assert_eq!(res.status, 404);
    assert_eq!(c.whoami().await.as_deref(), Some(ALICE), "still logged in");
}

// --------------------------------------------------------------------- AC19

#[sqlx::test(migrations = "../migrations")]
async fn ac19_idle_session_expires_and_protected_page_redirects_to_login(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[("INACTIVITY_LIMIT", "2")], Opts::default());
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    assert_eq!(c.get(DASHBOARD_PATH).await.status, 200);

    // The session store keeps the UTC expiry (`INACTIVITY_LIMIT` after the last request);
    // moving it into the past is "idle for longer than the limit".
    sqlx::query(
        "UPDATE sessions SET expiry_date = DATE_SUB(CURRENT_TIMESTAMP(6), INTERVAL 1 MINUTE)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let res = c.get(DASHBOARD_PATH).await;
    assert!(res.status.is_redirection(), "got {}", res.status);
    assert!(res.location().unwrap().starts_with("/auth/login?next="));
}

// --------------------------------------------------------------------- AC20

#[sqlx::test(migrations = "../migrations")]
async fn ac20_session_of_a_deleted_user_is_anonymous_without_an_error_page(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();

    let home = c.get("/").await;
    assert_eq!(home.status, 200, "pages render for such a session");
    let res = c.get(DASHBOARD_PATH).await;
    assert!(res.status.is_redirection(), "got {}", res.status);
    assert!(res.location().unwrap().starts_with("/auth/login?next="));
    assert_eq!(c.whoami().await, None);
}

// --------------------------------------------------------------------- AC47

#[sqlx::test(migrations = "../migrations")]
async fn ac47_login_adopts_the_account_language(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    set_user_locale(&pool, alice, Some("de")).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app).with_cookie("locale", "en");

    // Before login the browser's cookie wins: English.
    assert_eq!(lang_of(&c.get("/auth/login").await.body), "en");

    let reply = c.login(ALICE, ALICE_PASSWORD).await;
    reply.ok();
    assert_eq!(
        reply.set_cookie("locale").as_deref(),
        Some(locale_cookie(Locale::De, false).as_str()),
        "locale=de; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly"
    );
    assert_eq!(lang_of(&c.get(DASHBOARD_PATH).await.body), "de");

    // The session carries the language even if the cookie is stale.
    let sid = c.session_cookie().unwrap().to_string();
    let mut stale = Client::new(&app)
        .with_cookie(server::SESSION_COOKIE, &sid)
        .with_cookie("locale", "en");
    assert_eq!(lang_of(&stale.get(DASHBOARD_PATH).await.body), "de");

    // After logout the page is still German (the cookie stays).
    c.call("logout", &[]).await.ok();
    assert_eq!(lang_of(&c.get("/auth/login").await.body), "de");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac47_secure_flag_follows_https_app_url(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    set_user_locale(&pool, alice, Some("de")).await;
    let https = "https://wichteln.example";
    let app = router(&pool, &[("APP_URL", https)], Opts::default());
    let mut c = Client::new(&app).origin(https);
    let reply = c.login(ALICE, ALICE_PASSWORD).await;
    reply.ok();
    assert_eq!(
        reply.set_cookie("locale").as_deref(),
        Some(locale_cookie(Locale::De, true).as_str())
    );
    assert!(reply.set_cookie("locale").unwrap().ends_with("; Secure"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac47_account_without_language_keeps_the_page_language(pool: MySqlPool) {
    create_bob(&pool, BOB).await;
    let app = router(&pool, &[("APP_LOCALE", "de")], Opts::default());
    let mut c = Client::new(&app).with_cookie("locale", "en");

    let reply = c.login(BOB, BOB_PASSWORD).await;
    reply.ok();
    assert!(
        reply.set_cookie("locale").is_none(),
        "no locale cookie is set"
    );
    assert_eq!(lang_of(&c.get(DASHBOARD_PATH).await.body), "en");
}
