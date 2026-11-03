//! The same review form handles received shares and the copy/paste fallback.
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::auth::get_current_user;
use crate::components::RequireUser;
use crate::i18n::use_i18n;
use crate::share_api::{ShareDraft, get_shared_draft};
use crate::wishlist_api::get_wishlist_groups;

#[component]
pub fn SharedWishPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        crate::server::respond::token_page_headers();
    }
    let i18n = use_i18n();
    let query = use_query_map();
    let id = Memo::new(move |_| query.read().get("id").unwrap_or_default());
    let data = Resource::new_blocking(
        move || id.get(),
        |id| async move {
            if get_current_user().await?.is_none() {
                return Err(crate::AppError::Unauthorized);
            }
            let draft = if id.is_empty() {
                ShareDraft::default()
            } else {
                get_shared_draft(id).await?
            };
            Ok((draft, get_wishlist_groups().await?))
        },
    );
    view! {
        <RequireUser>
            <h1>{i18n.t("share.title")}</h1>
            <p>{i18n.t("share.intro")}</p>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    match data.await {
                        Err(error) => view! {
                            <div class="alert alert-danger" role="alert">{i18n.t(error.message_key())}</div>
                            <a href="/wishes/shared">{i18n.t("share.start_again")}</a>
                        }.into_any(),
                        Ok((_, groups)) if groups.is_empty() => view! {
                            <p class="alert alert-info" role="status">{i18n.t("share.no_lists")}</p>
                        }.into_any(),
                        Ok((draft, groups)) => view! {
                            <form method="post" action="/api/add_shared_wish" class="card card-body mb-4">
                                <input type="hidden" name="id" value=id.get_untracked()/>
                                <label for="share-group">{i18n.t("share.list")}</label>
                                <select id="share-group" name="group_id" class="form-select mb-3" required>
                                    {groups.into_iter().map(|group| view! {
                                        <option value=group.group_id>{group.group_name}</option>
                                    }).collect_view()}
                                </select>
                                <label for="share-title">{i18n.t("share.item_title")}</label>
                                <input id="share-title" name="title" class="form-control mb-3" required maxlength="255" value=draft.title/>
                                <label for="share-link">{i18n.t("share.link")}</label>
                                <input id="share-link" name="link" class="form-control mb-3" inputmode="url" maxlength="2048" value=draft.link/>
                                <label for="share-description">{i18n.t("share.description")}</label>
                                <textarea id="share-description" name="description" class="form-control mb-3" maxlength="2000">{draft.description}</textarea>
                                <p class="form-text">{i18n.t("share.review_help")}</p>
                                <button type="submit" class="btn btn-primary">{i18n.t("share.confirm")}</button>
                            </form>
                        }.into_any(),
                    }
                })}
            </Suspense>
            <h2 class="h4">{i18n.t("share.iphone.title")}</h2>
            <p>{i18n.t("share.iphone.help")}</p>
            <a href="/install">{i18n.t("app.install.title")}</a>
        </RequireUser>
    }
}
