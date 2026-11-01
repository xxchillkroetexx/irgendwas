//! Helpers shared by the integration tests.
#![allow(dead_code)]

use app::server::{AppState, Config};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, header};
use http_body_util::BodyExt;
use leptos::config::LeptosOptions;
use sqlx::MySqlPool;
use tower::ServiceExt;

/// Origin of the default test `APP_URL` (`http://localhost:3000`, the config default).
/// State-changing requests must carry it, or the CSRF check answers 403.
pub const TEST_ORIGIN: &str = "http://localhost:3000";

pub fn leptos_options() -> LeptosOptions {
    LeptosOptions::builder()
        .output_name("irgendwas")
        .site_root("target/site")
        .build()
}

pub fn config_with(vars: &[(&str, &str)]) -> Config {
    let vars: Vec<(String, String)> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Config::from_lookup(move |name| {
        vars.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .or_else(|| (name == "DB_PASSWORD").then(String::new))
    })
    .expect("config")
}

pub fn state(pool: MySqlPool, config: Config) -> AppState {
    server::app_state(leptos_options(), pool, config).expect("app state")
}

pub async fn send(app: &Router, req: Request<Body>) -> Response<Body> {
    app.clone().oneshot(req).await.expect("request")
}

pub fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::get(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).expect("request")
}

/// A same-origin form POST (`Origin: TEST_ORIGIN`), e.g. to a server fn under `/api/`.
/// For a config with another `APP_URL`, use [`post_from`] with `config.app_origin()`.
pub fn post(uri: &str, cookie: Option<&str>, form_body: &str) -> Request<Body> {
    post_from(Some(TEST_ORIGIN), uri, cookie, form_body)
}

/// A form POST with an explicit `Origin` (`None`: no Origin header at all).
pub fn post_from(
    origin: Option<&str>,
    uri: &str,
    cookie: Option<&str>,
    form_body: &str,
) -> Request<Body> {
    let mut builder =
        Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder
        .body(Body::from(form_body.to_string()))
        .expect("request")
}

pub async fn body_string(res: Response<Body>) -> String {
    let bytes = res.into_body().collect().await.expect("body").to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf8 body")
}

/// All `Set-Cookie` headers of a response.
pub fn set_cookies(res: &Response<Body>) -> Vec<String> {
    res.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().expect("ascii cookie").to_string())
        .collect()
}

/// `name=value` of the session cookie in a response, if one was set.
pub fn session_cookie(res: &Response<Body>) -> Option<String> {
    set_cookies(res)
        .into_iter()
        .find(|c| c.starts_with(&format!("{}=", server::SESSION_COOKIE)))
        .map(|c| c.split(';').next().expect("cookie pair").to_string())
}
