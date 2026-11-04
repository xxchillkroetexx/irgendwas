//! Wishlist pages (spec: wishlists, "Pages / routes"): my list's edit page
//! `/groups/:id/wishlist`, the read-only view `/groups/:id/wishlist/:user_id`, and the
//! panel on the group page.
//!
//! Every page is behind [`RequireUser`](crate::components::RequireUser). Its data resource
//! (and its server actions) are created synchronously at page level, never inside the
//! guard's async children, and only call the protected server fn for a logged-in visitor.
//! User text (titles, descriptions, names) is always rendered as text nodes.

mod edit;
mod panel;
mod view;

use std::future::Future;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::AppError;
use crate::auth::get_current_user;
use crate::i18n::{I18n, use_i18n};
use crate::pages::groups::parse_group_id;
use crate::pages::{NotFound, ServerError};
use crate::t;
use crate::wishlists_api::WishlistError;

pub use edit::WishlistEditPage;
pub use panel::{PanelData, PanelResource, WishlistPanel, panel_resource};
pub use view::WishlistViewPage;

/// A route parameter that must be a positive integer (else the 404 page).
fn route_id(name: &'static str) -> Memo<Option<i32>> {
    let params = use_params_map();
    Memo::new(move |_| params.read().get(name).and_then(|raw| parse_group_id(&raw)))
}

/// Runs `fetch` only for a logged-in visitor: the guard redirects everyone else, and the
/// protected fn's error status must not end up in that redirect response.
async fn when_logged_in<T>(
    fetch: impl Future<Output = Result<T, WishlistError>>,
) -> Result<T, WishlistError> {
    match get_current_user().await {
        Ok(Some(_)) => fetch.await,
        Ok(None) | Err(AppError::Unauthorized) => Err(WishlistError::Unauthorized),
        Err(_) => Err(WishlistError::Internal),
    }
}

/// What a wishlist page shows instead of its content: the 404 page (missing group, not a
/// member, target not a member: indistinguishable), nothing while the guard redirects,
/// else the 500 page.
#[component]
fn WishlistErrorView(error: WishlistError) -> impl IntoView {
    match error {
        WishlistError::NotFound => view! { <NotFound/> }.into_any(),
        WishlistError::Unauthorized => ().into_any(),
        _ => view! { <ServerError/> }.into_any(),
    }
}

/// The accessible name of a per-row control: the action and the row's title.
fn row_label(i18n: &I18n, action_key: &str, title: &str) -> String {
    t!(
        i18n,
        "wishlist.edit.itemAction",
        action = i18n.t(action_key),
        title = title
    )
}

/// An item's link: opens in a new tab without referrer or opener, followed by its host so
/// the reader sees where it goes (W10).
#[component]
fn ItemLink(link: String, host: Option<String>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <a
            href=link
            target="_blank"
            rel="noopener noreferrer nofollow"
            class="btn btn-sm btn-outline-primary"
        >
            {i18n.t("wishlist.view.viewItem")}
        </a>
        {host.map(|host| view! { <span class="ms-2 small text-muted text-break">{host}</span> })}
    }
}
