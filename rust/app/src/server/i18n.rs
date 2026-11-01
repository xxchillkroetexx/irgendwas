//! Locale persistence (session + cookie). The language switch (i18n-polish slice) should
//! call [`set_session_locale`] and also set the `locale` cookie so the choice survives
//! logout and session expiry.

use tower_sessions::Session;

use crate::error::AppError;
use crate::i18n::{LOCALE_KEY, Locale};

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
