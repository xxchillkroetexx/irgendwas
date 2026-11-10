//! Wishlist server functions, their error type and their view types (spec: wishlists).
//!
//! The endpoints are `POST /api/<fn name>`, form-urlencoded, with the arguments flat
//! (`title`, `description`, `link` instead of the spec's nested `ItemInput`, as in the
//! groups slice), so a plain HTML form can post them. `direction` is `Up` or `Down`;
//! `confirm` is `true` / `false`, and the `on` of a ticked checkbox counts as `true`.
//! Errors are the JSON-encoded [`WishlistError`] with the HTTP status of the variant.
//!
//! The acting user always comes from the session; group and item ids only select rows and
//! are checked on the server (`server::wishlists_flow`, which holds the bodies). An item id
//! only ever reaches the caller's own list in a group they are a member of.

use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::flash::FlashMessage;
use crate::i18n::I18n;
use crate::t;

/// Longest item title, in characters (after trimming).
pub const TITLE_MAX: usize = 255;
/// Longest item description, in characters (after trimming).
pub const DESCRIPTION_MAX: usize = 2000;
/// Longest link, in characters, before and after normalisation (`VARCHAR(2048)`).
pub const LINK_MAX: usize = 2048;
/// Most items one wishlist may hold.
pub const MAX_ITEMS: usize = 50;

/// The form field a validation error belongs to. `Items` is the whole list (item limit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WishlistField {
    Title,
    Description,
    Link,
    Items,
    Confirm,
}

/// One validation problem; each maps to one `validation.wishlist.*` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WishlistFieldError {
    TitleRequired,
    TitleTooLong,
    DescriptionTooLong,
    LinkInvalid,
    LinkTooLong,
    TooManyItems,
    ConfirmRequired,
    SelectionRequired,
}

impl WishlistFieldError {
    /// The i18n key of this problem.
    pub fn message_key(self) -> &'static str {
        match self {
            WishlistFieldError::TitleRequired => "validation.wishlist.title_required",
            WishlistFieldError::TitleTooLong => "validation.wishlist.title_too_long",
            WishlistFieldError::DescriptionTooLong => "validation.wishlist.description_too_long",
            WishlistFieldError::LinkInvalid => "validation.wishlist.link_invalid",
            WishlistFieldError::LinkTooLong => "validation.wishlist.link_too_long",
            WishlistFieldError::TooManyItems => "validation.wishlist.too_many_items",
            WishlistFieldError::ConfirmRequired => "validation.wishlist.confirm_required",
            WishlistFieldError::SelectionRequired => "validation.wishlist.selection_required",
        }
    }

    /// The `{max}` of the limits.
    fn max(self) -> Option<usize> {
        match self {
            WishlistFieldError::TitleTooLong => Some(TITLE_MAX),
            WishlistFieldError::DescriptionTooLong => Some(DESCRIPTION_MAX),
            WishlistFieldError::LinkTooLong => Some(LINK_MAX),
            WishlistFieldError::TooManyItems => Some(MAX_ITEMS),
            _ => None,
        }
    }

    /// The text shown under the field.
    pub fn text(self, i18n: &I18n) -> String {
        match self {
            WishlistFieldError::TitleTooLong => {
                t!(i18n, "validation.wishlist.title_too_long", max = TITLE_MAX)
            }
            WishlistFieldError::DescriptionTooLong => t!(
                i18n,
                "validation.wishlist.description_too_long",
                max = DESCRIPTION_MAX
            ),
            WishlistFieldError::LinkTooLong => {
                t!(i18n, "validation.wishlist.link_too_long", max = LINK_MAX)
            }
            WishlistFieldError::TooManyItems => {
                t!(i18n, "validation.wishlist.too_many_items", max = MAX_ITEMS)
            }
            other => i18n.t(other.message_key()),
        }
    }

    /// The same text as a flash message (plain form posts).
    pub fn flash(self) -> FlashMessage {
        let message = FlashMessage::error(self.message_key());
        match self.max() {
            Some(max) => message.with_number("max", max as i64),
            None => message,
        }
    }
}

/// The error of every wishlist server fn. Carries no internal detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum WishlistError {
    #[error("authentication required")]
    Unauthorized,
    /// Missing group, not a member, target not a member, item not the caller's own (or
    /// missing), or an id that is no positive integer: all the same.
    #[error("not found")]
    NotFound,
    #[error("validation failed")]
    Validation(Vec<(WishlistField, WishlistFieldError)>),
    #[error("internal server error")]
    Internal,
}

impl WishlistError {
    /// The i18n key of the message; for `Validation`, the key of the first problem.
    pub fn message_key(&self) -> &'static str {
        match self {
            WishlistError::Unauthorized => "auth.login.required",
            WishlistError::NotFound => "error.notFound.message",
            WishlistError::Validation(list) => list
                .first()
                .map(|(_, problem)| problem.message_key())
                .unwrap_or("error.generic.message"),
            WishlistError::Internal => "error.generic.message",
        }
    }

    /// The text shown to the user.
    pub fn text(&self, i18n: &I18n) -> String {
        match self {
            WishlistError::Validation(list) => match list.first() {
                Some((_, problem)) => problem.text(i18n),
                None => i18n.t("error.generic.message"),
            },
            other => i18n.t(other.message_key()),
        }
    }

    /// The message as a flash (plain HTML form posts); renders like [`WishlistError::text`].
    pub fn flash(&self) -> FlashMessage {
        match self {
            WishlistError::Validation(list) => match list.first() {
                Some((_, problem)) => problem.flash(),
                None => FlashMessage::error("error.generic.message"),
            },
            other => FlashMessage::error(other.message_key()),
        }
    }

    /// HTTP status of the server fn response carrying this error.
    pub fn status_code(&self) -> u16 {
        match self {
            WishlistError::Unauthorized => 401,
            WishlistError::NotFound => 404,
            WishlistError::Validation(_) => 422,
            WishlistError::Internal => 500,
        }
    }

    /// The problem reported for `field`, if this is a validation error.
    pub fn field_error(&self, field: WishlistField) -> Option<WishlistFieldError> {
        match self {
            WishlistError::Validation(list) => list
                .iter()
                .find(|(f, _)| *f == field)
                .map(|(_, problem)| *problem),
            _ => None,
        }
    }
}

impl crate::error::HttpStatus for WishlistError {
    fn status_code(&self) -> u16 {
        WishlistError::status_code(self)
    }
}

impl FromServerFnError for WishlistError {
    type Encoder = JsonEncoding;

    /// Arguments that do not deserialise (an id that is no integer or out of range, an
    /// unknown direction, a missing field) address nothing that exists: `NotFound` (404),
    /// never a 500. Everything else is `Internal`.
    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        match value {
            ServerFnErrorErr::Args(_) | ServerFnErrorErr::MissingArg(_) => {
                #[cfg(feature = "ssr")]
                let _ = crate::server::respond::with_error_status::<(), _>(Err(
                    WishlistError::NotFound,
                ));
                WishlistError::NotFound
            }
            _ => WishlistError::Internal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Down,
}

/// The "yes, really" of `clear_wishlist`: `true` or a ticked checkbox (`on`); anything
/// else, and a missing field, is no confirmation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Confirmation(pub bool);

impl Serialize for Confirmation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(self.0)
    }
}

impl<'de> Deserialize<'de> for Confirmation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = Confirmation;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a boolean or a checkbox value")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Confirmation, E> {
                Ok(Confirmation(value))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Confirmation, E> {
                Ok(Confirmation(matches!(value, "true" | "on")))
            }
        }
        deserializer.deserialize_any(V)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedRef {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    /// Stored value, retained so a legacy URL can be corrected in the editor.
    pub link: Option<String>,
    /// A verified absolute HTTP(S) URL used for navigation, if the stored link is safe.
    pub link_target: Option<String>,
    /// The verified link's host (computed on the server, punycode as is).
    pub link_host: Option<String>,
}

/// A list as a member sees it. Never contains emails or assignment data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WishlistView {
    pub group: NamedRef,
    pub owner: NamedRef,
    pub is_own: bool,
    /// In the owner's order (`position, id`).
    pub items: Vec<ItemView>,
    /// `None` when the owner has no list row (nothing added yet, or cleared).
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelMember {
    pub user_id: i32,
    pub name: String,
    pub is_me: bool,
    pub item_count: u32,
}

/// The group page's wishlist card: my count and every member's (ordered like the group
/// page). Names only, no emails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WishlistPanel {
    pub my_item_count: u32,
    pub members: Vec<PanelMember>,
}

/// Another member's list (or mine). `NotFound` for non-members, missing groups and
/// targets that are not current members.
#[server(prefix = "/api", endpoint = "get_wishlist")]
pub async fn get_wishlist(group_id: i32, user_id: i32) -> Result<WishlistView, WishlistError> {
    crate::server::respond::with_error_status(
        crate::server::wishlists_flow::get_wishlist(group_id, user_id).await,
    )
}

/// My list in the group (the edit page). Creates nothing.
#[server(prefix = "/api", endpoint = "get_my_wishlist")]
pub async fn get_my_wishlist(group_id: i32) -> Result<WishlistView, WishlistError> {
    crate::server::respond::with_error_status(
        crate::server::wishlists_flow::get_my_wishlist(group_id).await,
    )
}

#[server(prefix = "/api", endpoint = "get_wishlist_panel")]
pub async fn get_wishlist_panel(group_id: i32) -> Result<WishlistPanel, WishlistError> {
    crate::server::respond::with_error_status(
        crate::server::wishlists_flow::get_wishlist_panel(group_id).await,
    )
}

/// Appends an item to my list in the group (creating the list); returns the item id.
#[server(prefix = "/api", endpoint = "add_item")]
pub async fn add_item(
    group_id: i32,
    title: String,
    description: String,
    link: String,
) -> Result<i32, WishlistError> {
    use crate::server::wishlists_flow as flow;
    let result = flow::add_item(group_id, title, description, link).await;
    crate::server::respond::with_error_status(
        flow::outcome(Some(group_id), result, Some("flash.success.item_added")).await,
    )
}

/// Changes one of my items (it keeps its position).
#[server(prefix = "/api", endpoint = "update_item")]
pub async fn update_item(
    item_id: i32,
    title: String,
    description: String,
    link: String,
) -> Result<(), WishlistError> {
    use crate::server::wishlists_flow as flow;
    let (group_id, result) = flow::update_item(item_id, title, description, link).await;
    crate::server::respond::with_error_status(
        flow::outcome(group_id, result, Some("flash.success.item_updated")).await,
    )
}

#[server(prefix = "/api", endpoint = "delete_item")]
pub async fn delete_item(item_id: i32) -> Result<(), WishlistError> {
    use crate::server::wishlists_flow as flow;
    let (group_id, result) = flow::delete_item(item_id).await;
    crate::server::respond::with_error_status(
        flow::outcome(group_id, result, Some("flash.success.item_deleted")).await,
    )
}

/// Swaps one of my items with its neighbour; at either end a successful no-op.
#[server(prefix = "/api", endpoint = "move_item")]
pub async fn move_item(item_id: i32, direction: Direction) -> Result<(), WishlistError> {
    use crate::server::wishlists_flow as flow;
    let (group_id, result) = flow::move_item(item_id, direction).await;
    crate::server::respond::with_error_status(flow::outcome(group_id, result, None).await)
}

/// Removes my whole list in the group. Idempotent; needs the confirmation.
#[server(prefix = "/api", endpoint = "clear_wishlist")]
pub async fn clear_wishlist(
    group_id: i32,
    #[server(default)] confirm: Confirmation,
) -> Result<(), WishlistError> {
    use crate::server::wishlists_flow as flow;
    let result = flow::clear_wishlist(group_id, confirm.0).await;
    crate::server::respond::with_error_status(
        flow::outcome(
            Some(group_id),
            result,
            Some("flash.success.wishlist_deleted"),
        )
        .await,
    )
}

/// Nonempty own lists in other groups where the caller remains a member.
#[server(prefix = "/api", endpoint = "get_wishlist_sources")]
pub async fn get_wishlist_sources(group_id: i32) -> Result<Vec<NamedRef>, WishlistError> {
    crate::server::respond::with_error_status(
        crate::server::wishlists_flow::get_sources(group_id).await,
    )
}

/// Copies selected own wishes in source order, leaving the source unchanged.
#[server(prefix = "/api", endpoint = "copy_wishlist_items")]
pub async fn copy_wishlist_items(
    group_id: i32,
    source_group_id: i32,
    item_ids: Option<Vec<i32>>,
) -> Result<(), WishlistError> {
    use crate::server::wishlists_flow as flow;
    let result = flow::copy_items(group_id, source_group_id, item_ids.unwrap_or_default()).await;
    crate::server::respond::with_error_status(
        flow::outcome(
            Some(group_id),
            result,
            Some("flash.success.wishlist_copied"),
        )
        .await,
    )
}
