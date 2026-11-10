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
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::SessionStore;

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
async fn identical_flash_messages_receive_distinct_ids_and_remain_readable(pool: MySqlPool) {
    let app = app(&pool, &[]);
    let alice = create_user(&pool, "Alice", "flash-ids@example.com").await;
    let cookie =
        session_cookie(&send(&app, get(&format!("/test/login/{alice}"), None)).await).unwrap();
    for _ in 0..2 {
        assert_eq!(
            send(&app, get("/test/flash", Some(&cookie))).await.status(),
            StatusCode::OK
        );
    }
    let bytes: Vec<u8> = sqlx::query_scalar("SELECT data FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let stored: Record = serde_json::from_slice(&bytes).unwrap();
    let entries = stored.data["flash"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0]["id"].as_str().is_some());
    assert_ne!(entries[0]["id"], entries[1]["id"]);
    let response = send(&app, post("/api/session_info", Some(&cookie), "")).await;
    let info: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(info["flash"].as_array().unwrap().len(), 2);
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

#[sqlx::test(migrations = "../migrations")]
async fn dynamic_responses_are_not_stored_across_sessions_or_locales(pool: MySqlPool) {
    use axum::http::header::CACHE_CONTROL;

    let app = app(&pool, &[]);
    let alice = create_user(&pool, "Alice", "cache-alice@example.com").await;
    let bob = create_user(&pool, "Bob", "cache-bob@example.com").await;
    let alice_cookie =
        session_cookie(&send(&app, get(&format!("/test/login/{alice}"), None)).await).unwrap();
    let bob_cookie =
        session_cookie(&send(&app, get(&format!("/test/login/{bob}"), None)).await).unwrap();

    for (path, cookie, marker, other_user) in [
        ("/", None, Some("<html lang=\"en\""), None),
        ("/", Some("locale=de"), Some("<html lang=\"de\""), None),
        (
            "/groups",
            Some(alice_cookie.as_str()),
            Some("Alice"),
            Some("Bob"),
        ),
        (
            "/groups",
            Some(bob_cookie.as_str()),
            Some("Bob"),
            Some("Alice"),
        ),
        ("/groups", None, None, None),
        ("/does-not-exist", Some(alice_cookie.as_str()), None, None),
    ] {
        let response = send(&app, get(path, cookie)).await;
        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "no-store",
            "{path}"
        );
        let body = body_string(response).await;
        if let Some(marker) = marker {
            assert!(body.contains(marker), "{path}: missing {marker}");
        }
        if let Some(other_user) = other_user {
            assert!(
                !body.contains(other_user),
                "{path}: contains another user's name"
            );
        }
    }
    for cookie in [&alice_cookie, &bob_cookie] {
        let response = send(&app, post("/api/session_info", Some(cookie), "")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ordinary_static_assets_remain_cacheable(pool: MySqlPool) {
    use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};

    let root = std::env::temp_dir().join(format!("irgendwas-cache-{}", rand::random::<u64>()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("fixture.css"), "body { color: red; }").unwrap();
    let options = leptos::config::LeptosOptions::builder()
        .output_name("irgendwas")
        .site_root(root.to_str().unwrap().to_string())
        .build();
    let app = server::router(server::app_state(options, pool, config_with(&[])).unwrap());
    let response = send(&app, get("/fixture.css", None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/css")
    );
    assert!(response.headers().get(CACHE_CONTROL).is_none());
    assert_eq!(body_string(response).await, "body { color: red; }");
    std::fs::remove_dir_all(root).unwrap();
}

#[sqlx::test(migrations = "../migrations")]
async fn a_late_response_cannot_restore_a_logged_out_session(pool: MySqlPool) {
    use std::sync::Arc;
    use tokio::sync::Notify;

    let loaded = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let loaded_handler = loaded.clone();
    let release_handler = release.clone();
    let extra = test_routes().route(
        "/test/hold",
        route(move |session: Session, State(state): State<AppState>| {
            let loaded = loaded_handler.clone();
            let release = release_handler.clone();
            async move {
                assert!(
                    auth::load_user(&session, &state.pool)
                        .await
                        .unwrap()
                        .is_some()
                );
                loaded.notify_one();
                release.notified().await;
                "ok"
            }
        }),
    );
    let app = server::router_with(state(pool.clone(), config_with(&[])), extra);
    let alice = create_user(&pool, "Alice", "logout-race@example.com").await;
    let cookie =
        session_cookie(&send(&app, get(&format!("/test/login/{alice}"), None)).await).unwrap();

    let held_app = app.clone();
    let held_cookie = cookie.clone();
    let held =
        tokio::spawn(async move { send(&held_app, get("/test/hold", Some(&held_cookie))).await });
    loaded.notified().await;
    assert_eq!(
        send(&app, get("/test/logout", Some(&cookie)))
            .await
            .status(),
        StatusCode::OK
    );
    release.notify_one();
    let late_response = held.await.unwrap();
    assert_eq!(late_response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        session_cookie(&late_response).is_none(),
        "late response must not restore the old cookie"
    );
    assert_eq!(
        body_string(send(&app, get("/test/me", Some(&cookie))).await).await,
        "anonymous"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn session_create_collision_preserves_the_existing_record(pool: MySqlPool) {
    let store = server::MySqlSessionStore::new(pool);
    let mut original = Record {
        id: Id::default(),
        data: Default::default(),
        expiry_date: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    store.create(&mut original).await.unwrap();
    let mut collision = original.clone();
    collision.data.insert("new".into(), serde_json::json!(true));
    store.create(&mut collision).await.unwrap();
    assert_ne!(original.id, collision.id);
    assert_eq!(store.load(&original.id).await.unwrap(), Some(original));
    assert_eq!(store.load(&collision.id).await.unwrap(), Some(collision));
}

#[sqlx::test(migrations = "../migrations")]
async fn independent_session_updates_merge_but_conflicting_updates_fail(pool: MySqlPool) {
    let initial = server::MySqlSessionStore::new(pool.clone());
    let mut record = Record {
        id: Id::default(),
        data: Default::default(),
        expiry_date: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    record.data.insert("user".into(), serde_json::json!(42));
    initial.create(&mut record).await.unwrap();
    // Distinct store instances model requests handled by different app processes.
    let a = server::MySqlSessionStore::new(pool.clone());
    let b = server::MySqlSessionStore::new(pool.clone());
    let mut left = a.load(&record.id).await.unwrap().unwrap();
    let mut right = b.load(&record.id).await.unwrap().unwrap();
    left.data.insert("locale".into(), serde_json::json!("de"));
    right.data.insert("draft".into(), serde_json::json!("wish"));
    a.save(&left).await.unwrap();
    b.save(&right).await.unwrap();
    let merged = initial.load(&record.id).await.unwrap().unwrap();
    assert_eq!(merged.data.get("locale"), Some(&serde_json::json!("de")));
    assert_eq!(merged.data.get("draft"), Some(&serde_json::json!("wish")));

    let a = server::MySqlSessionStore::new(pool.clone());
    let b = server::MySqlSessionStore::new(pool);
    let mut left = a.load(&record.id).await.unwrap().unwrap();
    let mut right = b.load(&record.id).await.unwrap().unwrap();
    left.data.insert("locale".into(), serde_json::json!("en"));
    right.data.insert("locale".into(), serde_json::json!("fr"));
    a.save(&left).await.unwrap();
    assert!(
        b.save(&right).await.is_err(),
        "a conflict must not silently erase a successful update"
    );
    assert_eq!(
        a.load(&record.id)
            .await
            .unwrap()
            .unwrap()
            .data
            .get("locale"),
        Some(&serde_json::json!("en"))
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn concurrent_flash_consumption_preserves_new_messages(pool: MySqlPool) {
    let initial = server::MySqlSessionStore::new(pool.clone());
    let mut record = Record {
        id: Id::default(),
        data: Default::default(),
        expiry_date: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    record
        .data
        .insert("flash".into(), serde_json::json!(["old"]));
    initial.create(&mut record).await.unwrap();
    let reader = server::MySqlSessionStore::new(pool.clone());
    let writer = server::MySqlSessionStore::new(pool.clone());
    let mut consumed = reader.load(&record.id).await.unwrap().unwrap();
    let mut appended = writer.load(&record.id).await.unwrap().unwrap();
    consumed.data.remove("flash");
    appended
        .data
        .insert("flash".into(), serde_json::json!(["old", "new"]));
    writer.save(&appended).await.unwrap();
    reader.save(&consumed).await.unwrap();
    assert_eq!(
        initial
            .load(&record.id)
            .await
            .unwrap()
            .unwrap()
            .data
            .get("flash"),
        Some(&serde_json::json!(["new"]))
    );

    // Two simultaneous producers preserve both additions, even if their message
    // values happen to be identical.
    let first = server::MySqlSessionStore::new(pool.clone());
    let second = server::MySqlSessionStore::new(pool.clone());
    let mut first_record = first.load(&record.id).await.unwrap().unwrap();
    let mut second_record = second.load(&record.id).await.unwrap().unwrap();
    first_record
        .data
        .insert("flash".into(), serde_json::json!(["new", "same"]));
    second_record
        .data
        .insert("flash".into(), serde_json::json!(["new", "same"]));
    first.save(&first_record).await.unwrap();
    second.save(&second_record).await.unwrap();
    assert_eq!(
        initial
            .load(&record.id)
            .await
            .unwrap()
            .unwrap()
            .data
            .get("flash"),
        Some(&serde_json::json!(["new", "same", "same"]))
    );

    // Two consumers may not both return the same messages successfully.
    let first = server::MySqlSessionStore::new(pool.clone());
    let second = server::MySqlSessionStore::new(pool.clone());
    let mut first_record = first.load(&record.id).await.unwrap().unwrap();
    let mut second_record = second.load(&record.id).await.unwrap().unwrap();
    first_record.data.remove("flash");
    second_record.data.remove("flash");
    first.save(&first_record).await.unwrap();
    assert!(second.save(&second_record).await.is_err());

    // A third request may still append against the old snapshot after another
    // request consumed that snapshot and a writer added a new queue.
    let mut reset = initial.load(&record.id).await.unwrap().unwrap();
    reset
        .data
        .insert("flash".into(), serde_json::json!(["old"]));
    initial.save(&reset).await.unwrap();
    let reader = server::MySqlSessionStore::new(pool.clone());
    let first_writer = server::MySqlSessionStore::new(pool.clone());
    let late_writer = server::MySqlSessionStore::new(pool);
    let mut read_record = reader.load(&record.id).await.unwrap().unwrap();
    let mut first_record = first_writer.load(&record.id).await.unwrap().unwrap();
    let mut late_record = late_writer.load(&record.id).await.unwrap().unwrap();
    read_record.data.remove("flash");
    first_record
        .data
        .insert("flash".into(), serde_json::json!(["old", "first"]));
    late_record
        .data
        .insert("flash".into(), serde_json::json!(["old", "late"]));
    reader.save(&read_record).await.unwrap();
    first_writer.save(&first_record).await.unwrap();
    late_writer.save(&late_record).await.unwrap();
    assert_eq!(
        initial
            .load(&record.id)
            .await
            .unwrap()
            .unwrap()
            .data
            .get("flash"),
        Some(&serde_json::json!(["first", "late"]))
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn stale_flash_consumer_cannot_remove_an_identical_new_message(pool: MySqlPool) {
    let initial = server::MySqlSessionStore::new(pool.clone());
    let mut record = Record {
        id: Id::default(),
        data: Default::default(),
        expiry_date: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let old =
        serde_json::json!({"kind":"Success","key":"saved","params":[],"plural":null,"id":"old"});
    let new =
        serde_json::json!({"kind":"Success","key":"saved","params":[],"plural":null,"id":"new"});
    record.data.insert("flash".into(), serde_json::json!([old]));
    initial.create(&mut record).await.unwrap();
    let reader = server::MySqlSessionStore::new(pool.clone());
    let stale = server::MySqlSessionStore::new(pool.clone());
    let writer = server::MySqlSessionStore::new(pool);
    let mut consumed = reader.load(&record.id).await.unwrap().unwrap();
    let mut stale_consumed = stale.load(&record.id).await.unwrap().unwrap();
    consumed.data.remove("flash");
    stale_consumed.data.remove("flash");
    reader.save(&consumed).await.unwrap();
    let mut appended = writer.load(&record.id).await.unwrap().unwrap();
    appended
        .data
        .insert("flash".into(), serde_json::json!([new]));
    writer.save(&appended).await.unwrap();
    assert!(stale.save(&stale_consumed).await.is_err());
    assert_eq!(
        initial
            .load(&record.id)
            .await
            .unwrap()
            .unwrap()
            .data
            .get("flash"),
        Some(&serde_json::json!([new]))
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn simultaneous_share_drafts_keep_distinct_changes_and_the_five_draft_limit(pool: MySqlPool) {
    fn draft(id: &str) -> serde_json::Value {
        serde_json::json!({"id": id, "draft": {"title": id, "description": "", "link": ""}})
    }
    let initial = server::MySqlSessionStore::new(pool.clone());
    let mut record = Record {
        id: Id::default(),
        data: Default::default(),
        expiry_date: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    record.data.insert(
        "shared_link_drafts".into(),
        serde_json::json!([draft("a"), draft("b"), draft("c"), draft("d"), draft("e")]),
    );
    initial.create(&mut record).await.unwrap();
    let first = server::MySqlSessionStore::new(pool.clone());
    let second = server::MySqlSessionStore::new(pool);
    let mut first_record = first.load(&record.id).await.unwrap().unwrap();
    let mut second_record = second.load(&record.id).await.unwrap().unwrap();
    first_record.data.insert(
        "shared_link_drafts".into(),
        serde_json::json!([draft("b"), draft("c"), draft("d"), draft("e"), draft("f")]),
    );
    second_record.data.insert(
        "shared_link_drafts".into(),
        serde_json::json!([draft("b"), draft("c"), draft("d"), draft("e"), draft("g")]),
    );
    first.save(&first_record).await.unwrap();
    second.save(&second_record).await.unwrap();
    assert_eq!(
        initial
            .load(&record.id)
            .await
            .unwrap()
            .unwrap()
            .data
            .get("shared_link_drafts"),
        Some(&serde_json::json!([
            draft("c"),
            draft("d"),
            draft("e"),
            draft("f"),
            draft("g")
        ]))
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn concurrent_http_requests_merge_independent_session_writes(pool: MySqlPool) {
    use std::sync::Arc;
    use tokio::sync::Notify;
    let loaded = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let extra = test_routes()
        .route(
            "/test/hold-read",
            route({
                let loaded = loaded.clone();
                let release = release.clone();
                move |session: Session| {
                    let loaded = loaded.clone();
                    let release = release.clone();
                    async move {
                        let _: Option<i32> = session.get(auth::USER_ID_KEY).await.unwrap();
                        loaded.notify_one();
                        release.notified().await;
                        session.insert("race_other", "other").await.unwrap();
                        "ok"
                    }
                }
            }),
        )
        .route(
            "/test/write-value",
            route(|session: Session| async move {
                session.insert("race_value", "new").await.unwrap();
                "ok"
            }),
        )
        .route(
            "/test/read-value",
            route(|session: Session| async move {
                let value = session
                    .get::<String>("race_value")
                    .await
                    .unwrap()
                    .unwrap_or_default();
                let other = session
                    .get::<String>("race_other")
                    .await
                    .unwrap()
                    .unwrap_or_default();
                format!("{value}:{other}")
            }),
        );
    let app = server::router_with(state(pool.clone(), config_with(&[])), extra);
    let alice = create_user(&pool, "Alice", "session-update-race@example.com").await;
    let cookie =
        session_cookie(&send(&app, get(&format!("/test/login/{alice}"), None)).await).unwrap();
    let held_app = app.clone();
    let held_cookie = cookie.clone();
    let held =
        tokio::spawn(
            async move { send(&held_app, get("/test/hold-read", Some(&held_cookie))).await },
        );
    loaded.notified().await;
    assert_eq!(
        send(&app, get("/test/write-value", Some(&cookie)))
            .await
            .status(),
        StatusCode::OK
    );
    release.notify_one();
    assert_eq!(held.await.unwrap().status(), StatusCode::OK);
    assert_eq!(
        body_string(send(&app, get("/test/read-value", Some(&cookie))).await).await,
        "new:other"
    );
}
