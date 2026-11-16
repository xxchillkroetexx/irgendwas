//! `/groups/:id/wishlist`: the caller's own list in a group (spec: wishlists, UI notes
//! "edit"). Every change is an `ActionForm` (a plain form post without JavaScript, which the
//! server answers with `302` back here and a flash). With JavaScript the page stays, reads
//! the flash and re-reads the list. `?edit={item_id}` renders that item's row as an inline
//! form; an id that is not one of my items in this group is ignored.
//!
//! Every row (plain or inline form) is `<li id="item-{id}">`; Edit links to
//! `?edit={id}#item-{id}` and Cancel to `#item-{id}`, so a page load (no JavaScript, a new
//! tab) lands on the row instead of the top of the page (#69). With JavaScript a plain click
//! opens and closes the form in place and keeps the scroll position (#81).
//!
//! Between the add form and the items sits the copy section (spec: copy-wishes, #76): my
//! other non-empty lists, each with its own copy form. It renders from its own resource,
//! outside the list's `Transition`s, so adding, editing or moving items never re-renders it
//! (its open state and ticks stay). `?copy` (any value) renders it open.

use std::time::Duration;

use leptos::ev::MouseEvent;
use leptos::html::{Details, Div, Form, Input};
use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::hooks::{use_navigate, use_query_map};

use super::{ItemLink, WishlistErrorView, route_id, row_label, when_logged_in};
use crate::auth::use_session_resource;
use crate::components::{RequireUser, TextArea};
use crate::flash::use_toasts;
use crate::format::LocalDate;
use crate::i18n::{I18n, use_i18n};
use crate::pages::groups::{ConfirmSubmit, parse_group_id};
use crate::paths::{
    group_path, login_url, wishlist_copy_open_path, wishlist_edit_path, wishlist_item_anchor,
    wishlist_item_edit_path, wishlist_item_path, wishlist_view_path,
};
use crate::wishlists_api::{
    AddItem, ClearWishlist, CopyItem, CopySource, CopySources, CopyWishlistItems, DESCRIPTION_MAX,
    DeleteItem, Direction, ItemView, LINK_MAX, MAX_ITEMS, MoveItem, TITLE_MAX, UpdateItem,
    WishlistError, WishlistField, WishlistFieldError, WishlistView, get_copy_sources,
    get_my_wishlist,
};
use crate::{t, t_plural};

type ListResource = Resource<Result<WishlistView, WishlistError>>;
type SourcesResource = Resource<Result<CopySources, WishlistError>>;

/// The page's server actions. Created at page level, so a re-read of the list (which
/// re-renders it) keeps them and their results.
#[derive(Clone, Copy)]
struct Actions {
    add: ServerAction<AddItem>,
    delete: ServerAction<DeleteItem>,
    move_item: ServerAction<MoveItem>,
    clear: ServerAction<ClearWishlist>,
}

/// The add form and its title input, for emptying it after a successful add (JS path).
#[derive(Clone, Copy)]
struct AddForm {
    form: NodeRef<Form>,
    title: NodeRef<Input>,
}

/// The open inline form whose title no longer takes the focus when it mounts. A row mounts
/// more often than the user opens a form: every re-read of the list re-runs the rows. Once
/// something was submitted on the page since the form opened (add, move, remove, copy), a
/// mount is such a re-read and the user is busy elsewhere (#69). Cleared whenever `?edit=`
/// changes.
type Settled = StoredValue<Option<i32>>;

/// With JavaScript, a plain click on an Edit or Cancel link goes to `to` (its href without
/// the `#item-{id}` fragment) and keeps the scroll position: the router scrolls a fragment's
/// element to the top even with `noscroll`, and the row is already in view (#81). A click
/// that opens a new tab or window keeps the fragment.
fn in_place(to: String) -> impl Fn(MouseEvent) + 'static {
    let navigate = use_navigate();
    move |ev: MouseEvent| {
        let modified = ev.ctrl_key() || ev.meta_key() || ev.shift_key() || ev.alt_key();
        if ev.button() == 0 && !modified {
            ev.prevent_default();
            navigate(
                &to,
                NavigateOptions {
                    scroll: false,
                    ..Default::default()
                },
            );
        }
    }
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
    let sources: SourcesResource = Resource::new(
        move || id.get(),
        |group| async move {
            match group {
                Some(group) => when_logged_in(get_copy_sources(group)).await,
                None => Err(WishlistError::NotFound),
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
    let add_form = AddForm {
        form: NodeRef::new(),
        title: NodeRef::new(),
    };
    let settled: Settled = StoredValue::new(None);
    Effect::new(move |_| {
        editing.track();
        settled.set_value(None);
    });
    let busy = [
        actions.add.pending(),
        actions.delete.pending(),
        actions.move_item.pending(),
        actions.clear.pending(),
        copy.pending(),
    ];
    Effect::new(move |_| {
        // Read every flag (no short-circuit), so the effect tracks all of them.
        if busy.map(|pending| pending.get()).contains(&true) {
            settled.set_value(editing.get_untracked());
        }
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
        // An earlier add error no longer belongs to anything on the page.
        actions.add.value().set(None);
        list.refetch();
    };
    // After a successful add, empty the form for the next wish. The re-read list diffs the
    // form in place (its `value` attributes stay ""), which would keep the browser's live
    // values. A failed add keeps them for correction. The add form is disabled while the add
    // runs, so the reset never takes text typed for the next wish meanwhile; disabling it
    // also took the focus, which goes back to the title once the form is enabled again.
    Effect::new(move |_| {
        let Some(result) = actions.add.value().get() else {
            return;
        };
        if result.is_ok() {
            if let Some(form) = add_form.form.get_untracked() {
                form.reset();
            }
            session.refetch();
            list.refetch();
        }
        set_timeout(
            move || {
                if let Some(title) = add_form.title.get_untracked() {
                    let _ = title.focus();
                }
            },
            Duration::ZERO,
        );
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

    view! {
        <RequireUser>
            <Transition fallback=|| ()>
                {move || Suspend::new(async move {
                    match list.await {
                        Ok(wishlist) => view! { <EditHead wishlist actions add_form/> }.into_any(),
                        Err(error) => view! { <WishlistErrorView error/> }.into_any(),
                    }
                })}
            </Transition>
            <CopySection group=id sources list action=copy/>
            <Transition fallback=|| ()>
                {move || Suspend::new(async move {
                    match list.await {
                        Ok(wishlist) => {
                            view! { <ItemsView wishlist editing list actions settled/> }.into_any()
                        }
                        // The first Transition shows the error.
                        Err(_) => ().into_any(),
                    }
                })}
            </Transition>
        </RequireUser>
    }
}

/// Everything above the copy section: title, links and the add form.
#[component]
fn EditHead(wishlist: WishlistView, actions: Actions, add_form: AddForm) -> impl IntoView {
    let i18n = use_i18n();
    let group_id = wishlist.group.id;
    let count = wishlist.items.len();
    let full = count >= MAX_ITEMS;
    let add_error = last_error(actions.add);
    let adding = actions.add.pending();

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
                <ActionForm action=actions.add node_ref=add_form.form>
                    <input type="hidden" name="group_id" value=group_id/>
                    <fieldset disabled=move || full || adding.get()>
                        <FormAlert error=add_error/>
                        <ItemFields
                            prefix="add"
                            values=FormValues::default()
                            error=add_error
                            title_ref=add_form.title
                        />
                        <button type="submit" class="btn btn-primary">
                            {i18n.t("wishlist.edit.addItem")}
                        </button>
                    </fieldset>
                </ActionForm>
            </div>
        </div>
    }
}

/// Everything below the copy section: the items and "Remove all items".
#[component]
fn ItemsView(
    wishlist: WishlistView,
    editing: Memo<Option<i32>>,
    list: ListResource,
    actions: Actions,
    settled: Settled,
) -> impl IntoView {
    let i18n = use_i18n();
    let group_id = wishlist.group.id;
    let items = wishlist.items;
    let has_items = !items.is_empty();
    let last = items.len().saturating_sub(1);

    let rows = move || {
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                if editing.get() == Some(item.id) {
                    view! { <InlineEdit item=item.clone() group_id list settled/> }.into_any()
                } else {
                    let (first, last) = (index == 0, index == last);
                    view! { <ItemRow item=item.clone() first last group_id actions/> }.into_any()
                }
            })
            .collect_view()
    };

    view! {
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
fn FieldFeedback(field: WishlistField, error: Signal<Option<WishlistError>>) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        error
            .get()
            .and_then(|e| e.field_error(field))
            .map(|problem| view! { <div class="invalid-feedback">{problem.text(&i18n)}</div> })
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
/// and of an inline form apart. `title_ref` gives the caller the title input.
#[component]
fn ItemFields(
    #[prop(into)] prefix: String,
    values: FormValues,
    error: Signal<Option<WishlistError>>,
    title_ref: NodeRef<Input>,
) -> impl IntoView {
    let i18n = use_i18n();
    let title_id = format!("{prefix}-title");
    let description_id = format!("{prefix}-description");
    let description_help = format!("{prefix}-description-help");
    let link_id = format!("{prefix}-link");
    let link_help = format!("{prefix}-link-help");
    view! {
        <div class="mb-3">
            <label for=title_id.clone() class="form-label">{i18n.t("wishlist.edit.itemTitle")}</label>
            <input
                type="text"
                id=title_id
                name="title"
                node_ref=title_ref
                class=control_class(error, WishlistField::Title)
                maxlength=TITLE_MAX.to_string()
                required
                placeholder=i18n.t("wishlist.edit.itemTitlePlaceholder")
                value=values.title
            />
            <FieldFeedback field=WishlistField::Title error/>
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
                aria_describedby=description_help.clone()
                value=values.description
            />
            <FieldFeedback field=WishlistField::Description error/>
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
                aria-describedby=link_help.clone()
                value=values.link
            />
            <FieldFeedback field=WishlistField::Link error/>
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
) -> impl IntoView {
    let i18n = use_i18n();
    let id = item.id;
    let title = item.title.clone();
    let host = item.link_host.clone();
    let on_edit = in_place(format!("{}?edit={id}", wishlist_edit_path(group_id)));
    view! {
        <li class="list-group-item" id=wishlist_item_anchor(id)>
            <div class="d-flex flex-wrap justify-content-between align-items-start gap-2">
                <div class="flex-grow-1 text-break">
                    <div class="fw-semibold">{item.title}</div>
                    {item
                        .description
                        .map(|text| {
                            view! { <div class="small text-muted" style="white-space: pre-line">{text}</div> }
                        })}
                    {item.link.map(|link| view! { <div class="mt-1"><ItemLink link host/></div> })}
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
                        href=wishlist_item_edit_path(group_id, id)
                        on:click=on_edit
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
/// fields (never in the add form). Its title gets the focus when the form opens. Saved: back
/// to the page without `?edit`, at the same scroll position.
#[component]
fn InlineEdit(
    item: ItemView,
    group_id: i32,
    list: ListResource,
    settled: Settled,
) -> impl IntoView {
    let i18n = use_i18n();
    let action = ServerAction::<UpdateItem>::new();
    let error = last_error(action);
    let navigate = use_navigate();
    let id = item.id;
    let title_ref = NodeRef::<Input>::new();
    // Focus the title when this item's form opens, but not when a re-read of the list
    // remounts it after something else was submitted on the page.
    Effect::new(move |_| {
        if let Some(title) = title_ref.get()
            && settled.get_value() != Some(id)
        {
            let _ = title.focus();
        }
    });
    let on_cancel = in_place(wishlist_edit_path(group_id));
    Effect::new(move |_| {
        if let Some(Ok(())) = action.value().get() {
            list.refetch();
            // A new search string: the layout re-reads the session and shows the flash. The
            // URL matches the no-JS redirect (no fragment); the row stays where it is.
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
        <li class="list-group-item" id=wishlist_item_anchor(id)>
            <ActionForm action>
                <input type="hidden" name="item_id" value=item.id/>
                <FormAlert error/>
                <ItemFields prefix=format!("edit-{id}") values error title_ref/>
                <div class="d-flex gap-2">
                    <button type="submit" class="btn btn-sm btn-primary">
                        {i18n.t("wishlist.edit.save")}
                    </button>
                    // The fragment brings a page load (no JS, a new tab) to the row.
                    <a
                        href=wishlist_item_path(group_id, id)
                        class="btn btn-sm btn-outline-secondary"
                        on:click=on_cancel
                    >
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

// ------------------------------------------------------------ copy section (#76)

/// Which source form sent the last copy and why it failed (JavaScript path). Lives at
/// section level, so a re-read of the section data keeps the alert.
#[derive(Clone, Copy)]
struct CopyState {
    action: ServerAction<CopyWishlistItems>,
    /// The source group of the copy in flight or last sent.
    submitted: StoredValue<Option<i32>>,
    /// The failed source and its error; cleared by the next submission or a success.
    failed: RwSignal<Option<(i32, WishlistError)>>,
}

/// The text of a failed copy: a vanished wish or membership (`NotFound`) means the shown
/// data is stale.
fn copy_error_text(error: &WishlistError, i18n: &I18n) -> String {
    match error {
        WishlistError::NotFound => i18n.t("wishlist.copy.changed"),
        other => other.text(i18n),
    }
}

/// The failures that mean the section showed stale data: its data is read again.
fn is_stale(error: &WishlistError) -> bool {
    matches!(error, WishlistError::NotFound)
        || matches!(
            error.field_error(WishlistField::Items),
            Some(WishlistFieldError::AlreadyOnList | WishlistFieldError::ItemNotCopyable)
        )
}

/// Unticks every box of the copy section (client only). After a re-read the rows are
/// diffed by position, so a tick could otherwise land on another wish.
fn reset_copy_forms(section: NodeRef<Details>) {
    #[cfg(feature = "hydrate")]
    if let Some(details) = section.get_untracked() {
        use leptos::wasm_bindgen::JsCast;
        if let Ok(forms) = details.query_selector_all("form") {
            for index in 0..forms.length() {
                if let Some(form) = forms
                    .item(index)
                    .and_then(|node| node.dyn_into::<web_sys::HtmlFormElement>().ok())
                {
                    form.reset();
                }
            }
        }
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = section;
}

/// "Select all": ticks (or unticks) every enabled wish checkbox of its own form; disabled
/// boxes and other sources stay as they are.
fn select_all(ev: leptos::ev::Event) {
    #[cfg(feature = "hydrate")]
    {
        use leptos::wasm_bindgen::JsCast;
        let Some(all) = ev
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(form) = all.form() else {
            return;
        };
        let Ok(boxes) = form.query_selector_all("input[name^=\"item_ids\"]:not(:disabled)") else {
            return;
        };
        for index in 0..boxes.length() {
            if let Some(item) = boxes
                .item(index)
                .and_then(|node| node.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                item.set_checked(all.checked());
            }
        }
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = ev;
}

/// The copy section: a native `<details id="copy">` with one nested `<details>` per other
/// list of mine (newest first, the first one open) and its own copy form. Shown only when
/// there is something to copy from. Open when the URL has `copy` or my list here was empty
/// when the page rendered; decided once, so later query changes and re-reads do not open
/// or close it (a re-read keeps the `open` attribute's value, so the DOM keeps the user's
/// toggles).
///
/// With JavaScript, a copy re-reads the list and the section data; a success closes the
/// section, shows the flash and drops `?copy`; a failure shows an alert in the failing
/// source's form and, when the data was stale, re-reads it.
#[component]
fn CopySection(
    group: Memo<Option<i32>>,
    sources: SourcesResource,
    list: ListResource,
    action: ServerAction<CopyWishlistItems>,
) -> impl IntoView {
    let query = use_query_map();
    let copy_param = query.with_untracked(|q| q.get("copy").is_some());
    let initially_open = StoredValue::new(None::<bool>);
    let section = NodeRef::<Details>::new();
    let state = CopyState {
        action,
        submitted: StoredValue::new(None),
        failed: RwSignal::new(None),
    };
    let session = use_session_resource().0;
    let navigate = use_navigate();

    Effect::new(move |_| {
        if let Some(input) = action.input().get() {
            state.submitted.set_value(Some(input.source_group_id));
            state.failed.set(None);
        }
    });
    Effect::new(move |_| {
        let Some(result) = action.value().get() else {
            return;
        };
        let Some(group_id) = group.get_untracked() else {
            return;
        };
        match result {
            Ok(_) => {
                state.failed.set(None);
                if let Some(details) = section.get_untracked() {
                    let _ = details.remove_attribute("open");
                }
                reset_copy_forms(section);
                list.refetch();
                sources.refetch();
                // The flash: a new search string makes the layout read the session (one
                // read; a second one would come back without the flash). That URL is the
                // end state of the no-JS redirect, kept at the same scroll position.
                if !query.with_untracked(|q| q.get("copy").is_some()) {
                    session.refetch();
                } else {
                    navigate(
                        &wishlist_edit_path(group_id),
                        NavigateOptions {
                            replace: true,
                            scroll: false,
                            ..Default::default()
                        },
                    );
                }
            }
            Err(WishlistError::Unauthorized) => {
                navigate(
                    &login_url(&wishlist_copy_open_path(group_id)),
                    NavigateOptions::default(),
                );
            }
            Err(error) => {
                // Set before the re-read: if the failing source (or every source) is gone
                // afterwards, the section still shows the message.
                let stale = is_stale(&error);
                if let Some(source) = state.submitted.get_value() {
                    state.failed.set(Some((source, error)));
                }
                if stale {
                    reset_copy_forms(section);
                    sources.refetch();
                }
            }
        }
    });

    view! {
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                let Ok(data) = sources.await else {
                    return ().into_any();
                };
                // No sources left: nothing to show, unless a copy failed because its source
                // vanished (then the section stays for its message).
                if data.sources.is_empty() && state.failed.with_untracked(Option::is_none) {
                    return ().into_any();
                }
                let Some(group_id) = group.get_untracked() else {
                    return ().into_any();
                };
                let open = initially_open
                    .get_value()
                    .unwrap_or_else(|| {
                        let open = copy_param || data.free as usize >= MAX_ITEMS;
                        initially_open.set_value(Some(open));
                        open
                    });
                view! { <CopyDetails data group_id open section state/> }.into_any()
            })}
        </Transition>
    }
}

#[component]
fn CopyDetails(
    data: CopySources,
    group_id: i32,
    open: bool,
    section: NodeRef<Details>,
    state: CopyState,
) -> impl IntoView {
    let i18n = use_i18n();
    // A failed copy whose source is no longer listed (emptied or left meanwhile): its
    // message shows above the lists, as the form that held it is gone.
    let listed: Vec<i32> = data.sources.iter().map(|source| source.group.id).collect();
    let failed = state.failed;
    let orphaned = move || {
        failed
            .get()
            .filter(|(source, _)| !listed.contains(source))
            .map(|(_, error)| {
                view! {
                    <div class="alert alert-danger" role="alert">
                        {copy_error_text(&error, &i18n)}
                    </div>
                }
            })
    };
    let body = if data.sources.is_empty() {
        ().into_any()
    } else if data.free == 0 {
        view! { <p class="mb-0">{WishlistFieldError::TooManyItems.text(&i18n)}</p> }.into_any()
    } else {
        let sources = data
            .sources
            .into_iter()
            .enumerate()
            .map(|(index, source)| {
                view! { <CopySourceList source group_id open=index == 0 state/> }
            })
            .collect_view();
        view! {
            <p class="text-muted">{i18n.t("wishlist.copy.instructions")}</p>
            {sources}
        }
        .into_any()
    };
    view! {
        <details id="copy" class="border rounded bg-white mb-4" open=open node_ref=section>
            <summary class="h5 mb-0 p-3">{i18n.t("wishlist.copy.title")}</summary>
            <div class="px-3 pb-3">{orphaned}{body}</div>
        </details>
    }
}

/// One source list: its summary (name, item count, last change) and its copy form.
#[component]
fn CopySourceList(
    source: CopySource,
    group_id: i32,
    open: bool,
    state: CopyState,
) -> impl IntoView {
    let i18n = use_i18n();
    let source_id = source.group.id;
    let name = source.group.name;
    let n = i64::try_from(source.items.len()).unwrap_or(i64::MAX);
    let count = t_plural!(i18n, "wishlist.panel.itemCount", n, count = n);
    let updated = source
        .updated_at
        .map(|at| t!(i18n, "wishlist.view.lastUpdated", date = LocalDate(at)));
    let selectable = source
        .items
        .iter()
        .any(|item| !item.on_target && item.copyable);
    let rows = source
        .items
        .into_iter()
        .enumerate()
        .map(|(index, item)| view! { <CopyRow item index source_id/> })
        .collect_view();
    let failed = state.failed;
    let alert = move || {
        failed
            .get()
            .filter(|(source, _)| *source == source_id)
            .map(|(_, error)| {
                view! {
                    <div class="alert alert-danger mt-2 mb-0" role="alert">
                        {copy_error_text(&error, &i18n)}
                    </div>
                }
            })
    };
    let pending = state.action.pending();
    let controls = selectable.then(|| {
        let label = row_label(&i18n, "wishlist.copy.submit", &name);
        view! {
            <div class="d-grid d-sm-block mt-2">
                <button
                    type="submit"
                    class="btn btn-primary"
                    aria-label=label
                    disabled=move || pending.get()
                >
                    {i18n.t("wishlist.copy.submit")}
                </button>
            </div>
        }
    });
    view! {
        <details class="border rounded mb-2" open=open>
            <summary class="px-3 py-2">
                <span class="fw-semibold text-break">{name}</span>
                <span class="small text-muted ms-2">{count}</span>
                {updated.map(|text| view! { <span class="small text-muted ms-2">{text}</span> })}
            </summary>
            <div class="px-3 pb-3">
                <ActionForm action=state.action>
                    <input type="hidden" name="group_id" value=group_id/>
                    <input type="hidden" name="source_group_id" value=source_id/>
                    {selectable.then(|| view! { <SelectAll source_id/> })}
                    {rows}
                    {alert}
                    {controls}
                </ActionForm>
            </div>
        </details>
    }
}

/// The "Select all" checkbox of a source. It has no `name` (never submitted) and is
/// rendered `hidden`; a script reveals it, so a visitor without JavaScript never sees a
/// control that does nothing.
#[component]
fn SelectAll(source_id: i32) -> impl IntoView {
    let i18n = use_i18n();
    let id = format!("copy-all-{source_id}");
    let row = NodeRef::<Div>::new();
    let input = NodeRef::<Input>::new();
    Effect::new(move |_| {
        if let Some(row) = row.get() {
            let _ = row.remove_attribute("hidden");
        }
        if let Some(input) = input.get() {
            let _ = input.remove_attribute("hidden");
        }
    });
    view! {
        <div class="form-check mb-2" hidden node_ref=row>
            <input
                type="checkbox"
                class="form-check-input"
                id=id.clone()
                hidden
                node_ref=input
                on:change=select_all
            />
            <label class="form-check-label" for=id>{i18n.t("wishlist.copy.selectAll")}</label>
        </div>
    }
}

/// One wish of a source: a checkbox labelled with its title, then its description (two
/// lines at most) and its link host as text (a tap must not leave the page). A wish already
/// on my list, or one that fails today's rules, is greyed out and cannot be ticked; the
/// latter links to its row in its own list.
#[component]
fn CopyRow(item: CopyItem, index: usize, source_id: i32) -> impl IntoView {
    let i18n = use_i18n();
    let id = format!("copy-{source_id}-{}", item.id);
    let greyed = item.on_target || !item.copyable;
    let class = if greyed {
        "form-check mb-2 text-muted"
    } else {
        "form-check mb-2"
    };
    let hint = if item.on_target {
        Some(view! { <span class="ms-2 small fst-italic">{i18n.t("wishlist.copy.alreadyOnList")}</span> }.into_any())
    } else if !item.copyable {
        Some(
            view! {
                <div class="small">
                    {i18n.t("wishlist.copy.notCopyable")} " "
                    <a href=wishlist_item_edit_path(source_id, item.id)>
                        {i18n.t("wishlist.copy.fixInSource")}
                    </a>
                </div>
            }
            .into_any(),
        )
    } else {
        None
    };
    let (in_label, below) = if item.on_target {
        (hint, None)
    } else {
        (None, hint)
    };
    view! {
        <div class=class>
            <input
                type="checkbox"
                class="form-check-input"
                id=id.clone()
                name=format!("item_ids[{index}]")
                value=item.id
                disabled=greyed
            />
            <label class="form-check-label d-block" for=id>
                <span class="fw-semibold text-break">{item.title}</span>
                {in_label}
            </label>
            {item
                .description
                .map(|text| view! { <div class="small text-muted text-break copy-description">{text}</div> })}
            {item.link_host.map(|host| view! { <div class="small text-muted text-break">{host}</div> })}
            {below}
        </div>
    }
}
