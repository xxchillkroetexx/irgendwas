//! The bodies of the account server fns (spec: account, "Server functions"). Called only
//! from [`crate::account_api`]. The user id always comes from the session (`require_user`),
//! never from an argument. Logs carry user ids only: no address, token, link or password.

use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::account_api::{AccountError, ConfirmResult, PasswordChanged, ProfileView};
use crate::auth_api::FieldError;
use crate::error::AppError;
use crate::flash::FlashMessage;
use crate::paths::{PROFILE_PATH, login_url};
use crate::server::{
    AppState, account_mail, auth, auth_mail, flash, i18n, password, rate_limit, respond, token,
};
use crate::validation::{normalize_email, password_errors, validate_email, validate_name};

/// How long a verification link is valid, in hours (spec D6).
const VERIFY_HOURS: i64 = 24;
/// Minimum time between two email-change requests, in seconds (spec D7).
const REQUEST_INTERVAL_SECS: i64 = 60;
/// Length of a token as sent in links (32 bytes, base64url without padding).
const TOKEN_LEN: usize = 43;

/// Logs a database error without its message (it can quote values such as an email) and
/// returns `Internal`.
fn db_error(context: &'static str, err: sqlx::Error) -> AccountError {
    tracing::error!(
        context,
        error = %crate::server::db::redacted(&err),
        "account: database error"
    );
    AccountError::Internal
}

/// Logs an unexpected error whose text carries no user data and returns `Internal`.
fn internal(context: &'static str, err: impl std::fmt::Display) -> AccountError {
    tracing::error!(context, error = %err, "account: internal error");
    AccountError::Internal
}

fn password_error(context: &'static str, err: password::PasswordError) -> AccountError {
    match err {
        password::PasswordError::Busy => AccountError::Busy,
        other => internal(context, other),
    }
}

fn state() -> Result<AppState, AccountError> {
    crate::server::state().map_err(|_| AccountError::Internal)
}

/// The logged-in user's id, or `Unauthorized`.
async fn user_id() -> Result<i32, AccountError> {
    match auth::require_user().await {
        Ok(user) => Ok(user.id),
        Err(AppError::Unauthorized) => Err(AccountError::Unauthorized),
        Err(_) => Err(AccountError::Internal),
    }
}

/// Verifies the caller's current password against the stored hash (on the blocking pool),
/// under the `account:pw:<user_id>` limit (AC40). The attempt is reserved *before* argon2
/// runs, so concurrent calls cannot all get past the check; a failure keeps it counted, a
/// success clears the counter. A used-up limit answers `TooManyAttempts` without verifying.
async fn check_current_password(
    state: &AppState,
    user_id: i32,
    password: String,
    stored_hash: String,
) -> Result<(), AccountError> {
    let key = rate_limit::account_password_key(user_id);
    if let Err(wait) = state
        .rate_limiter
        .try_acquire_all(&[(&key, rate_limit::ACCOUNT_PASSWORD)])
    {
        return Err(AccountError::TooManyAttempts {
            minutes: rate_limit::minutes_rounded_up(wait),
        });
    }
    let verified = match password::verify_async(password, Some(stored_hash)).await {
        Ok(verified) => verified,
        Err(err) => {
            // Overload is not an incorrect password and must not use an attempt.
            state.rate_limiter.release(&key);
            return Err(password_error("verify_password", err));
        }
    };
    if verified {
        state.rate_limiter.clear(&key);
        Ok(())
    } else {
        Err(AccountError::CurrentPasswordIncorrect)
    }
}

// --------------------------------------------------------- plain form posts

/// Stores `message` for the next page and answers the plain form post with `302 to`.
async fn flash_and_redirect(message: FlashMessage, to: &str) {
    if let Err(err) = flash::flash(message).await {
        tracing::warn!(?err, "account: could not store the result flash");
    }
    respond::redirect_document(to);
}

/// A plain HTML form post on the profile (no JavaScript, or before hydration; AC44) ends on
/// the profile with the result as a flash: `success_key` or the error's message. The server
/// fn client is left alone (the page shows the result in the card).
pub async fn plain_form_outcome<T>(
    result: Result<T, AccountError>,
    success_key: &'static str,
) -> Result<T, AccountError> {
    if respond::is_plain_form_post() {
        match &result {
            Ok(_) => flash_and_redirect(FlashMessage::success(success_key), PROFILE_PATH).await,
            // No session: the login page explains itself (`next` is set) and returns here.
            Err(AccountError::Unauthorized) => respond::redirect_document(&login_url(PROFILE_PATH)),
            Err(err) => flash_and_redirect(err.flash(), PROFILE_PATH).await,
        }
    }
    result
}

/// A plain form post of the verify page's confirm button (AC41) ends on the profile with
/// the result as a flash. Without a session the profile would only redirect to the login
/// page (and that redirect would take the flash), so the caller goes straight to the login
/// page that returns to the profile.
pub async fn plain_confirm_outcome(
    result: Result<ConfirmResult, AccountError>,
) -> Result<ConfirmResult, AccountError> {
    if respond::is_plain_form_post() {
        let (message, logged_in) = match &result {
            Ok(confirmed) => (
                FlashMessage::success("flash.success.email_verified"),
                confirmed.logged_in,
            ),
            Err(err) => (
                err.flash(),
                matches!(auth::current_user().await, Ok(Some(_))),
            ),
        };
        let to = if logged_in {
            PROFILE_PATH.to_string()
        } else {
            login_url(PROFILE_PATH)
        };
        flash_and_redirect(message, &to).await;
    }
    result
}

// ------------------------------------------------------------------- profile

#[derive(FromRow)]
struct ProfileRow {
    name: String,
    email: String,
    pending_email: Option<String>,
    email_verification_expires: Option<DateTime<Utc>>,
}

pub async fn get_profile() -> Result<ProfileView, AccountError> {
    let user_id = user_id().await?;
    let state = state()?;
    let row = sqlx::query_as::<_, ProfileRow>(
        "SELECT name, email, pending_email, email_verification_expires FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| db_error("get_profile", e))?
    .ok_or(AccountError::Unauthorized)?;

    // An expired change is as good as none.
    let now = Utc::now();
    let (pending_email, pending_expires_at) =
        match (row.pending_email, row.email_verification_expires) {
            (Some(pending), Some(expires)) if expires > now => (Some(pending), Some(expires)),
            _ => (None, None),
        };
    Ok(ProfileView {
        name: row.name,
        email: row.email,
        pending_email,
        pending_expires_at,
    })
}

pub async fn update_name(name: String) -> Result<(), AccountError> {
    let user_id = user_id().await?;
    let name = validate_name(&name).map_err(|e| match e {
        FieldError::NameRequired => AccountError::NameRequired,
        FieldError::NameInvalid => AccountError::NameInvalid,
        FieldError::NameTooLong => AccountError::NameTooLong,
        _ => AccountError::Internal,
    })?;
    let state = state()?;
    sqlx::query("UPDATE users SET name = ? WHERE id = ?")
        .bind(&name)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| db_error("update_name", e))?;
    Ok(())
}

// ------------------------------------------------------------------ password

pub async fn change_password(
    current_password: String,
    new_password: String,
    new_password_confirm: String,
) -> Result<PasswordChanged, AccountError> {
    let user_id = user_id().await?;
    // Length first (too short, then too long), then the confirmation.
    if let Some((_, problem)) = password_errors(&new_password, &new_password_confirm)
        .into_iter()
        .next()
    {
        return Err(match problem {
            FieldError::PasswordTooShort => AccountError::PasswordTooShort,
            FieldError::PasswordTooLong => AccountError::PasswordTooLong,
            FieldError::PasswordsDontMatch => AccountError::PasswordsDontMatch,
            _ => AccountError::Internal,
        });
    }
    let state = state()?;
    let stored: String = sqlx::query_scalar("SELECT password FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| db_error("change_password: load", e))?
        .ok_or(AccountError::Unauthorized)?;
    check_current_password(&state, user_id, current_password, stored.clone()).await?;
    let hash = password::hash_async(new_password)
        .await
        .map_err(|e| password_error("change_password: hash", e))?;

    // One statement for the credentials, epoch, and outstanding recovery tokens. The old hash in the
    // condition: a concurrent change in between makes this one fail instead of winning.
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| db_error("change_password: begin", e))?;
    // Inspect the pending change under the same row lock as its cancellation.
    let pending_live: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(pending_email IS NOT NULL AND \
         email_verification_expires > CURRENT_TIMESTAMP, 0) AS SIGNED) \
         FROM users WHERE id = ? FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| db_error("change_password: lock", e))?
    .ok_or(AccountError::Unauthorized)?;
    let updated = sqlx::query(
        "UPDATE users SET password = ?, session_epoch = session_epoch + 1, \
         reset_token = NULL, reset_token_expires = NULL, pending_email = NULL, \
         email_verification_token = NULL, email_verification_expires = NULL \
         WHERE id = ? AND password = ?",
    )
    .bind(&hash)
    .bind(user_id)
    .bind(&stored)
    .execute(&mut *tx)
    .await
    .map_err(|e| db_error("change_password: update", e))?;
    if updated.rows_affected() == 0 {
        return Err(AccountError::CurrentPasswordIncorrect);
    }
    let epoch: u32 = sqlx::query_scalar("SELECT session_epoch FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| db_error("change_password: epoch", e))?;
    tx.commit()
        .await
        .map_err(|e| db_error("change_password: commit", e))?;
    tracing::info!(user_id, "password changed");

    // Every other session now fails the epoch check; this one carries the new epoch under
    // a new session id.
    let session = auth::session().await.map_err(|_| AccountError::Internal)?;
    auth::login_with_epoch(&session, user_id, epoch)
        .await
        .map_err(|_| AccountError::Internal)?;
    Ok(PasswordChanged {
        pending_email_cancelled: pending_live != 0,
    })
}

// -------------------------------------------------------------- email change

#[derive(FromRow)]
struct EmailChangeRow {
    email: String,
    name: String,
    password: String,
    session_epoch: u32,
    locale: Option<String>,
}

/// The three pending columns as they were before a request (restored if the mail fails).
#[derive(FromRow)]
struct PendingColumns {
    pending_email: Option<String>,
    email_verification_token: Option<String>,
    email_verification_expires: Option<DateTime<Utc>>,
    session_epoch: u32,
    /// 1 if a live change was issued less than [`REQUEST_INTERVAL_SECS`] ago.
    too_soon: i64,
}

pub async fn request_email_change(
    new_email: String,
    current_password: String,
) -> Result<(), AccountError> {
    let user_id = user_id().await?;
    let new_email = normalize_email(&new_email);
    validate_email(&new_email).map_err(|_| AccountError::InvalidEmail)?;
    let state = state()?;

    let user = sqlx::query_as::<_, EmailChangeRow>(
        "SELECT email, name, password, session_epoch, locale FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| db_error("request_email_change: load", e))?
    .ok_or(AccountError::Unauthorized)?;
    // Stored addresses are normalised, so a byte comparison is the right one.
    if new_email == user.email {
        return Err(AccountError::EmailUnchanged);
    }
    check_current_password(&state, user_id, current_password, user.password).await?;
    // No check whether another account uses or awaits the address (non-enumerating, D4).

    let token = token::generate().map_err(|_| AccountError::Internal)?;
    let token_hash = token::hash(&token);
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| db_error("request_email_change: begin", e))?;
    let before = sqlx::query_as::<_, PendingColumns>(
        "SELECT pending_email, email_verification_token, email_verification_expires, session_epoch, \
         CAST(COALESCE(pending_email IS NOT NULL \
              AND email_verification_expires > CURRENT_TIMESTAMP \
              AND email_verification_expires > CURRENT_TIMESTAMP \
                  + INTERVAL ? HOUR - INTERVAL ? SECOND, 0) AS SIGNED) AS too_soon \
         FROM users WHERE id = ? FOR UPDATE",
    )
    .bind(VERIFY_HOURS)
    .bind(REQUEST_INTERVAL_SECS)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| db_error("request_email_change: lock", e))?;
    // Password reset/change increments the epoch while holding this same row lock.
    // A request verified before that change must not issue a fresh token afterwards.
    if before.session_epoch != user.session_epoch {
        return Err(AccountError::CurrentPasswordIncorrect);
    }
    if before.too_soon != 0 {
        return Err(AccountError::EmailChangeTooSoon);
    }
    // Auth's per-recipient mail budget (AC43), shared with registration mails. Counted
    // only here, after the account's own throttle passed; over it, nothing is written.
    if !state.rate_limiter.try_acquire(
        &rate_limit::mail_email_key(&new_email),
        rate_limit::MAIL_PER_EMAIL,
    ) {
        tracing::info!(user_id, "email change refused: recipient mail limit");
        return Err(AccountError::EmailChangeTooSoon);
    }
    sqlx::query(
        "UPDATE users SET pending_email = ?, email_verification_token = ?, \
         email_verification_expires = CURRENT_TIMESTAMP + INTERVAL ? HOUR WHERE id = ?",
    )
    .bind(&new_email)
    .bind(&token_hash)
    .bind(VERIFY_HOURS)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| db_error("request_email_change: update", e))?;
    // The mail goes to the address as stored with this token.
    let stored_pending: String = sqlx::query_scalar("SELECT pending_email FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| db_error("request_email_change: reread", e))?;
    tx.commit()
        .await
        .map_err(|e| db_error("request_email_change: commit", e))?;

    let i18n = i18n::mail_i18n(user.locale.as_deref(), &state.config);
    let link = auth_mail::app_link(&state.config, &format!("/user/verify-email/{token}"));
    let message = account_mail::verify_email(&i18n, &stored_pending, &user.name, &link);
    let handle = state.mailer.send(message);
    let mail_id = handle.id();
    // The mailer logs the transport error itself (recipient domain only).
    if handle.outcome().await.is_err() {
        tracing::warn!(user_id, mail_id, "verification mail not delivered");
        // Put back what was there before, unless a newer request replaced it meanwhile.
        sqlx::query(
            "UPDATE users SET pending_email = ?, email_verification_token = ?, \
             email_verification_expires = ? WHERE id = ? AND email_verification_token = ?",
        )
        .bind(before.pending_email)
        .bind(before.email_verification_token)
        .bind(before.email_verification_expires)
        .bind(user_id)
        .bind(&token_hash)
        .execute(&state.pool)
        .await
        .map_err(|e| db_error("request_email_change: restore", e))?;
        return Err(AccountError::MailSendFailed);
    }
    tracing::info!(user_id, "email change requested");
    Ok(())
}

pub async fn cancel_email_change() -> Result<(), AccountError> {
    let user_id = user_id().await?;
    let state = state()?;
    sqlx::query(
        "UPDATE users SET pending_email = NULL, email_verification_token = NULL, \
         email_verification_expires = NULL WHERE id = ?",
    )
    .bind(user_id)
    .execute(&state.pool)
    .await
    .map_err(|e| db_error("cancel_email_change", e))?;
    Ok(())
}

/// 43 characters of the base64url alphabet.
fn is_token_shaped(token: &str) -> bool {
    token.len() == TOKEN_LEN
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(FromRow)]
struct TokenOwner {
    id: i32,
    email: String,
    pending_email: String,
    locale: Option<String>,
}

/// Clears the pending change that `token_hash` belongs to.
async fn clear_pending(
    state: &AppState,
    user_id: i32,
    token_hash: &str,
) -> Result<(), AccountError> {
    sqlx::query(
        "UPDATE users SET pending_email = NULL, email_verification_token = NULL, \
         email_verification_expires = NULL WHERE id = ? AND email_verification_token = ?",
    )
    .bind(user_id)
    .bind(token_hash)
    .execute(&state.pool)
    .await
    .map_err(|e| db_error("confirm_email_change: clear", e))?;
    Ok(())
}

pub async fn confirm_email_change(token: String) -> Result<ConfirmResult, AccountError> {
    if !is_token_shaped(&token) {
        return Err(AccountError::InvalidOrExpiredToken);
    }
    let state = state()?;
    let token_hash = token::hash(&token);
    let owner = sqlx::query_as::<_, TokenOwner>(
        "SELECT id, email, pending_email, locale FROM users \
         WHERE email_verification_token = ? AND pending_email IS NOT NULL \
         AND email_verification_expires > CURRENT_TIMESTAMP",
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| db_error("confirm_email_change: lookup", e))?
    .ok_or(AccountError::InvalidOrExpiredToken)?;

    // Byte-wise (utf8mb4_bin): a lookalike of another account's address is not that address.
    let taken: Option<i32> = sqlx::query_scalar("SELECT id FROM users WHERE email = ? AND id <> ?")
        .bind(&owner.pending_email)
        .bind(owner.id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| db_error("confirm_email_change: taken", e))?;
    if taken.is_some() {
        clear_pending(&state, owner.id, &token_hash).await?;
        tracing::info!(user_id = owner.id, "email change refused: address taken");
        return Err(AccountError::EmailTaken);
    }

    // The token condition again: exactly one of concurrent confirms wins.
    let updated = sqlx::query(
        "UPDATE users SET email = pending_email, pending_email = NULL, \
         email_verification_token = NULL, email_verification_expires = NULL, \
         reset_token = NULL, reset_token_expires = NULL \
         WHERE id = ? AND email_verification_token = ? AND pending_email IS NOT NULL \
         AND email_verification_expires > CURRENT_TIMESTAMP",
    )
    .bind(owner.id)
    .bind(&token_hash)
    .execute(&state.pool)
    .await;
    match updated {
        Ok(result) if result.rows_affected() == 0 => {
            return Err(AccountError::InvalidOrExpiredToken);
        }
        Ok(_) => {}
        // Taken in the meantime (a concurrent registration or email change).
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            clear_pending(&state, owner.id, &token_hash).await?;
            tracing::info!(
                user_id = owner.id,
                "email change refused: address taken meanwhile"
            );
            return Err(AccountError::EmailTaken);
        }
        Err(err) => return Err(db_error("confirm_email_change: update", err)),
    }
    tracing::info!(user_id = owner.id, "email changed");

    // Best effort: the notice to the old address, in the owner's account language.
    let i18n = i18n::mail_i18n(owner.locale.as_deref(), &state.config);
    let message = account_mail::email_changed(&i18n, &owner.email, &owner.pending_email);
    let handle = state.mailer.send(message);
    let mail_id = handle.id();
    let user_id = owner.id;
    tokio::spawn(async move {
        if handle.outcome().await.is_err() {
            tracing::warn!(user_id, mail_id, "email-changed notice not delivered");
        }
    });

    // Sessions are not touched (D8); the caller may be anyone, or nobody.
    let logged_in = matches!(auth::current_user().await, Ok(Some(_)));
    Ok(ConfirmResult { logged_in })
}
