//! Locale persistence (session + `locale` cookie) and the account/mail locale helpers.
//!
//! Login (auth) calls [`adopt_account_locale`] and attaches the returned cookie with
//! [`append_set_cookie`]; the language switch (i18n-polish) uses [`remember_locale`].
//! Mails to an account build their texts with [`mail_i18n`].

use tower_sessions::Session;

use crate::error::AppError;
use crate::i18n::{I18n, LOCALE_KEY, Locale};
use crate::server::Config;

/// Lifetime of the `locale` cookie: one year.
pub const LOCALE_COOKIE_MAX_AGE: i64 = 31_536_000;

/// `Set-Cookie` value of the `locale` cookie. HttpOnly is fine: the client reads
/// `<html lang>`, never the cookie.
pub fn locale_cookie(locale: Locale, secure: bool) -> String {
    let mut cookie = format!(
        "{LOCALE_KEY}={}; Path=/; Max-Age={LOCALE_COOKIE_MAX_AGE}; SameSite=Lax; HttpOnly",
        locale.code()
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Stores `locale` in the session and returns the `locale` cookie the caller must attach
/// (`secure` = `config.secure_cookies()`).
pub async fn remember_locale(
    session: &Session,
    locale: Locale,
    secure: bool,
) -> Result<String, AppError> {
    set_session_locale(session, locale).await?;
    Ok(locale_cookie(locale, secure))
}

/// Adopts the account language (`users.locale`) at login: if `stored` is a known locale,
/// it goes into the session and its cookie is returned; otherwise nothing changes.
pub async fn adopt_account_locale(
    session: &Session,
    stored: Option<&str>,
    secure: bool,
) -> Result<Option<String>, AppError> {
    match stored.and_then(Locale::from_code) {
        Some(locale) => remember_locale(session, locale, secure).await.map(Some),
        None => Ok(None),
    }
}

/// Appends a `Set-Cookie` header to the current server fn's response
/// (`leptos_axum::ResponseOptions`). Outside a server fn / SSR render it only logs.
pub fn append_set_cookie(value: &str) {
    use axum::http::{HeaderValue, header::SET_COOKIE};

    let Some(response) = leptos::prelude::use_context::<leptos_axum::ResponseOptions>() else {
        tracing::warn!("append_set_cookie called without ResponseOptions; cookie dropped");
        return;
    };
    match HeaderValue::from_str(value) {
        Ok(value) => response.append_header(SET_COOKIE, value),
        Err(err) => tracing::error!(%err, "invalid Set-Cookie value; cookie dropped"),
    }
}

/// Account language from the stored `users.locale`; NULL or an unknown value → `default`.
pub fn account_locale(stored: Option<&str>, default: Locale) -> Locale {
    stored.and_then(Locale::from_code).unwrap_or(default)
}

/// Translator for every mail to an account: its `users.locale` (else `APP_LOCALE`) and
/// `APP_TIMEZONE`.
pub fn mail_i18n(stored: Option<&str>, config: &Config) -> I18n {
    I18n::new(
        account_locale(stored, config.app_locale),
        config.app_timezone,
    )
}

/// The raw `users.locale` of `user_id` (interpret it with [`account_locale`]).
/// No such user → `NotFound`.
pub async fn load_account_locale<'e>(
    executor: impl sqlx::MySqlExecutor<'e>,
    user_id: i32,
) -> Result<Option<String>, AppError> {
    let locale: Option<String> = sqlx::query_scalar("SELECT locale FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(executor)
        .await?;
    Ok(locale)
}

pub async fn session_locale(session: &Session) -> Result<Option<Locale>, AppError> {
    let code: Option<String> = session.get(LOCALE_KEY).await?;
    Ok(code.as_deref().and_then(Locale::from_code))
}

pub async fn set_session_locale(session: &Session, locale: Locale) -> Result<(), AppError> {
    session.insert(LOCALE_KEY, locale.code()).await?;
    Ok(())
}

/// Value of the `locale` cookie in a `Cookie` header.
pub fn cookie_locale(cookie_header: &str) -> Option<Locale> {
    cookie_header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == LOCALE_KEY)
        .and_then(|(_, value)| Locale::from_code(value.trim_matches('"')))
}
