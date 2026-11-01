//! Response tweaks from inside a server fn or an SSR render (`leptos_axum::ResponseOptions`).

use axum::http::header::{ACCEPT, CACHE_CONTROL, LOCATION, REFERRER_POLICY};
use axum::http::{HeaderValue, StatusCode, request::Parts};
use leptos::prelude::use_context;
use leptos_axum::ResponseOptions;

use crate::auth_api::AuthError;

fn response() -> Option<ResponseOptions> {
    let response = use_context::<ResponseOptions>();
    if response.is_none() {
        tracing::warn!("no ResponseOptions in context; response unchanged");
    }
    response
}

fn accepts_html() -> bool {
    use_context::<Parts>()
        .and_then(|parts| {
            parts
                .headers
                .get(ACCEPT)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("text/html"))
        })
        .unwrap_or(false)
}

/// Gives an auth server fn's error response its status (instead of the default 500). Not
/// for plain HTML form posts: leptos answers those with a redirect back to the form.
pub fn with_error_status<T>(result: Result<T, AuthError>) -> Result<T, AuthError> {
    if let Err(err) = &result
        && !accepts_html()
        && let (Some(response), Ok(status)) = (
            use_context::<ResponseOptions>(),
            StatusCode::from_u16(err.status_code()),
        )
    {
        response.set_status(status);
    }
    result
}

fn location(path: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(path)
        .map_err(|err| tracing::error!(%err, "redirect target is not a header value"))
        .ok()
}

/// During an SSR page render: answer `302 Found` with `Location: path`. Must run before the
/// response head is sent (in a blocking resource or synchronously in a component).
pub fn redirect_page(path: &str) {
    if let (Some(response), Some(value)) = (response(), location(path)) {
        response.set_status(StatusCode::FOUND);
        response.insert_header(LOCATION, value);
    }
}

/// From a server fn whose client reloads the whole document itself (login, logout: the
/// page language may change). Sets `Location: path`; a plain HTML form post (no JS) also
/// gets `302`, the server fn client gets no redirect header, so it does not navigate
/// client-side.
pub fn redirect_document(path: &str) {
    let Some(response) = response() else { return };
    let Some(value) = location(path) else { return };
    response.insert_header(LOCATION, value);
    if accepts_html() {
        response.set_status(StatusCode::FOUND);
    }
}

/// Pages whose URL carries a secret token: never sent as a referrer, never cached.
pub fn token_page_headers() {
    if let Some(response) = response() {
        response.insert_header(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
        response.insert_header(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
}
