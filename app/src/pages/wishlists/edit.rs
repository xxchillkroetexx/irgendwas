//! `/groups/:id/wishlist`: the caller's own list in a group (spec: wishlists, UI notes
//! "edit"). Every change is an `ActionForm` (a plain form post without JavaScript, which the
//! server answers with `302` back here and a flash). With JavaScript the page stays, reads
//! the flash and re-reads the list. `?edit={item_id}` renders that item's row as an inline
//! form; an id that is not one of my items in this group is ignored.
//! Each item has an anchor so editing and cancelling remain discoverable without JS.

use leptos::html::{Input, Li, Textarea};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use leptos_router::{NavigateOptions, components::Form};

use super::{ItemLink, WishlistErrorView, route_id, row_label, when_logged_in};
use crate::auth::use_session_resource;
use crate::components::{RequireUser, TextArea};
use crate::flash::use_toasts;
use crate::i18n::use_i18n;
use crate::pages::groups::{ConfirmSubmit, parse_group_id};
use crate::paths::{group_path, wishlist_edit_path, wishlist_view_path};
use crate::t;
use crate::wishlists_api::{
    AddItem, ClearWishlist, CopyWishlistItems, DESCRIPTION_MAX, DeleteItem, Direction, ItemView,
    LINK_MAX, MAX_ITEMS, MoveItem, NamedRef, TITLE_MAX, UpdateItem, WishlistError, WishlistField,
    WishlistFieldError, WishlistView, get_my_wishlist, get_wishlist_sources,
};

type ListResource = Resource<Result<WishlistView, WishlistError>>;

/// The page's server actions. Created at page level, so a re-read of the list (which
/// re-renders it) keeps them and their results.
#[derive(Clone, Copy)]
struct Actions {
    add: ServerAction<AddItem>,
    delete: ServerAction<DeleteItem>,
    move_item: ServerAction<MoveItem>,
    clear: ServerAction<ClearWishlist>,
}

/// Retained across list re-reads and the guard's query-only remounts.
#[derive(Clone, Copy)]
struct Placement {
    focused: StoredValue<Option<i32>>,
    scroll_to: StoredValue<Option<i32>>,
}

fn item_anchor(item_id: i32) -> String {
    format!("item-{item_id}")
}

/// The error of the last submission, if it failed.
fn last_error<T>(action: ServerAction<T>) -> Signal<Option<WishlistError>>
where
    T: leptos::server_fn::ServerFn<Error = WishlistError> + Clone + Send + Sync + 'static,
    T::Output: Send + Sync + 'static,
{
    let value = action.value();
    Signal::derive(move || value.with(|v| v.as_ref().and_then(|r| r.as_ref().err().cloned())))
}

#[component]
pub fn WishlistEditPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    let id = route_id("id");
    let query = use_query_map();
    let editing = Memo::new(move |_| {
        query
            .read()
            .get("edit")
            .and_then(|raw| parse_group_id(&raw))
    });
    let list: ListResource = Resource::new(
        move || id.get(),
        |id| async move {
            match id {
                Some(id) => when_logged_in(get_my_wishlist(id)).await,
                None => Err(WishlistError::NotFound),
            }
        },
    );
    let source_group = Memo::new(move |_| {
        query
            .read()
            .get("source")
            .and_then(|raw| parse_group_id(&raw))
    });
    let sources = Resource::new(
        move || id.get(),
        |group| async move {
            match group {
                Some(group) => when_logged_in(get_wishlist_sources(group)).await,
                None => Err(WishlistError::NotFound),
            }
        },
    );
    let source_list = Resource::new(
        move || source_group.get(),
        |group| async move {
            match group {
                Some(group) => when_logged_in(get_my_wishlist(group)).await.map(Some),
                None => Ok(None),
            }
        },
    );
    let copy = ServerAction::<CopyWishlistItems>::new();
    let actions = Actions {
        add: ServerAction::new(),
        delete: ServerAction::new(),
        move_item: ServerAction::new(),
        clear: ServerAction::new(),
    };
    let add_title = NodeRef::<Input>::new();
    let add_description = NodeRef::<Textarea>::new();
    let add_link = NodeRef::<Input>::new();
    let placement = Placement {
        focused: StoredValue::new(None),
        scroll_to: StoredValue::new(None),
    };
    Effect::new(move |_| match editing.get() {
        Some(_) => placement.scroll_to.set_value(None),
        None => placement.focused.set_value(None),
    });
    Effect::new(move |_| {
        let busy = [
            actions.add.pending(),
            actions.delete.pending(),
            actions.move_item.pending(),
            actions.clear.pending(),
            copy.pending(),
        ];
        if busy.map(|pending| pending.get()).contains(&true) {
            placement.scroll_to.set_value(None);
        }
    });
    // Source selection or a different group is a new task, not a request to revisit
    // the previously saved row.
    Effect::new(move |_| {
        id.track();
        source_group.track();
        placement.scroll_to.set_value(None);
    });

    // The server stored the flash of a success: read it and the new list. Field errors of
    // the add form show in the form; other failures as a notice.
    let session = use_session_resource().0;
    let toasts = use_toasts();
    let after = move |result: Result<(), WishlistError>| {
        match result {
            Ok(()) => session.refetch(),
            Err(err) => toasts.push(err.flash()),
        }
        // The re-read list renders an empty add form: an earlier add error no longer
        // belongs to anything on the page.
        actions.add.value().set(None);
        list.refetch();
    };
    Effect::new(move |_| {
        if let Some(Ok(_)) = actions.add.value().get() {
            if let Some(input) = add_title.get_untracked() {
                input.set_value("");
                let _ = input.focus();
            }
            if let Some(input) = add_description.get_untracked() {
                input.set_value("");
            }
            if let Some(input) = add_link.get_untracked() {
                input.set_value("");
            }
            session.refetch();
            list.refetch();
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.delete.value().get() {
            after(result);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.move_item.value().get() {
            after(result);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.clear.value().get() {
            after(result);
        }
    });

    Effect::new(move |_| {
        if let Some(result) = copy.value().get() {
            match result {
                Ok(()) => session.refetch(),
                Err(error) => toasts.push(error.flash()),
            }
            list.refetch();
        }
    });
    view! {
        <RequireUser>
            <Transition fallback=|| ()>
                {move || Suspend::new(async move {
                    match list.await {
                        Ok(wishlist) => {
                            view! { <EditView wishlist editing list actions add_title add_description add_link placement/> }.into_any()
                        }
                        Err(error) => view! { <WishlistErrorView error/> }.into_any(),
                    }
                })}
            </Transition>
            <CopySection group=id source_group sources source_list action=copy/>
        </RequireUser>
    }
}

#[component]
fn EditView(
    wishlist: WishlistView,
    editing: Memo<Option<i32>>,
    list: ListResource,
    actions: Actions,
    add_title: NodeRef<Input>,
    add_description: NodeRef<Textarea>,
    add_link: NodeRef<Input>,
    placement: Placement,
) -> impl IntoView {
    let i18n = use_i18n();
    let group_id = wishlist.group.id;
    let count = wishlist.items.len();
    let full = count >= MAX_ITEMS;
    let items = wishlist.items;
    let has_items = !items.is_empty();
    let last = count.saturating_sub(1);
    let add_error = last_error(actions.add);

    let rows = move || {
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                if editing.get() == Some(item.id) {
                    view! { <InlineEdit item=item.clone() group_id list placement/> }.into_any()
                } else {
                    let (first, last) = (index == 0, index == last);
                    view! { <ItemRow item=item.clone() first last group_id actions placement/> }
                        .into_any()
                }
            })
            .collect_view()
    };

    view! {
        <h1 class="h2">{i18n.t("wishlist.edit.title")}</h1>
        <p class="mb-1">
            {i18n.t("wishlist.edit.forGroup")} " "
            <a href=group_path(group_id)>{wishlist.group.name}</a>
        </p>
        <p class="text-muted">{i18n.t("wishlist.edit.description")}</p>
        <p><a href="/wishes/shared">{i18n.t("share.title")}</a></p>
        <p>
            <a href=wishlist_view_path(group_id, wishlist.owner.id)>
                {i18n.t("group.show.viewYourWishlist")}
            </a>
        </p>

        <div class="card mb-4">
            <div class="card-body">
                <p class="text-muted mb-3">
                    {t!(i18n, "wishlist.edit.itemCount", count = count, max = MAX_ITEMS)}
                </p>
                {full
                    .then(|| {
                        view! {
                            <div class="alert alert-warning" role="status">
                                {WishlistFieldError::TooManyItems.text(&i18n)}
                            </div>
                        }
                    })}
                <ActionForm action=actions.add>
                    <input type="hidden" name="group_id" value=group_id/>
                    <fieldset disabled=full>
                        <FormAlert error=add_error/>
                        <ItemFields prefix="add" values=FormValues::default() error=add_error title_ref=add_title description_ref=add_description link_ref=add_link/>
                        <button type="submit" class="btn btn-primary">
                            {i18n.t("wishlist.edit.addItem")}
                        </button>
                    </fieldset>
                </ActionForm>
            </div>
        </div>

        <h2 class="h4">{i18n.t("wishlist.edit.itemsHeading")}</h2>
        {if has_items {
            view! { <ul class="list-group mb-4">{rows}</ul> }.into_any()
        } else {
            view! {
                <div class="mb-4">
                    <p class="mb-1">{i18n.t("wishlist.edit.noItems")}</p>
                    <p class="text-muted">{i18n.t("wishlist.edit.addSome")}</p>
                </div>
            }
                .into_any()
        }}

        {has_items.then(|| view! { <ClearSection group_id action=actions.clear/> })}
    }
}

/// What the fields start with (as `<input>` values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FormValues {
    title: String,
    description: String,
    link: String,
}

/// Alert for an error that belongs to no field (the item limit, a list that is gone).
#[component]
fn FormAlert(error: Signal<Option<WishlistError>>) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        error
            .get()
            .and_then(|e| match &e {
                WishlistError::Validation(_) => e
                    .field_error(WishlistField::Items)
                    .map(|problem| problem.text(&i18n)),
                other => Some(other.text(&i18n)),
            })
            .map(|text| view! { <div class="alert alert-danger" role="alert">{text}</div> })
    }
}

/// The `invalid-feedback` of `field`.
#[component]
fn FieldFeedback(
    field: WishlistField,
    error: Signal<Option<WishlistError>>,
    id: String,
) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        let id = id.clone();
        error
            .get()
            .and_then(|e| e.field_error(field))
            .map(|problem| view! { <div id=id class="invalid-feedback" role="alert">{problem.text(&i18n)}</div> })
    }
}

fn control_class(
    error: Signal<Option<WishlistError>>,
    field: WishlistField,
) -> impl Fn() -> &'static str + Send + Sync + Copy + 'static {
    move || {
        if error.get().and_then(|e| e.field_error(field)).is_some() {
            "form-control is-invalid"
        } else {
            "form-control"
        }
    }
}

/// Title, description and link of an item. `prefix` keeps the element ids of the add form
/// and of an inline form apart.
#[component]
fn ItemFields(
    #[prop(into)] prefix: String,
    values: FormValues,
    error: Signal<Option<WishlistError>>,
    #[prop(optional)] title_ref: NodeRef<Input>,
    #[prop(optional)] description_ref: NodeRef<Textarea>,
    #[prop(optional)] link_ref: NodeRef<Input>,
) -> impl IntoView {
    let i18n = use_i18n();
    let title_id = format!("{prefix}-title");
    let title_error = format!("{prefix}-title-error");
    let description_id = format!("{prefix}-description");
    let description_help = format!("{prefix}-description-help");
    let description_error = format!("{prefix}-description-error");
    let link_id = format!("{prefix}-link");
    let link_help = format!("{prefix}-link-help");
    let link_error = format!("{prefix}-link-error");
    view! {
        <div class="mb-3">
            <label for=title_id.clone() class="form-label">{i18n.t("wishlist.edit.itemTitle")}</label>
            <input
                type="text"
                id=title_id
                name="title"
                class=control_class(error, WishlistField::Title)
                aria-describedby=title_error.clone()
                aria-invalid=move || if error.get().and_then(|e| e.field_error(WishlistField::Title)).is_some() { "true" } else { "false" }
                maxlength=TITLE_MAX.to_string()
                required
                placeholder=i18n.t("wishlist.edit.itemTitlePlaceholder")
                value=values.title
                node_ref=title_ref
            />
            <FieldFeedback field=WishlistField::Title error id=title_error/>
        </div>
        <div class="mb-3">
            <label for=description_id.clone() class="form-label">
                {i18n.t("wishlist.edit.itemDescription")}
            </label>
            <TextArea
                id=description_id
                name="description"
                rows="2"
                class=Signal::derive(control_class(error, WishlistField::Description))
                maxlength=DESCRIPTION_MAX.to_string()
                placeholder=i18n.t("wishlist.edit.descriptionPlaceholder")
                aria_describedby=format!("{description_help} {description_error}")
                aria_invalid=Signal::derive(move || error.get().and_then(|e| e.field_error(WishlistField::Description)).is_some())
                value=values.description
                node_ref=description_ref
            />
            <FieldFeedback field=WishlistField::Description error id=description_error/>
            <div id=description_help class="form-text">{i18n.t("wishlist.edit.descriptionHelp")}</div>
        </div>
        <div class="mb-3">
            <label for=link_id.clone() class="form-label">{i18n.t("wishlist.edit.link")}</label>
            <input
                type="text"
                inputmode="url"
                id=link_id
                name="link"
                class=control_class(error, WishlistField::Link)
                maxlength=LINK_MAX.to_string()
                placeholder=i18n.t("wishlist.edit.linkPlaceholder")
                aria-describedby=format!("{link_help} {link_error}")
                aria-invalid=move || if error.get().and_then(|e| e.field_error(WishlistField::Link)).is_some() { "true" } else { "false" }
                value=values.link
                node_ref=link_ref
            />
            <FieldFeedback field=WishlistField::Link error id=link_error/>
            <div id=link_help class="form-text">{i18n.t("wishlist.edit.linkHelp")}</div>
        </div>
    }
}

/// One item with its buttons. Move up is missing on the first row, Move down on the last.
#[component]
fn ItemRow(
    item: ItemView,
    first: bool,
    last: bool,
    group_id: i32,
    actions: Actions,
    placement: Placement,
) -> impl IntoView {
    let i18n = use_i18n();
    let id = item.id;
    let title = item.title.clone();
    let host = item.link_host.clone();
    let row = NodeRef::<Li>::new();
    Effect::new(move |_| {
        if let Some(row) = row.get()
            && placement.scroll_to.get_value() == Some(id)
        {
            row.scroll_into_view();
        }
    });
    view! {
        <li class="list-group-item" id=item_anchor(id) node_ref=row>
            <div class="d-flex flex-wrap justify-content-between align-items-start gap-2">
                <div class="flex-grow-1 text-break">
                    <div class="fw-semibold">{item.title}</div>
                    {item
                        .description
                        .map(|text| {
                            view! { <div class="small text-muted" style="white-space: pre-line">{text}</div> }
                        })}
                    {item.link_target.map(|link| view! { <div class="mt-1"><ItemLink link host/></div> })}
                </div>
                <div class="d-flex flex-wrap gap-1">
                    {(!first)
                        .then(|| {
                            view! {
                                <MoveButton item_id=id direction=Direction::Up title=title.clone() action=actions.move_item/>
                            }
                        })}
                    {(!last)
                        .then(|| {
                            view! {
                                <MoveButton item_id=id direction=Direction::Down title=title.clone() action=actions.move_item/>
                            }
                        })}
                    <a
                        href=format!("{}?edit={id}#{}", wishlist_edit_path(group_id), item_anchor(id))
                        class="btn btn-sm btn-outline-secondary"
                        aria-label=row_label(&i18n, "wishlist.edit.edit", &title)
                    >
                        {i18n.t("wishlist.edit.edit")}
                    </a>
                    <ActionForm action=actions.delete>
                        <input type="hidden" name="item_id" value=id/>
                        <ConfirmSubmit
                            label=i18n.t("wishlist.edit.removeItem")
                            confirm=i18n.t("wishlist.edit.removeItemConfirm")
                            class="btn btn-sm btn-outline-danger"
                            aria_label=row_label(&i18n, "wishlist.edit.removeItem", &title)
                        />
                    </ActionForm>
                </div>
            </div>
        </li>
    }
}

/// A Move up / Move down button: a form of its own, so it works without JavaScript.
#[component]
fn MoveButton(
    item_id: i32,
    direction: Direction,
    title: String,
    action: ServerAction<MoveItem>,
) -> impl IntoView {
    let i18n = use_i18n();
    let (key, value) = match direction {
        Direction::Up => ("wishlist.edit.moveUp", "Up"),
        Direction::Down => ("wishlist.edit.moveDown", "Down"),
    };
    view! {
        <ActionForm action>
            <input type="hidden" name="item_id" value=item_id/>
            <input type="hidden" name="direction" value=value/>
            <button
                type="submit"
                class="btn btn-sm btn-outline-secondary"
                aria-label=row_label(&i18n, key, &title)
            >
                {i18n.t(key)}
            </button>
        </ActionForm>
    }
}

/// The inline form of `?edit={id}`. It has its own action, so its errors show under its own
/// fields (never in the add form). Saved: back to the page without `?edit`.
#[component]
fn InlineEdit(
    item: ItemView,
    group_id: i32,
    list: ListResource,
    placement: Placement,
) -> impl IntoView {
    let i18n = use_i18n();
    let action = ServerAction::<UpdateItem>::new();
    let error = last_error(action);
    let navigate = use_navigate();
    let id = item.id;
    let anchor = item_anchor(id);
    let title_ref = NodeRef::<Input>::new();
    Effect::new(move |_| {
        let Some(title) = title_ref.get() else {
            return;
        };
        let idle = document()
            .active_element()
            .is_none_or(|el| el.tag_name().eq_ignore_ascii_case("body"));
        if placement.focused.get_value() != Some(id) || idle {
            placement.focused.set_value(Some(id));
            let _ = title.focus();
        }
    });
    let cancel_href = format!("{}#{anchor}", wishlist_edit_path(group_id));
    Effect::new(move |_| {
        if let Some(Ok(())) = action.value().get() {
            placement.scroll_to.set_value(Some(id));
            list.refetch();
            // A new search string: the layout re-reads the session and shows the flash.
            navigate(
                &wishlist_edit_path(group_id),
                NavigateOptions {
                    scroll: false,
                    ..Default::default()
                },
            );
        }
    });
    let values = FormValues {
        title: item.title,
        description: item.description.unwrap_or_default(),
        link: item.link.unwrap_or_default(),
    };
    view! {
        <li class="list-group-item" id=anchor>
            <ActionForm action>
                <input type="hidden" name="item_id" value=item.id/>
                <FormAlert error/>
                <ItemFields prefix=format!("edit-{id}") values error title_ref/>
                <div class="d-flex gap-2">
                    <button type="submit" class="btn btn-sm btn-primary">
                        {i18n.t("wishlist.edit.save")}
                    </button>
                    <a href=cancel_href class="btn btn-sm btn-outline-secondary"
                       on:click=move |_| placement.scroll_to.set_value(Some(id))>
                        {i18n.t("wishlist.edit.cancel")}
                    </a>
                </div>
            </ActionForm>
        </li>
    }
}

/// Remove all items: the required checkbox is the confirmation (works without JavaScript).
#[component]
fn ClearSection(group_id: i32, action: ServerAction<ClearWishlist>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="card border-danger">
            <div class="card-body">
                <ActionForm action>
                    <input type="hidden" name="group_id" value=group_id/>
                    <div class="form-check mb-3">
                        <input
                            class="form-check-input"
                            type="checkbox"
                            id="clear-confirm"
                            name="confirm"
                            required
                        />
                        <label class="form-check-label" for="clear-confirm">
                            {i18n.t("wishlist.edit.deleteWishlistConfirm")}
                        </label>
                    </div>
                    <button type="submit" class="btn btn-outline-danger">
                        {i18n.t("wishlist.edit.deleteWishlist")}
                    </button>
                </ActionForm>
            </div>
        </div>
    }
}

#[component]
fn CopySection(
    group: Memo<Option<i32>>,
    source_group: Memo<Option<i32>>,
    sources: Resource<Result<Vec<NamedRef>, WishlistError>>,
    source_list: Resource<Result<Option<WishlistView>, WishlistError>>,
    action: ServerAction<CopyWishlistItems>,
) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                let Ok(sources) = sources.await else { return ().into_any(); };
                if sources.is_empty() { return ().into_any(); }
                let group_id = group.get_untracked().unwrap_or_default();
                view! {
                    <section class="card my-4">
                        <div class="card-body">
                            <h2 class="h4">{i18n.t("wishlist.copy.title")}</h2>
                            // A normal GET still works without JS; after hydration only
                            // the query changes, keeping unsaved editor values and scroll.
                            <Form method="get" action=wishlist_edit_path(group_id) noscroll=true>
                                <label for="copy-source" class="form-label">{i18n.t("wishlist.copy.source")}</label>
                                <select id="copy-source" name="source" class="form-select mb-2" required>
                                    <option value="">{i18n.t("wishlist.copy.choose")}</option>
                                    {sources.into_iter().map(|source| view! {
                                        <option value=source.id selected=move || source_group.get() == Some(source.id)>{source.name}</option>
                                    }).collect_view()}
                                </select>
                                <button type="submit" class="btn btn-outline-secondary">{i18n.t("wishlist.copy.load")}</button>
                            </Form>
                            <Suspense fallback=|| ()>
                                {move || Suspend::new(async move {
                                    match source_list.await {
                                        Ok(Some(source)) if source.group.id != group_id => view! {
                                            <ActionForm action>
                                                <input type="hidden" name="group_id" value=group_id/>
                                                <input type="hidden" name="source_group_id" value=source.group.id/>
                                                <p class="mt-3">{i18n.t("wishlist.copy.instructions")}</p>
                                                {source.items.into_iter().enumerate().map(|(index,item)| view! {
                                                    <div class="form-check">
                                                        <input type="checkbox" class="form-check-input" id=format!("copy-item-{}",item.id) name=format!("item_ids[{index}]") value=item.id/>
                                                        <label class="form-check-label" for=format!("copy-item-{}",item.id)>{item.title}</label>
                                                    </div>
                                                }).collect_view()}
                                                <button type="submit" class="btn btn-primary mt-3" disabled=move || action.pending().get()>{i18n.t("wishlist.copy.submit")}</button>
                                            </ActionForm>
                                        }.into_any(),
                                        Ok(_) => ().into_any(),
                                        Err(error) => view! { <WishlistErrorView error/> }.into_any(),
                                    }
                                })}
                            </Suspense>
                        </div>
                    </section>
                }.into_any()
            })}
        </Suspense>
    }
}
