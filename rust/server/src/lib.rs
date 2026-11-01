//! Process wiring: configuration, DB, migrations, sessions, middleware, router.
//!
//! Layer order (outermost first): Origin/CSRF check → session → locale → routes.

pub mod health;
pub mod healthcheck;
pub mod middleware;
pub mod session_store;

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use app::server::mail::{MailError, Mailer};
use app::server::{AppState, Config, db};
use app::{App, shell};
use axum::Router;
use axum::routing::get;
use leptos::config::LeptosOptions;
use leptos_axum::{AxumRouteListing, LeptosRoutes, generate_route_list};
use sqlx::MySqlPool;
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

/// The complete application router (pages, server fns, static files, `/health`), behind
/// the Origin check ([`middleware::origin_check`]) and the session and locale layers.
pub fn router(state: AppState) -> Router {
    router_with(state, Router::new())
}

/// [`router`] plus `extra` routes, which run inside the session and locale layers.
/// Used by integration tests to exercise session helpers directly.
pub fn router_with(state: AppState, extra: Router<AppState>) -> Router {
    let routes = route_list();
    let session_layer = session_layer(state.pool.clone(), &state.config);
    let leptos_options = state.leptos_options.clone();
    let app_origin: Arc<str> = state.config.app_origin().into();

    Router::new()
        .route("/health", get(health::health))
        .merge(extra)
        .leptos_routes(&state, routes, move || shell(leptos_options.clone()))
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::locale,
        ))
        .layer(session_layer)
        // Outermost: cross-origin writes are rejected before they touch the session.
        .layer(axum::middleware::from_fn_with_state(
            app_origin,
            middleware::origin_check,
        ))
        .with_state(state)
}

/// Builds the state from already-loaded parts (the mailer from `config.mail`).
pub fn app_state(
    leptos_options: LeptosOptions,
    pool: MySqlPool,
    config: Config,
) -> Result<AppState, MailError> {
    let mailer = Mailer::new(&config.mail)?;
    Ok(AppState {
        leptos_options,
        pool,
        config: Arc::new(config),
        mailer,
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

    let leptos_options = leptos::config::get_configuration(None)?.leptos_options;
    let addr = leptos_options.site_addr;

    let pool = db::connect(&config.db, Duration::from_secs(60)).await?;
    db::migrate(&pool).await?;
    tracing::info!("database migrated");

    MySqlSessionStore::new(pool.clone()).spawn_cleanup(Duration::from_secs(300));

    let app = router(app_state(leptos_options, pool, config)?);
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
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
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
