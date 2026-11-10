//! Exclusion server fn bodies and the data contract for the draw (spec: exclusions). Called
//! only from [`crate::exclusions_api`] (and [`load_rules`] from the draw slice).
//!
//! The acting user always comes from the session (`require_user`) and is always the giver
//! of the rules read or written (D2); a group id from the client only selects a row and goes
//! through [`authz`] (a missing group and a group the caller is not in are both `NotFound`).
//!
//! Locking (D9, plan "group row first"): add and remove run in one transaction that starts
//! with `SELECT is_drawn FROM `groups` WHERE id = ? FOR UPDATE` (the lock membership changes
//! and the draw take), then re-check membership, `is_drawn`, the target, the cap and
//! feasibility under it, so two members' rules cannot jointly slip past the feasibility
//! check and no rule commits after the draw read them. The writes run under READ COMMITTED,
//! so every check under the lock reads what committed before it (also with MariaDB's
//! snapshot isolation), and no gap locks are taken. The page read runs in one REPEATABLE
//! READ snapshot, so members and rules are consistent with each other.
//!
//! Plain HTML form posts (no JavaScript) end with `302` to the exclusions page and a
//! session flash; the server fn client gets the flash too and re-reads the page itself.

use sqlx::pool::PoolConnection;
use sqlx::{FromRow, MySql, Transaction};

use crate::domain::feasibility::{Feasibility, Infeasibility, UserId, check_feasibility};
use crate::error::AppError;
use crate::exclusions_api::{
    ExclusionError, ExclusionsPage, FeasibilityNotice, MAX_EXCLUSIONS_PER_MEMBER, MemberRef,
};
use crate::flash::FlashMessage;
use crate::paths::{exclusions_path, login_url};
use crate::server::db::{self, IsolationLevel};
use crate::server::draw::MAX_DRAW_MEMBERS;
use crate::server::{AppState, auth, authz, flash, respond};

/// Where a plain post goes when the caller may not see the group.
const LIST_PATH: &str = "/groups";

// ------------------------------------------------------------- draw contract

/// The group's rules as `(user_id, excluded_user_id)` pairs, ordered by `user_id,
/// excluded_user_id` (spec: "Data contract for the draw slice"). Each pair means "the giver
/// `user_id` must not be assigned to `excluded_user_id`". Both are current members of the
/// group (composite foreign keys, migration 0007).
///
/// The draw calls this inside its group-row lock (`FOR UPDATE` on `groups`) and passes the
/// result with the members to [`check_feasibility`]. The pairs are internal: never render
/// them, and never log the ids.
pub async fn load_rules(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
) -> Result<Vec<(UserId, UserId)>, AppError> {
    let rules = sqlx::query_as::<_, (i32, i32)>(
        "SELECT user_id, excluded_user_id FROM exclusion_rules WHERE group_id = ? \
         ORDER BY user_id, excluded_user_id",
    )
    .bind(group_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rules)
}

// ------------------------------------------------------------------- errors

/// Logs a database error without its message (it can quote values) and returns `Internal`.
fn db_error(context: &'static str, err: sqlx::Error) -> ExclusionError {
    tracing::error!(
        context,
        error = %crate::server::db::redacted(&err),
        "exclusions: database error"
    );
    ExclusionError::Internal
}

fn from_app(err: AppError) -> ExclusionError {
    match err {
        AppError::Unauthorized => ExclusionError::Unauthorized,
        AppError::NotFound => ExclusionError::NotFound,
        _ => ExclusionError::Internal,
    }
}

fn state() -> Result<AppState, ExclusionError> {
    crate::server::state().map_err(|_| ExclusionError::Internal)
}

/// The logged-in user's id, or `Unauthorized`.
async fn user_id() -> Result<i32, ExclusionError> {
    auth::require_user()
        .await
        .map(|user| user.id)
        .map_err(from_app)
}

// ------------------------------------------------------------- transactions

/// Begins a transaction on `conn` with an explicit isolation level ([`db::begin_with`]).
async fn begin<'c>(
    conn: &'c mut PoolConnection<MySql>,
    level: IsolationLevel,
    context: &'static str,
) -> Result<Transaction<'c, MySql>, ExclusionError> {
    db::begin_with(conn, level)
        .await
        .map_err(|e| db_error(context, e))
}

async fn acquire(state: &AppState) -> Result<PoolConnection<MySql>, ExclusionError> {
    state
        .pool
        .acquire()
        .await
        .map_err(|e| db_error("acquire", e))
}

/// Step 1–3 of add and remove: the group row `FOR UPDATE` (gone: `NotFound`), the caller's
/// membership under the lock (`NotFound`), then `is_drawn` (NULL counts as false:
/// `AlreadyDrawn`).
async fn lock_open_group(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    user_id: i32,
) -> Result<(), ExclusionError> {
    let is_drawn = sqlx::query_scalar::<_, Option<bool>>(
        "SELECT is_drawn FROM `groups` WHERE id = ? FOR UPDATE",
    )
    .bind(group_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| db_error("lock group", e))?
    .ok_or(ExclusionError::NotFound)?;
    authz::require_member(&mut **tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    if is_drawn.unwrap_or(false) {
        return Err(ExclusionError::AlreadyDrawn);
    }
    Ok(())
}

/// The current members' ids.
async fn member_ids(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
) -> Result<Vec<i32>, ExclusionError> {
    sqlx::query_scalar::<_, i32>("SELECT user_id FROM group_members WHERE group_id = ?")
        .bind(group_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| db_error("members", e))
}

// --------------------------------------------------------- plain form posts

async fn push_flash(message: FlashMessage) {
    if let Err(err) = flash::flash(message).await {
        tracing::warn!(?err, "exclusions: could not store the result flash");
    }
}

/// The end of add and remove. Success: the flash `success` is stored; a plain form post
/// gets `302` to the exclusions page, the server fn client re-reads the page itself. A
/// failed plain post goes back to the page with the error as a flash, to the group list if
/// the caller may not see the group, or to the login page (returning to the page) without a
/// session.
pub async fn outcome(
    group_id: i32,
    result: Result<(), ExclusionError>,
    success: &'static str,
) -> Result<(), ExclusionError> {
    let plain = respond::is_plain_form_post();
    let back = exclusions_path(group_id);
    match &result {
        Ok(()) => {
            push_flash(FlashMessage::success(success)).await;
            if plain {
                respond::redirect_document(&back);
            }
        }
        Err(ExclusionError::Unauthorized) if plain => {
            respond::redirect_document(&login_url(&back));
        }
        Err(err) if plain => {
            push_flash(err.flash()).await;
            let to = match err {
                ExclusionError::NotFound => LIST_PATH,
                _ => &back,
            };
            respond::redirect_document(to);
        }
        Err(_) => {}
    }
    result
}

// ------------------------------------------------------------------- reads

#[derive(FromRow)]
struct GroupRow {
    name: String,
    is_drawn: Option<bool>,
}

#[derive(FromRow)]
struct MemberRow {
    user_id: i32,
    name: String,
    /// The caller has a rule against this member.
    excluded: bool,
}

pub async fn get_exclusions(group_id: i32) -> Result<ExclusionsPage, ExclusionError> {
    let me = user_id().await?;
    let state = state()?;
    let mut conn = acquire(&state).await?;
    // One snapshot for the membership check, the members and the rules.
    let mut tx = begin(
        &mut conn,
        IsolationLevel::RepeatableRead,
        "get_exclusions: begin",
    )
    .await?;
    authz::require_member(&mut *tx, me, group_id)
        .await
        .map_err(from_app)?;
    let group = sqlx::query_as::<_, GroupRow>("SELECT name, is_drawn FROM `groups` WHERE id = ?")
        .bind(group_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| db_error("get_exclusions: group", e))?
        .ok_or(ExclusionError::NotFound)?;
    let is_drawn = group.is_drawn.unwrap_or(false);
    let members = sqlx::query_as::<_, MemberRow>(
        "SELECT u.id AS user_id, u.name, \
                EXISTS(SELECT 1 FROM exclusion_rules e \
                       WHERE e.group_id = m.group_id AND e.user_id = ? \
                         AND e.excluded_user_id = m.user_id) AS excluded \
         FROM group_members m JOIN users u ON u.id = m.user_id \
         WHERE m.group_id = ? ORDER BY u.name, u.id",
    )
    .bind(me)
    .bind(group_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| db_error("get_exclusions: members", e))?;
    // The whole group's rules feed the one-bit notice only; they never leave this fn.
    let too_large = members.len() > MAX_DRAW_MEMBERS;
    let rules = if is_drawn || too_large {
        Vec::new()
    } else {
        load_rules(&mut tx, group_id).await.map_err(from_app)?
    };
    tx.rollback()
        .await
        .map_err(|e| db_error("get_exclusions: end", e))?;

    let member_ref = |row: &MemberRow| MemberRef {
        user_id: row.user_id,
        name: row.name.clone(),
    };
    let my_rules: Vec<MemberRef> = members
        .iter()
        .filter(|row| row.excluded && row.user_id != me)
        .map(member_ref)
        .collect();
    let candidates: Vec<MemberRef> = if is_drawn {
        Vec::new()
    } else {
        members
            .iter()
            .filter(|row| !row.excluded && row.user_id != me)
            .map(member_ref)
            .collect()
    };
    let notice = if is_drawn {
        None
    } else if too_large {
        Some(FeasibilityNotice::GroupTooLarge {
            max: MAX_DRAW_MEMBERS as u32,
        })
    } else {
        let ids: Vec<i32> = members.iter().map(|row| row.user_id).collect();
        match check_feasibility(&ids, &rules) {
            // Fewer than 2 members: the group page and the draw cover that case.
            Feasibility::Feasible | Feasibility::Infeasible(Infeasibility::TooFewMembers) => None,
            Feasibility::Infeasible(_) => Some(FeasibilityNotice::DrawImpossible {
                // Computed from my own rules only (spec): nobody left besides me and them.
                no_receiver_for_me: members.iter().all(|row| row.user_id == me || row.excluded),
            }),
        }
    };
    let max_rules = MAX_EXCLUSIONS_PER_MEMBER;
    let can_add = !is_drawn
        && !too_large
        && my_rules.len() < usize::from(max_rules)
        && !candidates.is_empty();
    Ok(ExclusionsPage {
        group_id,
        group_name: group.name,
        is_drawn,
        my_rules,
        candidates,
        max_rules,
        can_add,
        notice,
    })
}

// ------------------------------------------------------------------ writes

pub async fn add_exclusion(group_id: i32, excluded_user_id: i32) -> Result<(), ExclusionError> {
    let me = user_id().await?;
    let state = state()?;
    // Keeps non-members from taking the group lock; re-checked under it.
    authz::require_member(&state.pool, me, group_id)
        .await
        .map_err(from_app)?;

    let mut conn = acquire(&state).await?;
    let mut tx = begin(
        &mut conn,
        IsolationLevel::ReadCommitted,
        "add_exclusion: begin",
    )
    .await?;
    lock_open_group(&mut tx, group_id, me).await?;
    if excluded_user_id == me {
        return Err(ExclusionError::CannotExcludeSelf);
    }
    let members = member_ids(&mut tx, group_id).await?;
    if !members.contains(&excluded_user_id) {
        return Err(ExclusionError::TargetNotMember);
    }
    if members.len() > MAX_DRAW_MEMBERS {
        // Preserve idempotence and allow deleting existing rules in oversized groups.
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM exclusion_rules WHERE group_id = ? AND user_id = ? AND excluded_user_id = ?)")
            .bind(group_id).bind(me).bind(excluded_user_id)
            .fetch_one(&mut *tx).await.map_err(|e| db_error("add_exclusion: existing", e))?;
        return if exists {
            Ok(())
        } else {
            Err(ExclusionError::GroupTooLarge {
                max: MAX_DRAW_MEMBERS as u32,
            })
        };
    }
    let rules = load_rules(&mut tx, group_id).await.map_err(from_app)?;
    if rules.contains(&(me, excluded_user_id)) {
        // Idempotent (a double click): no second row, and it does not count.
        return Ok(());
    }
    let mine = rules.iter().filter(|(giver, _)| *giver == me).count();
    if mine >= usize::from(MAX_EXCLUSIONS_PER_MEMBER) {
        return Err(ExclusionError::TooManyExclusions {
            max: MAX_EXCLUSIONS_PER_MEMBER,
        });
    }
    let mut with_new = rules;
    with_new.push((me, excluded_user_id));
    if check_feasibility(&members, &with_new) != Feasibility::Feasible {
        return Err(ExclusionError::WouldMakeDrawImpossible);
    }
    sqlx::query(
        "INSERT INTO exclusion_rules (group_id, user_id, excluded_user_id) VALUES (?, ?, ?)",
    )
    .bind(group_id)
    .bind(me)
    .bind(excluded_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| db_error("add_exclusion: insert", e))?;
    tx.commit()
        .await
        .map_err(|e| db_error("add_exclusion: commit", e))?;
    tracing::info!(user_id = me, group_id, "exclusion added");
    Ok(())
}

pub async fn remove_exclusion(group_id: i32, excluded_user_id: i32) -> Result<(), ExclusionError> {
    let me = user_id().await?;
    let state = state()?;
    authz::require_member(&state.pool, me, group_id)
        .await
        .map_err(from_app)?;

    let mut conn = acquire(&state).await?;
    let mut tx = begin(
        &mut conn,
        IsolationLevel::ReadCommitted,
        "remove_exclusion: begin",
    )
    .await?;
    lock_open_group(&mut tx, group_id, me).await?;
    // Only ever the caller's own rule; none is fine too (idempotent).
    sqlx::query(
        "DELETE FROM exclusion_rules WHERE group_id = ? AND user_id = ? AND excluded_user_id = ?",
    )
    .bind(group_id)
    .bind(me)
    .bind(excluded_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| db_error("remove_exclusion: delete", e))?;
    tx.commit()
        .await
        .map_err(|e| db_error("remove_exclusion: commit", e))?;
    tracing::info!(user_id = me, group_id, "exclusion removed");
    Ok(())
}
