//! Group pages (spec: groups, "Pages / routes"): the list, a group, create/edit, the
//! invitation pages `/join` and `/join/:code`, and the archive page `/groups/archive`
//! (spec: group-archive).
//!
//! Every page is behind [`RequireUser`](crate::components::RequireUser). Its data resource is
//! created synchronously at page level (never inside the guard's async children) and only
//! calls the protected server fn for a logged-in visitor. Hooks for later slices (wishlist
//! panel, exclusions link, draw panel) come from those slices' page modules.

mod archive;
mod form;
mod join;
mod list;
mod show;

use std::future::Future;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::AppError;
use crate::auth::get_current_user;
use crate::components::LoginRedirect;
use crate::groups_api::GroupError;
use crate::pages::{Forbidden, NotFound, ServerError};
use crate::paths::group_path;

pub use archive::GroupArchivePage;
pub use form::{GroupCreatePage, GroupEditPage};
pub use join::{JoinPage, JoinPreviewPage};
pub use list::GroupsPage;
pub use show::GroupPage;

/// `/groups/:id` with `:id` a positive integer (digits only), else `None` (the 404 page).
pub fn parse_group_id(raw: &str) -> Option<i32> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<i32>().ok().filter(|id| *id > 0)
}

/// The `:id` route parameter, parsed with [`parse_group_id`].
fn route_group_id() -> Memo<Option<i32>> {
    let params = use_params_map();
    Memo::new(move |_| params.read().get("id").and_then(|raw| parse_group_id(&raw)))
}

/// Runs `fetch` only for a logged-in visitor: the guard redirects everyone else, and the
/// protected fn's error status must not end up in that redirect response.
async fn when_logged_in<T>(
    fetch: impl Future<Output = Result<T, GroupError>>,
) -> Result<T, GroupError> {
    match get_current_user().await {
        Ok(Some(_)) => fetch.await,
        Ok(None) | Err(AppError::Unauthorized) => Err(GroupError::Unauthorized),
        Err(_) => Err(GroupError::Internal),
    }
}

/// What a group page shows instead of its content: the 404 page (missing group or not a
/// member, indistinguishable), the 403 page, the login redirect when the session ended
/// after the guard let the visitor in, else the 500 page.
#[component]
fn GroupErrorView(error: GroupError) -> impl IntoView {
    match error {
        GroupError::NotFound => view! { <NotFound/> }.into_any(),
        GroupError::Forbidden => view! { <Forbidden/> }.into_any(),
        GroupError::Unauthorized => view! { <LoginRedirect/> }.into_any(),
        _ => view! { <ServerError/> }.into_any(),
    }
}
