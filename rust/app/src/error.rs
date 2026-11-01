//! The typed error every server function returns.
//!
//! Variants are deliberately coarse and carry no internal details: anything unexpected
//! is logged on the server and surfaces to the client as [`AppError::Internal`].

use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum AppError {
    /// No user is logged in (or the session expired).
    #[error("authentication required")]
    Unauthorized,
    /// The user is logged in but may not perform this action.
    #[error("forbidden")]
    Forbidden,
    /// The requested entity does not exist.
    #[error("not found")]
    NotFound,
    /// User input was rejected; carries an i18n key describing the problem.
    #[error("validation failed: {0}")]
    Validation(String),
    /// Unexpected server-side failure (details are only logged on the server).
    #[error("internal server error")]
    Internal,
    /// Transport / (de)serialization error produced by the server fn machinery.
    #[error("request failed: {0}")]
    ServerFn(String),
}

impl AppError {
    /// i18n key for showing this error to a user.
    pub fn message_key(&self) -> &str {
        match self {
            AppError::Unauthorized => "error.unauthorized",
            AppError::Forbidden => "error.forbidden",
            AppError::NotFound => "error.notFound.message",
            AppError::Validation(key) => key,
            AppError::Internal | AppError::ServerFn(_) => "error.generic.message",
        }
    }

    /// HTTP status code that matches this error.
    pub fn status_code(&self) -> u16 {
        match self {
            AppError::Unauthorized => 401,
            AppError::Forbidden => 403,
            AppError::NotFound => 404,
            AppError::Validation(_) => 422,
            AppError::Internal | AppError::ServerFn(_) => 500,
        }
    }

    /// Log an unexpected error on the server and turn it into [`AppError::Internal`].
    #[cfg(feature = "ssr")]
    pub fn internal(err: impl std::fmt::Display) -> Self {
        tracing::error!(error = %err, "internal error");
        AppError::Internal
    }
}

impl FromServerFnError for AppError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        AppError::ServerFn(value.to_string())
    }
}

#[cfg(feature = "ssr")]
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound,
            other => AppError::internal(other),
        }
    }
}

#[cfg(feature = "ssr")]
impl From<tower_sessions::session::Error> for AppError {
    fn from(err: tower_sessions::session::Error) -> Self {
        AppError::internal(err)
    }
}
