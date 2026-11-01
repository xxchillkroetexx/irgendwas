//! i18n-foundation: locale resolution per request (AC10), cookie and session helpers
//! (AC11), `users.locale` (AC12) and the account/mail locale helpers (AC13, AC14).

mod common;

use app::error::AppError;
use app::i18n::{LOCALE_KEY, Locale, translate};
use app::server::i18n::{
    LOCALE_COOKIE_MAX_AGE, account_locale, adopt_account_locale, load_account_locale,
    locale_cookie, mail_i18n, remember_locale, session_locale,
};
use app::server::{AppState, Config};
use app::test_support::{create_user, set_user_locale, test_config};
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{Request, header};
use axum::response::IntoResponse;
use axum::routing::get as route;
use common::*;
use sqlx::MySqlPool;
use tower_sessions::Session;

// ------------------------------------------------------------------- helpers

fn test_routes() -> Router<AppState> {
    Router::new()
        // Session value `locale` set directly (as a past `remember_locale` would have).
        .route(
            "/test/session-locale/{code}",
            route(|session: Session, Path(code): Path<String>| async move {
                session.insert(LOCALE_KEY, code).await.unwrap();
                "ok"
            }),
        )
        .route(
            "/test/read-session-locale",
            route(|session: Session| async move {
                match session_locale(&session).await.unwrap() {
                    Some(locale) => locale.code().to_string(),
                    None => "none".to_string(),
                }
            }),
        )
        // What auth's `login` does with the stored account language.
        .route(
            "/test/adopt/{id}",
            route(
                |session: Session, State(state): State<AppState>, Path(id): Path<i32>| async move {
                    let stored: Option<String> =
                        sqlx::query_scalar("SELECT locale FROM users WHERE id = ?")
                            .bind(id)
                            .fetch_one(&state.pool)
                            .await
                            .unwrap();
                    let cookie = adopt_account_locale(
                        &session,
                        stored.as_deref(),
                        state.config.secure_cookies(),
                    )
                    .await
                    .unwrap();
                    match cookie {
                        Some(cookie) => ([(header::SET_COOKIE, cookie)], "adopted").into_response(),
                        None => "unchanged".into_response(),
                    }
                },
            ),
        )
        .route(
            "/test/remember/{code}",
            route(
                |session: Session, State(state): State<AppState>, Path(code): Path<String>| async move {
                    let locale = Locale::from_code(&code).unwrap();
                    let cookie =
                        remember_locale(&session, locale, state.config.secure_cookies())
                            .await
                            .unwrap();
                    ([(header::SET_COOKIE, cookie)], "remembered").into_response()
                },
            ),
        )
}

fn app(pool: &MySqlPool, vars: &[(&str, &str)]) -> Router {
    server::router_with(state(pool.clone(), config_with(vars)), test_routes())
}

fn get_with_headers(
    uri: &str,
    cookie: Option<&str>,
    accept_language: &[&str],
) -> Request<axum::body::Body> {
    let mut builder = Request::get(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    for value in accept_language {
        builder = builder.header(header::ACCEPT_LANGUAGE, *value);
    }
    builder.body(axum::body::Body::empty()).unwrap()
}

async fn home(app: &Router, cookie: Option<&str>, accept_language: &[&str]) -> String {
    let res = send(app, get_with_headers("/", cookie, accept_language)).await;
    body_string(res).await
}

fn lang_of(html: &str) -> &str {
    let start = html.find("<html lang=\"").expect("<html lang>") + "<html lang=\"".len();
    let rest = &html[start..];
    &rest[..rest.find('"').unwrap()]
}

fn locale_cookies(res: &axum::http::Response<axum::body::Body>) -> Vec<String> {
    set_cookies(res)
        .into_iter()
        .filter(|c| c.starts_with("locale="))
        .collect()
}

// ---------------------------------------------------------------------- AC10

#[sqlx::test(migrations = "../migrations")]
async fn ac10_accept_language_selects_the_page_locale(pool: MySqlPool) {
    let app = app(&pool, &[("APP_LOCALE", "en")]);
    let html = home(&app, None, &["de-DE,de;q=0.9"]).await;
    assert_eq!(lang_of(&html), "de");
    let title = translate(Locale::De, "home.title");
    assert!(
        html.contains(&format!(">{title}</h1>")),
        "German home.title expected"
    );
    assert!(html.contains(&translate(Locale::De, "home.subtitle")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_unsupported_accept_language_falls_back_to_app_locale(pool: MySqlPool) {
    let app_de = app(&pool, &[("APP_LOCALE", "de")]);
    assert_eq!(lang_of(&home(&app_de, None, &["fr"]).await), "de");
    // no header at all
    assert_eq!(lang_of(&home(&app_de, None, &[]).await), "de");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_accept_language_beats_app_locale(pool: MySqlPool) {
    let app_de = app(&pool, &[("APP_LOCALE", "de")]);
    assert_eq!(lang_of(&home(&app_de, None, &["en-US"]).await), "en");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_cookie_beats_accept_language(pool: MySqlPool) {
    let app = app(&pool, &[("APP_LOCALE", "de")]);
    assert_eq!(lang_of(&home(&app, Some("locale=en"), &["de"]).await), "en");
    // an invalid cookie value is skipped
    assert_eq!(lang_of(&home(&app, Some("locale=xx"), &["en"]).await), "en");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_multiple_accept_language_headers_are_joined(pool: MySqlPool) {
    let app = app(&pool, &[("APP_LOCALE", "en")]);
    let html = home(&app, None, &["fr", "de"]).await;
    assert_eq!(lang_of(&html), "de");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_session_locale_beats_cookie_and_accept_language(pool: MySqlPool) {
    let app = app(&pool, &[("APP_LOCALE", "en")]);
    let res = send(&app, get("/test/session-locale/de", None)).await;
    let session = session_cookie(&res).expect("session cookie");

    let cookie_header = format!("{session}; locale=en");
    let html = home(&app, Some(&cookie_header), &["en-US"]).await;
    assert_eq!(lang_of(&html), "de");
}

// ---------------------------------------------------------------------- AC11

#[test]
fn ac11_locale_cookie_format() {
    assert_eq!(LOCALE_COOKIE_MAX_AGE, 31_536_000);
    assert_eq!(
        locale_cookie(Locale::De, false),
        "locale=de; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly"
    );
    assert_eq!(
        locale_cookie(Locale::En, false),
        "locale=en; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly"
    );
    let secure = locale_cookie(Locale::De, true);
    assert!(secure.ends_with("; Secure"), "{secure}");
    assert!(secure.starts_with("locale=de; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_adopt_account_locale_sets_cookie_and_session(pool: MySqlPool) {
    let app = app(
        &pool,
        &[("APP_URL", "http://localhost:3000"), ("APP_LOCALE", "en")],
    );
    let user = create_user(&pool, "Dora", "dora@example.com").await;
    set_user_locale(&pool, user, Some("de")).await;

    let res = send(&app, get(&format!("/test/adopt/{user}"), None)).await;
    assert_eq!(
        locale_cookies(&res),
        ["locale=de; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly"]
    );
    let session = session_cookie(&res).expect("session cookie");

    // Only the session cookie and an English browser: the session's locale wins.
    let html = home(&app, Some(&session), &["en"]).await;
    assert_eq!(lang_of(&html), "de");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_adopt_account_locale_cookie_is_secure_on_https(pool: MySqlPool) {
    let app = app(
        &pool,
        &[
            ("APP_URL", "https://wichteln.example"),
            ("APP_LOCALE", "en"),
        ],
    );
    let user = create_user(&pool, "Dora", "dora@example.com").await;
    set_user_locale(&pool, user, Some("de")).await;

    let res = send(&app, get(&format!("/test/adopt/{user}"), None)).await;
    let cookies = locale_cookies(&res);
    assert_eq!(cookies.len(), 1, "{cookies:?}");
    assert!(cookies[0].ends_with("; Secure"), "{}", cookies[0]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_adopt_account_locale_ignores_null_unknown_and_empty(pool: MySqlPool) {
    let app = app(&pool, &[("APP_LOCALE", "en")]);
    for (index, stored) in [None, Some("fr"), Some("")].into_iter().enumerate() {
        let user = create_user(&pool, "Eve", &format!("eve{index}@example.com")).await;
        set_user_locale(&pool, user, stored).await;

        // Start from a session whose locale is `en`.
        let res = send(&app, get("/test/session-locale/en", None)).await;
        let session = session_cookie(&res).expect("session cookie");

        let res = send(&app, get(&format!("/test/adopt/{user}"), Some(&session))).await;
        assert!(
            locale_cookies(&res).is_empty(),
            "stored {stored:?} must not set a locale cookie: {:?}",
            set_cookies(&res)
        );
        let res = send(&app, get("/test/read-session-locale", Some(&session))).await;
        assert_eq!(body_string(res).await, "en", "stored {stored:?}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_remember_locale_returns_the_cookie_and_stores_the_session(pool: MySqlPool) {
    let app = app(&pool, &[("APP_LOCALE", "en")]);
    let res = send(&app, get("/test/remember/de", None)).await;
    assert_eq!(
        locale_cookies(&res),
        ["locale=de; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly"]
    );
    let session = session_cookie(&res).expect("session cookie");
    let res = send(&app, get("/test/read-session-locale", Some(&session))).await;
    assert_eq!(body_string(res).await, "de");
}

// ---------------------------------------------------------------------- AC12

#[sqlx::test(migrations = "../migrations")]
async fn ac12_users_locale_column_is_nullable_varchar8(pool: MySqlPool) {
    let row: (String, String, Option<String>) = sqlx::query_as(
        "SELECT CAST(column_type AS CHAR), CAST(is_nullable AS CHAR), CAST(column_default AS CHAR) \
         FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = 'users' AND column_name = 'locale'",
    )
    .fetch_one(&pool)
    .await
    .expect("users.locale must exist");
    assert_eq!(row.0.to_lowercase(), "varchar(8)");
    assert_eq!(row.1, "YES");
    // MySQL reports a NULL default as NULL, MariaDB as the string 'NULL'.
    assert!(
        row.2
            .as_deref()
            .is_none_or(|d| d.eq_ignore_ascii_case("null")),
        "default must be NULL, got {:?}",
        row.2
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_fixture_users_start_with_null_and_set_user_locale_stores(pool: MySqlPool) {
    let user = create_user(&pool, "Finn", "finn@example.com").await;
    let stored: Option<String> = sqlx::query_scalar("SELECT locale FROM users WHERE id = ?")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, None);

    set_user_locale(&pool, user, Some("de")).await;
    let stored: Option<String> = sqlx::query_scalar("SELECT locale FROM users WHERE id = ?")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored.as_deref(), Some("de"));

    set_user_locale(&pool, user, None).await;
    let stored: Option<String> = sqlx::query_scalar("SELECT locale FROM users WHERE id = ?")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, None);
}

// ---------------------------------------------------------------------- AC13

#[test]
fn ac13_account_locale() {
    use Locale::{De, En};
    assert_eq!(account_locale(None, En), En);
    assert_eq!(account_locale(Some("de"), En), De);
    assert_eq!(account_locale(Some("DE-at"), En), De);
    assert_eq!(account_locale(Some("fr"), De), De);
    assert_eq!(account_locale(Some(""), En), En);
}

#[test]
fn ac13_mail_i18n_uses_account_locale_and_app_timezone() {
    let config: Config = test_config();
    let i18n = mail_i18n(Some("de"), &config);
    assert_eq!(i18n.locale(), Locale::De);
    assert_eq!(i18n.tz(), config.app_timezone);

    let config = config_with(&[("APP_LOCALE", "de"), ("APP_TIMEZONE", "America/New_York")]);
    let i18n = mail_i18n(None, &config);
    assert_eq!(i18n.locale(), Locale::De);
    assert_eq!(i18n.tz(), chrono_tz::America::New_York);

    // Unknown stored values behave like NULL: APP_LOCALE.
    assert_eq!(mail_i18n(Some("fr"), &config).locale(), Locale::De);
    // A stored language beats APP_LOCALE.
    assert_eq!(mail_i18n(Some("en"), &config).locale(), Locale::En);
}

// ---------------------------------------------------------------------- AC14

#[sqlx::test(migrations = "../migrations")]
async fn ac14_load_account_locale_returns_the_raw_value(pool: MySqlPool) {
    let german = create_user(&pool, "Gerd", "gerd@example.com").await;
    let none = create_user(&pool, "Nora", "nora@example.com").await;
    let french = create_user(&pool, "Fabien", "fabien@example.com").await;
    set_user_locale(&pool, german, Some("de")).await;
    set_user_locale(&pool, french, Some("fr")).await;

    assert_eq!(
        load_account_locale(&pool, german).await,
        Ok(Some("de".to_string()))
    );
    assert_eq!(load_account_locale(&pool, none).await, Ok(None));
    assert_eq!(
        load_account_locale(&pool, french).await,
        Ok(Some("fr".to_string()))
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_load_account_locale_unknown_user_is_not_found(pool: MySqlPool) {
    assert_eq!(
        load_account_locale(&pool, 987_654).await,
        Err(AppError::NotFound)
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_load_account_locale_works_inside_a_transaction(pool: MySqlPool) {
    let user = create_user(&pool, "Tina", "tina@example.com").await;
    set_user_locale(&pool, user, Some("de")).await;
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        load_account_locale(&mut *tx, user).await,
        Ok(Some("de".to_string()))
    );
}
