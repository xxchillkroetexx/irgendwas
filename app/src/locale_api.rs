//! The language switch (spec: i18n-polish, `set_locale`).
//!
//! `POST /api/set_locale` with the form fields `locale` and `next`. The nav posts it from a
//! native `<form method="post">` (works without JavaScript and always ends in a full page
//! load: the locale is fixed per page load). The body lives in `server::locale_flow`.
//!
//! Not instrumented with `tracing`: `next` can contain a reset or registration token.

use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Endpoint of [`set_locale`], the `action` of the nav's switch forms.
pub const SET_LOCALE_PATH: &str = "/api/set_locale";

/// The error of [`set_locale`]: an [`AppError`] on the wire (same JSON), but arguments
/// that do not deserialise (e.g. a POST without `locale`) become `NotFound` (404), like
/// the other slices' undeserialisable arguments, instead of a 500.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(transparent)]
#[error(transparent)]
pub struct SetLocaleError(pub AppError);

impl From<AppError> for SetLocaleError {
    fn from(err: AppError) -> Self {
        SetLocaleError(err)
    }
}

impl crate::error::HttpStatus for SetLocaleError {
    fn status_code(&self) -> u16 {
        self.0.status_code()
    }
}

impl FromServerFnError for SetLocaleError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        let err = match value {
            ServerFnErrorErr::Args(_) | ServerFnErrorErr::MissingArg(_) => AppError::NotFound,
            _ => AppError::Internal,
        };
        // `next` is unknown here: a plain form post goes to `/` (never the Referer).
        #[cfg(feature = "ssr")]
        let _ = crate::server::locale_flow::respond_error::<()>(Err(err.clone()), "/");
        SetLocaleError(err)
    }
}

/// Remembers `locale` (session, `locale` cookie and, when logged in, `users.locale`) and
/// redirects to `safe_return_path(next)` or `/`. An unsupported locale is
/// `Validation("validation.locale_unsupported")` and changes nothing.
#[server(prefix = "/api", endpoint = "set_locale")]
pub async fn set_locale(locale: String, next: Option<String>) -> Result<(), SetLocaleError> {
    crate::server::locale_flow::set_locale(locale, next)
        .await
        .map_err(SetLocaleError)
}
