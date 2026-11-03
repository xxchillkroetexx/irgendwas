//! Wishlist writes serialize on group rows followed by wishlist rows, in ascending group order.
use super::{auth::require_user, authz, pool};
use crate::wishlist_api::{ItemView, MAX_ITEMS, WishlistSource, WishlistView, validate_item};
use crate::{AppError, flash::FlashMessage};
use sqlx::{MySql, Transaction};

pub async fn get_my_wishlist(group_id: i32) -> Result<WishlistView, AppError> {
    let user = require_user().await?;
    let pool = pool()?;
    authz::require_member(&pool, user.id, group_id).await?;
    let group_name = sqlx::query_scalar("SELECT name FROM `groups` WHERE id = ?")
        .bind(group_id)
        .fetch_one(&pool)
        .await?;
    let items = sqlx::query_as::<_, ItemView>("SELECT i.id, i.title, i.description, i.link FROM wishlist_items i JOIN wishlists w ON w.id=i.wishlist_id WHERE w.user_id=? AND w.group_id=? ORDER BY i.position,i.id")
        .bind(user.id).bind(group_id).fetch_all(&pool).await?;
    Ok(WishlistView {
        group_id,
        group_name,
        items,
    })
}

pub async fn get_sources(group_id: i32) -> Result<Vec<WishlistSource>, AppError> {
    let user = require_user().await?;
    let pool = pool()?;
    authz::require_member(&pool, user.id, group_id).await?;
    Ok(sqlx::query_as::<_, WishlistSource>("SELECT g.id AS group_id, g.name AS group_name FROM wishlists w JOIN `groups` g ON g.id=w.group_id WHERE w.user_id=? AND g.id<>? AND (g.admin_id=? OR EXISTS (SELECT 1 FROM group_members m WHERE m.group_id=g.id AND m.user_id=?)) AND EXISTS (SELECT 1 FROM wishlist_items i WHERE i.wishlist_id=w.id) ORDER BY g.name,g.id")
        .bind(user.id).bind(group_id).bind(user.id).bind(user.id).fetch_all(&pool).await?)
}

async fn lock_group(
    tx: &mut Transaction<'_, MySql>,
    user_id: i32,
    group_id: i32,
) -> Result<(), AppError> {
    let _: i32 = sqlx::query_scalar("SELECT id FROM `groups` WHERE id=? LOCK IN SHARE MODE")
        .bind(group_id)
        .fetch_one(&mut **tx)
        .await?;
    authz::require_member(&mut **tx, user_id, group_id).await?;
    Ok(())
}

async fn lock_list(
    tx: &mut Transaction<'_, MySql>,
    user_id: i32,
    group_id: i32,
    create: bool,
) -> Result<i32, AppError> {
    if create {
        sqlx::query("INSERT INTO wishlists (user_id,group_id) VALUES (?,?) ON DUPLICATE KEY UPDATE id=LAST_INSERT_ID(id)").bind(user_id).bind(group_id).execute(&mut **tx).await?;
    }
    Ok(
        sqlx::query_scalar("SELECT id FROM wishlists WHERE user_id=? AND group_id=? FOR UPDATE")
            .bind(user_id)
            .bind(group_id)
            .fetch_one(&mut **tx)
            .await?,
    )
}

async fn append(
    tx: &mut Transaction<'_, MySql>,
    list_id: i32,
    items: &[ItemView],
) -> Result<(), AppError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wishlist_items WHERE wishlist_id=?")
        .bind(list_id)
        .fetch_one(&mut **tx)
        .await?;
    if count as usize + items.len() > MAX_ITEMS {
        return Err(AppError::Validation(
            "validation.wishlist.too_many_items".into(),
        ));
    }
    let mut position: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position),0) FROM wishlist_items WHERE wishlist_id=?",
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await?;
    for item in items {
        let (title, description, link) = validate_item(
            &item.title,
            item.description.as_deref().unwrap_or_default(),
            item.link.as_deref().unwrap_or_default(),
        )?;
        position += 1;
        sqlx::query("INSERT INTO wishlist_items (wishlist_id,title,description,link,position) VALUES (?,?,?,?,?)").bind(list_id).bind(title).bind(description).bind(link).bind(position).execute(&mut **tx).await?;
    }
    sqlx::query("UPDATE wishlists SET updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(list_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn add_item(
    group_id: i32,
    title: String,
    description: String,
    link: String,
) -> Result<(), AppError> {
    let user = require_user().await?;
    validate_item(&title, &description, &link)?;
    let pool = pool()?;
    let mut tx = pool.begin().await?;
    lock_group(&mut tx, user.id, group_id).await?;
    let list_id = lock_list(&mut tx, user.id, group_id, true).await?;
    append(
        &mut tx,
        list_id,
        &[ItemView {
            id: 0,
            title,
            description: Some(description),
            link: Some(link),
        }],
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn copy_items(
    group_id: i32,
    source_group_id: i32,
    item_ids: Vec<i32>,
) -> Result<(), AppError> {
    let user = require_user().await?;
    let ids: std::collections::BTreeSet<i32> = item_ids.into_iter().collect();
    if group_id == source_group_id
        || ids.is_empty()
        || ids.len() > MAX_ITEMS
        || ids.iter().any(|id| *id <= 0)
    {
        return Err(AppError::Validation(
            "validation.wishlist.selection_required".into(),
        ));
    }
    let pool = pool()?;
    let mut tx = pool.begin().await?;
    let ordered = if group_id < source_group_id {
        [group_id, source_group_id]
    } else {
        [source_group_id, group_id]
    };
    for id in ordered {
        lock_group(&mut tx, user.id, id).await?;
    }
    let mut destination = 0;
    let mut source = 0;
    for id in ordered {
        let list = lock_list(&mut tx, user.id, id, id == group_id).await?;
        if id == group_id {
            destination = list;
        } else {
            source = list;
        }
    }
    let items = sqlx::query_as::<_, ItemView>("SELECT id,title,description,link FROM wishlist_items WHERE wishlist_id=? ORDER BY position,id").bind(source).fetch_all(&mut *tx).await?;
    let selected: Vec<_> = items
        .into_iter()
        .filter(|item| ids.contains(&item.id))
        .collect();
    if selected.len() != ids.len() {
        return Err(AppError::NotFound);
    }
    append(&mut tx, destination, &selected).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn outcome(group_id: i32, result: Result<(), AppError>) -> Result<(), AppError> {
    if super::respond::is_plain_form_post() {
        let message = match &result {
            Ok(()) => FlashMessage::success("flash.success.wishlist_saved"),
            Err(err) => FlashMessage::error(err.message_key()),
        };
        super::flash::push(&super::auth::session().await?, message).await?;
        super::respond::redirect_document(&format!("/groups/{group_id}/wishlist"));
        Ok(())
    } else {
        if result.is_ok() {
            leptos_axum::redirect(&format!("/groups/{group_id}/wishlist"));
        }
        super::respond::with_error_status(result)
    }
}
