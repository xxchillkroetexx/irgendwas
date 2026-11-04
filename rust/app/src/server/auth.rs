//! Session → current user.
//!
//! [`load_user`] is the single choke point that turns a session into a [`CurrentUser`].
//! Every other helper (and every later check such as a `users.session_epoch` comparison)
//! goes through it. A session whose user no longer exists, whose user id cannot be read, or
//! whose `session_epoch` differs from `users.session_epoch` (password reset/change) counts
//! as logged out and is cleared.

use sqlx::{FromRow, MySqlPool};
use tower_sessions::Session;

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::i18n::LOCALE_KEY;

/// Session key holding the logged-in user's id (`i32`).
pub const USER_ID_KEY: &str = "user_id";
/// Session key holding `users.session_epoch` at login (`u32`; missing counts as 0).
pub const SESSION_EPOCH_KEY: &str = "session_epoch";

#[derive(FromRow)]
struct UserRow {
    id: i32,
    email: String,
    name: String,
    session_epoch: u32,
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

    let epoch = match session.get::<u32>(SESSION_EPOCH_KEY).await {
        Ok(epoch) => epoch.unwrap_or(0),
        Err(tower_sessions::session::Error::SerdeJson(err)) => {
            tracing::warn!(error = %err, "unreadable session epoch, clearing the session");
            session.flush().await?;
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };

    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, name, session_epoch FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) if row.session_epoch == epoch => Ok(Some(CurrentUser {
            id: row.id,
            email: row.email,
            name: row.name,
        })),
        Some(_) => {
            tracing::info!(user_id, "session epoch is outdated, clearing the session");
            session.flush().await?;
            Ok(None)
        }
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

/// Marks `session` as logged in as `user_id` at session epoch 0 (a new account). Issues a
/// new session id (prevents fixation). Existing accounts use [`login_with_epoch`].
pub async fn login(session: &Session, user_id: i32) -> Result<(), AppError> {
    login_with_epoch(session, user_id, 0).await
}

/// Marks `session` as logged in as `user_id` with the user's current `users.session_epoch`.
/// Issues a new session id (prevents fixation).
pub async fn login_with_epoch(session: &Session, user_id: i32, epoch: u32) -> Result<(), AppError> {
    session.cycle_id().await?;
    session.insert(USER_ID_KEY, user_id).await?;
    session.insert(SESSION_EPOCH_KEY, epoch).await?;
    Ok(())
}

/// Ends the login: deletes the session, keeping only the chosen locale (under a new id).
pub async fn logout(session: &Session) -> Result<(), AppError> {
    let locale: Option<String> = session.get(LOCALE_KEY).await.ok().flatten();
    // flush() retains the cached record's id in tower-sessions. The logout flash
    // would then recreate that id as an anonymous session. Cycle first (loading
    // the record fallibly), then clear the cached data: retained locale/flash
    // gets a genuinely fresh id, and a failed initial load cannot retain auth.
    session.cycle_id().await?;
    session.clear().await;
    if let Some(locale) = locale {
        session.insert(LOCALE_KEY, locale).await?;
    }
    Ok(())
}
