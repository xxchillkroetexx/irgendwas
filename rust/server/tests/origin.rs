//! CSRF defence: state-changing requests must come from the `APP_URL` origin
//! (`server::middleware::origin_check`).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use app::server::AppState;
use app::server::config::normalize_origin;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use axum::routing::post as route_post;
use common::*;
use server::middleware::check_origin;
use sqlx::MySqlPool;

/// A POST route that counts how often its handler actually ran.
fn counting_routes(hits: Arc<AtomicUsize>) -> Router<AppState> {
    Router::new().route(
        "/test/write",
        route_post(move || {
            let hits = hits.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                "written"
            }
        }),
    )
}

fn app(pool: &MySqlPool, vars: &[(&str, &str)], hits: Arc<AtomicUsize>) -> Router {
    server::router_with(
        state(pool.clone(), config_with(vars)),
        counting_routes(hits),
    )
}

fn post_with_fetch_site(origin: Option<&str>, site: &str) -> Request<Body> {
    let mut req = post_from(origin, "/test/write", None, "");
    req.headers_mut()
        .insert("sec-fetch-site", HeaderValue::from_str(site).unwrap());
    req
}

#[sqlx::test(migrations = "../migrations")]
async fn foreign_origin_is_rejected_same_origin_passes(pool: MySqlPool) {
    let hits = Arc::new(AtomicUsize::new(0));
    let app = app(&pool, &[], hits.clone());

    // Same origin (the default APP_URL): accepted.
    let res = send(&app, post("/test/write", None, "")).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_string(res).await, "written");
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Foreign origin: 403, the handler never runs, no session cookie is created.
    for origin in [
        "https://evil.example",
        "http://localhost:3001",
        "https://localhost:3000",
        "http://localhost.evil.example:3000",
        "null",
    ] {
        let res = send(&app, post_from(Some(origin), "/test/write", None, "")).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "origin {origin}");
        assert!(session_cookie(&res).is_none());
    }
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Neither Origin nor Sec-Fetch-Site: 403.
    let res = send(&app, post_from(None, "/test/write", None, "")).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // No Origin, but the browser says same-origin / user-initiated: accepted.
    for site in ["same-origin", "none"] {
        let res = send(&app, post_with_fetch_site(None, site)).await;
        assert_eq!(res.status(), StatusCode::OK, "Sec-Fetch-Site {site}");
    }
    assert_eq!(hits.load(Ordering::SeqCst), 3);

    // No Origin and a cross-site / same-site fetch: rejected.
    for site in ["cross-site", "same-site"] {
        let res = send(&app, post_with_fetch_site(None, site)).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "Sec-Fetch-Site {site}");
    }
    // A foreign Origin is not rescued by Sec-Fetch-Site.
    let res = send(
        &app,
        post_with_fetch_site(Some("https://evil.example"), "same-origin"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(hits.load(Ordering::SeqCst), 3);
}

#[sqlx::test(migrations = "../migrations")]
async fn server_fns_are_protected_and_gets_are_exempt(pool: MySqlPool) {
    let app = app(&pool, &[], Arc::new(AtomicUsize::new(0)));

    // A real server fn.
    let res = send(&app, post("/api/session_info", None, "")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = send(
        &app,
        post_from(Some("https://evil.example"), "/api/session_info", None, ""),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Other unsafe methods are checked too.
    let req = Request::delete("/test/write")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.status(), StatusCode::FORBIDDEN);

    // GET needs no Origin: pages and /health.
    let res = send(&app, get("/health", None)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let req = Request::get("/")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "../migrations")]
async fn origin_follows_app_url(pool: MySqlPool) {
    let hits = Arc::new(AtomicUsize::new(0));
    let config = config_with(&[("APP_URL", "https://Wichteln.Example:443/")]);
    assert_eq!(config.app_origin(), "https://wichteln.example");
    let app = server::router_with(state(pool.clone(), config), counting_routes(hits.clone()));

    for origin in ["https://wichteln.example", "https://WICHTELN.example:443"] {
        let res = send(&app, post_from(Some(origin), "/test/write", None, "")).await;
        assert_eq!(res.status(), StatusCode::OK, "origin {origin}");
    }
    // The dev default is not accepted for a production APP_URL.
    let res = send(&app, post("/test/write", None, "")).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let res = send(
        &app,
        post_from(Some("http://wichteln.example"), "/test/write", None, ""),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[test]
fn origin_normalisation() {
    assert_eq!(
        normalize_origin("http://localhost:3000").as_deref(),
        Some("http://localhost:3000")
    );
    assert_eq!(
        normalize_origin("HTTP://LocalHost:80/path?q=1").as_deref(),
        Some("http://localhost")
    );
    assert_eq!(
        normalize_origin("https://[::1]:8443").as_deref(),
        Some("https://[::1]:8443")
    );
    assert_eq!(
        normalize_origin("https://[::1]").as_deref(),
        Some("https://[::1]")
    );
    for bad in [
        "null",
        "",
        "ftp://example.com",
        "https://",
        "https://user@evil.example",
        "https://example.com:notaport",
    ] {
        assert_eq!(normalize_origin(bad), None, "{bad:?}");
    }

    let mut headers = HeaderMap::new();
    headers.append(header::ORIGIN, HeaderValue::from_static(TEST_ORIGIN));
    assert_eq!(check_origin(&headers, TEST_ORIGIN), Ok(()));
    headers.append(header::ORIGIN, HeaderValue::from_static(TEST_ORIGIN));
    assert!(
        check_origin(&headers, TEST_ORIGIN).is_err(),
        "duplicate Origin headers are rejected"
    );
}
