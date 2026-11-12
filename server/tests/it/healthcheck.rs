//! `server --healthcheck` probe against a real listening server.

use std::net::SocketAddr;

use crate::common::*;
use axum::http::{StatusCode, header};
use server::healthcheck::probe;
use sqlx::MySqlPool;

#[sqlx::test(migrations = "../migrations")]
async fn health_ignores_session_cookie_when_session_table_is_unavailable(pool: MySqlPool) {
    let app = server::router(state(pool.clone(), config_with(&[])));
    // A syntactically valid session ID would normally make locale middleware load it.
    // Removing the table is safe here: sqlx creates a disposable database per test.
    sqlx::query("DROP TABLE sessions")
        .execute(&pool)
        .await
        .unwrap();
    let cookie = format!(
        "{}={}",
        server::SESSION_COOKIE,
        tower_sessions::session::Id::default()
    );
    let res = send(&app, get("/health", Some(&cookie))).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert!(res.headers().get(header::SET_COOKIE).is_none());
    assert!(body_string(res).await.contains("\"status\":\"ok\""));

    // The common origin guard still covers health, even for unsupported methods.
    assert_eq!(
        send(
            &app,
            post_from(Some("https://foreign.example"), "/health", None, "")
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, post("/health", None, "")).await.status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn health_still_reports_an_unavailable_database(pool: MySqlPool) {
    let app = server::router(state(pool.clone(), config_with(&[])));
    pool.close().await;
    let res = send(&app, get("/health", None)).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn probe_reports_healthy_server_and_unreachable_port(pool: MySqlPool) {
    let app = server::router(state(pool, config_with(&[])));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let ok = tokio::task::spawn_blocking(move || probe(addr))
        .await
        .unwrap();
    assert_eq!(ok, Ok(()));

    // A port with nothing listening (bind, read the port, close).
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let err = tokio::task::spawn_blocking(move || probe(closed))
        .await
        .unwrap();
    assert!(err.is_err());
}
