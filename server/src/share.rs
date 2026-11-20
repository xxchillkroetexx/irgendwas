//! The operating system navigates here with untrusted share parameters.
use std::net::SocketAddr;

use app::server::client_ip::resolve_client_ip;
use app::server::rate_limit::{SHARE_PER_IP, share_ip_key};
use app::server::share::ANONYMOUS_SHARE_LIFETIME_SECS;
use app::share_api::ShareInput;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{Extensions, HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use tower_sessions::{Expiry, Session};

pub async fn receive(
    State(state): State<app::server::AppState>,
    extensions: Extensions,
    request_headers: HeaderMap,
    session: Session,
    Query(input): Query<ShareInput>,
) -> Response {
    let mut response = stage(&state, &extensions, &request_headers, &session, input).await;
    let headers: &mut HeaderMap = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

async fn stage(
    state: &app::server::AppState,
    extensions: &Extensions,
    request_headers: &HeaderMap,
    session: &Session,
    input: ShareInput,
) -> Response {
    // Per client IP, before the session is touched: over the limit, a cookie-less request
    // leaves no session row behind (#104). The peer is always known in production
    // (`into_make_service_with_connect_info`).
    if let Some(ConnectInfo(peer)) = extensions.get::<ConnectInfo<SocketAddr>>() {
        let ip = resolve_client_ip(peer.ip(), request_headers, &state.config.trusted_proxies);
        if !state
            .rate_limiter
            .try_acquire(&share_ip_key(&ip.to_string()), SHARE_PER_IP)
        {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
    }
    // A stale authentication may flush its session. Do that before staging this share.
    let Ok(user) = app::server::auth::load_user(session, &state.pool).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    match app::server::share::receive(session, input).await {
        Ok(path) => {
            if user.is_none() {
                session.set_expiry(Some(Expiry::OnInactivity(time::Duration::seconds(
                    ANONYMOUS_SHARE_LIFETIME_SECS,
                ))));
            }
            Redirect::to(&path).into_response()
        }
        Err(error) => StatusCode::from_u16(error.status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            .into_response(),
    }
}
