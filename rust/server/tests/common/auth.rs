//! HTTP-level helpers for the auth tests: a cookie-keeping client that POSTs to the server
//! fn endpoints (`/api/<fn>`), plus router/log/HTML helpers.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use app::auth::SessionInfo;
use app::auth_api::AuthError;
use app::flash::FlashMessage;
use app::server::AppState;
use app::server::rate_limit::RateLimiter;
use app::test_support::ManualClock;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::routing::get as route;
use serde::de::DeserializeOwned;
use sqlx::MySqlPool;
use tower_sessions::Session;

use super::{TEST_ORIGIN, body_string, config_with, send, state};

/// Default TCP peer of test requests.
pub const DEFAULT_PEER: &str = "192.0.2.1";

#[derive(Default)]
pub struct Opts {
    /// Point the mailer at Mailpit (otherwise mails go nowhere and fail silently).
    pub mail: bool,
    /// Replace the rate limiter's clock.
    pub clock: Option<Arc<ManualClock>>,
}

impl Opts {
    pub fn mail() -> Self {
        Opts {
            mail: true,
            ..Opts::default()
        }
    }

    pub fn clock(clock: &Arc<ManualClock>) -> Self {
        Opts {
            clock: Some(clock.clone()),
            ..Opts::default()
        }
    }
}

/// The full router with the auth test routes (`/test/touch` puts data into the session).
pub fn router(pool: &MySqlPool, vars: &[(&str, &str)], opts: Opts) -> Router {
    let mut all: Vec<(String, String)> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    if opts.mail {
        all.extend(
            app::test_support::mailpit::smtp_vars()
                .into_iter()
                .map(|(k, v)| (k.to_string(), v)),
        );
    }
    let borrowed: Vec<(&str, &str)> = all.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut st = state(pool.clone(), config_with(&borrowed));
    if let Some(clock) = opts.clock {
        st.rate_limiter = Arc::new(RateLimiter::new(clock));
    }
    server::router_with(st, test_routes())
}

fn test_routes() -> Router<AppState> {
    Router::new().route(
        "/test/touch",
        route(|session: Session| async move {
            session.insert("touched", true).await.unwrap();
            "ok"
        }),
    )
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
pub fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// A buffered response.
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

impl Reply {
    pub fn location(&self) -> Option<String> {
        self.headers
            .get(header::LOCATION)
            .map(|v| v.to_str().expect("ascii location").to_string())
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .map(|v| v.to_str().expect("ascii header").to_string())
    }

    pub fn set_cookies(&self) -> Vec<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().expect("ascii cookie").to_string())
            .collect()
    }

    /// The `Set-Cookie` header of cookie `name`, if any.
    pub fn set_cookie(&self, name: &str) -> Option<String> {
        self.set_cookies()
            .into_iter()
            .find(|c| c.starts_with(&format!("{name}=")))
    }

    /// A server fn result: 2xx -> the JSON value, anything else -> the JSON `AuthError`.
    pub fn result<T: DeserializeOwned>(&self) -> Result<T, AuthError> {
        if self.status.is_success() {
            Ok(serde_json::from_str(&self.body)
                .unwrap_or_else(|e| panic!("bad success body {:?}: {e}", self.body)))
        } else {
            Err(serde_json::from_str(&self.body).unwrap_or_else(|e| {
                panic!(
                    "bad error body {:?} (status {}): {e}",
                    self.body, self.status
                )
            }))
        }
    }

    pub fn ok(&self) -> &Self {
        if let Err(err) = self.result::<serde_json::Value>() {
            panic!("expected success, got {err:?}");
        }
        self
    }

    pub fn err(&self) -> AuthError {
        match self.result::<serde_json::Value>() {
            Ok(v) => panic!("expected an AuthError, got success {v}"),
            Err(e) => e,
        }
    }
}

/// A browser: cookie jar, fixed TCP peer, persistent extra headers.
pub struct Client {
    app: Router,
    cookies: BTreeMap<String, String>,
    pub peer: String,
    pub origin: String,
    pub headers: Vec<(String, String)>,
}

impl Client {
    pub fn new(app: &Router) -> Self {
        Client {
            app: app.clone(),
            cookies: BTreeMap::new(),
            peer: DEFAULT_PEER.to_string(),
            origin: TEST_ORIGIN.to_string(),
            headers: Vec::new(),
        }
    }

    pub fn peer(mut self, ip: &str) -> Self {
        self.peer = ip.to_string();
        self
    }

    pub fn origin(mut self, origin: &str) -> Self {
        self.origin = origin.to_string();
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Preset a cookie, as if the browser had it.
    pub fn with_cookie(mut self, name: &str, value: &str) -> Self {
        self.cookies.insert(name.to_string(), value.to_string());
        self
    }

    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(String::as_str)
    }

    pub fn session_cookie(&self) -> Option<&str> {
        self.cookie(server::SESSION_COOKIE)
    }

    /// A new browser sharing this one's cookies (e.g. a second tab).
    pub fn fork(&self) -> Client {
        Client {
            app: self.app.clone(),
            cookies: self.cookies.clone(),
            peer: self.peer.clone(),
            origin: self.origin.clone(),
            headers: self.headers.clone(),
        }
    }

    /// A fresh browser (no cookies) on the same app.
    pub fn fresh(&self) -> Client {
        Client {
            cookies: BTreeMap::new(),
            ..self.fork()
        }
    }

    async fn run(&mut self, mut req: Request<Body>) -> Reply {
        let headers = req.headers_mut();
        if !self.cookies.is_empty() {
            let jar = self
                .cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            headers.insert(header::COOKIE, jar.parse().unwrap());
        }
        for (name, value) in &self.headers {
            headers.append(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        let addr: SocketAddr = format!("{}:40000", self.peer).parse().expect("peer ip");
        req.extensions_mut().insert(ConnectInfo(addr));

        let res = send(&self.app, req).await;
        let (parts, body) = res.into_parts();
        let reply = Reply {
            status: parts.status,
            headers: parts.headers,
            body: body_string(axum::http::Response::new(body)).await,
        };
        for raw in reply.set_cookies() {
            let pair = raw.split(';').next().unwrap();
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let expired = raw
                .split(';')
                .any(|a| a.trim().eq_ignore_ascii_case("max-age=0"));
            if value.is_empty() || expired {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_string(), value.to_string());
            }
        }
        reply
    }

    pub async fn get(&mut self, uri: &str) -> Reply {
        let req = Request::get(uri).body(Body::empty()).unwrap();
        self.run(req).await
    }

    /// POST to `/api/<name>` like the server fn client does (no `Accept: text/html`, so a
    /// redirect shows up as a `Location` header on a 200).
    pub async fn call(&mut self, name: &str, fields: &[(&str, &str)]) -> Reply {
        let req = Request::post(format!("/api/{name}"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::ORIGIN, self.origin.clone())
            .body(Body::from(form(fields)))
            .unwrap();
        self.run(req).await
    }

    pub async fn session_info(&mut self) -> SessionInfo {
        let reply = self.call("session_info", &[]).await;
        assert!(reply.status.is_success(), "session_info: {}", reply.body);
        serde_json::from_str(&reply.body).expect("session info json")
    }

    /// Logged-in user's email, or `None`. Takes pending flash messages.
    pub async fn whoami(&mut self) -> Option<String> {
        self.session_info().await.user.map(|u| u.email)
    }

    pub async fn login(&mut self, email: &str, password: &str) -> Reply {
        self.call("login", &[("email", email), ("password", password)])
            .await
    }

    pub async fn login_next(&mut self, email: &str, password: &str, next: &str) -> Reply {
        self.call(
            "login",
            &[("email", email), ("password", password), ("next", next)],
        )
        .await
    }

    pub async fn register(&mut self, email: &str, next: Option<&str>) -> Reply {
        match next {
            Some(next) => {
                self.call("register", &[("email", email), ("next", next)])
                    .await
            }
            None => self.call("register", &[("email", email)]).await,
        }
    }

    pub async fn complete(
        &mut self,
        token: &str,
        name: &str,
        password: &str,
        confirm: &str,
    ) -> Reply {
        self.call(
            "complete_registration",
            &[
                ("token", token),
                ("name", name),
                ("password", password),
                ("password_confirm", confirm),
            ],
        )
        .await
    }

    pub async fn reset(&mut self, token: &str, password: &str, confirm: &str) -> Reply {
        self.call(
            "reset_password",
            &[
                ("token", token),
                ("password", password),
                ("password_confirm", confirm),
            ],
        )
        .await
    }

    pub async fn forgot(&mut self, email: &str) -> Reply {
        self.call("request_password_reset", &[("email", email)])
            .await
    }
}

/// Assert `info` carries exactly this flash message.
pub fn assert_flash(info: &SessionInfo, expected: &FlashMessage) {
    assert!(
        info.flash.contains(expected),
        "flash {expected:?} not in {:?}",
        info.flash
    );
}

/// True if any flash message has this key.
pub fn has_flash_key(info: &SessionInfo, key: &str) -> bool {
    info.flash.iter().any(|m| m.key == key)
}

/// `html` contains `text`, allowing for HTML escaping of `& < > " '`.
pub fn html_contains(html: &str, text: &str) -> bool {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    html.contains(text)
        || html.contains(&escaped)
        || html.contains(&escaped.replace('\'', "&#x27;"))
        || html.contains(&escaped.replace('\'', "&#39;"))
}

/// `<html lang="..">` of a page.
pub fn lang_of(html: &str) -> &str {
    let start = html.find("<html lang=\"").expect("<html lang>") + "<html lang=\"".len();
    let rest = &html[start..];
    &rest[..rest.find('"').unwrap()]
}

/// The token (last path segment) of a mail link `.../<prefix><token>`.
pub fn token_in(url: &str, path_prefix: &str) -> String {
    let (_, tail) = url.split_once(path_prefix).expect("link has the prefix");
    tail.to_string()
}

/// 43 base64url characters (32 bytes without padding).
pub fn is_token(s: &str) -> bool {
    s.len() == 43
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// ------------------------------------------------------------------ log capture

thread_local! {
    static CURRENT_LOG: std::cell::RefCell<Option<LogBuf>> = const { std::cell::RefCell::new(None) };
}

/// Collects the tracing output (DEBUG and up) emitted on the capturing test's thread.
#[derive(Clone, Default)]
pub struct LogBuf(Arc<Mutex<Vec<u8>>>);

impl LogBuf {
    pub fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// Writer of the process-wide subscriber: appends to the buffer of the current thread's
/// active capture, drops everything else.
struct RoutedWriter;

impl io::Write for RoutedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        CURRENT_LOG.with(|cell| {
            if let Some(log) = cell.borrow().as_ref() {
                log.0.lock().unwrap().extend_from_slice(buf);
            }
        });
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for RoutedWriter {
    type Writer = RoutedWriter;

    fn make_writer(&'a self) -> RoutedWriter {
        RoutedWriter
    }
}

/// Ends the capture of the current thread when dropped.
pub struct CaptureGuard;

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CURRENT_LOG.with(|cell| *cell.borrow_mut() = None);
    }
}

/// Routes this thread's tracing events (DEBUG and up) into the returned buffer until the
/// guard drops. A process-wide subscriber (installed once) does the work, so callsite
/// interest never depends on which thread registered a callsite first. The `#[sqlx::test]`
/// runtime is single-threaded, so server fns and spawned mail tasks log on this thread.
///
/// Limitation: events emitted on other threads or runtimes (e.g. inside `spawn_blocking`,
/// or by a `std::thread`) are NOT captured, so a leak from such code would go unnoticed.
pub fn capture_logs() -> (LogBuf, CaptureGuard) {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(RoutedWriter)
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .finish();
        // Another global subscriber would make the capture empty; the tests then fail loudly.
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
    let buf = LogBuf::default();
    CURRENT_LOG.with(|cell| *cell.borrow_mut() = Some(buf.clone()));
    (buf, CaptureGuard)
}
