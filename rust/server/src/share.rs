//! The operating system navigates here with untrusted share parameters.
use app::share_api::ShareInput;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use tower_sessions::Session;

pub async fn receive(
    State(state): State<app::server::AppState>,
    session: Session,
    Query(input): Query<ShareInput>,
) -> Response {
    // A stale authentication may flush its session. Do that before staging this share.
    if app::server::auth::load_user(&session, &state.pool)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let mut response = match app::server::share::receive(&session, input).await {
        Ok(path) => Redirect::to(&path).into_response(),
        Err(error) => StatusCode::from_u16(error.status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            .into_response(),
    };
    let headers: &mut HeaderMap = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}
