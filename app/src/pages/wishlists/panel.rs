//! The wishlist card on the group page (spec: wishlists, UI notes "WishlistPanel"): my item
//! count with links to edit and to view my list, and every other member's count with a link
//! to their list. The resource is created by the group page at page level
//! ([`panel_resource`]); the page hands the resolved data in.

use leptos::prelude::*;

use super::{row_label, when_logged_in};
use crate::i18n::use_i18n;
use crate::paths::{wishlist_edit_path, wishlist_view_path};
use crate::t_plural;
use crate::wishlists_api::{WishlistError, get_wishlist_panel};

pub type PanelResource = Resource<Result<PanelData, WishlistError>>;
pub use crate::wishlists_api::WishlistPanel as PanelData;

/// The panel's data for the group in `group_id` (`None`: not a valid id).
pub fn panel_resource(group_id: Memo<Option<i32>>) -> PanelResource {
    Resource::new(
        move || group_id.get(),
        |id| async move {
            match id {
                Some(id) => when_logged_in(get_wishlist_panel(id)).await,
                None => Err(WishlistError::NotFound),
            }
        },
    )
}

/// The card. The group page awaits the panel data together with the group (both resources
/// resolve before the page is sent, also when the group is not found).
#[component]
pub fn WishlistPanel(group_id: i32, data: PanelData) -> impl IntoView {
    let i18n = use_i18n();
    let me = data.members.iter().find(|m| m.is_me).map(|m| m.user_id);
    let mine = i64::from(data.my_item_count);
    let others: Vec<_> = data.members.into_iter().filter(|m| !m.is_me).collect();
    let has_others = !others.is_empty();
    let rows = others
        .into_iter()
        .map(|member| {
            let count = if member.item_count == 0 {
                i18n.t("wishlist.view.empty")
            } else {
                let n = i64::from(member.item_count);
                t_plural!(i18n, "wishlist.panel.itemCount", n, count = n)
            };
            let label = row_label(&i18n, "wishlist.panel.view", &member.name);
            view! {
                <li class="list-group-item d-flex flex-wrap justify-content-between align-items-center gap-2">
                    <div>
                        <span class="fw-semibold">{member.name}</span>
                        <div class="small text-muted">{count}</div>
                    </div>
                    // A document load (not a client-side route change): the list is
                    // complete as soon as the new URL shows.
                    <a
                        href=wishlist_view_path(group_id, member.user_id)
                        rel="external"
                        class="btn btn-sm btn-outline-primary"
                        aria-label=label
                    >
                        {i18n.t("wishlist.panel.view")}
                    </a>
                </li>
            }
        })
        .collect_view();
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("group.show.yourWishlist")}</h2>
                <p class="text-muted">{i18n.t("group.show.wishlistHelp")}</p>
                <p class="mb-2">{t_plural!(i18n, "wishlist.panel.myItemCount", mine, count = mine)}</p>
                <a href=wishlist_edit_path(group_id) class="btn btn-primary">
                    {i18n.t("group.show.editWishlist")}
                </a>
                {me
                    .map(|me| {
                        view! {
                            <p class="small mt-2 mb-0">
                                <a href=wishlist_view_path(group_id, me)>
                                    {i18n.t("group.show.viewYourWishlist")}
                                </a>
                                " "
                                {i18n.t("group.show.viewYourWishlistHelp")}
                            </p>
                        }
                    })}
                {has_others
                    .then(|| {
                        view! {
                            <h3 class="h6 mt-4">{i18n.t("wishlist.panel.membersTitle")}</h3>
                            <ul class="list-group list-group-flush">{rows}</ul>
                        }
                    })}
            </div>
        </div>
    }
}
