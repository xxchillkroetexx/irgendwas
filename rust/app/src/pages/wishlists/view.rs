//! `/groups/:id/wishlist/:user_id`: a member's list, read-only (spec: wishlists, UI notes
//! "view"). Every current member may see every current member's list; the owner also gets
//! an Edit button.

use leptos::prelude::*;

use super::{ItemLink, WishlistErrorView, route_id, when_logged_in};
use crate::components::RequireUser;
use crate::format::LocalDate;
use crate::i18n::use_i18n;
use crate::paths::{group_path, wishlist_edit_path};
use crate::t;
use crate::wishlists_api::{ItemView, WishlistError, WishlistView, get_wishlist};

#[component]
pub fn WishlistViewPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    let group_id = route_id("id");
    let user_id = route_id("user_id");
    let list: Resource<Result<WishlistView, WishlistError>> = Resource::new(
        move || (group_id.get(), user_id.get()),
        |ids| async move {
            match ids {
                (Some(group), Some(user)) => when_logged_in(get_wishlist(group, user)).await,
                _ => Err(WishlistError::NotFound),
            }
        },
    );
    view! {
        <RequireUser>
            <Transition fallback=|| ()>
                {move || Suspend::new(async move {
                    match list.await {
                        Ok(wishlist) => view! { <ReadOnlyView wishlist/> }.into_any(),
                        Err(error) => view! { <WishlistErrorView error/> }.into_any(),
                    }
                })}
            </Transition>
        </RequireUser>
    }
}

#[component]
fn ReadOnlyView(wishlist: WishlistView) -> impl IntoView {
    let i18n = use_i18n();
    let group_id = wishlist.group.id;
    let is_own = wishlist.is_own;
    let has_items = !wishlist.items.is_empty();
    // Only a list with items says when it changed (and that it is in order).
    let updated = wishlist.updated_at.filter(|_| has_items);

    let body = if has_items {
        let cards = wishlist
            .items
            .into_iter()
            .map(|item| view! { <ItemCard item/> })
            .collect_view();
        view! {
            <p class="text-muted mb-1">{i18n.t("wishlist.view.orderHint")}</p>
            {updated
                .map(|at| {
                    view! {
                        <p class="text-muted small">
                            {t!(i18n, "wishlist.view.lastUpdated", date = LocalDate(at))}
                        </p>
                    }
                })}
            <ol class="list-unstyled">{cards}</ol>
        }
        .into_any()
    } else if is_own {
        view! {
            <div class="alert alert-info">
                <p class="mb-1">{i18n.t("wishlist.view.empty")}</p>
                <p class="mb-2">{i18n.t("wishlist.edit.addSome")}</p>
                <a href=wishlist_edit_path(group_id) class="btn btn-primary btn-sm">
                    {i18n.t("wishlist.view.addNow")}
                </a>
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class="alert alert-info">
                <p class="mb-1">{i18n.t("wishlist.view.empty")}</p>
                <p class="mb-1">{i18n.t("wishlist.view.noPreference")}</p>
                <p class="mb-0">{i18n.t("wishlist.view.suggestGiftCard")}</p>
            </div>
        }
        .into_any()
    };

    view! {
        <div class="d-flex flex-wrap justify-content-between align-items-start gap-3 mb-3">
            <div>
                <h1 class="h2">{t!(i18n, "wishlist.view.title", name = wishlist.owner.name)}</h1>
                <p class="mb-0">
                    {i18n.t("wishlist.view.forGroup")} " "
                    <a href=group_path(group_id)>{wishlist.group.name}</a>
                </p>
            </div>
            {is_own
                .then(|| {
                    view! {
                        <a href=wishlist_edit_path(group_id) class="btn btn-primary">
                            {i18n.t("wishlist.edit.title")}
                        </a>
                    }
                })}
        </div>
        {body}
        <a href=group_path(group_id) class="btn btn-outline-secondary mt-2">
            {i18n.t("wishlist.view.backToGroup")}
        </a>
    }
}

#[component]
fn ItemCard(item: ItemView) -> impl IntoView {
    view! {
        <li class="card mb-3">
            <div class="card-body text-break">
                <h2 class="h5 card-title">{item.title}</h2>
                {item
                    .description
                    .map(|text| view! { <p class="card-text" style="white-space: pre-line">{text}</p> })}
                {item.link.map(|link| view! { <ItemLink link host=item.link_host/> })}
            </div>
        </li>
    }
}
