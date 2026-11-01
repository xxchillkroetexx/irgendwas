//! Session → current user.
//!
//! [`load_user`] is the single choke point that turns a session into a [`CurrentUser`].
//! Every other helper (and every later check such as a `users.session_epoch` comparison)
//! goes through it. A session whose user no longer exists, or whose user id cannot be read,
//! counts as logged out and is cleared.

use sqlx::{FromRow, MySqlPool};
use tower_sessions::Session;

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::i18n::LOCALE_KEY;

/// Session key holding the logged-in user's id (`i32`).
pub const USER_ID_KEY: &str = "user_id";

#[derive(FromRow)]
struct UserRow {
    id: i32,
    email: String,
    name: String,
}

/// The request's [`Session`] inside a server fn (or during SSR).
pub async fn session() -> Result<Session, AppError> {
    leptos_axum::extract::<Session>()
        .await
        .map_err(|e| AppError::internal(format!("session layer missing: {e}")))
}

/// Resolves the logged-in user of `session`. Clears stale sessions.
pub async fn load_user(
    session: &Session,
    pool: &MySqlPool,
) -> Result<Option<CurrentUser>, AppError> {
    let user_id = match session.get::<i32>(USER_ID_KEY).await {
        Ok(Some(id)) => id,
        Ok(None) => return Ok(None),
        Err(tower_sessions::session::Error::SerdeJson(err)) => {
            tracing::warn!(error = %err, "unreadable user id in session, clearing it");
            session.flush().await?;
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };

    let row = sqlx::query_as::<_, UserRow>("SELECT id, email, name FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(UserRow { id, email, name }) => Ok(Some(CurrentUser { id, email, name })),
        None => {
            tracing::info!(user_id, "session refers to a deleted user, clearing it");
            session.flush().await?;
            Ok(None)
        }
    }
}

/// The logged-in user, or `None`. For use in server fns.
pub async fn current_user() -> Result<Option<CurrentUser>, AppError> {
    let session = session().await?;
    let pool = super::pool()?;
    load_user(&session, &pool).await
}

/// The logged-in user, or [`AppError::Unauthorized`]. For use in server fns.
pub async fn require_user() -> Result<CurrentUser, AppError> {
    current_user().await?.ok_or(AppError::Unauthorized)
}

/// Marks `session` as logged in as `user_id`. Issues a new session id (prevents fixation).
pub async fn login(session: &Session, user_id: i32) -> Result<(), AppError> {
    session.cycle_id().await?;
    session.insert(USER_ID_KEY, user_id).await?;
    Ok(())
}

/// Ends the login: deletes the session, keeping only the chosen locale (under a new id).
pub async fn logout(session: &Session) -> Result<(), AppError> {
    let locale: Option<String> = session.get(LOCALE_KEY).await.ok().flatten();
    session.flush().await?;
    if let Some(locale) = locale {
        session.insert(LOCALE_KEY, locale).await?;
    }
    Ok(())
}
