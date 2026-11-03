//! Own wishlist editing and copying. IDs never grant access without server authorization.
use crate::AppError;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

pub const MAX_ITEMS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct ItemView {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WishlistView {
    pub group_id: i32,
    pub group_name: String,
    pub items: Vec<ItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct WishlistSource {
    pub group_id: i32,
    pub group_name: String,
}

/// Shared validation for entered and copied wishes; never accept active URL schemes.
pub fn validate_item(
    title: &str,
    description: &str,
    link: &str,
) -> Result<(String, Option<String>, Option<String>), AppError> {
    let title = title.trim();
    let description = description.trim();
    let link = link.trim();
    let invalid = || AppError::Validation("validation.wishlist.item_invalid".into());
    if title.is_empty()
        || title.chars().count() > 255
        || description.chars().count() > 2000
        || link.chars().count() > 2048
    {
        return Err(invalid());
    }
    let link = if link.is_empty() {
        None
    } else {
        let value = if link.contains("://") {
            link.to_string()
        } else {
            format!("https://{link}")
        };
        let url = url::Url::parse(&value).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.as_str().chars().count() > 2048
        {
            return Err(invalid());
        }
        Some(url.to_string())
    };
    Ok((
        title.into(),
        (!description.is_empty()).then(|| description.into()),
        link,
    ))
}

#[server(prefix = "/api", endpoint = "get_my_wishlist")]
pub async fn get_my_wishlist(group_id: i32) -> Result<WishlistView, AppError> {
    crate::server::respond::with_error_status(
        crate::server::wishlist::get_my_wishlist(group_id).await,
    )
}

#[server(prefix = "/api", endpoint = "get_wishlist_sources")]
pub async fn get_wishlist_sources(group_id: i32) -> Result<Vec<WishlistSource>, AppError> {
    crate::server::respond::with_error_status(crate::server::wishlist::get_sources(group_id).await)
}

#[server(prefix = "/api", endpoint = "add_wishlist_item")]
pub async fn add_wishlist_item(
    group_id: i32,
    title: String,
    description: String,
    link: String,
) -> Result<(), AppError> {
    let result = crate::server::wishlist::add_item(group_id, title, description, link).await;
    crate::server::wishlist::outcome(group_id, result).await
}

/// Selected item IDs from the source wishlist.
#[server(prefix = "/api", endpoint = "copy_wishlist_items")]
pub async fn copy_wishlist_items(
    group_id: i32,
    source_group_id: i32,
    item_ids: Option<Vec<i32>>,
) -> Result<(), AppError> {
    let result = crate::server::wishlist::copy_items(
        group_id,
        source_group_id,
        item_ids.unwrap_or_default(),
    )
    .await;
    crate::server::wishlist::outcome(group_id, result).await
}

/// All current groups, including empty lists, for editor navigation and share intake.
#[server(prefix = "/api", endpoint = "get_wishlist_groups")]
pub async fn get_wishlist_groups() -> Result<Vec<WishlistSource>, AppError> {
    let user = crate::server::auth::require_user().await?;
    let pool = crate::server::pool()?;
    crate::server::respond::with_error_status(Ok(sqlx::query_as::<_, WishlistSource>("SELECT g.id AS group_id,g.name AS group_name FROM `groups` g WHERE g.admin_id=? OR EXISTS (SELECT 1 FROM group_members m WHERE m.group_id=g.id AND m.user_id=?) ORDER BY g.name,g.id").bind(user.id).bind(user.id).fetch_all(&pool).await?))
}
