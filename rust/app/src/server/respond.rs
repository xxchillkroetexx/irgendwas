//! Response tweaks from inside a server fn or an SSR render (`leptos_axum::ResponseOptions`).

use axum::http::header::{ACCEPT, CACHE_CONTROL, LOCATION, REFERRER_POLICY};
use axum::http::{HeaderValue, StatusCode, request::Parts};
use leptos::prelude::use_context;
use leptos_axum::ResponseOptions;

use crate::error::HttpStatus;

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

/// Gives a server fn's error response the error's status (instead of the default 500). Not
/// for plain HTML form posts: leptos answers those with a redirect back to the form.
pub fn with_error_status<T, E: HttpStatus>(result: Result<T, E>) -> Result<T, E> {
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

/// Whether the current server fn call is a plain HTML form post (JavaScript disabled or
/// not hydrated yet), as opposed to the server fn client.
pub fn is_plain_form_post() -> bool {
    accepts_html()
}

/// Pages whose URL carries a secret token: the URL never goes to another origin as a
/// referrer (`same-origin` still lets a no-JS form post carry the real `Origin`; with
/// `no-referrer` browsers send `Origin: null`, which the Origin check rejects), and the
/// page is never cached.
pub fn token_page_headers() {
    if let Some(response) = response() {
        response.insert_header(REFERRER_POLICY, HeaderValue::from_static("same-origin"));
        response.insert_header(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
}

/// Pages with personal data that no cache should keep (`Cache-Control: no-store`).
pub fn no_store() {
    if let Some(response) = response() {
        response.insert_header(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
}
