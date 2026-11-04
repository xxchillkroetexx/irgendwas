//! Group authorization. The only sanctioned way for server fns to turn a client-supplied
//! group id into access rights:
//!
//! ```ignore
//! let user = require_user().await?;
//! let access = authz::require_member(&pool, user.id, group_id).await?;
//! ```
//!
//! Errors (groups spec D2, no existence leak):
//! - [`require_member`]: [`AppError::NotFound`] if the group does not exist **or** the user
//!   is not a member, so group ids cannot be enumerated.
//! - [`require_admin`]: [`AppError::NotFound`] for a missing group or a non-member,
//!   [`AppError::Forbidden`] for a member who is not the admin.
//!
//! The functions take any executor, so they also work inside a transaction (`&mut *tx`).

use sqlx::{Executor, FromRow, MySql};

use crate::error::AppError;

/// Proof that `user_id` may access `group_id`, and in which role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupAccess {
    pub group_id: i32,
    pub user_id: i32,
    /// `groups.admin_id == user_id`.
    pub is_admin: bool,
    /// A `group_members` row exists.
    pub is_member: bool,
}

#[derive(FromRow)]
struct AccessRow {
    admin_id: i32,
    is_member: bool,
}

/// Loads the caller's relation to a group (without deciding anything).
pub async fn group_access<'e, E>(
    db: E,
    user_id: i32,
    group_id: i32,
) -> Result<Option<GroupAccess>, AppError>
where
    E: Executor<'e, Database = MySql>,
{
    let row = sqlx::query_as::<_, AccessRow>(
        "SELECT g.admin_id, \
                EXISTS(SELECT 1 FROM group_members gm WHERE gm.group_id = g.id AND gm.user_id = ?) AS is_member \
         FROM `groups` g WHERE g.id = ?",
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| GroupAccess {
        group_id,
        user_id,
        is_admin: r.admin_id == user_id,
        is_member: r.is_member,
    }))
}

/// The user must be a member of the group (the admin always counts as a member).
/// Missing group and non-member both give [`AppError::NotFound`].
pub async fn require_member<'e, E>(
    db: E,
    user_id: i32,
    group_id: i32,
) -> Result<GroupAccess, AppError>
where
    E: Executor<'e, Database = MySql>,
{
    let access = group_access(db, user_id, group_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if access.is_member || access.is_admin {
        Ok(access)
    } else {
        Err(AppError::NotFound)
    }
}

/// The user must be the group's admin (`groups.admin_id`). Missing group or non-member:
/// [`AppError::NotFound`]; member but not admin: [`AppError::Forbidden`].
pub async fn require_admin<'e, E>(
    db: E,
    user_id: i32,
    group_id: i32,
) -> Result<GroupAccess, AppError>
where
    E: Executor<'e, Database = MySql>,
{
    let access = group_access(db, user_id, group_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if access.is_admin {
        Ok(access)
    } else if access.is_member {
        Err(AppError::Forbidden)
    } else {
        Err(AppError::NotFound)
    }
}
