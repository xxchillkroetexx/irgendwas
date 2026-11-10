//! Request middleware.

use std::sync::Arc;

use app::i18n::{RequestLocale, resolve_locale};
use app::server::AppState;
use app::server::config::normalize_origin;
use app::server::i18n::{cookie_locale, session_locale};
use axum::extract::{Request, State};
use axum::http::header::{ACCEPT_LANGUAGE, CACHE_CONTROL, CONTENT_TYPE, COOKIE, ORIGIN};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower::{Layer, ServiceExt};
use tower_sessions::Session;

/// Build the session layer per request. The underlying pool is shared, while the
/// store's baseline is private to this request, including its final save.
pub async fn session(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let layer = crate::session_layer(state.pool.clone(), &state.config);
    match layer.layer(next).oneshot(req).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

/// All registered application routes may depend on the session or locale.
pub async fn dynamic_no_store(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The static-file fallback can itself render a personalized HTML 404. Protect HTML
/// returned from it too, while leaving ordinary CSS, JS, WASM and images cacheable.
pub async fn html_no_store(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let is_html = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|s| s.trim().eq_ignore_ascii_case("text/html"))
        });
    if is_html {
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

/// Resolves the request locale (session → `locale` cookie → `Accept-Language` (all headers
/// joined with `,`) → `APP_LOCALE`) and stores it as
/// a [`RequestLocale`] request extension, which SSR reads synchronously.
pub async fn locale(
    State(state): State<AppState>,
    session: Session,
    mut req: Request,
    next: Next,
) -> Response {
    let from_session = session_locale(&session).await.ok().flatten();
    let from_cookie = req
        .headers()
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(cookie_locale);
    let accept_language = req
        .headers()
        .get_all(ACCEPT_LANGUAGE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect::<Vec<_>>()
        .join(",");
    let locale = resolve_locale(
        from_session.map(|l| l.code()),
        from_cookie.map(|l| l.code()),
        Some(accept_language.as_str()),
        state.config.app_locale,
    );
    req.extensions_mut().insert(RequestLocale(locale));
    next.run(req).await
}

/// CSRF defence for every state-changing request (server-fn POSTs and any other non-GET
/// route). `GET`, `HEAD` and `OPTIONS` pass untouched (so does `GET /health`).
///
/// A request is accepted when
/// - its `Origin` header equals the origin of `APP_URL` (see [`Config::app_origin`]), or
/// - it has no `Origin` and `Sec-Fetch-Site` is `same-origin` or `none`.
///
/// Everything else (foreign or `null` origin, neither header present) gets `403` before it
/// reaches the session layer or a handler. Browsers send `Origin` on every POST, including
/// same-origin `fetch` and form posts, so legitimate traffic always carries one of them.
///
/// [`Config::app_origin`]: app::server::Config::app_origin
pub async fn origin_check(
    State(expected_origin): State<Arc<str>>,
    req: Request,
    next: Next,
) -> Response {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }
    match check_origin(req.headers(), &expected_origin) {
        Ok(()) => next.run(req).await,
        Err(reason) => {
            tracing::warn!(
                method = %req.method(),
                path = %req.uri().path(),
                reason,
                "rejected cross-origin request"
            );
            (StatusCode::FORBIDDEN, "Forbidden").into_response()
        }
    }
}

/// The decision of [`origin_check`]. `expected_origin` must already be normalised.
pub fn check_origin(headers: &HeaderMap, expected_origin: &str) -> Result<(), &'static str> {
    let mut origins = headers.get_all(ORIGIN).iter();
    match (origins.next(), origins.next()) {
        (Some(origin), None) => {
            let origin = origin.to_str().ok().and_then(normalize_origin);
            if origin.as_deref() == Some(expected_origin) {
                Ok(())
            } else {
                Err("foreign origin")
            }
        }
        (Some(_), Some(_)) => Err("multiple Origin headers"),
        (None, _) => {
            let site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok());
            match site {
                Some(s)
                    if s.eq_ignore_ascii_case("same-origin") || s.eq_ignore_ascii_case("none") =>
                {
                    Ok(())
                }
                Some(_) => Err("cross-site Sec-Fetch-Site"),
                None => Err("neither Origin nor Sec-Fetch-Site"),
            }
        }
    }
}
