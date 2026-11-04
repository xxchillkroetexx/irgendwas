//! Sessions (MySQL store, cookie flags, inactivity expiry), current-user resolution,
//! flash messages and locale, through the real router.

use crate::common::*;
use app::flash::FlashMessage;
use app::server::{AppState, auth, flash};
use app::test_support::create_user;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get as route;
use chrono::{DateTime, Utc};
use sqlx::MySqlPool;
use tower_sessions::Session;

fn test_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/test/login/{id}",
            route(|session: Session, Path(id): Path<i32>| async move {
                auth::login(&session, id).await.unwrap();
                "ok"
            }),
        )
        .route(
            "/test/me",
            route(
                |session: Session, State(state): State<AppState>| async move {
                    match auth::load_user(&session, &state.pool).await.unwrap() {
                        Some(user) => format!("user:{}:{}", user.id, user.name),
                        None => "anonymous".to_string(),
                    }
                },
            ),
        )
        .route(
            "/test/logout",
            route(|session: Session| async move {
                auth::logout(&session).await.unwrap();
                "ok"
            }),
        )
        .route(
            "/test/flash",
            route(|session: Session| async move {
                flash::push(&session, FlashMessage::info("error.forbidden"))
                    .await
                    .unwrap();
                "ok"
            }),
        )
}

fn app(pool: &MySqlPool, vars: &[(&str, &str)]) -> Router {
    server::router_with(state(pool.clone(), config_with(vars)), test_routes())
}

#[sqlx::test(migrations = "../migrations")]
async fn session_login_current_user_and_flash(pool: MySqlPool) {
    let app = app(
        &pool,
        &[
            ("APP_URL", "http://localhost:3000"),
            ("INACTIVITY_LIMIT", "1200"),
        ],
    );
    let alice = create_user(&pool, "Alice", "alice@example.com").await;

    // Anonymous: no session cookie is set for an empty session.
    let res = send(&app, get("/test/me", None)).await;
    assert!(session_cookie(&res).is_none());
    assert_eq!(body_string(res).await, "anonymous");

    // Login sets an HttpOnly, SameSite=Lax cookie (not Secure on http) that expires
    // after INACTIVITY_LIMIT seconds.
    let res = send(&app, get(&format!("/test/login/{alice}"), None)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let raw = set_cookies(&res)
        .into_iter()
        .find(|c| c.starts_with(server::SESSION_COOKIE))
        .unwrap();
    assert!(raw.contains("HttpOnly"), "{raw}");
    assert!(raw.contains("SameSite=Lax"), "{raw}");
    assert!(!raw.contains("Secure"), "{raw}");
    assert!(raw.contains("Max-Age=1200"), "{raw}");
    let cookie = session_cookie(&res).unwrap();

    // The session lives in the `sessions` table with a UTC expiry ~20 minutes ahead.
    let expiry: DateTime<Utc> = sqlx::query_scalar("SELECT expiry_date FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let ahead = (expiry - Utc::now()).num_seconds();
    assert!((1100..=1200).contains(&ahead), "expiry {ahead}s ahead");

    // The cookie identifies the user.
    let res = send(&app, get("/test/me", Some(&cookie))).await;
    assert_eq!(body_string(res).await, format!("user:{alice}:Alice"));

    // The layout's server fn sees the same user (same-origin POST, see `origin.rs`).
    let res = send(&app, post("/api/session_info", Some(&cookie), "")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let info: serde_json::Value = serde_json::from_str(&body_string(res).await).unwrap();
    assert_eq!(info["user"]["name"], "Alice");

    // The rendered page shows the user in the navbar.
    let html = body_string(send(&app, get("/", Some(&cookie))).await).await;
    assert!(html.contains("Alice"), "navbar should show the user name");
    assert!(html.contains("/user/dashboard"));

    // A flash message is shown on the next page exactly once.
    send(&app, get("/test/flash", Some(&cookie))).await;
    let html = body_string(send(&app, get("/", Some(&cookie))).await).await;
    assert!(
        html.contains("You are not allowed to do that."),
        "flash missing"
    );
    let html = body_string(send(&app, get("/", Some(&cookie))).await).await;
    assert!(
        !html.contains("You are not allowed to do that."),
        "flash shown twice"
    );

    // Logout ends the login.
    let res = send(&app, get("/test/logout", Some(&cookie))).await;
    let cookie_after = session_cookie(&res).unwrap_or(cookie.clone());
    let res = send(&app, get("/test/me", Some(&cookie_after))).await;
    assert_eq!(body_string(res).await, "anonymous");
    let res = send(&app, get("/test/me", Some(&cookie))).await;
    assert_eq!(
        body_string(res).await,
        "anonymous",
        "old session id must be dead"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn session_of_deleted_user_counts_as_logged_out(pool: MySqlPool) {
    let app = app(&pool, &[]);
    let bob = create_user(&pool, "Bob", "bob@example.com").await;
    let res = send(&app, get(&format!("/test/login/{bob}"), None)).await;
    let cookie = session_cookie(&res).unwrap();

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(bob)
        .execute(&pool)
        .await
        .unwrap();

    let res = send(&app, get("/test/me", Some(&cookie))).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_string(res).await, "anonymous");
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0, "stale session must be cleared");

    // Pages still render for such a session.
    let res = send(&app, get("/", Some(&cookie))).await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "../migrations")]
async fn https_app_url_sets_secure_cookie(pool: MySqlPool) {
    let app = app(
        &pool,
        &[
            ("APP_URL", "https://wichteln.example"),
            ("INACTIVITY_LIMIT", "60"),
        ],
    );
    let carol = create_user(&pool, "Carol", "carol@example.com").await;
    let res = send(&app, get(&format!("/test/login/{carol}"), None)).await;
    let raw = set_cookies(&res)
        .into_iter()
        .find(|c| c.starts_with(server::SESSION_COOKIE))
        .unwrap();
    assert!(raw.contains("Secure"), "{raw}");
    assert!(raw.contains("Max-Age=60"), "{raw}");
}

#[sqlx::test(migrations = "../migrations")]
async fn health_layout_404_and_locale(pool: MySqlPool) {
    let app = app(&pool, &[("APP_LOCALE", "en")]);

    let res = send(&app, get("/health", None)).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(body_string(res).await.contains("\"status\":\"ok\""));

    let res = send(&app, get("/", None)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let html = body_string(res).await;
    assert!(html.contains("<html lang=\"en\" data-timezone=\"Europe/Berlin\">"));
    assert!(html.contains("navbar"));
    assert!(html.contains("/auth/login"));
    assert!(html.contains("bootstrap@5.3.0"));

    let res = send(&app, get("/does-not-exist", None)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(body_string(res).await.contains("Page not found"));

    // The `locale` cookie selects German.
    let html = body_string(send(&app, get("/", Some("locale=de"))).await).await;
    assert!(html.contains("<html lang=\"de\""));
    assert!(
        html.contains("Anmelden") || html.contains("Registrieren"),
        "German nav expected"
    );
}
