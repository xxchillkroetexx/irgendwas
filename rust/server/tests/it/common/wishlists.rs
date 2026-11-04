//! Helpers for the wishlists tests: the `WishlistError` view of a server fn reply, one
//! wrapper per wishlist server fn, DB readers and the spec's fixture world.
#![allow(dead_code)]

use app::test_support::{self as ts};
use app::wishlists_api::{WishlistError, WishlistField, WishlistFieldError};
use serde::de::DeserializeOwned;
use sqlx::MySqlPool;

use super::auth::{Client, Reply};
use super::groups::{W, world};

pub trait WlReply {
    fn wl_result<T: DeserializeOwned>(&self) -> Result<T, WishlistError>;
    fn wl_value<T: DeserializeOwned>(&self) -> T;
    fn wl_ok(&self) -> &Self;
    fn wl_err(&self) -> WishlistError;
    /// The (field, problem) list of a validation error; panics otherwise.
    fn problems(&self) -> Vec<(WishlistField, WishlistFieldError)>;
}

impl WlReply for Reply {
    fn wl_result<T: DeserializeOwned>(&self) -> Result<T, WishlistError> {
        if self.status.is_success() {
            Ok(serde_json::from_str(&self.body)
                .unwrap_or_else(|e| panic!("bad success body {:?}: {e}", self.body)))
        } else {
            Err(serde_json::from_str(&self.body).unwrap_or_else(|e| {
                panic!(
                    "bad error body {:?} (status {}): {e}",
                    self.body, self.status
                )
            }))
        }
    }

    fn wl_value<T: DeserializeOwned>(&self) -> T {
        self.wl_result::<T>()
            .unwrap_or_else(|e| panic!("expected success, got {e:?}"))
    }

    fn wl_ok(&self) -> &Self {
        if let Err(err) = self.wl_result::<serde_json::Value>() {
            panic!("expected success, got {err:?}");
        }
        self
    }

    fn wl_err(&self) -> WishlistError {
        match self.wl_result::<serde_json::Value>() {
            Ok(v) => panic!("expected a WishlistError, got success {v}"),
            Err(e) => e,
        }
    }

    fn problems(&self) -> Vec<(WishlistField, WishlistFieldError)> {
        match self.wl_err() {
            WishlistError::Validation(list) => list,
            other => panic!("expected a validation error, got {other:?}"),
        }
    }
}

pub async fn get_wishlist(c: &mut Client, group: i32, user: i32) -> Reply {
    c.call(
        "get_wishlist",
        &[
            ("group_id", &group.to_string()),
            ("user_id", &user.to_string()),
        ],
    )
    .await
}

pub async fn get_my_wishlist(c: &mut Client, group: i32) -> Reply {
    c.call("get_my_wishlist", &[("group_id", &group.to_string())])
        .await
}

pub async fn get_wishlist_panel(c: &mut Client, group: i32) -> Reply {
    c.call("get_wishlist_panel", &[("group_id", &group.to_string())])
        .await
}

pub async fn add_item(c: &mut Client, group: i32, title: &str, desc: &str, link: &str) -> Reply {
    c.call(
        "add_item",
        &[
            ("group_id", &group.to_string()),
            ("title", title),
            ("description", desc),
            ("link", link),
        ],
    )
    .await
}

pub async fn update_item(c: &mut Client, item: i32, title: &str, desc: &str, link: &str) -> Reply {
    c.call(
        "update_item",
        &[
            ("item_id", &item.to_string()),
            ("title", title),
            ("description", desc),
            ("link", link),
        ],
    )
    .await
}

pub async fn delete_item(c: &mut Client, item: i32) -> Reply {
    c.call("delete_item", &[("item_id", &item.to_string())])
        .await
}

pub async fn move_item(c: &mut Client, item: i32, direction: &str) -> Reply {
    c.call(
        "move_item",
        &[("item_id", &item.to_string()), ("direction", direction)],
    )
    .await
}

pub async fn up(c: &mut Client, item: i32) -> Reply {
    move_item(c, item, "Up").await
}

pub async fn down(c: &mut Client, item: i32) -> Reply {
    move_item(c, item, "Down").await
}

pub async fn clear_wishlist(c: &mut Client, group: i32, confirm: bool) -> Reply {
    c.call(
        "clear_wishlist",
        &[
            ("group_id", &group.to_string()),
            ("confirm", if confirm { "true" } else { "false" }),
        ],
    )
    .await
}

/// `/groups/{g}/wishlist`
pub fn edit_path(g: i32) -> String {
    format!("/groups/{g}/wishlist")
}

/// `/groups/{g}/wishlist/{u}`
pub fn view_path(g: i32, u: i32) -> String {
    format!("/groups/{g}/wishlist/{u}")
}

/// Titles of the user's list in the group, in `ORDER BY position, id`.
pub async fn titles(pool: &MySqlPool, g: i32, user: i32) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT i.title FROM wishlist_items i JOIN wishlists w ON w.id = i.wishlist_id \
         WHERE w.group_id = ? AND w.user_id = ? ORDER BY i.position, i.id",
    )
    .bind(g)
    .bind(user)
    .fetch_all(pool)
    .await
    .expect("read titles")
}

/// Positions of the user's list in the group, in `ORDER BY position, id`.
pub async fn positions(pool: &MySqlPool, g: i32, user: i32) -> Vec<i32> {
    sqlx::query_scalar(
        "SELECT i.position FROM wishlist_items i JOIN wishlists w ON w.id = i.wishlist_id \
         WHERE w.group_id = ? AND w.user_id = ? ORDER BY i.position, i.id",
    )
    .bind(g)
    .bind(user)
    .fetch_all(pool)
    .await
    .expect("read positions")
}

pub async fn wishlist_rows(pool: &MySqlPool, g: i32, user: i32) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM wishlists WHERE group_id = ? AND user_id = ?")
        .bind(g)
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn total_items(pool: &MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM wishlist_items")
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn total_wishlists(pool: &MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM wishlists")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// One stored item.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ItemRow {
    pub id: i32,
    pub wishlist_id: i32,
    pub title: String,
    pub description: Option<String>,
    pub link: Option<String>,
}

pub async fn item_row(pool: &MySqlPool, id: i32) -> Option<ItemRow> {
    sqlx::query_as::<_, ItemRow>(
        "SELECT id, wishlist_id, title, description, link FROM wishlist_items WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// Every item and wishlist row (without positions), for "nothing changed" assertions.
pub async fn dump(pool: &MySqlPool) -> (Vec<ItemRow>, Vec<(i32, i32, i32)>) {
    let items = sqlx::query_as::<_, ItemRow>(
        "SELECT id, wishlist_id, title, description, link FROM wishlist_items ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let lists = sqlx::query_as::<_, (i32, i32, i32)>(
        "SELECT id, user_id, group_id FROM wishlists ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    (items, lists)
}

pub async fn updated_at(
    pool: &MySqlPool,
    g: i32,
    user: i32,
) -> Option<chrono::DateTime<chrono::Utc>> {
    sqlx::query_scalar("SELECT updated_at FROM wishlists WHERE group_id = ? AND user_id = ?")
        .bind(g)
        .bind(user)
        .fetch_optional(pool)
        .await
        .unwrap()
}

pub async fn set_updated_at(pool: &MySqlPool, wishlist_id: i32, at: chrono::DateTime<chrono::Utc>) {
    sqlx::query("UPDATE wishlists SET updated_at = ? WHERE id = ?")
        .bind(at)
        .bind(wishlist_id)
        .execute(pool)
        .await
        .unwrap();
}

/// The spec's fixture. G: admin Alice, members Bob and Carol, not drawn. H: admin Bob,
/// member Dave. Bob's list in G: items A, B, C (in that order); in H: item X. Alice's list
/// in G: one item. Carol has no wishlist row.
pub struct F {
    pub w: W,
    pub h: i32,
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub x: i32,
    pub alice_item: i32,
    pub bob_list: i32,
    pub bob_list_h: i32,
    pub alice_list: i32,
}

pub const ITEM_A: &str = "Item A";
pub const ITEM_B: &str = "Item B";
pub const ITEM_C: &str = "Item C";
pub const ITEM_X: &str = "Item X in H";
pub const ALICE_ITEM: &str = "Alice thing";
pub const H_CODE: &str = "HOBB2345";

async fn first_item(pool: &MySqlPool, list: i32) -> i32 {
    sqlx::query_scalar("SELECT id FROM wishlist_items WHERE wishlist_id = ? ORDER BY id LIMIT 1")
        .bind(list)
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn fixture(pool: &MySqlPool) -> F {
    let w = world(pool).await;
    ts::add_member(pool, w.g, w.carol).await;
    let h = ts::create_group_with_code(pool, w.bob, "Hobby", H_CODE).await;
    ts::add_member(pool, h, w.dave).await;

    let bob_list = ts::create_wishlist(pool, w.g, w.bob, &[ITEM_A, ITEM_B, ITEM_C]).await;
    sqlx::query(
        "UPDATE wishlist_items SET description = 'Desc A', link = 'https://a.example/a' \
         WHERE wishlist_id = ? AND title = ?",
    )
    .bind(bob_list)
    .bind(ITEM_A)
    .execute(pool)
    .await
    .unwrap();
    let ids: Vec<i32> =
        sqlx::query_scalar("SELECT id FROM wishlist_items WHERE wishlist_id = ? ORDER BY id")
            .bind(bob_list)
            .fetch_all(pool)
            .await
            .unwrap();
    let bob_list_h = ts::create_wishlist(pool, h, w.bob, &[ITEM_X]).await;
    let alice_list = ts::create_wishlist(pool, w.g, w.alice, &[ALICE_ITEM]).await;
    F {
        h,
        a: ids[0],
        b: ids[1],
        c: ids[2],
        x: first_item(pool, bob_list_h).await,
        alice_item: first_item(pool, alice_list).await,
        bob_list,
        bob_list_h,
        alice_list,
        w,
    }
}

/// Position of `needle` in `html` (panics when absent).
pub fn pos(html: &str, needle: &str) -> usize {
    html.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found in the page"))
}

/// Without a body mention of any of the catalog keys the pages must translate.
pub fn assert_no_raw_keys(html: &str, what: &str) {
    for prefix in [
        "wishlist.edit.",
        "wishlist.view.",
        "wishlist.panel.",
        "validation.wishlist.",
        "flash.success.item_",
        "flash.success.wishlist_",
        "group.show.yourWishlist",
        "group.show.editWishlist",
        "group.show.viewYourWishlist",
    ] {
        assert!(
            !html.contains(prefix),
            "{what}: raw i18n key {prefix}* in the page"
        );
    }
}
