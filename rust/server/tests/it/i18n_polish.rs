//! i18n-polish: the POST language switch `set_locale` (AC1 to AC12). Named after the
//! acceptance criteria of docs/rewrite/specs/i18n-polish.md. The server fn is POSTed
//! through the real router at `/api/set_locale`; "plain form post" = `Accept: text/html`
//! (what a browser sends from a native `<form method="post">` without JavaScript).

use crate::common::auth::*;
use crate::common::groups::{de_with, en_with};
use crate::common::{post_from, send};
use app::error::AppError;
use app::i18n::{Locale, translate};
use app::server::i18n::locale_cookie;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, create_alice, create_bob, set_reset_token, user_row, user_rows,
};
use axum::http::{StatusCode, header};
use sqlx::MySqlPool;

use crate::common::account::{AccountReply, change_password};

const ALICE: &str = "alice@example.com";
const BOB: &str = "bob@example.com";
const DE_COOKIE: &str = "locale=de; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly";
const EN_COOKIE: &str = "locale=en; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly";

fn app(pool: &MySqlPool) -> axum::Router {
    router(pool, &[("APP_LOCALE", "en")], Opts::default())
}

/// `set_locale` as the server fn client calls it.
async fn switch(c: &mut Client, locale: &str, next: Option<&str>) -> Reply {
    match next {
        Some(next) => {
            c.call("set_locale", &[("locale", locale), ("next", next)])
                .await
        }
        None => c.call("set_locale", &[("locale", locale)]).await,
    }
}

/// `set_locale` as a native form post (no JS): `Accept: text/html`.
async fn switch_plain(c: &mut Client, locale: &str, next: Option<&str>) -> Reply {
    c.headers.push((
        "accept".to_string(),
        "text/html,application/xhtml+xml".to_string(),
    ));
    let reply = switch(c, locale, next).await;
    c.headers.pop();
    reply
}

fn locale_cookies(reply: &Reply) -> Vec<String> {
    reply
        .set_cookies()
        .into_iter()
        .filter(|c| c.starts_with("locale="))
        .collect()
}

fn app_error(reply: &Reply) -> AppError {
    assert!(
        !reply.status.is_success(),
        "expected an error, got {}",
        reply.body
    );
    serde_json::from_str(&reply.body).unwrap_or_else(|e| {
        panic!(
            "bad error body {:?} (status {}): {e}",
            reply.body, reply.status
        )
    })
}

async fn page_lang(c: &mut Client, path: &str) -> String {
    lang_of(&c.get(path).await.body).to_string()
}

/// The `<form>` elements of `html` that post to `/api/set_locale`.
fn switch_forms(html: &str) -> Vec<&str> {
    html.split("<form")
        .skip(1)
        .filter_map(|chunk| chunk.split("</form>").next())
        .filter(|form| form.contains("/api/set_locale"))
        .collect()
}

/// The `value` attribute of the `<input>` named `name` inside `form`.
fn input_value(form: &str, name: &str) -> Option<String> {
    let needle = format!("name=\"{name}\"");
    let tag_start = form
        .split("<input")
        .skip(1)
        .find(|tag| tag.split('>').next().is_some_and(|t| t.contains(&needle)))?;
    let tag = tag_start.split('>').next()?;
    let after = tag.split("value=\"").nth(1)?;
    Some(after.split('"').next()?.replace("&amp;", "&"))
}

// ---------------------------------------------------------------------- AC1

#[sqlx::test(migrations = "../migrations")]
async fn ac1_switch_sets_cookie_session_and_the_next_page_is_german(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    assert_eq!(page_lang(&mut c, "/").await, "en");

    let reply = switch(&mut c, "de", Some("/")).await;
    assert!(
        reply.status.is_success() || reply.status.is_redirection(),
        "{} {}",
        reply.status,
        reply.body
    );
    assert_eq!(locale_cookies(&reply), [DE_COOKIE]);
    assert!(!DE_COOKIE.contains("Secure"));

    let page = c.get("/").await;
    assert_eq!(lang_of(&page.body), "de");
    assert!(html_contains(
        &page.body,
        &translate(Locale::De, "home.title")
    ));
    // The session holds it too: without the locale cookie the page stays German.
    let mut no_cookie = Client::new(&app).with_cookie(
        server::SESSION_COOKIE,
        c.session_cookie().expect("session cookie"),
    );
    assert_eq!(page_lang(&mut no_cookie, "/").await, "de");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac1_cookie_is_secure_on_an_https_app_url(pool: MySqlPool) {
    let app = router(
        &pool,
        &[
            ("APP_URL", "https://wichteln.example"),
            ("APP_LOCALE", "en"),
        ],
        Opts::default(),
    );
    let mut c = Client::new(&app).origin("https://wichteln.example");
    let reply = switch(&mut c, "de", Some("/")).await;
    assert_eq!(locale_cookies(&reply), [locale_cookie(Locale::De, true)]);
    assert!(locale_cookies(&reply)[0].ends_with("; Secure"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac1_a_logged_out_switch_touches_no_user_row(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    create_bob(&pool, BOB).await;
    let before = user_rows(&pool).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    let reply = switch(&mut c, "de", Some("/")).await;
    assert_eq!(
        locale_cookies(&reply),
        [DE_COOKIE],
        "the switch itself works"
    );
    assert_eq!(user_rows(&pool).await, before);
}

// ---------------------------------------------------------------------- AC2

#[sqlx::test(migrations = "../migrations")]
async fn ac2_plain_form_post_redirects_302_and_switches_back_and_forth(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);

    let reply = switch_plain(&mut c, "de", Some("/auth/login")).await;
    assert_eq!(reply.status, StatusCode::FOUND, "{}", reply.body);
    assert_eq!(reply.location().as_deref(), Some("/auth/login"));
    assert_eq!(locale_cookies(&reply), [DE_COOKIE]);
    assert_eq!(page_lang(&mut c, "/auth/login").await, "de");

    let reply = switch_plain(&mut c, "en", Some("/auth/login")).await;
    assert_eq!(reply.status, StatusCode::FOUND);
    assert_eq!(reply.location().as_deref(), Some("/auth/login"));
    assert_eq!(locale_cookies(&reply), [EN_COOKIE]);
    assert_eq!(page_lang(&mut c, "/auth/login").await, "en");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac2_the_switch_queues_no_flash_message(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let reply = switch_plain(&mut c, "de", Some("/")).await;
    assert_eq!(reply.status, StatusCode::FOUND);
    assert!(c.session_info().await.flash.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac2_plain_form_post_with_an_unsupported_locale_does_not_redirect_to_a_foreign_target(
    pool: MySqlPool,
) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let reply = switch_plain(&mut c, "fr", Some("https://evil.example/")).await;
    assert!(
        reply.status != StatusCode::NOT_FOUND && reply.status != StatusCode::METHOD_NOT_ALLOWED,
        "the endpoint must exist: {}",
        reply.status
    );
    if let Some(location) = reply.location() {
        assert!(
            location.starts_with('/') && !location.starts_with("//"),
            "{location}"
        );
    }
    assert!(locale_cookies(&reply).is_empty());
}

// ---------------------------------------------------------------------- AC3

#[sqlx::test(migrations = "../migrations")]
async fn ac3_next_is_the_current_path_and_query_in_the_rendered_form(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let path = "/auth/login?next=%2Fjoin%2FABC234";
    let page = c.get(path).await;
    let forms = switch_forms(&page.body);
    assert_eq!(forms.len(), 1, "one switch form (the other locale)");
    assert_eq!(input_value(forms[0], "locale").as_deref(), Some("de"));
    assert_eq!(input_value(forms[0], "next").as_deref(), Some(path));

    // Submitting it returns to exactly that URL, in German.
    let reply = switch_plain(&mut c, "de", Some(path)).await;
    assert_eq!(reply.status, StatusCode::FOUND);
    assert_eq!(reply.location().as_deref(), Some(path));
    let page = c.get(path).await;
    assert_eq!(lang_of(&page.body), "de");
    let forms = switch_forms(&page.body);
    assert_eq!(input_value(forms[0], "locale").as_deref(), Some("en"));
    assert_eq!(input_value(forms[0], "next").as_deref(), Some(path));
}

// ---------------------------------------------------------------------- AC4

#[sqlx::test(migrations = "../migrations")]
async fn ac4_switching_on_a_reset_page_keeps_the_token_out_of_everything_but_next(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    let token = "T".repeat(43);
    set_reset_token(&pool, alice, &token, 1800).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    let path = format!("/auth/reset-password/{token}");

    let page = c.get(&path).await;
    assert_eq!(page.status, 200);
    // Referrer-Policy stays same-origin so the token URL never reaches another origin.
    assert_eq!(
        page.header("referrer-policy").as_deref(),
        Some("same-origin")
    );
    let forms = switch_forms(&page.body);
    assert_eq!(forms.len(), 1);
    assert!(
        !forms[0].contains("action=\"/api/set_locale?"),
        "no query on the action"
    );
    assert_eq!(
        input_value(forms[0], "next").as_deref(),
        Some(path.as_str())
    );

    let reply = switch_plain(&mut c, "de", Some(&path)).await;
    assert_eq!(reply.location().as_deref(), Some(path.as_str()));
    for cookie in reply.set_cookies() {
        assert!(!cookie.contains(&token), "token leaked into {cookie}");
    }
    assert_eq!(page_lang(&mut c, &path).await, "de");

    // The token still works.
    c.reset(&token, "brand-new-pass-1", "brand-new-pass-1")
        .await
        .ok();
    c.login(ALICE, "brand-new-pass-1").await.ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac4_the_switch_response_does_not_echo_next(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let secret = format!("/auth/reset-password/{}", "S".repeat(43));
    let reply = switch(&mut c, "de", Some(&secret)).await;
    assert_eq!(locale_cookies(&reply), [DE_COOKIE]);
    assert!(
        !reply.body.contains(&"S".repeat(43)),
        "body: {}",
        reply.body
    );
    for (name, value) in &reply.headers {
        if name != header::LOCATION {
            assert!(
                !value.to_str().unwrap_or("").contains(&"S".repeat(43)),
                "{name} leaks next"
            );
        }
    }
}

// ---------------------------------------------------------------------- AC5

#[sqlx::test(migrations = "../migrations")]
async fn ac5_next_is_validated_with_safe_return_path(pool: MySqlPool) {
    let app = app(&pool);
    let long_513 = format!("/{}", "a".repeat(512));
    let ok_512 = format!("/{}", "a".repeat(511));
    let cases: Vec<(&str, &str)> = vec![
        ("/groups/5?tab=a", "/groups/5?tab=a"),
        ("/auth/register", "/auth/register"),
        ("//evil.example/", "/"),
        ("https://evil.example/", "/"),
        ("/\\evil.example", "/"),
        ("/api/set_locale", "/"),
        ("/api", "/"),
        ("/a b", "/"),
        ("/a\r\nSet-Cookie: x=1", "/"),
        ("/a\tb", "/"),
        (&long_513, "/"),
        (&ok_512, &ok_512),
        ("", "/"),
        ("evil.example", "/"),
        ("javascript:alert(1)", "/"),
    ];
    for (next, expected) in cases {
        for plain in [false, true] {
            let mut c = Client::new(&app);
            let reply = if plain {
                switch_plain(&mut c, "de", Some(next)).await
            } else {
                switch(&mut c, "de", Some(next)).await
            };
            assert_eq!(
                reply.location().as_deref(),
                Some(expected),
                "next {next:?} (plain: {plain}), status {}",
                reply.status
            );
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_percent_encoded_slashes_are_not_decoded_into_a_protocol_relative_target(
    pool: MySqlPool,
) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    // The field value `%2F%2Fevil.example` is a literal `%2F%2Fevil...` after decoding the
    // form once: it does not start with `/`, so it falls back.
    let reply = c
        .call(
            "set_locale",
            &[("locale", "de"), ("next", "%2F%2Fevil.example")],
        )
        .await;
    assert_eq!(reply.location().as_deref(), Some("/"));
    // A leading-slash path with an encoded second slash stays a same-site path, never a host.
    let reply = switch(&mut c, "de", Some("/%2Fevil.example")).await;
    let location = reply.location().expect("location");
    assert!(
        location.starts_with('/') && !location.starts_with("//") && !location.starts_with("/\\")
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_missing_next_goes_to_root_and_the_referer_is_never_read(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app).header("referer", "https://evil.example/x");
    let reply = switch(&mut c, "de", None).await;
    assert_eq!(reply.location().as_deref(), Some("/"));
    let reply = switch_plain(&mut c, "de", None).await;
    assert_eq!(reply.location().as_deref(), Some("/"));
    // Not even a same-site Referer is used when next is absent.
    let mut c = Client::new(&app).header("referer", "http://localhost:3000/groups/5");
    let reply = switch(&mut c, "de", None).await;
    assert_eq!(reply.location().as_deref(), Some("/"));
}

// ---------------------------------------------------------------------- AC6

#[sqlx::test(migrations = "../migrations")]
async fn ac6_unsupported_locale_is_a_validation_error_and_changes_nothing(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    create_bob(&pool, BOB).await;
    let before = user_rows(&pool).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    // An earlier choice that must survive the rejected one.
    switch(&mut c, "de", Some("/")).await;

    for bad in ["fr", "", "xx", "../de", "de;en"] {
        let reply = switch(&mut c, bad, Some("/")).await;
        assert_eq!(
            app_error(&reply),
            AppError::Validation("validation.locale_unsupported".into()),
            "{bad:?}"
        );
        assert!(locale_cookies(&reply).is_empty(), "{bad:?} set a cookie");
        assert!(reply.location().is_none(), "{bad:?} redirected");
    }
    assert_eq!(
        page_lang(&mut c, "/").await,
        "de",
        "session locale unchanged"
    );
    assert_eq!(user_rows(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_unsupported_locale_on_a_fresh_session_sets_no_locale_cookie(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let reply = switch(&mut c, "fr", Some("/")).await;
    assert_eq!(
        app_error(&reply),
        AppError::Validation("validation.locale_unsupported".into())
    );
    assert!(locale_cookies(&reply).is_empty());
    assert_eq!(page_lang(&mut c, "/").await, "en");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_a_regional_code_maps_to_its_language(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let reply = switch(&mut c, "de-AT", Some("/")).await;
    assert_eq!(
        locale_cookies(&reply),
        [DE_COOKIE],
        "canonical code in the cookie"
    );
    assert_eq!(page_lang(&mut c, "/").await, "de");
}

// ---------------------------------------------------------------------- AC7

#[sqlx::test(migrations = "../migrations")]
async fn ac7_a_foreign_origin_is_rejected_and_changes_nothing(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    let before = user_rows(&pool).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    let mut evil = c.fork().origin("https://evil.example");
    let reply = switch(&mut evil, "de", Some("/")).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(locale_cookies(&reply).is_empty());
    assert_eq!(page_lang(&mut c, "/auth/login").await, "en");
    assert_eq!(user_rows(&pool).await, before);
    assert_eq!(user_row(&pool, alice).await.locale, None);

    // No Origin header at all is rejected as well.
    let cookie = format!("{}={}", server::SESSION_COOKIE, c.session_cookie().unwrap());
    let req = post_from(None, "/api/set_locale", Some(&cookie), "locale=de&next=%2F");
    let res = send(&app, req).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(user_rows(&pool).await, before);
}

// ---------------------------------------------------------------------- AC8

#[sqlx::test(migrations = "../migrations")]
async fn ac8_get_language_route_is_gone(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let reply = c.get("/language/de").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert!(locale_cookies(&reply).is_empty());
    assert_eq!(page_lang(&mut c, "/").await, "en");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_get_on_the_server_fn_endpoint_changes_nothing(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let before = user_rows(&pool).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    let reply = c.get("/api/set_locale?locale=de&next=/").await;
    assert!(
        !reply.status.is_success() && !reply.status.is_redirection(),
        "got {}",
        reply.status
    );
    assert!(locale_cookies(&reply).is_empty());
    assert_eq!(page_lang(&mut c, "/").await, "en");
    assert_eq!(user_rows(&pool).await, before);
}

// ---------------------------------------------------------------------- AC9

#[sqlx::test(migrations = "../migrations")]
async fn ac9_a_logged_in_switch_updates_only_the_callers_account_language(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    let bob = create_bob(&pool, BOB).await;
    let bob_before = user_row(&pool, bob).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    assert_eq!(user_row(&pool, alice).await.locale, None);

    let reply = switch(&mut c, "de", Some("/user/dashboard")).await;
    assert_eq!(locale_cookies(&reply), [DE_COOKIE]);
    assert_eq!(user_row(&pool, alice).await.locale.as_deref(), Some("de"));
    assert_eq!(user_row(&pool, bob).await, bob_before);

    // And back: the canonical code is stored, also for a regional spelling.
    switch(&mut c, "en", Some("/")).await;
    assert_eq!(user_row(&pool, alice).await.locale.as_deref(), Some("en"));
    switch(&mut c, "DE-at", Some("/")).await;
    assert_eq!(user_row(&pool, alice).await.locale.as_deref(), Some("de"));
    assert_eq!(user_row(&pool, bob).await, bob_before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_a_rejected_switch_leaves_the_account_language(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    app::test_support::set_user_locale(&pool, alice, Some("de")).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    let reply = switch(&mut c, "fr", Some("/")).await;
    assert_eq!(
        app_error(&reply),
        AppError::Validation("validation.locale_unsupported".into())
    );
    assert_eq!(user_row(&pool, alice).await.locale.as_deref(), Some("de"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_a_stale_session_sets_cookie_and_session_but_not_the_account(pool: MySqlPool) {
    let mut w = crate::common::account::alice_world_with(&pool, &[], Opts::default()).await;
    // Device 1 = w.c (logged in). Device 2 changes the password: epoch bump.
    let mut other = Client::new(&w.app);
    other.login(&w.email, ALICE_PASSWORD).await.ok();
    change_password(
        &mut other,
        ALICE_PASSWORD,
        "another-pass-1",
        "another-pass-1",
    )
    .await
    .account_ok();
    let before = user_row(&pool, w.id).await;
    assert_eq!(w.c.whoami().await, None, "the old session is stale");

    let reply = switch(&mut w.c, "de", Some("/")).await;
    assert_eq!(locale_cookies(&reply), [DE_COOKIE]);
    assert_eq!(page_lang(&mut w.c, "/").await, "de");
    assert_eq!(user_row(&pool, w.id).await, before, "users row unchanged");
    assert_eq!(before.locale, None);
}

// --------------------------------------------------------------------- AC10

#[sqlx::test(migrations = "../migrations")]
async fn ac10_the_cookie_alone_keeps_the_choice_when_the_session_is_gone(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let reply = switch(&mut c, "de", Some("/")).await;
    let cookie = locale_cookies(&reply).remove(0);
    let value = cookie
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_string();
    // Session record "deleted": a browser with the locale cookie only.
    let mut fresh = Client::new(&app).with_cookie("locale", &value);
    assert_eq!(fresh.session_cookie(), None);
    assert_eq!(page_lang(&mut fresh, "/").await, "de");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_the_page_after_logout_is_german(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    switch(&mut c, "de", Some("/")).await;
    c.call("logout", &[]).await;
    assert_eq!(c.whoami().await, None);
    assert_eq!(page_lang(&mut c, "/").await, "de");
    assert_eq!(page_lang(&mut c, "/auth/login").await, "de");
}

// --------------------------------------------------------------------- AC11

#[sqlx::test(migrations = "../migrations")]
async fn ac11_the_switch_button_is_server_rendered_outside_the_collapsed_menu(pool: MySqlPool) {
    let app = app(&pool);
    for (cookie_locale, other_code, other_name, current_lang) in [
        ("en", "de", "Deutsch", Locale::En),
        ("de", "en", "English", Locale::De),
    ] {
        let mut c = Client::new(&app).with_cookie("locale", cookie_locale);
        for path in ["/", "/auth/login", "/auth/register"] {
            let html = c.get(path).await.body;
            assert!(!html.contains("languageDropdown"), "{path}");
            assert!(!html.contains("/language/"), "{path}");
            let forms = switch_forms(&html);
            assert_eq!(forms.len(), 1, "{path}: only the other locale is offered");
            let form = forms[0];
            assert!(form.contains("method=\"post\""), "{form}");
            assert!(form.contains("action=\"/api/set_locale\""), "{form}");
            assert_eq!(input_value(form, "locale").as_deref(), Some(other_code));
            assert_eq!(input_value(form, "next").as_deref(), Some(path));
            assert!(form.contains("type=\"submit\""));
            // `lang` sits on a span around the native name only (user decision
            // 2026-11-02), so the page-language aria-label keeps its pronunciation.
            assert!(
                form.contains(&format!(
                    "<span lang=\"{other_code}\">{other_name}</span></button>"
                )),
                "{form}"
            );
            assert!(!form.contains("<button lang="), "{form}");
            let label = if current_lang == Locale::En {
                en_with("language.switchTo", &[("language", other_name)])
            } else {
                de_with("language.switchTo", &[("language", other_name)])
            };
            assert!(form.contains(&format!("aria-label=\"{label}\"")), "{form}");

            // Outside `.navbar-collapse`: the form precedes the collapse container.
            let form_at = html.find("/api/set_locale").unwrap();
            let collapse_at = html.find("navbar-collapse").expect("collapse container");
            assert!(
                form_at < collapse_at,
                "{path}: switch must precede .navbar-collapse"
            );
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_the_button_is_there_for_logged_in_users_too(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    let html = c.get("/user/dashboard").await.body;
    assert_eq!(switch_forms(&html).len(), 1);
    assert!(!html.contains("languageDropdown"));
}

// --------------------------------------------------------------------- AC12

#[sqlx::test(migrations = "../migrations")]
async fn ac12_a_switch_on_one_device_reaches_mails_at_once_and_other_devices_at_login(
    pool: MySqlPool,
) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("alice");
    let alice = create_alice(&pool, &email).await;
    let app = router(&pool, &[("APP_LOCALE", "en")], Opts::mail());

    let mut one = Client::new(&app).with_cookie("locale", "en");
    one.login(&email, ALICE_PASSWORD).await.ok();
    let mut two = Client::new(&app).with_cookie("locale", "en");
    two.login(&email, ALICE_PASSWORD).await.ok();
    assert_eq!(page_lang(&mut two, "/user/dashboard").await, "en");

    switch(&mut one, "de", Some("/user/dashboard")).await;
    assert_eq!(user_row(&pool, alice).await.locale.as_deref(), Some("de"));
    assert_eq!(page_lang(&mut one, "/user/dashboard").await, "de");
    // Device 2 keeps its session locale until the next login.
    assert_eq!(page_lang(&mut two, "/user/dashboard").await, "en");

    // A reset requested from an English page: the mail is German (account language).
    let mut anon = Client::new(&app).with_cookie("locale", "en");
    anon.forgot(&email).await.ok();
    let mail = mailpit.wait_for_latest(&email).await;
    assert_eq!(
        mail.subject,
        translate(Locale::De, "email.passwordReset.subject")
    );
    assert!(mail.html.contains("lang=\"de\""), "{}", mail.html);

    // Logging in on a device whose cookie says en adopts the account language.
    let mut three = Client::new(&app).with_cookie("locale", "en");
    three.login(&email, BOB_PASSWORD).await; // wrong password: nothing adopted
    assert_eq!(page_lang(&mut three, "/").await, "en");
    three.login(&email, ALICE_PASSWORD).await.ok();
    assert_eq!(page_lang(&mut three, "/user/dashboard").await, "de");
}

// A call without the `locale` field cannot be deserialised: never a 500, and the Referer
// is not a redirect target.
#[sqlx::test(migrations = "../migrations")]
async fn ac5_a_post_without_the_locale_field_is_a_404_and_never_follows_the_referer(
    pool: MySqlPool,
) {
    let app = app(&pool);
    // Plain form post (Accept: text/html).
    let mut c = Client::new(&app)
        .header("referer", "https://evil.example/x")
        .header("accept", "text/html,application/xhtml+xml");
    let reply = c.call("set_locale", &[("next", "/groups")]).await;
    assert_ne!(
        reply.status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "{}",
        reply.body
    );
    assert!(
        reply.location().is_none_or(|l| !l.contains("evil.example")),
        "{:?}",
        reply.location()
    );
    assert!(locale_cookies(&reply).is_empty());
    // The JS client: 404, like the other slices' undeserialisable arguments.
    let mut js = Client::new(&app).header("referer", "https://evil.example/x");
    let reply = js.call("set_locale", &[("next", "/groups")]).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND, "{}", reply.body);
    assert!(reply.location().is_none());
}
