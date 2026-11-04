//! The bodies of the wishlist server fns (spec: wishlists, "Server functions"). Called only
//! from [`crate::wishlists_api`]. The acting user always comes from the session
//! (`require_user`); a group id from the client only selects a row and goes through
//! [`authz`] (a missing group and a group the caller is not in are both `NotFound`). An item
//! id is only looked up together with the caller's user id, so someone else's item, a
//! missing one and one in a group the caller left all read as `NotFound`.
//!
//! Visibility (W1): every current member sees every current member's list. No fn here
//! reads who gives to whom.
//!
//! Locking (W11, plan "group row first"): every write runs in one transaction that takes,
//! in this order,
//! 1. the group row in shared mode (`LOCK IN SHARE MODE`): membership changes and the draw
//!    take it `FOR UPDATE`, so a write never interleaves with them, while wishlist writes
//!    of different members do not queue behind each other;
//! 2. the caller's own `group_members` row `FOR UPDATE`, then the membership check (inside
//!    the transaction, after the locks): the row serialises the owner's writes (two tabs)
//!    before any wishlist row is touched, also while the list row does not exist yet;
//! 3. the caller's `wishlists` row `FOR UPDATE`, then its items (locking reads, so counts
//!    and positions are current).
//!
//! The transactions run under READ COMMITTED (see [`begin`]): no gap locks, so writes to
//! different lists never block each other, and every statement reads committed data. A
//! deadlock or snapshot conflict reported by the database is still retried (safety net).
//! Before any of this, a user's writes wait in a per-user queue in this process
//! ([`writer_turn`]), so one user holds at most one pool connection while waiting for a lock.
//!
//! Plain HTML form posts (no JavaScript) end with `302` to the edit page and a session flash;
//! the server fn client gets the flash and re-reads the page itself.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex, Weak};

use chrono::{DateTime, Utc};
use sqlx::pool::PoolConnection;
use sqlx::{FromRow, MySql, Transaction};

use crate::error::AppError;
use crate::flash::FlashMessage;
use crate::paths::{login_url, wishlist_edit_path as edit_path};
use crate::server::db::{self, IsolationLevel};
use crate::server::{AppState, auth, authz, flash, respond};
use crate::validation::normalize_newlines;
use crate::wishlists_api::{
    DESCRIPTION_MAX, Direction, ItemView, LINK_MAX, MAX_ITEMS, NamedRef, PanelMember, TITLE_MAX,
    WishlistError, WishlistField, WishlistFieldError, WishlistPanel, WishlistView,
};

/// Where a failed plain post goes when it has no list to return to.
const LIST_PATH: &str = "/groups";
/// How often a transaction is tried when the database reports a deadlock.
const ATTEMPTS: usize = 3;

// ------------------------------------------------------------------- errors

/// Logs a database error without its message (it can quote values) and returns `Internal`.
fn db_error(context: &'static str, err: sqlx::Error) -> WishlistError {
    tracing::error!(
        context,
        error = %crate::server::db::redacted(&err),
        "wishlists: database error"
    );
    WishlistError::Internal
}

fn from_app(err: AppError) -> WishlistError {
    match err {
        AppError::Unauthorized => WishlistError::Unauthorized,
        AppError::NotFound => WishlistError::NotFound,
        _ => WishlistError::Internal,
    }
}

fn state() -> Result<AppState, WishlistError> {
    crate::server::state().map_err(|_| WishlistError::Internal)
}

/// The logged-in user's id, or `Unauthorized`.
async fn user_id() -> Result<i32, WishlistError> {
    auth::require_user()
        .await
        .map(|user| user.id)
        .map_err(from_app)
}

/// Why a write transaction stopped.
enum Fail {
    /// A result for the caller.
    Error(WishlistError),
    /// The database chose this transaction as a deadlock victim: try again.
    Deadlock,
}

impl From<WishlistError> for Fail {
    fn from(err: WishlistError) -> Self {
        Fail::Error(err)
    }
}

/// A deadlock (1213, SQLSTATE 40001), or MariaDB's snapshot-isolation conflict "record has
/// changed since last read" (1020): the transaction is rolled back and may simply run again.
fn is_deadlock(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => {
            db.code().as_deref() == Some("40001")
                || db
                    .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                    .is_some_and(|e| matches!(e.number(), 1213 | 1020))
        }
        _ => false,
    }
}

/// A database error inside a write transaction.
fn tx_error(context: &'static str) -> impl FnOnce(sqlx::Error) -> Fail {
    move |err| {
        if is_deadlock(&err) {
            tracing::warn!(context, "wishlists: deadlock, retrying");
            Fail::Deadlock
        } else {
            Fail::Error(db_error(context, err))
        }
    }
}

/// Runs the transaction `attempt` until it is not a deadlock victim (at most [`ATTEMPTS`]).
async fn retrying<T, F, Fut>(mut attempt: F) -> Result<T, WishlistError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Fail>>,
{
    for _ in 1..ATTEMPTS {
        match attempt().await {
            Ok(value) => return Ok(value),
            Err(Fail::Error(err)) => return Err(err),
            Err(Fail::Deadlock) => {}
        }
    }
    match attempt().await {
        Ok(value) => Ok(value),
        Err(Fail::Error(err)) => Err(err),
        Err(Fail::Deadlock) => {
            tracing::error!("wishlists: deadlock after {ATTEMPTS} attempts");
            Err(WishlistError::Internal)
        }
    }
}

// --------------------------------------------------------- plain form posts

async fn push_flash(message: FlashMessage) {
    if let Err(err) = flash::flash(message).await {
        tracing::warn!(?err, "wishlists: could not store the result flash");
    }
}

/// The end of every mutation. Success: the flash `success` (if any) is stored; a plain form
/// post gets `302` to the edit page, the server fn client re-reads the page itself. A failed
/// plain post goes back to the edit page of `group_id` with the error as a flash (to the
/// group list when there is no list to go back to), or to the login page without a session.
pub async fn outcome<T>(
    group_id: Option<i32>,
    result: Result<T, WishlistError>,
    success: Option<&'static str>,
) -> Result<T, WishlistError> {
    let plain = respond::is_plain_form_post();
    let back = group_id.map(edit_path);
    match &result {
        Ok(_) => {
            if let Some(key) = success {
                push_flash(FlashMessage::success(key)).await;
            }
            if plain {
                respond::redirect_document(back.as_deref().unwrap_or(LIST_PATH));
            }
        }
        Err(err) if plain => {
            let back = match err {
                WishlistError::NotFound => LIST_PATH.to_string(),
                _ => back.unwrap_or_else(|| LIST_PATH.to_string()),
            };
            if *err == WishlistError::Unauthorized {
                respond::redirect_document(&login_url(&back));
            } else {
                push_flash(err.flash()).await;
                respond::redirect_document(&back);
            }
        }
        Err(_) => {}
    }
    result
}

// --------------------------------------------------------------- validation

/// A validated item form.
struct ItemValues {
    title: String,
    description: Option<String>,
    link: Option<String>,
}

/// W9: trim; empty is no link; without `://` the input gets `https://` in front; it must
/// parse as an `http`/`https` URL with a host and without user name or password; the
/// normalised form is stored. Both the input and the normalised form are at most
/// [`LINK_MAX`] characters.
fn normalize_link(raw: &str) -> Result<Option<String>, WishlistFieldError> {
    let link = raw.trim();
    if link.is_empty() {
        return Ok(None);
    }
    if link.chars().count() > LINK_MAX {
        return Err(WishlistFieldError::LinkTooLong);
    }
    // Scheme-less input gets `https://`; so does input whose `://` only comes later and
    // that therefore has no scheme (`www.shop.de/x?ret=https://y`).
    let with_https = || url::Url::parse(&format!("https://{link}"));
    let parsed = if link.contains("://") {
        match url::Url::parse(link) {
            Err(url::ParseError::RelativeUrlWithoutBase) => with_https(),
            other => other,
        }
    } else {
        with_https()
    };
    let url = parsed.map_err(|_| WishlistFieldError::LinkInvalid)?;
    let host_ok = url.host_str().is_some_and(|host| !host.is_empty());
    if !matches!(url.scheme(), "http" | "https")
        || !host_ok
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(WishlistFieldError::LinkInvalid);
    }
    let normalized = url.as_str();
    if normalized.chars().count() > LINK_MAX {
        return Err(WishlistFieldError::LinkTooLong);
    }
    Ok(Some(normalized.to_string()))
}

/// The host shown next to a stored link.
fn link_host(link: &str) -> Option<String> {
    url::Url::parse(link)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
}

/// Checks every field (W8, W9) and returns all problems together.
fn validate(title: &str, description: &str, link: &str) -> Result<ItemValues, WishlistError> {
    let mut errors = Vec::new();

    let title = title.trim();
    if title.is_empty() {
        errors.push((WishlistField::Title, WishlistFieldError::TitleRequired));
    } else if title.chars().count() > TITLE_MAX {
        errors.push((WishlistField::Title, WishlistFieldError::TitleTooLong));
    }

    // Browsers submit a line break as CRLF but count it as one character (maxlength).
    let description = normalize_newlines(description);
    let description = description.trim();
    if description.chars().count() > DESCRIPTION_MAX {
        errors.push((
            WishlistField::Description,
            WishlistFieldError::DescriptionTooLong,
        ));
    }

    let link = match normalize_link(link) {
        Ok(link) => link,
        Err(problem) => {
            errors.push((WishlistField::Link, problem));
            None
        }
    };

    if !errors.is_empty() {
        return Err(WishlistError::Validation(errors));
    }
    Ok(ItemValues {
        title: title.to_string(),
        description: (!description.is_empty()).then(|| description.to_string()),
        link,
    })
}

// ------------------------------------------------------------------- reads

#[derive(FromRow)]
struct NameRow {
    id: i32,
    name: String,
}

#[derive(FromRow)]
struct ListRow {
    id: i32,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct ItemRow {
    id: i32,
    title: String,
    description: Option<String>,
    link: Option<String>,
}

/// `owner`'s list in `group_id` as `viewer` sees it. The caller has checked that `viewer`
/// is a member; `owner` must be a current member too.
async fn load_view(
    state: &AppState,
    viewer: i32,
    group_id: i32,
    owner: i32,
) -> Result<WishlistView, WishlistError> {
    let owner_row = sqlx::query_as::<_, NameRow>(
        "SELECT u.id, u.name FROM group_members m JOIN users u ON u.id = m.user_id \
         WHERE m.group_id = ? AND m.user_id = ?",
    )
    .bind(group_id)
    .bind(owner)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| db_error("view: owner", e))?
    .ok_or(WishlistError::NotFound)?;
    let group = sqlx::query_as::<_, NameRow>("SELECT id, name FROM `groups` WHERE id = ?")
        .bind(group_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| db_error("view: group", e))?
        .ok_or(WishlistError::NotFound)?;
    let list = sqlx::query_as::<_, ListRow>(
        "SELECT id, updated_at FROM wishlists WHERE user_id = ? AND group_id = ?",
    )
    .bind(owner)
    .bind(group_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| db_error("view: list", e))?;

    let (items, updated_at) = match list {
        Some(list) => {
            let rows = sqlx::query_as::<_, ItemRow>(
                "SELECT id, title, description, link FROM wishlist_items \
                 WHERE wishlist_id = ? ORDER BY position, id",
            )
            .bind(list.id)
            .fetch_all(&state.pool)
            .await
            .map_err(|e| db_error("view: items", e))?;
            (rows, list.updated_at)
        }
        None => (Vec::new(), None),
    };
    Ok(WishlistView {
        group: NamedRef {
            id: group.id,
            name: group.name,
        },
        is_own: owner_row.id == viewer,
        owner: NamedRef {
            id: owner_row.id,
            name: owner_row.name,
        },
        items: items
            .into_iter()
            .map(|row| ItemView {
                link_host: row.link.as_deref().and_then(link_host),
                id: row.id,
                title: row.title,
                description: row.description,
                link: row.link,
            })
            .collect(),
        updated_at,
    })
}

pub async fn get_wishlist(group_id: i32, owner: i32) -> Result<WishlistView, WishlistError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_member(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    load_view(&state, user_id, group_id, owner).await
}

pub async fn get_my_wishlist(group_id: i32) -> Result<WishlistView, WishlistError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_member(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    load_view(&state, user_id, group_id, user_id).await
}

#[derive(FromRow)]
struct PanelRow {
    user_id: i32,
    name: String,
    item_count: i64,
}

pub async fn get_wishlist_panel(group_id: i32) -> Result<WishlistPanel, WishlistError> {
    let user_id = user_id().await?;
    let state = state()?;
    authz::require_member(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    let rows = sqlx::query_as::<_, PanelRow>(
        "SELECT m.user_id, u.name, COUNT(i.id) AS item_count \
         FROM group_members m \
         JOIN users u ON u.id = m.user_id \
         LEFT JOIN wishlists w ON w.user_id = m.user_id AND w.group_id = m.group_id \
         LEFT JOIN wishlist_items i ON i.wishlist_id = w.id \
         WHERE m.group_id = ? \
         GROUP BY m.id, m.user_id, u.name, m.joined_at \
         ORDER BY m.joined_at, m.id",
    )
    .bind(group_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| db_error("panel", e))?;
    let members: Vec<PanelMember> = rows
        .into_iter()
        .map(|row| PanelMember {
            is_me: row.user_id == user_id,
            user_id: row.user_id,
            name: row.name,
            item_count: u32::try_from(row.item_count).unwrap_or(u32::MAX),
        })
        .collect();
    Ok(WishlistPanel {
        my_item_count: members.iter().find(|m| m.is_me).map_or(0, |m| m.item_count),
        members,
    })
}

// ------------------------------------------------------------------- locks

/// A pool connection for one write transaction.
async fn connection(state: &AppState) -> Result<PoolConnection<MySql>, Fail> {
    state.pool.acquire().await.map_err(tx_error("acquire"))
}

/// Starts a READ COMMITTED transaction on `conn` ([`db::begin_with`]).
///
/// READ COMMITTED takes no gap locks on searches: under REPEATABLE READ the `FOR UPDATE`
/// scan of a new, empty list locked the gap after it (up to the supremum), and two members'
/// first items deadlocked. The row locks still serialise every list.
async fn begin(conn: &mut PoolConnection<MySql>) -> Result<Transaction<'_, MySql>, Fail> {
    db::begin_with(conn, IsolationLevel::ReadCommitted)
        .await
        .map_err(tx_error("begin"))
}

async fn commit(tx: Transaction<'_, MySql>) -> Result<(), Fail> {
    tx.commit().await.map_err(tx_error("commit"))
}

/// Steps 1 and 2 of the lock order: the group row in shared mode, the caller's membership
/// row `FOR UPDATE`, then the membership check. `NotFound` when the group is gone or the
/// caller is no (longer a) member.
///
/// The membership check runs after the locks are held, so it sees every membership change
/// that committed before them.
async fn lock_group_for_member(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    user_id: i32,
) -> Result<(), Fail> {
    sqlx::query_scalar::<_, i32>("SELECT id FROM `groups` WHERE id = ? LOCK IN SHARE MODE")
        .bind(group_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(tx_error("lock group"))?
        .ok_or(WishlistError::NotFound)?;
    sqlx::query_scalar::<_, i32>(
        "SELECT id FROM group_members WHERE group_id = ? AND user_id = ? FOR UPDATE",
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(tx_error("lock membership"))?;
    authz::require_member(&mut **tx, user_id, group_id)
        .await
        .map_err(from_app)?;
    Ok(())
}

/// Step 3: the caller's list row in the group, `FOR UPDATE`. `None`: no list.
async fn lock_list(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    user_id: i32,
) -> Result<Option<i32>, Fail> {
    sqlx::query_scalar::<_, i32>(
        "SELECT id FROM wishlists WHERE user_id = ? AND group_id = ? FOR UPDATE",
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(tx_error("lock list"))
}

#[derive(FromRow)]
struct PositionRow {
    id: i32,
    position: i32,
}

/// The list's items in order, locked (a current read, not the transaction's snapshot).
async fn lock_items(
    tx: &mut Transaction<'_, MySql>,
    list_id: i32,
) -> Result<Vec<PositionRow>, Fail> {
    sqlx::query_as::<_, PositionRow>(
        "SELECT id, position FROM wishlist_items WHERE wishlist_id = ? \
         ORDER BY position, id FOR UPDATE",
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(tx_error("lock items"))
}

/// W4: every change of the items shows in the list's "last updated".
async fn touch_list(tx: &mut Transaction<'_, MySql>, list_id: i32) -> Result<(), Fail> {
    sqlx::query("UPDATE wishlists SET updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .map_err(tx_error("touch list"))?;
    Ok(())
}

/// The group of `item_id` if it is on the caller's own list (no lock; the transaction
/// re-checks). Someone else's item and a missing one look the same.
async fn own_item_group(
    state: &AppState,
    user_id: i32,
    item_id: i32,
) -> Result<i32, WishlistError> {
    sqlx::query_scalar::<_, i32>(
        "SELECT w.group_id FROM wishlist_items i JOIN wishlists w ON w.id = i.wishlist_id \
         WHERE i.id = ? AND w.user_id = ?",
    )
    .bind(item_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| db_error("item lookup", e))?
    .ok_or(WishlistError::NotFound)
}

/// Locks everything for a change of `item_id` (on the caller's list in `group_id`) and
/// returns the list id; `NotFound` when the item is no longer there.
async fn lock_own_item(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    user_id: i32,
    item_id: i32,
) -> Result<i32, Fail> {
    lock_group_for_member(tx, group_id, user_id).await?;
    let list_id = lock_list(tx, group_id, user_id)
        .await?
        .ok_or(WishlistError::NotFound)?;
    sqlx::query_scalar::<_, i32>(
        "SELECT id FROM wishlist_items WHERE id = ? AND wishlist_id = ? FOR UPDATE",
    )
    .bind(item_id)
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(tx_error("lock item"))?
    .ok_or(WishlistError::NotFound)?;
    Ok(list_id)
}

// ------------------------------------------------------------ write queue

/// One queue per user in this process: a user's wishlist writes (several tabs, repeated
/// clicks) run one after another, in arrival order, before they take a database
/// connection. So one user waiting for a long group lock (e.g. a draw) holds at most one
/// pool connection, however many writes they fire. The database locks below stay the
/// authority (they also order writes across processes).
static WRITERS: LazyLock<Mutex<HashMap<i32, Weak<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

async fn writer_turn(user_id: i32) -> tokio::sync::OwnedMutexGuard<()> {
    let queue = {
        let mut writers = WRITERS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        writers.retain(|_, queue| queue.strong_count() > 0);
        match writers.get(&user_id).and_then(Weak::upgrade) {
            Some(queue) => queue,
            None => {
                let queue = Arc::new(tokio::sync::Mutex::new(()));
                writers.insert(user_id, Arc::downgrade(&queue));
                queue
            }
        }
    };
    queue.lock_owned().await
}

/// The unlocked checks before an item transaction: their own item, and that they are a
/// member of its group. Returns the group id.
async fn item_group(state: &AppState, user_id: i32, item_id: i32) -> Result<i32, WishlistError> {
    let group_id = own_item_group(state, user_id, item_id).await?;
    authz::require_member(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    Ok(group_id)
}

/// Runs the item change `change` for the caller, in their write queue. Returns the group
/// of the item when the caller may know it (their own item in a group they are in).
async fn item_write<F, Fut>(item_id: i32, change: F) -> (Option<i32>, Result<(), WishlistError>)
where
    F: FnOnce(AppState, i32, i32) -> Fut,
    Fut: Future<Output = Result<(), WishlistError>>,
{
    let user_id = match user_id().await {
        Ok(id) => id,
        Err(err) => return (None, Err(err)),
    };
    let state = match state() {
        Ok(state) => state,
        Err(err) => return (None, Err(err)),
    };
    let _turn = writer_turn(user_id).await;
    let group_id = match item_group(&state, user_id, item_id).await {
        Ok(id) => id,
        Err(err) => return (None, Err(err)),
    };
    (Some(group_id), change(state, user_id, group_id).await)
}

// ----------------------------------------------------------------- writes

pub async fn add_item(
    group_id: i32,
    title: String,
    description: String,
    link: String,
) -> Result<i32, WishlistError> {
    let user_id = user_id().await?;
    let state = state()?;
    let _turn = writer_turn(user_id).await;
    // Membership first: a non-member learns nothing from validation.
    authz::require_member(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    let values = validate(&title, &description, &link)?;

    let item_id = retrying(|| add_item_tx(&state, group_id, user_id, &values)).await?;
    tracing::info!(user_id, group_id, item_id, "wishlist item added");
    Ok(item_id)
}

/// Writes `positions[i] = i + 1` for every row whose position differs.
async fn renumber(tx: &mut Transaction<'_, MySql>, rows: &[PositionRow]) -> Result<(), Fail> {
    for (n, row) in rows.iter().enumerate() {
        let position = i32::try_from(n + 1).unwrap_or(i32::MAX);
        if row.position != position {
            sqlx::query("UPDATE wishlist_items SET position = ? WHERE id = ?")
                .bind(position)
                .bind(row.id)
                .execute(&mut **tx)
                .await
                .map_err(tx_error("renumber"))?;
        }
    }
    Ok(())
}

async fn add_item_tx(
    state: &AppState,
    group_id: i32,
    user_id: i32,
    values: &ItemValues,
) -> Result<i32, Fail> {
    let mut conn = connection(state).await?;
    let mut tx = begin(&mut conn).await?;
    lock_group_for_member(&mut tx, group_id, user_id).await?;
    // W5: the list row is created by the first item.
    sqlx::query(
        "INSERT INTO wishlists (user_id, group_id) VALUES (?, ?) \
         ON DUPLICATE KEY UPDATE id = LAST_INSERT_ID(id)",
    )
    .bind(user_id)
    .bind(group_id)
    .execute(&mut *tx)
    .await
    .map_err(tx_error("create list"))?;
    let list_id = lock_list(&mut tx, group_id, user_id)
        .await?
        .ok_or(WishlistError::Internal)?;
    let items = lock_items(&mut tx, list_id).await?;
    if items.len() >= MAX_ITEMS {
        return Err(WishlistError::Validation(vec![(
            WishlistField::Items,
            WishlistFieldError::TooManyItems,
        )])
        .into());
    }
    // Rows that never got a position of their own (the column default) are numbered
    // first, so the list's positions stay distinct.
    let mut position = items.iter().map(|row| row.position).max().unwrap_or(0) + 1;
    if items
        .windows(2)
        .any(|pair| pair[0].position >= pair[1].position)
    {
        renumber(&mut tx, &items).await?;
        position = i32::try_from(items.len() + 1).unwrap_or(i32::MAX);
    }
    let inserted = sqlx::query(
        "INSERT INTO wishlist_items (wishlist_id, title, description, link, position) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(list_id)
    .bind(&values.title)
    .bind(&values.description)
    .bind(&values.link)
    .bind(position)
    .execute(&mut *tx)
    .await
    .map_err(tx_error("insert item"))?;
    let item_id = i32::try_from(inserted.last_insert_id()).map_err(|_| {
        tracing::error!("wishlists: item id out of range");
        WishlistError::Internal
    })?;
    touch_list(&mut tx, list_id).await?;
    commit(tx).await?;
    Ok(item_id)
}

/// `(group of the item, when the caller may know it; result)`.
pub async fn update_item(
    item_id: i32,
    title: String,
    description: String,
    link: String,
) -> (Option<i32>, Result<(), WishlistError>) {
    item_write(item_id, |state, user_id, group_id| async move {
        let values = &validate(&title, &description, &link)?;
        let state = &state;
        retrying(move || async move {
            let mut conn = connection(state).await?;
            let mut tx = begin(&mut conn).await?;
            let list_id = lock_own_item(&mut tx, group_id, user_id, item_id).await?;
            sqlx::query(
                "UPDATE wishlist_items SET title = ?, description = ?, link = ? \
                 WHERE id = ? AND wishlist_id = ?",
            )
            .bind(&values.title)
            .bind(&values.description)
            .bind(&values.link)
            .bind(item_id)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .map_err(tx_error("update item"))?;
            touch_list(&mut tx, list_id).await?;
            commit(tx).await
        })
        .await
    })
    .await
}

pub async fn delete_item(item_id: i32) -> (Option<i32>, Result<(), WishlistError>) {
    item_write(item_id, |state, user_id, group_id| async move {
        let state = &state;
        retrying(move || async move {
            let mut conn = connection(state).await?;
            let mut tx = begin(&mut conn).await?;
            let list_id = lock_own_item(&mut tx, group_id, user_id, item_id).await?;
            sqlx::query("DELETE FROM wishlist_items WHERE id = ? AND wishlist_id = ?")
                .bind(item_id)
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .map_err(tx_error("delete item"))?;
            touch_list(&mut tx, list_id).await?;
            commit(tx).await
        })
        .await
    })
    .await
}

pub async fn move_item(
    item_id: i32,
    direction: Direction,
) -> (Option<i32>, Result<(), WishlistError>) {
    item_write(item_id, |state, user_id, group_id| async move {
        retrying(|| move_item_tx(&state, group_id, user_id, item_id, direction)).await
    })
    .await
}

/// W7: swap with the neighbour, then number the list `1..n`. At either end nothing changes.
async fn move_item_tx(
    state: &AppState,
    group_id: i32,
    user_id: i32,
    item_id: i32,
    direction: Direction,
) -> Result<(), Fail> {
    let mut conn = connection(state).await?;
    let mut tx = begin(&mut conn).await?;
    let list_id = lock_own_item(&mut tx, group_id, user_id, item_id).await?;
    let mut items = lock_items(&mut tx, list_id).await?;
    let index = items
        .iter()
        .position(|row| row.id == item_id)
        .ok_or(WishlistError::NotFound)?;
    let neighbour = match direction {
        Direction::Up => index.checked_sub(1),
        Direction::Down => Some(index + 1).filter(|n| *n < items.len()),
    };
    let Some(neighbour) = neighbour else {
        // First up or last down: a successful no-op.
        return commit(tx).await;
    };
    items.swap(index, neighbour);
    renumber(&mut tx, &items).await?;
    touch_list(&mut tx, list_id).await?;
    commit(tx).await
}

pub async fn clear_wishlist(group_id: i32, confirm: bool) -> Result<(), WishlistError> {
    let user_id = user_id().await?;
    let state = state()?;
    let _turn = writer_turn(user_id).await;
    authz::require_member(&state.pool, user_id, group_id)
        .await
        .map_err(from_app)?;
    if !confirm {
        return Err(WishlistError::Validation(vec![(
            WishlistField::Confirm,
            WishlistFieldError::ConfirmRequired,
        )]));
    }
    let state = &state;
    retrying(move || async move {
        let mut conn = connection(state).await?;
        let mut tx = begin(&mut conn).await?;
        lock_group_for_member(&mut tx, group_id, user_id).await?;
        if let Some(list_id) = lock_list(&mut tx, group_id, user_id).await? {
            // The items go with it (ON DELETE CASCADE).
            sqlx::query("DELETE FROM wishlists WHERE id = ?")
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .map_err(tx_error("clear list"))?;
        }
        commit(tx).await
    })
    .await?;
    tracing::info!(user_id, group_id, "wishlist cleared");
    Ok(())
}
