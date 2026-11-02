//! The bodies of the groups server fns (spec: groups, "Server functions"). Called only from
//! [`crate::groups_api`]. The acting user always comes from the session (`require_user`);
//! a group id from the client only selects a row and goes through [`authz`] (non-members
//! get `NotFound`, members calling admin-only fns `Forbidden`).
//!
//! Locking (spec D16, plan "group row first"): every change of membership or of the admin
//! runs in a transaction that starts with `SELECT … FROM `groups` WHERE id = ? FOR UPDATE`
//! and re-checks the caller's rights and the group's state under that lock. The checks
//! before the transaction only keep non-members from taking the lock.
//!
//! Plain HTML form posts (no JavaScript) end with `302` and a session flash; the server fn
//! client gets the flash too and navigates by the redirect header where the spec says so.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use sqlx::{FromRow, MySql, Transaction};

use crate::error::AppError;
use crate::flash::FlashMessage;
use crate::groups_api::{
    AdminView, ClosedReason, DESCRIPTION_MAX, GROUP_NAME_MAX, GroupDetail, GroupError, GroupField,
    GroupFieldError, GroupOp, GroupStatus, GroupSummary, Invitation, InvitationPreview, MemberView,
    join_path,
};
use crate::paths::{group_path, login_url};
use crate::server::{
    AppState, auth, auth_mail, authz, flash, invite, membership, rate_limit, respond,
};
use crate::tz;

const LIST_PATH: &str = "/groups";
const NEW_PATH: &str = "/groups/new";
const JOIN_PATH: &str = "/join";
/// Longest stored invitation code (`groups.invitation_code VARCHAR(32)`).
const CODE_COLUMN_MAX: usize = 32;

fn edit_path(group_id: i32) -> String {
    format!("/groups/{group_id}/edit")
}

// ------------------------------------------------------------------- errors

/// Logs a database error without its message (it can quote values) and returns `Internal`.
fn db_error(context: &'static str, err: sqlx::Error) -> GroupError {
    tracing::error!(
        context,
        error = %crate::server::db::redacted(&err),
        "groups: database error"
    );
    GroupError::Internal
}

fn from_app(err: AppError) -> GroupError {
    match err {
        AppError::Unauthorized => GroupError::Unauthorized,
        AppError::NotFound => GroupError::NotFound,
        AppError::Forbidden => GroupError::Forbidden,
        _ => GroupError::Internal,
    }
}

fn state() -> Result<AppState, GroupError> {
    crate::server::state().map_err(|_| GroupError::Internal)
}

/// The logged-in user's id, or `Unauthorized`.
async fn user_id() -> Result<i32, GroupError> {
    auth::require_user()
        .await
        .map(|user| user.id)
        .map_err(from_app)
}

async fn begin(
    state: &AppState,
    context: &'static str,
) -> Result<Transaction<'static, MySql>, GroupError> {
    state.pool.begin().await.map_err(|e| db_error(context, e))
}

async fn commit(tx: Transaction<'_, MySql>, context: &'static str) -> Result<(), GroupError> {
    tx.commit().await.map_err(|e| db_error(context, e))
}

// ------------------------------------------------------------- the row lock

/// The locked `groups` row.
#[derive(FromRow)]
struct LockedGroup {
    invitation_code: String,
    registration_deadline: Option<DateTime<Utc>>,
    is_drawn: Option<bool>,
}

impl LockedGroup {
    fn drawn(&self) -> bool {
        self.is_drawn.unwrap_or(false)
    }
}

/// `SELECT … FOR UPDATE` on the group row: the first statement of every membership or
/// admin change (the draw slice takes the same lock). `None`: the group is gone.
async fn lock_group(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
) -> Result<Option<LockedGroup>, GroupError> {
    sqlx::query_as::<_, LockedGroup>(
        "SELECT invitation_code, registration_deadline, is_drawn FROM `groups` \
         WHERE id = ? FOR UPDATE",
    )
    .bind(group_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| db_error("lock group", e))
}

/// Whether `user_id` has a `group_members` row in `group_id` (inside `tx`, after the lock).
async fn is_member(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    user_id: i32,
) -> Result<bool, GroupError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM group_members WHERE group_id = ? AND user_id = ?)",
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| db_error("member check", e))
}

// ------------------------------------------------------------------ status

fn deadline_passed(deadline: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    deadline.is_some_and(|deadline| now > deadline)
}

fn status_of(is_drawn: bool, deadline: Option<DateTime<Utc>>, now: DateTime<Utc>) -> GroupStatus {
    if is_drawn {
        GroupStatus::Drawn
    } else if deadline_passed(deadline, now) {
        GroupStatus::RegistrationClosed
    } else {
        GroupStatus::Open
    }
}

// --------------------------------------------------------------- validation

/// A validated create/update form.
struct GroupValues {
    name: String,
    description: Option<String>,
    registration_deadline: Option<DateTime<Utc>>,
    draw_date: Option<DateTime<Utc>>,
}

/// An empty field is "no date"; anything else must be exactly a real `YYYY-MM-DD` date
/// (no surrounding spaces).
fn parse_date(raw: &str) -> Result<Option<NaiveDate>, GroupFieldError> {
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.trim() != raw {
        return Err(GroupFieldError::InvalidDate);
    }
    tz::parse_date_input(raw)
        .map(Some)
        .map_err(|_| GroupFieldError::InvalidDate)
}

/// Local date `date` at `h:m:s` in `zone`, as a UTC instant (spec D9).
fn local_instant(
    date: NaiveDate,
    (h, m, s): (u32, u32, u32),
    zone: Tz,
) -> Result<DateTime<Utc>, GroupFieldError> {
    let time = NaiveTime::from_hms_opt(h, m, s).ok_or(GroupFieldError::InvalidDate)?;
    tz::from_local(date.and_time(time), zone).map_err(|_| GroupFieldError::InvalidDate)
}

/// Checks every field and returns all problems together (spec "Validation").
fn validate(
    name: &str,
    description: &str,
    registration_deadline: &str,
    draw_date: &str,
    zone: Tz,
) -> Result<GroupValues, GroupError> {
    let mut errors = Vec::new();

    let name = name.trim();
    if name.is_empty() {
        errors.push((GroupField::Name, GroupFieldError::NameRequired));
    } else if name.chars().count() > GROUP_NAME_MAX {
        errors.push((GroupField::Name, GroupFieldError::NameTooLong));
    }

    let description = description.trim();
    if description.chars().count() > DESCRIPTION_MAX {
        errors.push((GroupField::Description, GroupFieldError::DescriptionTooLong));
    }

    let mut date = |raw: &str, field: GroupField, at: (u32, u32, u32)| match parse_date(raw)
        .and_then(|d| match d {
            Some(day) => local_instant(day, at, zone).map(|instant| Some((day, instant))),
            None => Ok(None),
        }) {
        Ok(value) => value,
        Err(problem) => {
            errors.push((field, problem));
            None
        }
    };
    let deadline = date(
        registration_deadline,
        GroupField::RegistrationDeadline,
        (23, 59, 59),
    );
    let draw = date(draw_date, GroupField::DrawDate, (12, 0, 0));

    if let (Some((deadline_day, _)), Some((draw_day, _))) = (deadline, draw)
        && draw_day < deadline_day
    {
        errors.push((GroupField::DrawDate, GroupFieldError::DrawBeforeDeadline));
    }

    if !errors.is_empty() {
        return Err(GroupError::Validation(errors));
    }
    Ok(GroupValues {
        name: name.to_string(),
        description: (!description.is_empty()).then(|| description.to_string()),
        registration_deadline: deadline.map(|(_, instant)| instant),
        draw_date: draw.map(|(_, instant)| instant),
    })
}

// --------------------------------------------------------- plain form posts

async fn push_flash(message: FlashMessage) {
    if let Err(err) = flash::flash(message).await {
        tracing::warn!(?err, "groups: could not store the result flash");
    }
}

/// After a success: a plain form post gets `302 path`, the server fn client a redirect
/// header (it navigates within the app).
fn navigate(plain: bool, path: &str) {
    if plain {
        respond::redirect_document(path);
    } else {
        leptos_axum::redirect(path);
    }
}

/// A failed plain form post ends on `back` with the error as a flash; without a session it
/// goes to the login page that returns to `back`.
async fn plain_error(err: &GroupError, op: GroupOp, back: &str) {
    match err {
        GroupError::Unauthorized => respond::redirect_document(&login_url(back)),
        other => {
            push_flash(other.flash(op)).await;
            respond::redirect_document(back);
        }
    }
}

/// `create_group`: to the new group with `group_created`; a plain post's error back to the
/// form.
pub async fn create_outcome(result: Result<i32, GroupError>) -> Result<i32, GroupError> {
    let plain = respond::is_plain_form_post();
    match &result {
        Ok(group_id) => {
            push_flash(FlashMessage::success("flash.success.group_created")).await;
            navigate(plain, &group_path(*group_id));
        }
        Err(err) if plain => plain_error(err, GroupOp::Other, NEW_PATH).await,
        Err(_) => {}
    }
    result
}

/// `update_group`: to the group with `group_updated`; a plain post's error back to the form.
pub async fn update_outcome(
    group_id: i32,
    result: Result<(), GroupError>,
) -> Result<(), GroupError> {
    let plain = respond::is_plain_form_post();
    match &result {
        Ok(()) => {
            push_flash(FlashMessage::success("flash.success.group_updated")).await;
            navigate(plain, &group_path(group_id));
        }
        Err(err) if plain => plain_error(err, GroupOp::Other, &edit_path(group_id)).await,
        Err(_) => {}
    }
    result
}

/// `delete_group`: to the list with `group_deleted`.
pub async fn delete_outcome(
    group_id: i32,
    result: Result<(), GroupError>,
) -> Result<(), GroupError> {
    let plain = respond::is_plain_form_post();
    match &result {
        Ok(()) => {
            push_flash(FlashMessage::success("flash.success.group_deleted")).await;
            navigate(plain, LIST_PATH);
        }
        Err(err) if plain => plain_error(err, GroupOp::Other, &group_path(group_id)).await,
        Err(_) => {}
    }
    result
}

/// `leave_group`: to the list with `left_group`.
pub async fn leave_outcome(
    group_id: i32,
    result: Result<(), GroupError>,
) -> Result<(), GroupError> {
    let plain = respond::is_plain_form_post();
    match &result {
        Ok(()) => {
            push_flash(FlashMessage::success("flash.success.left_group")).await;
            navigate(plain, LIST_PATH);
        }
        Err(err) if plain => plain_error(err, GroupOp::Leave, &group_path(group_id)).await,
        Err(_) => {}
    }
    result
}

/// Actions that stay on the group page (regenerate, remove, transfer): the flash is stored;
/// the server fn client re-reads the page data and the flash itself, a plain post gets
/// `302` back to the group.
pub async fn stay_outcome<T>(
    group_id: i32,
    result: Result<T, GroupError>,
    success_key: &'static str,
    op: GroupOp,
) -> Result<T, GroupError> {
    let plain = respond::is_plain_form_post();
    match &result {
        Ok(_) => {
            push_flash(FlashMessage::success(success_key)).await;
            if plain {
                respond::redirect_document(&group_path(group_id));
            }
        }
        Err(err) if plain => plain_error(err, op, &group_path(group_id)).await,
        Err(_) => {}
    }
    result
}

/// `join_group`: to the group with `group_joined`. A plain post of an existing member goes
/// to the group with the info flash; other errors back to the preview (or the enter-code
/// form when the code itself was the problem).
pub async fn join_outcome(
    normalized_code: &str,
    result: Result<i32, GroupError>,
) -> Result<i32, GroupError> {
    let plain = respond::is_plain_form_post();
    match &result {
        Ok(group_id) => {
            push_flash(FlashMessage::success("flash.success.group_joined")).await;
            navigate(plain, &group_path(*group_id));
        }
        Err(GroupError::AlreadyMember { group_id }) if plain => {
            push_flash(FlashMessage::info("flash.info.already_member")).await;
            respond::redirect_document(&group_path(*group_id));
        }
        Err(err) if plain => {
            let back = match err {
                GroupError::CodeRequired
                | GroupError::InvalidCode
                | GroupError::TooManyAttempts { .. } => JOIN_PATH.to_string(),
                _ => join_path(normalized_code).unwrap_or_else(|| JOIN_PATH.to_string()),
            };
            plain_error(err, GroupOp::Join, &back).await;
        }
        Err(_) => {}
    }
    result
}

// ---------------------------------------------------------------- list/show

#[derive(FromRow)]
struct SummaryRow {
    id: i32,
    name: String,
    description: Option<String>,
    admin_id: i32,
    admin_name: String,
    member_count: i64,
    is_drawn: Option<bool>,
    registration_deadline: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
}

pub async fn list_my_groups() -> Result<Vec<GroupSummary>, GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    let rows = sqlx::query_as::<_, SummaryRow>(
        "SELECT g.id, g.name, g.description, g.admin_id, u.name AS admin_name, \
                (SELECT COUNT(*) FROM group_members c WHERE c.group_id = g.id) AS member_count, \
                g.is_drawn, g.registration_deadline, g.created_at \
         FROM `groups` g \
         JOIN group_members m ON m.group_id = g.id AND m.user_id = ? \
         JOIN users u ON u.id = g.admin_id \
         ORDER BY g.created_at DESC, g.id DESC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| db_error("list_my_groups", e))?;
    let now = Utc::now();
    Ok(rows
        .into_iter()
        .map(|row| GroupSummary {
            id: row.id,
            name: row.name,
            description: row.description,
            is_admin: row.admin_id == user_id,
            admin_name: row.admin_name,
            member_count: u32::try_from(row.member_count).unwrap_or(u32::MAX),
            status: status_of(
                row.is_drawn.unwrap_or(false),
                row.registration_deadline,
                now,
            ),
            created_at: row.created_at.unwrap_or(DateTime::UNIX_EPOCH),
        })
        .collect())
}

#[derive(FromRow)]
struct DetailRow {
    id: i32,
    name: String,
    description: Option<String>,
    admin_id: i32,
    admin_name: String,
    invitation_code: String,
    registration_deadline: Option<DateTime<Utc>>,
    draw_date: Option<DateTime<Utc>>,
    is_drawn: Option<bool>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct MemberRow {
    user_id: i32,
    name: String,
    email: String,
}

pub async fn get_group(group_id: i32) -> Result<GroupDetail, GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_member(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;

    let row = sqlx::query_as::<_, DetailRow>(
        "SELECT g.id, g.name, g.description, g.admin_id, u.name AS admin_name, \
                g.invitation_code, g.registration_deadline, g.draw_date, g.is_drawn, g.created_at \
         FROM `groups` g JOIN users u ON u.id = g.admin_id WHERE g.id = ?",
    )
    .bind(group_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| db_error("get_group", e))?
    .ok_or(GroupError::NotFound)?;
    let members = sqlx::query_as::<_, MemberRow>(
        "SELECT m.user_id, u.name, u.email FROM group_members m \
         JOIN users u ON u.id = m.user_id WHERE m.group_id = ? ORDER BY m.joined_at, m.id",
    )
    .bind(group_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| db_error("get_group: members", e))?;

    // Rights from the rows just read (a concurrent leave or transfer counts from here on).
    let is_admin = row.admin_id == user_id;
    if !is_admin && !members.iter().any(|m| m.user_id == user_id) {
        return Err(GroupError::NotFound);
    }
    let is_drawn = row.is_drawn.unwrap_or(false);
    let members = members
        .into_iter()
        .map(|m| MemberView {
            is_admin: m.user_id == row.admin_id,
            is_me: m.user_id == user_id,
            // Emails are for the admin only (D14).
            email: is_admin.then_some(m.email),
            user_id: m.user_id,
            name: m.name,
        })
        .collect();
    let invitation = is_admin.then(|| Invitation {
        link: auth_mail::app_link(
            &state.config,
            &join_path(&row.invitation_code).unwrap_or_else(|| JOIN_PATH.to_string()),
        ),
        code: row.invitation_code.clone(),
    });
    Ok(GroupDetail {
        id: row.id,
        name: row.name,
        description: row.description,
        created_at: row.created_at.unwrap_or(DateTime::UNIX_EPOCH),
        registration_deadline: row.registration_deadline,
        draw_date: row.draw_date,
        is_drawn,
        status: status_of(is_drawn, row.registration_deadline, Utc::now()),
        is_admin,
        admin: AdminView {
            id: row.admin_id,
            name: row.admin_name,
        },
        members,
        invitation,
        can_leave: !is_admin && !is_drawn,
    })
}

// ---------------------------------------------------------- create / update

pub async fn create_group(
    name: String,
    description: String,
    registration_deadline: String,
    draw_date: String,
) -> Result<i32, GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    let values = validate(
        &name,
        &description,
        &registration_deadline,
        &draw_date,
        state.config.app_timezone,
    )?;

    let mut tx = begin(&state, "create_group: begin").await?;
    let mut next_code = invite::generate_code;
    let (group_id, _) =
        invite::insert_group_with_code(&mut tx, &values.name, user_id, &mut next_code)
            .await
            .map_err(from_app)?;
    sqlx::query(
        "UPDATE `groups` SET description = ?, registration_deadline = ?, draw_date = ? \
         WHERE id = ?",
    )
    .bind(&values.description)
    .bind(values.registration_deadline)
    .bind(values.draw_date)
    .bind(group_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| db_error("create_group: details", e))?;
    commit(tx, "create_group: commit").await?;
    tracing::info!(user_id, group_id, "group created");
    Ok(group_id)
}

pub async fn update_group(
    group_id: i32,
    name: String,
    description: String,
    registration_deadline: String,
    draw_date: String,
) -> Result<(), GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_admin(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    let values = validate(
        &name,
        &description,
        &registration_deadline,
        &draw_date,
        state.config.app_timezone,
    )?;

    let mut tx = begin(&state, "update_group: begin").await?;
    lock_group(&mut tx, group_id)
        .await?
        .ok_or(GroupError::NotFound)?;
    authz::require_admin(&mut *tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    sqlx::query(
        "UPDATE `groups` SET name = ?, description = ?, registration_deadline = ?, \
         draw_date = ? WHERE id = ?",
    )
    .bind(&values.name)
    .bind(&values.description)
    .bind(values.registration_deadline)
    .bind(values.draw_date)
    .bind(group_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| db_error("update_group", e))?;
    commit(tx, "update_group: commit").await?;
    Ok(())
}

// ------------------------------------------------------- delete / regenerate

pub async fn delete_group(group_id: i32) -> Result<(), GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_admin(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;

    let mut tx = begin(&state, "delete_group: begin").await?;
    lock_group(&mut tx, group_id)
        .await?
        .ok_or(GroupError::NotFound)?;
    authz::require_admin(&mut *tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    // Members, wishlists (and items), exclusion rules and assignments go by ON DELETE
    // CASCADE (D13).
    sqlx::query("DELETE FROM `groups` WHERE id = ?")
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_error("delete_group", e))?;
    commit(tx, "delete_group: commit").await?;
    tracing::info!(user_id, group_id, "group deleted");
    Ok(())
}

pub async fn regenerate_invitation(group_id: i32) -> Result<String, GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_admin(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;

    let mut tx = begin(&state, "regenerate_invitation: begin").await?;
    let locked = lock_group(&mut tx, group_id)
        .await?
        .ok_or(GroupError::NotFound)?;
    authz::require_admin(&mut *tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    let old = locked.invitation_code;
    // A new code is never the old one (D12: the old one must stop working).
    let mut next_code = move || loop {
        let code = invite::generate_code();
        if code != old {
            return code;
        }
    };
    let code = invite::replace_code(&mut tx, group_id, &mut next_code)
        .await
        .map_err(from_app)?;
    commit(tx, "regenerate_invitation: commit").await?;
    tracing::info!(user_id, group_id, "invitation code regenerated");
    Ok(code)
}

// --------------------------------------------------------------- invitation

#[derive(FromRow)]
struct CodeRow {
    id: i32,
    name: String,
    admin_name: String,
    member_count: i64,
    already_member: bool,
    is_drawn: Option<bool>,
    registration_deadline: Option<DateTime<Utc>>,
}

/// Normalises `raw` and looks the code up, under the per-user limit on invalid codes
/// (AC45): an attempt is reserved before the lookup (concurrent calls cannot all get past
/// a full counter) and given back when the code exists, so only invalid codes count.
async fn checked_lookup(
    state: &AppState,
    user_id: i32,
    raw: &str,
) -> Result<(String, CodeRow), GroupError> {
    let code = invite::normalize_code(raw);
    if code.is_empty() {
        return Err(GroupError::CodeRequired);
    }
    let key = rate_limit::invite_code_key(user_id);
    if let Err(wait) = state
        .rate_limiter
        .try_acquire_all(&[(&key, rate_limit::INVITE_CODE)])
    {
        return Err(GroupError::TooManyAttempts {
            minutes: rate_limit::minutes_rounded_up(wait),
        });
    }
    let row = if code.chars().count() > CODE_COLUMN_MAX {
        None
    } else {
        sqlx::query_as::<_, CodeRow>(
            "SELECT g.id, g.name, u.name AS admin_name, \
                    (SELECT COUNT(*) FROM group_members c WHERE c.group_id = g.id) AS member_count, \
                    EXISTS(SELECT 1 FROM group_members m WHERE m.group_id = g.id AND m.user_id = ?) \
                        AS already_member, \
                    g.is_drawn, g.registration_deadline \
             FROM `groups` g JOIN users u ON u.id = g.admin_id WHERE g.invitation_code = ?",
        )
        .bind(user_id)
        .bind(&code)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| db_error("invitation lookup", e))?
    };
    match row {
        Some(row) => {
            state.rate_limiter.release(&key);
            Ok((code, row))
        }
        None => {
            tracing::info!(user_id, "invalid invitation code");
            Err(GroupError::InvalidCode)
        }
    }
}

pub async fn preview_invitation(code: String) -> Result<InvitationPreview, GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    let (_, row) = checked_lookup(&state, user_id, &code).await?;
    let closed = if row.is_drawn.unwrap_or(false) {
        Some(ClosedReason::Drawn)
    } else if deadline_passed(row.registration_deadline, Utc::now()) {
        Some(ClosedReason::DeadlinePassed)
    } else {
        None
    };
    Ok(InvitationPreview {
        group_id: row.id,
        name: row.name,
        admin_name: row.admin_name,
        member_count: u32::try_from(row.member_count).unwrap_or(u32::MAX),
        already_member: row.already_member,
        closed,
    })
}

pub async fn open_invitation(code: String) -> Result<String, GroupError> {
    let result = async {
        user_id().await?;
        let code = invite::normalize_code(&code);
        if code.is_empty() {
            return Err(GroupError::CodeRequired);
        }
        // Nothing that could be a code (e.g. `..`): the invalid-code result on /join.
        join_path(&code).ok_or(GroupError::InvalidCode)
    }
    .await;
    if respond::is_plain_form_post() {
        match &result {
            Ok(path) => respond::redirect_document(path),
            Err(err) => plain_error(err, GroupOp::Join, JOIN_PATH).await,
        }
    }
    result
}

pub async fn join_group(code: String) -> Result<i32, GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    let (code, row) = checked_lookup(&state, user_id, &code).await?;
    let group_id = row.id;

    let mut tx = begin(&state, "join_group: begin").await?;
    // Deleted or given a new code while we waited for the lock: the code is no longer valid.
    let locked = match lock_group(&mut tx, group_id).await? {
        Some(locked) if locked.invitation_code == code => locked,
        _ => return Err(GroupError::InvalidCode),
    };
    let access = authz::group_access(&mut *tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    if access.is_some_and(|a| a.is_member || a.is_admin) {
        return Err(GroupError::AlreadyMember { group_id });
    }
    if locked.drawn() {
        return Err(GroupError::AlreadyDrawn);
    }
    if deadline_passed(locked.registration_deadline, Utc::now()) {
        return Err(GroupError::RegistrationClosed);
    }
    let inserted = sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES (?, ?)")
        .bind(group_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await;
    match inserted {
        Ok(_) => {}
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            return Err(GroupError::AlreadyMember { group_id });
        }
        Err(err) => return Err(db_error("join_group: insert", err)),
    }
    commit(tx, "join_group: commit").await?;
    tracing::info!(user_id, group_id, "joined group");
    Ok(group_id)
}

// ------------------------------------------------------ leave / remove / admin

pub async fn leave_group(group_id: i32) -> Result<(), GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_member(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;

    let mut tx = begin(&state, "leave_group: begin").await?;
    let locked = lock_group(&mut tx, group_id)
        .await?
        .ok_or(GroupError::NotFound)?;
    let access = authz::require_member(&mut *tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    if access.is_admin {
        return Err(GroupError::AdminCannotLeave);
    }
    if locked.drawn() {
        return Err(GroupError::AlreadyDrawn);
    }
    membership::remove_member_data(&mut tx, group_id, user_id)
        .await
        .map_err(from_app)?;
    commit(tx, "leave_group: commit").await?;
    tracing::info!(user_id, group_id, "left group");
    Ok(())
}

pub async fn remove_member(group_id: i32, target: i32) -> Result<(), GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_admin(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    if target == user_id {
        return Err(GroupError::CannotRemoveSelf);
    }

    let mut tx = begin(&state, "remove_member: begin").await?;
    let locked = lock_group(&mut tx, group_id)
        .await?
        .ok_or(GroupError::NotFound)?;
    authz::require_admin(&mut *tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    // The target must be a member of *this* group, checked under its lock.
    if !is_member(&mut tx, group_id, target).await? {
        return Err(GroupError::TargetNotMember);
    }
    if locked.drawn() {
        return Err(GroupError::AlreadyDrawn);
    }
    membership::remove_member_data(&mut tx, group_id, target)
        .await
        .map_err(from_app)?;
    commit(tx, "remove_member: commit").await?;
    tracing::info!(user_id, group_id, target, "member removed");
    Ok(())
}

pub async fn transfer_admin(group_id: i32, target: i32) -> Result<(), GroupError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_admin(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    if target == user_id {
        return Err(GroupError::TargetIsSelf);
    }

    let mut tx = begin(&state, "transfer_admin: begin").await?;
    lock_group(&mut tx, group_id)
        .await?
        .ok_or(GroupError::NotFound)?;
    // A concurrent transfer that won the lock makes the caller a plain member.
    authz::require_admin(&mut *tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    if !is_member(&mut tx, group_id, target).await? {
        return Err(GroupError::TargetNotMember);
    }
    sqlx::query("UPDATE `groups` SET admin_id = ? WHERE id = ?")
        .bind(target)
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_error("transfer_admin", e))?;
    commit(tx, "transfer_admin: commit").await?;
    tracing::info!(user_id, group_id, target, "admin transferred");
    Ok(())
}
