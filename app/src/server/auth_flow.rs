//! The bodies of the auth server fns (spec: auth, "Server fns"). Called only from
//! [`crate::auth_api`]; each function reads the request context (session, client IP,
//! locale) itself and never trusts an id from the client.

use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::auth_api::{AuthError, Field, PendingRegistration};
use crate::flash::FlashMessage;
use crate::i18n::{I18n, request_locale};
use crate::paths::next_or_dashboard;
use crate::server::rate_limit::{
    self, LOGIN_PER_EMAIL, LOGIN_PER_IP, MAIL_PER_EMAIL, login_email_key, login_ip_key,
    mail_email_key, mail_ip_key,
};
use crate::server::{AppState, auth, auth_mail, client_ip, flash, i18n, password, respond, token};
use crate::validation::{
    EMAIL_MAX, normalize_email, password_errors, validate_email, validate_name,
};

/// Logs an unexpected non-database error (never shown to the client) and returns
/// `Internal`. Only for errors whose text carries no user data.
pub fn internal(err: impl std::fmt::Display) -> AuthError {
    tracing::error!(error = %err, "auth: internal error");
    AuthError::Internal
}

fn password_error(err: password::PasswordError) -> AuthError {
    match err {
        password::PasswordError::Busy => AuthError::TooManyRequests,
        other => internal(other),
    }
}

/// Logs a database error without its message (it can quote values such as an email) and
/// returns `Internal`.
pub fn db_error(err: sqlx::Error) -> AuthError {
    tracing::error!(error = %crate::server::db::redacted(&err), "auth: database error");
    AuthError::Internal
}

fn state() -> Result<AppState, AuthError> {
    crate::server::state().map_err(|_| AuthError::Internal)
}

// Scope token admission by database as well as token: isolated app instances and
// test databases must not share a budget merely because a token happens to match.
fn token_work_key(state: &AppState, token_hash: &str) -> String {
    let options = state.pool.connect_options();
    format!(
        "{}:{}:{}:{token_hash}",
        options.get_host(),
        options.get_port(),
        options.get_database().unwrap_or_default()
    )
}

async fn session() -> Result<tower_sessions::Session, AuthError> {
    auth::session().await.map_err(|_| AuthError::Internal)
}

/// Rate-limiter key part for the caller's address.
fn ip_key_part() -> String {
    client_ip::client_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// A normalised, valid email or the `Validation` error for it.
fn checked_email(raw: &str) -> Result<String, AuthError> {
    let email = normalize_email(raw);
    validate_email(&email).map_err(|e| AuthError::Validation(vec![(Field::Email, e)]))?;
    Ok(email)
}

// ------------------------------------------------------------------------ login

#[derive(FromRow)]
struct LoginRow {
    id: i32,
    password: String,
    last_login: Option<DateTime<Utc>>,
    session_epoch: u32,
    locale: Option<String>,
}

pub async fn login(email: String, password: String, next: Option<String>) -> Result<(), AuthError> {
    if email.trim().is_empty() || password.is_empty() {
        return Err(AuthError::MissingCredentials);
    }
    let state = state()?;
    let email = normalize_email(&email);
    let email_key = login_email_key(&email);
    let ip_key = login_ip_key(&ip_key_part());
    let limiter = &state.rate_limiter;

    // Reserve the attempt as a failure *before* the slow verification, so concurrent
    // attempts cannot all pass the check. A success gives it back below.
    if let Err(wait) =
        limiter.try_acquire_all(&[(&email_key, LOGIN_PER_EMAIL), (&ip_key, LOGIN_PER_IP)])
    {
        return Err(AuthError::TooManyAttempts {
            minutes: rate_limit::minutes_rounded_up(wait),
        });
    }

    // Longer than any stored address: no lookup, handled exactly like an unknown email.
    let row = if email.chars().count() <= EMAIL_MAX {
        let found = sqlx::query_as::<_, LoginRow>(
            "SELECT id, password, last_login, session_epoch, locale FROM users WHERE email = ?",
        )
        .bind(&email)
        .fetch_optional(&state.pool)
        .await;
        match found {
            Ok(row) => row,
            Err(err) => {
                // Not the caller's failure: do not count it.
                limiter.release(&email_key);
                limiter.release(&ip_key);
                return Err(db_error(err));
            }
        }
    } else {
        None
    };

    let stored_hash = row.as_ref().map(|r| r.password.clone());
    let verified = match password::verify_async(password.clone(), stored_hash).await {
        Ok(verified) => verified,
        Err(err) => {
            // Capacity errors are not failed credentials and must not lock a user out.
            limiter.release(&email_key);
            limiter.release(&ip_key);
            return Err(password_error(err));
        }
    };
    let user = match row {
        Some(user) if verified => user,
        // The reserved attempt stays counted as the failure.
        _ => return Err(AuthError::InvalidCredentials),
    };
    // Success: the email counter is cleared, the IP keeps only its failures.
    limiter.clear(&email_key);
    limiter.release(&ip_key);

    if password::needs_rehash(&user.password) {
        match password::hash_async(password.clone()).await {
            Ok(hash) => {
                // A password change/reset may have committed since verification.
                // Never restore the old password over newer credentials.
                let updated = sqlx::query(
                    "UPDATE users SET password = ? WHERE id = ? AND BINARY password = BINARY ? AND session_epoch = ?",
                )
                    .bind(hash)
                    .bind(user.id)
                    .bind(&user.password)
                    .bind(user.session_epoch)
                    .execute(&state.pool)
                    .await
                    .map_err(db_error)?;
                if updated.rows_affected() == 0 {
                    // Another login may have upgraded the same hash first. Accept
                    // that case only after verifying the latest credentials again.
                    let latest = sqlx::query_as::<_, (String, u32)>(
                        "SELECT password, session_epoch FROM users WHERE id = ?",
                    )
                    .bind(user.id)
                    .fetch_optional(&state.pool)
                    .await
                    .map_err(db_error)?;
                    let Some((hash, epoch)) = latest else {
                        return Err(AuthError::InvalidCredentials);
                    };
                    if epoch != user.session_epoch {
                        return Err(AuthError::InvalidCredentials);
                    }
                    if !password::verify_async(password, Some(hash))
                        .await
                        .map_err(password_error)?
                    {
                        return Err(AuthError::InvalidCredentials);
                    }
                }
            }
            Err(err) => tracing::warn!(user_id = user.id, error = %err, "password rehash failed"),
        }
    }
    // A reset or change may have committed since the password was verified,
    // even when the stored hash did not need rehashing.
    let current = sqlx::query(
        "UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = ? AND session_epoch = ?",
    )
    .bind(user.id)
    .bind(user.session_epoch)
    .execute(&state.pool)
    .await
    .map_err(db_error)?;
    if current.rows_affected() == 0 {
        return Err(AuthError::InvalidCredentials);
    }

    let session = session().await?;
    auth::login_with_epoch(&session, user.id, user.session_epoch)
        .await
        .map_err(|_| AuthError::Internal)?;
    let cookie = i18n::adopt_account_locale(
        &session,
        user.locale.as_deref(),
        state.config.secure_cookies(),
    )
    .await
    .map_err(|_| AuthError::Internal)?;
    if let Some(cookie) = cookie {
        i18n::append_set_cookie(&cookie);
    }
    let message = match user.last_login {
        Some(previous) => FlashMessage::success("flash.success.logged_in_last_login")
            .with_datetime("date", previous),
        None => FlashMessage::success("flash.success.logged_in"),
    };
    flash::push(&session, message)
        .await
        .map_err(|_| AuthError::Internal)?;
    respond::redirect_document(&next_or_dashboard(next.as_deref()));
    Ok(())
}

// ----------------------------------------------------------------------- logout

pub async fn logout() -> Result<(), AuthError> {
    let session = session().await?;
    auth::logout(&session)
        .await
        .map_err(|_| AuthError::Internal)?;
    flash::push(&session, FlashMessage::success("flash.success.logged_out"))
        .await
        .map_err(|_| AuthError::Internal)?;
    respond::redirect_document("/");
    Ok(())
}

// ----------------------------------------------------------------- registration

pub async fn register(email: String, next: Option<String>) -> Result<(), AuthError> {
    let email = checked_email(&email)?;
    let state = state()?;
    if !state.rate_limiter.try_acquire(
        &mail_ip_key(&ip_key_part()),
        rate_limit::mail_per_ip(&state.config),
    ) {
        return Err(AuthError::TooManyRequests);
    }

    sqlx::query(
        "DELETE FROM pending_registrations \
         WHERE expires_at IS NULL OR expires_at < CURRENT_TIMESTAMP",
    )
    .execute(&state.pool)
    .await
    .map_err(db_error)?;

    let existing = sqlx::query_as::<_, (i32, String, Option<String>)>(
        "SELECT id, email, locale FROM users WHERE email = ?",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_error)?;

    // Over the per-address limit: no mail and no row (the last delivered link stays valid).
    if !state
        .rate_limiter
        .try_acquire(&mail_email_key(&email), MAIL_PER_EMAIL)
    {
        return Ok(());
    }

    match existing {
        Some((user_id, stored_email, locale)) => {
            let i18n = i18n::mail_i18n(locale.as_deref(), &state.config);
            let message = auth_mail::existing_account(&i18n, &state.config, &stored_email);
            auth_mail::queue(&state, "existing_account", Some(user_id), message);
        }
        None => {
            let token = token::generate().map_err(|_| AuthError::Internal)?;
            let next_path = next.as_deref().and_then(crate::paths::safe_next);
            // Returning the stored recipient from the upsert makes this one atomic
            // operation: a concurrent registration can no longer replace our token
            // before a separate recipient lookup and turn a valid request into a 500.
            let stored_email: String = sqlx::query_scalar(
                "INSERT INTO pending_registrations (email, token_hash, next_path, created_at, expires_at) \
                 VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP + INTERVAL 24 HOUR) \
                 ON DUPLICATE KEY UPDATE token_hash = VALUES(token_hash), next_path = VALUES(next_path), \
                 created_at = VALUES(created_at), expires_at = VALUES(expires_at) RETURNING email",
            )
            .bind(&email)
            .bind(token::hash(&token))
            .bind(next_path)
            .fetch_one(&state.pool)
            .await
            .map_err(db_error)?;
            let i18n = I18n::new(request_locale(), state.config.app_timezone);
            let link =
                auth_mail::app_link(&state.config, &format!("/auth/register/confirm/{token}"));
            let message = auth_mail::registration(&i18n, &stored_email, &link);
            auth_mail::queue(&state, "registration", None, message);
        }
    }
    Ok(())
}

pub async fn get_pending_registration(token: String) -> Result<PendingRegistration, AuthError> {
    let state = state()?;
    let email: Option<String> = sqlx::query_scalar(
        "SELECT email FROM pending_registrations \
         WHERE token_hash = ? AND expires_at > CURRENT_TIMESTAMP",
    )
    .bind(token::hash(&token))
    .fetch_optional(&state.pool)
    .await
    .map_err(db_error)?;
    email
        .map(|email| PendingRegistration { email })
        .ok_or(AuthError::InvalidRegistrationToken)
}

pub async fn complete_registration(
    token: String,
    name: String,
    password: String,
    password_confirm: String,
) -> Result<(), AuthError> {
    let mut errors = Vec::new();
    let name = match validate_name(&name) {
        Ok(name) => Some(name),
        Err(e) => {
            errors.push((Field::Name, e));
            None
        }
    };
    errors.extend(password_errors(&password, &password_confirm));
    let Some(name) = name.filter(|_| errors.is_empty()) else {
        return Err(AuthError::Validation(errors));
    };

    let state = state()?;
    let token_hash = token::hash(&token);
    // Cheap check first: no argon2 work for unknown tokens. Hash outside the transaction
    // (no row lock or pool connection held during argon2), then re-check under the lock.
    get_pending_registration(token).await?;
    let hash = password::hash_for_token_async(password, token_work_key(&state, &token_hash))
        .await
        .map_err(password_error)?;

    let mut tx = state.pool.begin().await.map_err(db_error)?;
    let pending = sqlx::query_as::<_, (i32, String, Option<String>)>(
        "SELECT id, email, next_path FROM pending_registrations \
         WHERE token_hash = ? AND expires_at > CURRENT_TIMESTAMP FOR UPDATE",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    let Some((pending_id, email, next_path)) = pending else {
        return Err(AuthError::InvalidRegistrationToken);
    };
    let inserted = sqlx::query(
        "INSERT INTO users (email, name, password, locale, last_login) \
         VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)",
    )
    .bind(&email)
    .bind(&name)
    .bind(hash)
    .bind(request_locale().code())
    .execute(&mut *tx)
    .await;
    // The address was taken meanwhile (e.g. by another user's email change): the
    // registration is void. Any other error rolls back (the link keeps working).
    let user_id = match inserted {
        Ok(result) => Some(result.last_insert_id()),
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => None,
        Err(err) => return Err(db_error(err)),
    };
    sqlx::query("DELETE FROM pending_registrations WHERE id = ?")
        .bind(pending_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    let Some(user_id) = user_id else {
        tracing::info!("registration link used for an address that is taken meanwhile");
        return Err(AuthError::InvalidRegistrationToken);
    };
    let user_id = i32::try_from(user_id).map_err(internal)?;

    let session = session().await?;
    auth::login_with_epoch(&session, user_id, 0)
        .await
        .map_err(|_| AuthError::Internal)?;
    flash::push(
        &session,
        FlashMessage::success("flash.success.registration_complete").with_param("name", name),
    )
    .await
    .map_err(|_| AuthError::Internal)?;
    leptos_axum::redirect(&next_or_dashboard(next_path.as_deref()));
    Ok(())
}

// --------------------------------------------------------------- password reset

pub async fn request_password_reset(email: String) -> Result<(), AuthError> {
    let email = checked_email(&email)?;
    let state = state()?;
    if !state.rate_limiter.try_acquire(
        &mail_ip_key(&ip_key_part()),
        rate_limit::mail_per_ip(&state.config),
    ) {
        return Err(AuthError::TooManyRequests);
    }

    let user = sqlx::query_as::<_, (i32, String, String, Option<String>)>(
        "SELECT id, email, name, locale FROM users WHERE email = ?",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_error)?;

    if let Some((user_id, stored_email, name, locale)) = user
        && state
            .rate_limiter
            .try_acquire(&mail_email_key(&email), MAIL_PER_EMAIL)
    {
        let token = token::generate().map_err(|_| AuthError::Internal)?;
        let updated = sqlx::query(
            "UPDATE users SET reset_token = ?, \
             reset_token_expires = CURRENT_TIMESTAMP + INTERVAL 1 HOUR WHERE id = ? AND email = ?",
        )
        .bind(token::hash(&token))
        .bind(user_id)
        .bind(&stored_email)
        .execute(&state.pool)
        .await
        .map_err(db_error)?;
        if updated.rows_affected() != 0 {
            let i18n = i18n::mail_i18n(locale.as_deref(), &state.config);
            let link = auth_mail::app_link(&state.config, &format!("/auth/reset-password/{token}"));
            let message = auth_mail::password_reset(&i18n, &stored_email, &name, &link);
            auth_mail::queue(&state, "password_reset", Some(user_id), message);
        }
    }

    flash::flash(FlashMessage::success("flash.success.reset_instructions"))
        .await
        .map_err(|_| AuthError::Internal)?;
    leptos_axum::redirect("/auth/login");
    Ok(())
}

/// The user a valid (unexpired) reset token belongs to: (id, email).
async fn reset_token_owner(state: &AppState, token: &str) -> Result<(i32, String), AuthError> {
    sqlx::query_as::<_, (i32, String)>(
        "SELECT id, email FROM users \
         WHERE reset_token = ? AND reset_token_expires > CURRENT_TIMESTAMP",
    )
    .bind(token::hash(token))
    .fetch_optional(&state.pool)
    .await
    .map_err(db_error)?
    .ok_or(AuthError::InvalidResetToken)
}

pub async fn check_reset_token(token: String) -> Result<(), AuthError> {
    let state = state()?;
    reset_token_owner(&state, &token).await.map(|_| ())
}

pub async fn reset_password(
    token: String,
    password: String,
    password_confirm: String,
) -> Result<(), AuthError> {
    let errors = password_errors(&password, &password_confirm);
    if !errors.is_empty() {
        return Err(AuthError::Validation(errors));
    }
    let state = state()?;
    // Cheap check first: no argon2 work for unknown tokens.
    let (user_id, email) = reset_token_owner(&state, &token).await?;
    let hash =
        password::hash_for_token_async(password, token_work_key(&state, &token::hash(&token)))
            .await
            .map_err(password_error)?;
    // The token condition again in the UPDATE: exactly one of concurrent submits wins.
    let updated = sqlx::query(
        "UPDATE users SET password = ?, reset_token = NULL, reset_token_expires = NULL, \
         session_epoch = session_epoch + 1, pending_email = NULL, \
         email_verification_token = NULL, email_verification_expires = NULL \
         WHERE reset_token = ? AND reset_token_expires > CURRENT_TIMESTAMP",
    )
    .bind(hash)
    .bind(token::hash(&token))
    .execute(&state.pool)
    .await
    .map_err(db_error)?;
    if updated.rows_affected() == 0 {
        return Err(AuthError::InvalidResetToken);
    }
    tracing::info!(user_id, "password reset");
    state.rate_limiter.clear(&login_email_key(&email));

    // Every session of the user is dead now (epoch). If the caller's own session is one
    // of them, end it here so the flash below survives.
    let session = session().await?;
    if session.get::<i32>(auth::USER_ID_KEY).await.ok().flatten() == Some(user_id) {
        auth::logout(&session)
            .await
            .map_err(|_| AuthError::Internal)?;
    }
    flash::push(
        &session,
        FlashMessage::success("flash.success.password_reset"),
    )
    .await
    .map_err(|_| AuthError::Internal)?;
    leptos_axum::redirect("/auth/login");
    Ok(())
}
