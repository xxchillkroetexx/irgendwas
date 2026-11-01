//! Account server functions and their error type (spec: account, "Server functions").
//!
//! The endpoints are `POST /api/<fn name>` with form-urlencoded arguments named like the
//! Rust parameters; errors are the JSON-encoded [`AccountError`] of a non-2xx response. The
//! user always comes from the session, never from an argument. The bodies live in
//! `server::account_flow`.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

use crate::auth_api::{NAME_MAX, PASSWORD_MAX};
use crate::flash::FlashMessage;
use crate::i18n::I18n;
use crate::t;

/// The error of every account server fn. Carries no internal detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum AccountError {
    #[error("authentication required")]
    Unauthorized,
    #[error("name required")]
    NameRequired,
    #[error("name contains control characters")]
    NameInvalid,
    #[error("name too long")]
    NameTooLong,
    #[error("password too short")]
    PasswordTooShort,
    #[error("password too long")]
    PasswordTooLong,
    #[error("passwords do not match")]
    PasswordsDontMatch,
    #[error("current password incorrect")]
    CurrentPasswordIncorrect,
    #[error("invalid email")]
    InvalidEmail,
    #[error("email unchanged")]
    EmailUnchanged,
    #[error("email change requested too soon")]
    EmailChangeTooSoon,
    #[error("mail could not be sent")]
    MailSendFailed,
    #[error("invalid or expired token")]
    InvalidOrExpiredToken,
    #[error("email address taken")]
    EmailTaken,
    #[error("too many password attempts, retry in {minutes} min")]
    TooManyAttempts { minutes: u32 },
    #[error("internal server error")]
    Internal,
}

impl AccountError {
    /// The i18n key of the message shown for this error (spec: account, "i18n keys").
    pub fn message_key(&self) -> &'static str {
        match self {
            AccountError::Unauthorized => "auth.login.required",
            AccountError::NameRequired => "flash.error.name_required",
            AccountError::NameInvalid => "flash.error.name_invalid",
            AccountError::NameTooLong => "flash.error.name_too_long",
            AccountError::PasswordTooShort => "flash.error.password_too_short",
            AccountError::PasswordTooLong => "validation.password_too_long",
            AccountError::PasswordsDontMatch => "flash.error.passwords_dont_match",
            AccountError::CurrentPasswordIncorrect => "flash.error.current_password_incorrect",
            AccountError::InvalidEmail => "flash.error.invalid_email",
            AccountError::EmailUnchanged => "flash.error.email_unchanged",
            AccountError::EmailChangeTooSoon => "flash.error.email_change_too_soon",
            AccountError::MailSendFailed => "flash.error.mail_send_failed",
            AccountError::InvalidOrExpiredToken => "flash.error.email_verification_failed",
            AccountError::EmailTaken => "flash.error.email_already_in_use",
            AccountError::TooManyAttempts { .. } => "flash.error.too_many_password_attempts",
            AccountError::Internal => "error.generic.message",
        }
    }

    /// The text shown to the user (fills the `{max}` of the length limits).
    pub fn text(&self, i18n: &I18n) -> String {
        match self {
            AccountError::NameTooLong => t!(i18n, "flash.error.name_too_long", max = NAME_MAX),
            AccountError::PasswordTooLong => {
                t!(i18n, "validation.password_too_long", max = PASSWORD_MAX)
            }
            AccountError::TooManyAttempts { minutes } => crate::t_plural!(
                i18n,
                "flash.error.too_many_password_attempts",
                i64::from(*minutes),
                minutes = *minutes
            ),
            other => i18n.t(other.message_key()),
        }
    }

    /// The message as a flash (for a plain HTML form post, AC41/AC44); renders to the same
    /// text as [`AccountError::text`].
    pub fn flash(&self) -> FlashMessage {
        let message = FlashMessage::error(self.message_key());
        match self {
            AccountError::NameTooLong => message.with_number("max", i64::from(NAME_MAX)),
            AccountError::PasswordTooLong => message.with_number("max", i64::from(PASSWORD_MAX)),
            AccountError::TooManyAttempts { minutes } => message
                .with_plural(i64::from(*minutes))
                .with_number("minutes", i64::from(*minutes)),
            _ => message,
        }
    }

    /// HTTP status of the server fn response carrying this error.
    pub fn status_code(&self) -> u16 {
        match self {
            AccountError::Unauthorized => 401,
            AccountError::NameRequired
            | AccountError::NameInvalid
            | AccountError::NameTooLong
            | AccountError::PasswordTooShort
            | AccountError::PasswordTooLong
            | AccountError::PasswordsDontMatch
            | AccountError::CurrentPasswordIncorrect
            | AccountError::InvalidEmail
            | AccountError::EmailUnchanged => 422,
            AccountError::EmailChangeTooSoon | AccountError::TooManyAttempts { .. } => 429,
            AccountError::InvalidOrExpiredToken => 400,
            AccountError::EmailTaken => 409,
            AccountError::MailSendFailed => 503,
            AccountError::Internal => 500,
        }
    }
}

impl crate::error::HttpStatus for AccountError {
    fn status_code(&self) -> u16 {
        AccountError::status_code(self)
    }
}

impl FromServerFnError for AccountError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(_value: ServerFnErrorErr) -> Self {
        AccountError::Internal
    }
}

/// What the profile page shows. The pending fields are `None` if no change is pending or
/// it has expired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileView {
    pub name: String,
    pub email: String,
    pub pending_email: Option<String>,
    pub pending_expires_at: Option<DateTime<Utc>>,
}

/// Result of a confirmed email change: whether the caller has a session (profile link vs
/// login link on the result page).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmResult {
    pub logged_in: bool,
}

/// The caller's name, email and live pending change.
#[server(prefix = "/api", endpoint = "get_profile")]
pub async fn get_profile() -> Result<ProfileView, AccountError> {
    crate::server::respond::with_error_status(crate::server::account_flow::get_profile().await)
}

/// Stores the trimmed name.
#[server(prefix = "/api", endpoint = "update_name")]
pub async fn update_name(name: String) -> Result<(), AccountError> {
    let result = crate::server::account_flow::update_name(name).await;
    crate::server::respond::with_error_status(
        crate::server::account_flow::plain_form_outcome(result, "flash.success.profile_updated")
            .await,
    )
}

/// New password; logs out every other session of the caller.
#[server(prefix = "/api", endpoint = "change_password")]
pub async fn change_password(
    current_password: String,
    new_password: String,
    new_password_confirm: String,
) -> Result<(), AccountError> {
    let result = crate::server::account_flow::change_password(
        current_password,
        new_password,
        new_password_confirm,
    )
    .await;
    crate::server::respond::with_error_status(
        crate::server::account_flow::plain_form_outcome(result, "flash.success.password_changed")
            .await,
    )
}

/// Starts an email change: mails a verification link to the new address.
#[server(prefix = "/api", endpoint = "request_email_change")]
pub async fn request_email_change(
    new_email: String,
    current_password: String,
) -> Result<(), AccountError> {
    let result =
        crate::server::account_flow::request_email_change(new_email, current_password).await;
    crate::server::respond::with_error_status(
        crate::server::account_flow::plain_form_outcome(
            result,
            "flash.success.email_verification_sent",
        )
        .await,
    )
}

/// Drops the pending email change (idempotent).
#[server(prefix = "/api", endpoint = "cancel_email_change")]
pub async fn cancel_email_change() -> Result<(), AccountError> {
    let result = crate::server::account_flow::cancel_email_change().await;
    crate::server::respond::with_error_status(
        crate::server::account_flow::plain_form_outcome(
            result,
            "flash.success.email_change_cancelled",
        )
        .await,
    )
}

/// Applies the email change of the token's owner. Needs no session.
#[server(prefix = "/api", endpoint = "confirm_email_change")]
pub async fn confirm_email_change(token: String) -> Result<ConfirmResult, AccountError> {
    let result = crate::server::account_flow::confirm_email_change(token).await;
    crate::server::respond::with_error_status(
        crate::server::account_flow::plain_confirm_outcome(result).await,
    )
}
