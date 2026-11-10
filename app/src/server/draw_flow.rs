//! Draw server fn bodies (spec: draw, "Server functions"). Called only from
//! [`crate::draw_api`].
//!
//! The acting user always comes from the session (`require_user`); a group id from the
//! client only selects a row and goes through [`authz`] (a missing group and a group the
//! caller is not in are both `NotFound`, a member calling an admin fn gets `Forbidden`).
//!
//! Secrecy (S1–S5): receiver data is only ever read with `giver_id = <session user>`
//! ([`get_my_assignment`], [`list_my_assignments`]) and by the mail worker for the giver's
//! own mail. Nothing here logs names, receiver ids or the RNG.
//!
//! Locking (D5, plan "group row first"): `perform_draw` and `reset_draw` run in one
//! transaction that starts with `SELECT admin_id, is_drawn FROM `groups` WHERE id = ? FOR
//! UPDATE` (the lock joins, leaves, removals, transfers and rule changes take) and re-check
//! the admin and `is_drawn` under it. They run under READ COMMITTED, so everything read
//! under the lock is what committed before it, and the inserts take no gap locks. The lock
//! is held only for reading members and rules, drawing (on a blocking thread) and one
//! insert; the mails are queued after the commit.

use rand::SeedableRng;
use rand::rngs::StdRng;
use sqlx::pool::PoolConnection;
use sqlx::{FromRow, MySql, QueryBuilder, Transaction};

use crate::domain::feasibility::UserId;
use crate::draw_api::{
    DrawAdminStatus, DrawApiError, DrawBlocker, MyAssignment, MyAssignmentSummary,
    NotificationSummary,
};
use crate::error::AppError;
use crate::flash::FlashMessage;
use crate::paths::{group_path, login_url};
use crate::server::db::{self, IsolationLevel};
use crate::server::draw::{self as algorithm, DrawConfig, DrawError, infeasibility_kind};
use crate::server::draw_worker::MAX_ATTEMPTS;
use crate::server::exclusions::load_rules;
use crate::server::{AppState, auth, authz, flash, respond};

/// Where a plain post goes when the caller may not see the group.
const LIST_PATH: &str = "/groups";

// ------------------------------------------------------------------- errors

/// Logs a database error without its message (it can quote values) and returns `Internal`.
fn db_error(context: &'static str, err: sqlx::Error) -> DrawApiError {
    tracing::error!(context, error = %db::redacted(&err), "draw: database error");
    DrawApiError::Internal
}

fn from_app(err: AppError) -> DrawApiError {
    match err {
        AppError::Unauthorized => DrawApiError::Unauthorized,
        AppError::NotFound => DrawApiError::NotFound,
        AppError::Forbidden => DrawApiError::Forbidden,
        _ => DrawApiError::Internal,
    }
}

fn state() -> Result<AppState, DrawApiError> {
    crate::server::state().map_err(|_| DrawApiError::Internal)
}

/// The logged-in user's id, or `Unauthorized`.
async fn user_id() -> Result<i32, DrawApiError> {
    auth::require_user()
        .await
        .map(|user| user.id)
        .map_err(from_app)
}

fn count_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The algorithm's refusal as the server fn error. `Infeasible` becomes the generic
/// `DrawImpossible`; its kind is logged at debug level, never its ids (S5, D10).
fn map_draw_error(group_id: i32, err: DrawError) -> DrawApiError {
    match err {
        DrawError::TooFewMembers { count, min } => DrawApiError::TooFewMembers {
            count: count_u32(count),
            min: count_u32(min),
        },
        DrawError::TooManyMembers { count, max } => DrawApiError::TooManyMembers {
            count: count_u32(count),
            max: count_u32(max),
        },
        DrawError::Infeasible(reason) => {
            tracing::debug!(
                group_id,
                kind = infeasibility_kind(&reason),
                "draw impossible with the current rules"
            );
            DrawApiError::DrawImpossible
        }
    }
}

// ------------------------------------------------------------- transactions

async fn acquire(state: &AppState) -> Result<PoolConnection<MySql>, DrawApiError> {
    state
        .pool
        .acquire()
        .await
        .map_err(|e| db_error("acquire", e))
}

async fn begin<'c>(
    conn: &'c mut PoolConnection<MySql>,
    level: IsolationLevel,
    context: &'static str,
) -> Result<Transaction<'c, MySql>, DrawApiError> {
    db::begin_with(conn, level)
        .await
        .map_err(|e| db_error(context, e))
}

/// Runs `work` in `tx`: commits on success, rolls back explicitly on an error (so the group
/// row lock is released before the response).
async fn finish<T>(
    tx: Transaction<'_, MySql>,
    result: Result<T, DrawApiError>,
    context: &'static str,
) -> Result<T, DrawApiError> {
    match result {
        Ok(value) => {
            tx.commit().await.map_err(|e| db_error(context, e))?;
            Ok(value)
        }
        Err(err) => {
            if let Err(e) = tx.rollback().await {
                tracing::warn!(context, error = %db::redacted(&e), "draw: rollback failed");
            }
            Err(err)
        }
    }
}

/// D5 step 2: the group row `FOR UPDATE` (gone: `NotFound`), then the caller must still be
/// its admin (transferred meanwhile: `Forbidden`). Returns `is_drawn`.
async fn lock_group_as_admin(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    me: i32,
) -> Result<bool, DrawApiError> {
    let (admin_id, is_drawn) = sqlx::query_as::<_, (i32, bool)>(
        "SELECT admin_id, is_drawn FROM `groups` WHERE id = ? FOR UPDATE",
    )
    .bind(group_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| db_error("lock group", e))?
    .ok_or(DrawApiError::NotFound)?;
    if admin_id != me {
        return Err(DrawApiError::Forbidden);
    }
    Ok(is_drawn)
}

/// The current members' ids, ascending.
async fn member_ids(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
) -> Result<Vec<UserId>, DrawApiError> {
    sqlx::query_scalar::<_, i32>(
        "SELECT user_id FROM group_members WHERE group_id = ? ORDER BY user_id",
    )
    .bind(group_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| db_error("members", e))
}

// --------------------------------------------------------- plain form posts

async fn push_flash(message: FlashMessage) {
    if let Err(err) = flash::flash(message).await {
        tracing::warn!(?err, "draw: could not store the result flash");
    }
}

/// The end of perform, reset and resend: success stores the flash `success(&value)`; a
/// plain form post gets `302` back to the group page, the server fn client re-reads the
/// panel and the flash itself. A failed plain post goes back to the group with the error as
/// a flash, to the group list if the caller may not see the group, or to the login page
/// (returning to the group) without a session.
pub async fn outcome<T>(
    group_id: i32,
    result: Result<T, DrawApiError>,
    success: impl FnOnce(&T) -> FlashMessage,
) -> Result<T, DrawApiError> {
    let plain = respond::is_plain_form_post();
    let back = group_path(group_id);
    match &result {
        Ok(value) => {
            push_flash(success(value)).await;
            if plain {
                respond::redirect_document(&back);
            }
        }
        Err(DrawApiError::Unauthorized) if plain => {
            respond::redirect_document(&login_url(&back));
        }
        Err(err) if plain => {
            for message in err.flashes() {
                push_flash(message).await;
            }
            let to = match err {
                DrawApiError::NotFound => LIST_PATH,
                _ => &back,
            };
            respond::redirect_document(to);
        }
        Err(_) => {}
    }
    result
}

// ------------------------------------------------------------------- reads

/// The caller's own receiver (S1): the row with `giver_id = me`, only for a current member
/// (S3). Not drawn, or no row: `None` (S8). "Drawn" is the whole of D6: `is_drawn` and
/// `drawn_at` (a half-set state, which no code path writes, reveals nothing).
pub async fn get_my_assignment(group_id: i32) -> Result<Option<MyAssignment>, DrawApiError> {
    let me = user_id().await?;
    let state = state()?;
    authz::require_member(&state.pool, me, group_id)
        .await
        .map_err(from_app)?;
    sqlx::query_as::<_, (i32, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT ga.receiver_id, receiver.name, g.drawn_at \
         FROM gift_assignments ga \
         JOIN `groups` g ON g.id = ga.group_id \
         JOIN users receiver ON receiver.id = ga.receiver_id \
         WHERE ga.group_id = ? AND ga.giver_id = ? \
           AND g.is_drawn = TRUE AND g.drawn_at IS NOT NULL",
    )
    .bind(group_id)
    .bind(me)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| db_error("get_my_assignment", e))
    .map(|row| {
        row.map(|(receiver_id, receiver_name, drawn_at)| MyAssignment {
            receiver_id,
            receiver_name,
            drawn_at,
        })
    })
}

#[derive(FromRow)]
struct SummaryRow {
    group_id: i32,
    group_name: String,
    receiver_id: i32,
    receiver_name: String,
    drawn_at: chrono::DateTime<chrono::Utc>,
}

/// The caller's receivers in every drawn group they are still a member of (S3), newest draw
/// first. One query.
pub async fn list_my_assignments() -> Result<Vec<MyAssignmentSummary>, DrawApiError> {
    let me = user_id().await?;
    let state = state()?;
    let rows = sqlx::query_as::<_, SummaryRow>(
        "SELECT g.id AS group_id, g.name AS group_name, ga.receiver_id, \
                receiver.name AS receiver_name, \
                g.drawn_at \
         FROM gift_assignments ga \
         JOIN `groups` g ON g.id = ga.group_id \
         JOIN group_members gm ON gm.group_id = ga.group_id AND gm.user_id = ga.giver_id \
         JOIN users receiver ON receiver.id = ga.receiver_id \
         WHERE ga.giver_id = ? AND g.is_drawn = TRUE AND g.drawn_at IS NOT NULL \
         ORDER BY g.drawn_at DESC, g.id DESC",
    )
    .bind(me)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| db_error("list_my_assignments", e))?;
    Ok(rows
        .into_iter()
        .map(|row| MyAssignmentSummary {
            group_id: row.group_id,
            group_name: row.group_name,
            receiver_id: row.receiver_id,
            receiver_name: row.receiver_name,
            drawn_at: row.drawn_at,
        })
        .collect())
}

#[derive(FromRow)]
struct StatusGroupRow {
    is_drawn: bool,
    drawn_at: Option<chrono::DateTime<chrono::Utc>>,
    registration_deadline: Option<chrono::DateTime<chrono::Utc>>,
}

/// Delivery state of one row, by giver (the giver's name is shown only when failed).
#[derive(FromRow)]
struct DeliveryRow {
    giver_name: String,
    notification_sent: bool,
    notification_attempts: u8,
}

/// The admin's status (S2: no field about receivers). The blocker comes from the same steps
/// 0–2 as the draw, read in one snapshot without locking; `perform_draw` re-checks under the
/// lock.
pub async fn get_draw_admin_status(group_id: i32) -> Result<DrawAdminStatus, DrawApiError> {
    let me = user_id().await?;
    let state = state()?;
    authz::require_admin(&state.pool, me, group_id)
        .await
        .map_err(from_app)?;

    let mut conn = acquire(&state).await?;
    let mut tx = begin(
        &mut conn,
        IsolationLevel::RepeatableRead,
        "get_draw_admin_status: begin",
    )
    .await?;
    let read = read_status(&mut tx, group_id).await;
    if let Err(e) = tx.rollback().await {
        tracing::warn!(error = %db::redacted(&e), "draw: rollback of the status read failed");
    }
    read
}

async fn read_status(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
) -> Result<DrawAdminStatus, DrawApiError> {
    let group = sqlx::query_as::<_, StatusGroupRow>(
        "SELECT is_drawn, drawn_at, registration_deadline FROM `groups` WHERE id = ?",
    )
    .bind(group_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| db_error("status: group", e))?
    .ok_or(DrawApiError::NotFound)?;
    let members = member_ids(tx, group_id).await?;
    let member_count = count_u32(members.len());

    if !group.is_drawn {
        let rules = load_rules(tx, group_id).await.map_err(from_app)?;
        let blocker = match algorithm::precheck(&members, &rules) {
            Ok(()) => None,
            Err(err) => Some(match map_draw_error(group_id, err) {
                DrawApiError::TooFewMembers { count, min } => {
                    DrawBlocker::TooFewMembers { count, min }
                }
                DrawApiError::TooManyMembers { count, max } => {
                    DrawBlocker::TooManyMembers { count, max }
                }
                _ => DrawBlocker::DrawImpossible,
            }),
        };
        let now = chrono::Utc::now();
        return Ok(DrawAdminStatus {
            is_drawn: false,
            drawn_at: None,
            member_count,
            blocker,
            registration_open_until: group.registration_deadline.filter(|d| *d > now),
            notifications: None,
        });
    }

    let rows = sqlx::query_as::<_, DeliveryRow>(
        "SELECT giver.name AS giver_name, ga.notification_sent, ga.notification_attempts \
         FROM gift_assignments ga JOIN users giver ON giver.id = ga.giver_id \
         WHERE ga.group_id = ?",
    )
    .bind(group_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| db_error("status: delivery", e))?;
    let mut summary = NotificationSummary {
        total: count_u32(rows.len()),
        sent: 0,
        pending: 0,
        failed: 0,
        failed_member_names: Vec::new(),
    };
    for row in rows {
        if row.notification_sent {
            summary.sent += 1;
        } else if row.notification_attempts >= MAX_ATTEMPTS {
            summary.failed += 1;
            summary.failed_member_names.push(row.giver_name);
        } else {
            summary.pending += 1;
        }
    }
    summary.failed_member_names.sort();
    Ok(DrawAdminStatus {
        is_drawn: true,
        drawn_at: group.drawn_at,
        member_count,
        blocker: None,
        registration_open_until: None,
        notifications: Some(summary),
    })
}

// ------------------------------------------------------------------ writes

/// Performs the draw (spec: "perform_draw", steps 1–6).
pub async fn perform_draw(group_id: i32) -> Result<(), DrawApiError> {
    let me = user_id().await?;
    let state = state()?;
    // Fast fail: non-admins never take the group lock. Re-checked under it.
    authz::require_admin(&state.pool, me, group_id)
        .await
        .map_err(from_app)?;

    let mut conn = acquire(&state).await?;
    let mut tx = begin(
        &mut conn,
        IsolationLevel::ReadCommitted,
        "perform_draw: begin",
    )
    .await?;
    let result = draw_locked(&mut tx, group_id, me).await;
    let members = finish(tx, result, "perform_draw: commit").await?;
    drop(conn);

    tracing::info!(group_id, members, "draw performed");
    state.draw_worker.draw_result(group_id);
    Ok(())
}

/// Steps 2–5 under the group row lock. Returns the number of assignments written.
async fn draw_locked(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    me: i32,
) -> Result<usize, DrawApiError> {
    if lock_group_as_admin(tx, group_id, me).await? {
        return Err(DrawApiError::AlreadyDrawn);
    }
    let members = member_ids(tx, group_id).await?;
    let rules = load_rules(tx, group_id).await.map_err(from_app)?;
    // CPU-bound (bounded, see spec G5): off the async workers. A fresh OS-seeded RNG per
    // draw; it never leaves this closure (S6).
    let drawn = tokio::task::spawn_blocking(move || {
        let mut rng = StdRng::from_os_rng();
        algorithm::draw(&members, &rules, &DrawConfig::default(), &mut rng)
    })
    .await
    .map_err(|_| {
        tracing::error!(group_id, "draw: the draw task failed");
        DrawApiError::Internal
    })?;
    let pairs = drawn.map_err(|err| map_draw_error(group_id, err))?;

    sqlx::query("DELETE FROM gift_assignments WHERE group_id = ?")
        .bind(group_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| db_error("perform_draw: clear", e))?;
    let mut insert = QueryBuilder::<MySql>::new(
        "INSERT INTO gift_assignments \
         (group_id, giver_id, receiver_id, notification_sent, notification_attempts) ",
    );
    insert.push_values(&pairs, |mut row, (giver, receiver)| {
        row.push_bind(group_id)
            .push_bind(*giver)
            .push_bind(*receiver)
            .push("FALSE")
            .push("0");
    });
    insert
        .build()
        .execute(&mut **tx)
        .await
        .map_err(|e| db_error("perform_draw: insert", e))?;
    // A new generation makes any older reset notice obsolete. An already in-flight
    // SMTP delivery can still complete, so mail text must tolerate reordering.
    sqlx::query("DELETE FROM draw_reset_notifications WHERE group_id = ?")
        .bind(group_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| db_error("perform_draw: discard stale reset notices", e))?;
    sqlx::query("UPDATE `groups` SET is_drawn = TRUE, drawn_at = CURRENT_TIMESTAMP, draw_generation = draw_generation + 1 WHERE id = ?")
        .bind(group_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| db_error("perform_draw: mark drawn", e))?;
    Ok(pairs.len())
}

/// Resets the draw (spec: "reset_draw"): all assignments go, the group is undrawn again;
/// rules and wishlists stay. Every former giver except the caller gets a reset mail.
pub async fn reset_draw(group_id: i32) -> Result<(), DrawApiError> {
    let me = user_id().await?;
    let state = state()?;
    authz::require_admin(&state.pool, me, group_id)
        .await
        .map_err(from_app)?;

    let mut conn = acquire(&state).await?;
    let mut tx = begin(
        &mut conn,
        IsolationLevel::ReadCommitted,
        "reset_draw: begin",
    )
    .await?;
    let result = reset_locked(&mut tx, group_id, me).await;
    finish(tx, result, "reset_draw: commit").await?;
    drop(conn);

    tracing::info!(group_id, "draw reset");
    state.draw_worker.draw_reset(group_id);
    Ok(())
}

async fn reset_locked(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    me: i32,
) -> Result<(), DrawApiError> {
    if !lock_group_as_admin(tx, group_id, me).await? {
        return Err(DrawApiError::NotDrawn);
    }
    // Durable notices are committed atomically with the reset. They store giver ids
    // but never receiver ids or pairings, and are invalidated by a later draw.
    sqlx::query(
        "INSERT INTO draw_reset_notifications \
         (group_id, recipient_id, generation, group_name, admin_name) \
         SELECT ga.group_id, ga.giver_id, g.draw_generation + 1, g.name, admin.name \
         FROM gift_assignments ga JOIN `groups` g ON g.id = ga.group_id \
         JOIN users admin ON admin.id = g.admin_id \
         WHERE ga.group_id = ? AND ga.giver_id <> ?",
    )
    .bind(group_id)
    .bind(me)
    .execute(&mut **tx)
    .await
    .map_err(|e| db_error("reset_draw: save notices", e))?;
    sqlx::query("DELETE FROM gift_assignments WHERE group_id = ?")
        .bind(group_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| db_error("reset_draw: delete", e))?;
    sqlx::query("UPDATE `groups` SET is_drawn = FALSE, drawn_at = NULL, draw_generation = draw_generation + 1 WHERE id = ?")
        .bind(group_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| db_error("reset_draw: mark undrawn", e))?;
    Ok(())
}

/// Re-queues the mails that failed for good (spec: "resend_draw_notifications").
pub async fn resend_draw_notifications(group_id: i32) -> Result<u32, DrawApiError> {
    let me = user_id().await?;
    let state = state()?;
    authz::require_admin(&state.pool, me, group_id)
        .await
        .map_err(from_app)?;
    let is_drawn = sqlx::query_scalar::<_, bool>("SELECT is_drawn FROM `groups` WHERE id = ?")
        .bind(group_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| db_error("resend: group", e))?
        .ok_or(DrawApiError::NotFound)?;
    if !is_drawn {
        return Err(DrawApiError::NotDrawn);
    }
    let requeued = sqlx::query(
        "UPDATE gift_assignments SET notification_attempts = 0, notification_next_attempt_at = NULL \
         WHERE group_id = ? AND notification_sent = FALSE AND notification_attempts >= ?",
    )
    .bind(group_id)
    .bind(MAX_ATTEMPTS)
    .execute(&state.pool)
    .await
    .map_err(|e| db_error("resend: requeue", e))?
    .rows_affected();
    if requeued == 0 {
        return Err(DrawApiError::NothingToResend);
    }
    tracing::info!(group_id, rows = requeued, "draw mails re-queued");
    state.draw_worker.draw_result(group_id);
    Ok(u32::try_from(requeued).unwrap_or(u32::MAX))
}
