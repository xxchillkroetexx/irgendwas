//! Process wiring: configuration, DB, migrations, sessions, middleware, router.
//!
//! Layer order (outermost first): Origin/CSRF check → session → locale → app routes.
//! The health route bypasses session and locale while retaining the common outer layers.

pub mod health;
pub mod healthcheck;
pub mod middleware;
pub mod session_store;
mod share;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use app::server::draw_worker::{self, DrawWorker};
use app::server::mail::{MailError, Mailer};
use app::server::{AppState, Config, db};
use app::{App, shell};
use axum::Router;
use axum::extract::Query;
use axum::handler::Handler;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use leptos::config::LeptosOptions;
use leptos_axum::{AxumRouteListing, LeptosRoutes, generate_route_list};
use sqlx::MySqlPool;
use tower_http::services::ServeDir;
use tower_sessions::cookie::SameSite;
use tower_sessions::{Expiry, SessionManagerLayer};

pub use session_store::MySqlSessionStore;

/// Session cookie name.
pub const SESSION_COOKIE: &str = "irgendwas_session";

/// The session layer: MySQL store, HttpOnly, SameSite=Lax, Secure iff `APP_URL` is https,
/// expiry after `INACTIVITY_LIMIT` seconds without a request (every request extends it).
pub fn session_layer(pool: MySqlPool, config: &Config) -> SessionManagerLayer<MySqlSessionStore> {
    let inactivity =
        time::Duration::seconds(i64::try_from(config.inactivity_limit).unwrap_or(i64::MAX));
    SessionManagerLayer::new(MySqlSessionStore::new(pool))
        .with_name(SESSION_COOKIE)
        .with_http_only(true)
        .with_same_site(SameSite::Lax)
        .with_secure(config.secure_cookies())
        .with_path("/")
        .with_expiry(Expiry::OnInactivity(inactivity))
        .with_always_save(true)
}

/// The app's route list, generated once per process.
///
/// `generate_route_list` renders `App` with resource loading suppressed through a
/// process-wide flag (`leptos_server::SuppressResourceLoad`). Generating it again for every
/// router would make a page rendered concurrently on another thread (parallel tests that each
/// build a router) wait forever on its resources.
fn route_list() -> Vec<AxumRouteListing> {
    static ROUTES: OnceLock<Vec<AxumRouteListing>> = OnceLock::new();
    ROUTES.get_or_init(|| generate_route_list(App)).clone()
}

/// The complete application router (pages, server fns, static files, `/health`). The
/// Origin check ([`middleware::origin_check`]) covers every route; session and locale
/// layers apply only to the application routes and static-file fallback.
pub fn router(state: AppState) -> Router {
    router_with(state, Router::new())
}

/// Links shared by the PHP application use a query parameter. Convert only the
/// legacy eight-character code to the current preview URL, never join on GET.
async fn legacy_join(Query(query): Query<HashMap<String, String>>) -> Response {
    let code = query
        .get("code")
        .filter(|code| code.len() <= 128)
        .map(|code| app::groups_api::normalize_invitation_code(code))
        .filter(|code| code.len() == 8 && code.bytes().all(|byte| byte.is_ascii_alphanumeric()));
    let target = code.map_or_else(|| "/join".to_string(), |code| format!("/join/{code}"));
    let mut response = Redirect::temporary(&target).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

/// [`router`] plus `extra` routes, which run inside the session and locale layers.
/// Used by integration tests to exercise session helpers directly.
pub fn router_with(state: AppState, extra: Router<AppState>) -> Router {
    let routes = route_list();
    let session_layer = session_layer(state.pool.clone(), &state.config);
    let leptos_options = state.leptos_options.clone();
    let app_origin: Arc<str> = state.config.app_origin().into();

    // Static files (wasm, js, css, assets) are served by `ServeDir` directly; anything
    // that is not a file gets leptos' 404 page. Not `file_and_error_handler` for files: it
    // looks up `ResponseOptions` in a fresh `Owner`, which on a worker thread that is still
    // rendering another page (leptos' SSR root owner stays the thread's current owner while
    // that response streams) is a child of *that* request's owner. The file response then
    // took over the other page's status and headers (seen: `/pkg/irgendwas.wasm` answered
    // with the 302 of a guarded page, so the browser never hydrated).
    let not_found =
        leptos_axum::file_and_error_handler::<AppState, _>(shell).with_state(state.clone());
    let files = ServeDir::new(state.leptos_options.site_root.as_ref())
        .precompressed_gzip()
        .precompressed_br()
        .fallback(not_found);

    let app = Router::new()
        .route("/share-target", get(share::receive))
        .route("/groups/join", get(legacy_join))
        .merge(extra)
        .leptos_routes(&state, routes, move || shell(leptos_options.clone()))
        // Matched application routes include SSR, server functions and redirects.
        .route_layer(axum::middleware::from_fn(middleware::dynamic_no_store))
        .fallback_service(files)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::locale,
        ))
        .layer(session_layer);

    // The health probe only needs the database pool. A browser's session cookie must
    // not cause a session lookup or extend its inactivity timeout on this route.
    let health = Router::new()
        .route("/health", get(health::health))
        .route_layer(axum::middleware::from_fn(middleware::dynamic_no_store));

    app.merge(health)
        // Outermost: cross-origin writes are rejected before they touch the session.
        .layer(axum::middleware::from_fn_with_state(
            app_origin,
            middleware::origin_check,
        ))
        // The file fallback can render a localized, personalized HTML 404.
        .layer(axum::middleware::from_fn(middleware::html_no_store))
        .with_state(state)
}

/// Builds the state from already-loaded parts (the mailer from `config.mail`) and starts the
/// draw mail worker (call inside a Tokio runtime).
pub fn app_state(
    leptos_options: LeptosOptions,
    pool: MySqlPool,
    config: Config,
) -> Result<AppState, MailError> {
    let mailer = Mailer::new(&config.mail)?;
    let config = Arc::new(config);
    let draw_worker = DrawWorker::new(
        pool.clone(),
        config.clone(),
        Arc::new(mailer.clone()),
        draw_worker::default_retry_delays(),
    );
    Ok(AppState {
        leptos_options,
        pool,
        config,
        mailer,
        draw_worker,
        rate_limiter: Arc::new(app::server::rate_limit::RateLimiter::system()),
    })
}

/// Starts the server: config, DB (with retry), migrations, session cleanup, HTTP.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let dotenv = app::server::config::load_dotenv();
    init_tracing();
    match dotenv {
        Ok(true) => tracing::info!("loaded ./.env"),
        Ok(false) => {}
        Err(err) => tracing::warn!(error = %err, "could not parse ./.env"),
    }

    let config = Config::from_env()?;
    tracing::info!(?config, "configuration loaded");
    // Login verifies unknown emails against it; computing it now keeps the first login fast.
    app::server::password::init_dummy_hash();

    let leptos_options = leptos::config::get_configuration(None)?.leptos_options;
    let addr = leptos_options.site_addr;

    let pool = db::connect(&config.db, Duration::from_secs(60)).await?;
    db::migrate(&pool).await?;
    tracing::info!("database migrated");

    MySqlSessionStore::new(pool.clone()).spawn_cleanup(Duration::from_secs(300));

    let state = app_state(leptos_options, pool, config)?;
    // Draw mails a stopped server left unsent (spec: draw, "Startup recovery").
    if let Err(err) = state.draw_worker.recover().await {
        tracing::error!(?err, "draw mail recovery failed");
    }
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Colours only on a terminal: container logs (podman logs, journald) get plain text.
    let ansi = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(ansi)
        .try_init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}
