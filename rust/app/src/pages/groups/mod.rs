//! Group pages (spec: groups, "Pages / routes"): the list, a group, create/edit, and the
//! invitation pages `/join` and `/join/:code`.
//!
//! Every page is behind [`RequireUser`](crate::components::RequireUser). Its data resource is
//! created synchronously at page level (never inside the guard's async children) and only
//! calls the protected server fn for a logged-in visitor. Hooks for later slices (wishlist
//! panel, exclusions link, draw panel) come from those slices' page modules.

mod form;
mod join;
mod list;
mod show;

use std::future::Future;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::AppError;
use crate::auth::get_current_user;
use crate::groups_api::GroupError;
use crate::pages::{Forbidden, NotFound, ServerError};
use crate::paths::group_path;

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
/// member, indistinguishable), the 403 page, nothing while the guard redirects, else the
/// 500 page.
#[component]
fn GroupErrorView(error: GroupError) -> impl IntoView {
    match error {
        GroupError::NotFound => view! { <NotFound/> }.into_any(),
        GroupError::Forbidden => view! { <Forbidden/> }.into_any(),
        GroupError::Unauthorized => ().into_any(),
        _ => view! { <ServerError/> }.into_any(),
    }
}

/// Asks the browser's confirm dialog; `true` without JavaScript (never runs on the server).
pub(crate) fn confirmed(message: &str) -> bool {
    #[cfg(feature = "hydrate")]
    {
        window().confirm_with_message(message).unwrap_or(false)
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = message;
        true
    }
}

/// A submit button that asks `confirm` first; a dismissed dialog cancels the submission.
/// Without JavaScript the form posts directly. `aria_label` names the button for assistive
/// technology when the visible label alone is ambiguous (e.g. one per list row).
#[component]
pub(crate) fn ConfirmSubmit(
    #[prop(into)] label: String,
    #[prop(into)] confirm: String,
    class: &'static str,
    #[prop(optional, into)] aria_label: Option<String>,
) -> impl IntoView {
    let on_click = move |ev: leptos::ev::MouseEvent| {
        if !confirmed(&confirm) {
            ev.prevent_default();
        }
    };
    view! {
        <button type="submit" class=class aria-label=aria_label on:click=on_click>
            {label}
        </button>
    }
}
