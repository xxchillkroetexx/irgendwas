//! Request middleware.

use std::sync::Arc;

use app::i18n::{RequestLocale, resolve_locale};
use app::server::AppState;
use app::server::config::normalize_origin;
use app::server::i18n::{cookie_locale, session_locale};
use axum::extract::{Request, State};
use axum::http::header::{COOKIE, ORIGIN};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_sessions::Session;

/// Resolves the request locale (session → `locale` cookie → `APP_LOCALE`) and stores it as
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
    let locale = resolve_locale(
        from_session.map(|l| l.code()),
        from_cookie.map(|l| l.code()),
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
