//! Own wishlist editor, including source selection and progressive HTML checkbox forms.
use crate::{auth::get_current_user, components::RequireUser, i18n::use_i18n, wishlist_api::*};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

#[component]
pub fn WishlistPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    let i18n = use_i18n();
    let params = use_params_map();
    let query = use_query_map();
    let group_id = Memo::new(move |_| {
        params
            .get()
            .get("id")
            .and_then(|id| id.parse::<i32>().ok())
            .filter(|id| *id > 0)
            .unwrap_or(0)
    });
    let source_id = Memo::new(move |_| {
        query
            .get()
            .get("source")
            .and_then(|id| id.parse::<i32>().ok())
            .filter(|id| *id > 0)
    });
    let data = Resource::new_blocking(
        move || (group_id.get(), source_id.get()),
        |(id, source)| async move {
            if get_current_user().await?.is_none() {
                return Err(crate::AppError::Unauthorized);
            }
            let list = get_my_wishlist(id).await?;
            let sources = get_wishlist_sources(id).await?;
            let selected = match source {
                Some(source) if sources.iter().any(|s| s.group_id == source) => {
                    Some(get_my_wishlist(source).await?)
                }
                _ => None,
            };
            Ok((list, sources, selected))
        },
    );
    view! {
        <RequireUser>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    match data.await {
                        Err(err) => view! { <div class="alert alert-danger" role="alert">{i18n.t(err.message_key())}</div> }.into_any(),
                        Ok((list,sources,selected)) => view! {
                            <h1>{i18n.t("wishlist.edit.title")}</h1>
                            <h2 class="h4">{list.group_name}</h2>
                            <p>{i18n.t("wishlist.edit.visibility")}</p>
                            <form method="post" action="/api/add_wishlist_item" class="card card-body mb-4">
                                <input type="hidden" name="group_id" value=list.group_id/>
                                <label for="wish-title">{i18n.t("wishlist.edit.itemTitle")}</label>
                                <input id="wish-title" class="form-control mb-2" name="title" required maxlength="255"/>
                                <label for="wish-description">{i18n.t("wishlist.edit.itemDescription")}</label>
                                <textarea id="wish-description" class="form-control mb-2" name="description" maxlength="2000"></textarea>
                                <label for="wish-link">{i18n.t("wishlist.edit.link")}</label>
                                <input id="wish-link" class="form-control mb-2" name="link" inputmode="url" maxlength="2048"/>
                                <button class="btn btn-primary" type="submit" disabled=list.items.len()>=MAX_ITEMS>{i18n.t("wishlist.edit.addItem")}</button>
                            </form>
                            <ul class="list-group mb-4">
                                {list.items.into_iter().map(|item| view! {
                                    <li class="list-group-item">
                                        <strong>{item.title}</strong>
                                        <p style="white-space: pre-line">{item.description}</p>
                                        {item.link.map(|link| view! { <a href=link.clone() target="_blank" rel="noopener noreferrer nofollow">{link.clone()}</a> })}
                                    </li>
                                }).collect_view()}
                            </ul>
                            <h2 class="h4">{i18n.t("wishlist.copy.title")}</h2>
                            <p>{i18n.t("wishlist.copy.help")}</p>
                            <form method="get" class="mb-3">
                                <label for="wish-source">{i18n.t("wishlist.copy.source")}</label>
                                <select id="wish-source" name="source" class="form-select mb-2">
                                    {sources.into_iter().map(|source| view! { <option value=source.group_id selected=Some(source.group_id)==source_id.get_untracked()>{source.group_name}</option> }).collect_view()}
                                </select>
                                <button type="submit" class="btn btn-secondary">{i18n.t("wishlist.copy.choose")}</button>
                            </form>
                            {selected.map(|source| view! {
                                <form method="post" action="/api/copy_wishlist_items">
                                    <input type="hidden" name="group_id" value=list.group_id/>
                                    <input type="hidden" name="source_group_id" value=source.group_id/>
                                    <fieldset>
                                        <legend class="h5">{source.group_name}</legend>
                                        {source.items.into_iter().enumerate().map(|(index,item)| view! {
                                            <label class="d-block mb-2">
                                                <input type="checkbox" name=format!("item_ids[{index}]") value=item.id/>
                                                {item.title}
                                            </label>
                                        }).collect_view()}
                                    </fieldset>
                                    <button type="submit" class="btn btn-primary">{i18n.t("wishlist.copy.submit")}</button>
                                </form>
                            })}
                        }.into_any()
                    }
                })}
            </Suspense>
        </RequireUser>
    }
}

#[component]
pub fn WishlistsPage() -> impl IntoView {
    let i18n = use_i18n();
    let groups = Resource::new_blocking(
        || (),
        |_| async {
            if get_current_user().await?.is_none() {
                return Err(crate::AppError::Unauthorized);
            }
            get_wishlist_groups().await
        },
    );
    view! {
        <RequireUser>
            <h1>{i18n.t("wishlist.edit.title")}</h1>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    match groups.await {
                        Err(err) => view! { <p role="alert">{i18n.t(err.message_key())}</p> }.into_any(),
                        Ok(groups) => view! { <ul>{groups.into_iter().map(|group| view! { <li><a href=format!("/groups/{}/wishlist",group.group_id)>{group.group_name}</a></li> }).collect_view()}</ul> }.into_any(),
                    }
                })}
            </Suspense>
        </RequireUser>
    }
}
