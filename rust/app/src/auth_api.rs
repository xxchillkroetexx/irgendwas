//! Auth server functions and their error type (spec: auth, "Server fns").
//!
//! The endpoints are `POST /api/<fn name>` with form-urlencoded arguments named like the
//! Rust parameters; errors are the JSON-encoded [`AuthError`] of a non-2xx response; a
//! redirect is a `Location` header. The bodies live in `server::auth_flow`.

use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

use crate::i18n::I18n;
use crate::{t, t_plural};

/// Longest accepted name, in characters.
pub const NAME_MAX: u32 = 100;
/// Shortest accepted password, in characters.
pub const PASSWORD_MIN: u32 = 8;
/// Longest accepted password, in characters.
pub const PASSWORD_MAX: u32 = 256;

/// The form field a validation error belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    Email,
    Name,
    Password,
    PasswordConfirm,
}

/// One validation problem; each maps to one i18n key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldError {
    /// `validation.email_required`
    EmailRequired,
    /// `validation.email_invalid`
    EmailInvalid,
    /// `validation.name_required`
    NameRequired,
    /// `validation.name_too_long` (`{max}` = [`NAME_MAX`])
    NameTooLong,
    /// `validation.name_invalid`
    NameInvalid,
    /// `flash.error.password_too_short`
    PasswordTooShort,
    /// `validation.password_too_long` (`{max}` = [`PASSWORD_MAX`])
    PasswordTooLong,
    /// `flash.error.passwords_dont_match`
    PasswordsDontMatch,
}

impl FieldError {
    /// The i18n key of this problem (spec table).
    pub fn message_key(self) -> &'static str {
        match self {
            FieldError::EmailRequired => "validation.email_required",
            FieldError::EmailInvalid => "validation.email_invalid",
            FieldError::NameRequired => "validation.name_required",
            FieldError::NameTooLong => "validation.name_too_long",
            FieldError::NameInvalid => "validation.name_invalid",
            FieldError::PasswordTooShort => "flash.error.password_too_short",
            FieldError::PasswordTooLong => "validation.password_too_long",
            FieldError::PasswordsDontMatch => "flash.error.passwords_dont_match",
        }
    }

    /// The text shown under the field.
    pub fn text(self, i18n: &I18n) -> String {
        match self {
            FieldError::NameTooLong => t!(i18n, "validation.name_too_long", max = NAME_MAX),
            FieldError::PasswordTooLong => {
                t!(i18n, "validation.password_too_long", max = PASSWORD_MAX)
            }
            other => i18n.t(other.message_key()),
        }
    }
}

/// The error of every auth server fn. Carries no internal detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum AuthError {
    #[error("validation failed")]
    Validation(Vec<(Field, FieldError)>),
    #[error("email and password required")]
    MissingCredentials,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("too many failed attempts, retry in {minutes} min")]
    TooManyAttempts { minutes: u32 },
    #[error("too many requests")]
    TooManyRequests,
    #[error("invalid or expired registration link")]
    InvalidRegistrationToken,
    #[error("invalid or expired reset link")]
    InvalidResetToken,
    #[error("authentication required")]
    Unauthorized,
    #[error("internal server error")]
    Internal,
}

impl AuthError {
    /// i18n key for the non-field variants (spec table). `Validation` has one key per field
    /// error ([`FieldError::message_key`]); here it returns the key of the first one.
    pub fn message_key(&self) -> &'static str {
        match self {
            AuthError::Validation(errors) => errors
                .first()
                .map(|(_, e)| e.message_key())
                .unwrap_or("error.generic.message"),
            AuthError::MissingCredentials => "flash.error.email_password_required",
            AuthError::InvalidCredentials => "flash.error.invalid_credentials",
            AuthError::TooManyAttempts { .. } => "flash.error.too_many_attempts",
            AuthError::TooManyRequests => "flash.error.too_many_requests",
            AuthError::InvalidRegistrationToken => "flash.error.registration_link_invalid",
            AuthError::InvalidResetToken => "flash.error.reset_token_expired",
            AuthError::Unauthorized => "auth.login.required",
            AuthError::Internal => "error.generic.message",
        }
    }

    /// The text shown to the user, e.g. `TooManyAttempts` through `t_plural!` with the
    /// `minutes` count.
    pub fn text(&self, i18n: &I18n) -> String {
        match self {
            AuthError::Validation(errors) => errors
                .first()
                .map(|(_, e)| e.text(i18n))
                .unwrap_or_else(|| i18n.t("error.generic.message")),
            AuthError::TooManyAttempts { minutes } => t_plural!(
                i18n,
                "flash.error.too_many_attempts",
                i64::from(*minutes),
                minutes = *minutes
            ),
            other => i18n.t(other.message_key()),
        }
    }

    /// HTTP status of the server fn response carrying this error.
    pub fn status_code(&self) -> u16 {
        match self {
            AuthError::Validation(_) | AuthError::MissingCredentials => 422,
            AuthError::InvalidCredentials | AuthError::Unauthorized => 401,
            AuthError::TooManyAttempts { .. } | AuthError::TooManyRequests => 429,
            AuthError::InvalidRegistrationToken | AuthError::InvalidResetToken => 400,
            AuthError::Internal => 500,
        }
    }

    /// The validation problem of `field`, if any.
    pub fn field_error(&self, field: Field) -> Option<FieldError> {
        match self {
            AuthError::Validation(errors) => {
                errors.iter().find(|(f, _)| *f == field).map(|(_, e)| *e)
            }
            _ => None,
        }
    }
}

impl FromServerFnError for AuthError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(_value: ServerFnErrorErr) -> Self {
        // Transport problems carry no detail that is safe to show.
        AuthError::Internal
    }
}

/// What the registration completion page shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingRegistration {
    pub email: String,
}

/// Logs in; redirects to `safe_next(next)` or the dashboard.
#[server(prefix = "/api", endpoint = "login")]
pub async fn login(email: String, password: String, next: Option<String>) -> Result<(), AuthError> {
    crate::server::respond::with_error_status(
        crate::server::auth_flow::login(email, password, next).await,
    )
}

/// Deletes the session, flashes `flash.success.logged_out`, redirects to `/`.
#[server(prefix = "/api", endpoint = "logout")]
pub async fn logout() -> Result<(), AuthError> {
    crate::server::respond::with_error_status(crate::server::auth_flow::logout().await)
}

/// Step 1 of registration: always `Ok` for a well-formed email.
#[server(prefix = "/api", endpoint = "register")]
pub async fn register(email: String, next: Option<String>) -> Result<(), AuthError> {
    crate::server::respond::with_error_status(crate::server::auth_flow::register(email, next).await)
}

/// Read-only: the email a registration link belongs to.
#[server(prefix = "/api", endpoint = "get_pending_registration")]
pub async fn get_pending_registration(token: String) -> Result<PendingRegistration, AuthError> {
    crate::server::respond::with_error_status(
        crate::server::auth_flow::get_pending_registration(token).await,
    )
}

/// Step 2 of registration: creates the user, logs in, redirects.
#[server(prefix = "/api", endpoint = "complete_registration")]
pub async fn complete_registration(
    token: String,
    name: String,
    password: String,
    password_confirm: String,
) -> Result<(), AuthError> {
    crate::server::respond::with_error_status(
        crate::server::auth_flow::complete_registration(token, name, password, password_confirm)
            .await,
    )
}

/// Forgot password: always `Ok` for a well-formed email.
#[server(prefix = "/api", endpoint = "request_password_reset")]
pub async fn request_password_reset(email: String) -> Result<(), AuthError> {
    crate::server::respond::with_error_status(
        crate::server::auth_flow::request_password_reset(email).await,
    )
}

/// Read-only validity check of a reset token.
#[server(prefix = "/api", endpoint = "check_reset_token")]
pub async fn check_reset_token(token: String) -> Result<(), AuthError> {
    crate::server::respond::with_error_status(
        crate::server::auth_flow::check_reset_token(token).await,
    )
}

/// Sets a new password; redirects to `/auth/login` with `flash.success.password_reset`.
#[server(prefix = "/api", endpoint = "reset_password")]
pub async fn reset_password(
    token: String,
    password: String,
    password_confirm: String,
) -> Result<(), AuthError> {
    crate::server::respond::with_error_status(
        crate::server::auth_flow::reset_password(token, password, password_confirm).await,
    )
}
