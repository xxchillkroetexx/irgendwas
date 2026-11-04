//! The body of the `set_locale` server fn (spec: i18n-polish). Called only from
//! [`crate::locale_api`]. Never logs its arguments: `next` can contain a token.

use crate::error::AppError;
use crate::i18n::Locale;
use crate::paths::safe_return_path;
use crate::server::{auth, i18n, respond, state};

/// Where the switch returns to: the validated `next`, else `/`. The `Referer` is never read.
fn return_target(next: Option<&str>) -> String {
    next.and_then(safe_return_path)
        .unwrap_or_else(|| "/".to_string())
}

/// Finishes a failed call. The server fn client gets the error's status; a plain form post
/// gets a 302 to `target` (otherwise leptos would redirect it to the `Referer`), with
/// nothing changed and no flash.
pub fn respond_error<T>(result: Result<T, AppError>, target: &str) -> Result<T, AppError> {
    if result.is_err() && respond::is_plain_form_post() {
        respond::redirect_document(target);
    }
    respond::with_error_status(result)
}

pub async fn set_locale(locale: String, next: Option<String>) -> Result<(), AppError> {
    let target = return_target(next.as_deref());
    let result = switch(&locale, &target).await;
    respond_error(result, &target)
}

async fn switch(locale: &str, target: &str) -> Result<(), AppError> {
    let locale = Locale::from_code(locale)
        .ok_or_else(|| AppError::Validation("validation.locale_unsupported".into()))?;

    let state = state()?;
    let session = auth::session().await?;
    // A stale session is cleared here, so the locale below lands in the session that
    // stays, and a stale login never writes to `users`.
    let user = auth::load_user(&session, &state.pool).await?;
    // The account first: if it fails, session and cookie stay as they were.
    if let Some(user) = user {
        sqlx::query("UPDATE users SET locale = ? WHERE id = ?")
            .bind(locale.code())
            .bind(user.id)
            .execute(&state.pool)
            .await?;
    }
    let cookie = i18n::remember_locale(&session, locale, state.config.secure_cookies()).await?;
    i18n::append_set_cookie(&cookie);
    // A full page load in both cases: plain form posts get a 302, the server fn client a
    // `Location` without leptos' client-side redirect header.
    respond::redirect_document(target);
    Ok(())
}
