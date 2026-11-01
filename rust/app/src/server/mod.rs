//! Server-only building blocks shared by all server functions (feature `ssr`).
//!
//! - [`config`]: typed configuration from the environment.
//! - [`db`]: MySQL pool (UTC connections) and migrations.
//! - [`password`]: argon2id hashing and verification.
//! - [`client_ip`]: client address behind trusted proxies.
//! - [`auth`]: session → current user (`current_user`, `require_user`, `login`, `logout`).
//! - [`authz`]: group membership / admin checks. **Every** server fn that takes a group id
//!   must go through these.
//! - [`flash`]: session-backed flash messages.
//! - [`i18n`]: session/cookie locale helpers.
//! - [`mail`]: SMTP [`Mailer`](mail::Mailer) (off the request path) and `escape_html`.

pub mod auth;
pub mod authz;
pub mod client_ip;
pub mod config;
pub mod db;
pub mod flash;
pub mod i18n;
pub mod mail;
pub mod password;

use std::sync::Arc;

use axum::extract::FromRef;
use leptos::config::LeptosOptions;
use leptos::prelude::use_context;
use sqlx::MySqlPool;

use crate::error::AppError;
pub use config::Config;

/// Axum router state. Leptos also provides it as context to components and server fns.
#[derive(Clone, Debug)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub pool: MySqlPool,
    pub config: Arc<Config>,
    pub mailer: mail::Mailer,
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}

impl FromRef<AppState> for MySqlPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

/// The [`AppState`] inside a server fn or during SSR.
pub fn state() -> Result<AppState, AppError> {
    use_context::<AppState>().ok_or_else(|| AppError::internal("AppState missing from context"))
}

/// The DB pool inside a server fn.
pub fn pool() -> Result<MySqlPool, AppError> {
    Ok(state()?.pool)
}
