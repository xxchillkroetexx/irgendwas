//! Minimal groups foundation for invitation links (#59).
use super::{auth, authz, rate_limit::Limit, state};
use crate::{
    AppError,
    groups_api::{GroupSummary, InvitationPreview},
};
use chrono::TimeDelta;
use sqlx::FromRow;

const INVALID_CODES: Limit = Limit {
    max: 10,
    window: TimeDelta::minutes(10),
};

pub fn normalize_code(code: &str) -> String {
    code.trim()
        .chars()
        .filter(|c| *c != ' ' && *c != '-')
        .flat_map(char::to_uppercase)
        .collect()
}

/// A bounded path segment for form-error redirects; never interpolate untrusted paths.
pub fn return_code(code: &str) -> String {
    let code = normalize_code(code);
    if code.len() <= 32
        && !code.is_empty()
        && code
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        code
    } else {
        "INVALID".into()
    }
}

#[derive(FromRow)]
struct InvitationRow {
    id: i32,
    name: String,
    closed: bool,
}

async fn lookup(
    code: &str,
    user_id: i32,
    lock: bool,
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
) -> Result<InvitationRow, AppError> {
    let state = state()?;
    let key = format!("groups:code:{user_id}");
    if !state.rate_limiter.try_acquire(&key, INVALID_CODES) {
        return Err(AppError::Validation(
            "group.invitation.tooManyAttempts".into(),
        ));
    }
    // Binary lookup also protects against accent-insensitive legacy table collation.
    let query = if lock {
        "SELECT id, name, (COALESCE(is_drawn, FALSE) OR (registration_deadline IS NOT NULL AND registration_deadline < CURRENT_TIMESTAMP)) AS closed FROM `groups` WHERE BINARY invitation_code = BINARY ? FOR UPDATE"
    } else {
        "SELECT id, name, (COALESCE(is_drawn, FALSE) OR (registration_deadline IS NOT NULL AND registration_deadline < CURRENT_TIMESTAMP)) AS closed FROM `groups` WHERE BINARY invitation_code = BINARY ?"
    };
    let result = sqlx::query_as::<_, InvitationRow>(query)
        .bind(code)
        .fetch_optional(&mut **tx)
        .await;
    match result {
        Ok(Some(row)) => {
            state.rate_limiter.release(&key);
            Ok(row)
        }
        Ok(None) => Err(AppError::Validation("group.invitation.invalid".into())),
        Err(err) => {
            state.rate_limiter.release(&key);
            Err(err.into())
        }
    }
}

pub async fn preview(code: String) -> Result<InvitationPreview, AppError> {
    let user = auth::require_user().await?;
    let state = state()?;
    let mut tx = state.pool.begin().await?;
    let row = lookup(&normalize_code(&code), user.id, false, &mut tx).await?;
    let access = authz::group_access(&mut *tx, user.id, row.id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(InvitationPreview {
        group_id: row.id,
        name: row.name,
        already_member: access.is_member || access.is_admin,
        closed: row.closed,
    })
}

pub async fn join(code: String) -> Result<(), AppError> {
    let user = auth::require_user().await?;
    let state = state()?;
    let mut tx = state.pool.begin().await?;
    // Lock group before membership lookup/insert, matching the draw's lock order.
    let row = lookup(&normalize_code(&code), user.id, true, &mut tx).await?;
    let access = authz::group_access(&mut *tx, user.id, row.id)
        .await?
        .ok_or(AppError::NotFound)?;
    // Existing membership always wins, even after the draw/deadline.
    if !access.is_member && !access.is_admin {
        if row.closed {
            return Err(AppError::Validation("group.invitation.closed".into()));
        }
        sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES (?, ?)")
            .bind(row.id)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    leptos_axum::redirect(&format!("/groups/{}", row.id));
    Ok(())
}

pub async fn group(group_id: i32) -> Result<GroupSummary, AppError> {
    let user = auth::require_user().await?;
    let state = state()?;
    authz::require_member(&state.pool, user.id, group_id).await?;
    let (name, description): (String, Option<String>) =
        sqlx::query_as("SELECT name, description FROM `groups` WHERE id = ?")
            .bind(group_id)
            .fetch_one(&state.pool)
            .await?;
    Ok(GroupSummary { name, description })
}
