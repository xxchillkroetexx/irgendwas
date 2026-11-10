//! `/groups/:id/exclusions`: the caller's own exclusion rules in a group (spec: exclusions,
//! UI notes), and the group page's link to it ([`ExclusionsLink`]).
//!
//! Add and remove are `ActionForm`s (plain form posts without JavaScript, which the server
//! answers with `302` back here and a flash). With JavaScript the page stays, reads the
//! flash and re-reads its data; errors show as a toast. The page only ever shows the
//! caller's own rules and the one-bit feasibility notice.

use std::future::Future;

use leptos::prelude::*;

use crate::AppError;
use crate::auth::{get_current_user, use_session_resource};
use crate::components::RequireUser;
use crate::exclusions_api::{
    AddExclusion, ExclusionError, ExclusionsPage, FeasibilityNotice, MemberRef, RemoveExclusion,
    get_exclusions,
};
use crate::flash::use_toasts;
use crate::i18n::use_i18n;
use crate::pages::groups::{ConfirmSubmit, parse_group_id};
use crate::pages::{NotFound, ServerError};
use crate::paths::{exclusions_path, group_path};
use crate::t;

type PageResource = Resource<Result<ExclusionsPage, ExclusionError>>;

/// The page's server actions. Created at page level, so a re-read of the data (which
/// re-renders the page) keeps them.
#[derive(Clone, Copy)]
struct Actions {
    add: ServerAction<AddExclusion>,
    remove: ServerAction<RemoveExclusion>,
}

/// Runs `fetch` only for a logged-in visitor: the guard redirects everyone else, and the
/// protected fn's error status must not end up in that redirect response.
async fn when_logged_in<T>(
    fetch: impl Future<Output = Result<T, ExclusionError>>,
) -> Result<T, ExclusionError> {
    match get_current_user().await {
        Ok(Some(_)) => fetch.await,
        Ok(None) | Err(AppError::Unauthorized) => Err(ExclusionError::Unauthorized),
        Err(_) => Err(ExclusionError::Internal),
    }
}

#[component]
pub fn ExclusionsManagePage() -> impl IntoView {
    // Personal data: no cache keeps the page.
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    let params = leptos_router::hooks::use_params_map();
    let id = Memo::new(move |_| params.read().get("id").and_then(|raw| parse_group_id(&raw)));
    let data: PageResource = Resource::new(
        move || id.get(),
        |id| async move {
            match id {
                Some(id) => when_logged_in(get_exclusions(id)).await,
                None => Err(ExclusionError::NotFound),
            }
        },
    );
    let actions = Actions {
        add: ServerAction::new(),
        remove: ServerAction::new(),
    };

    // The server stored the flash of a success: read it and the new data. A failure shows
    // as a toast; the data is re-read either way (another tab may have changed it).
    let session = use_session_resource().0;
    let toasts = use_toasts();
    let after = move |result: Result<(), ExclusionError>| {
        match result {
            Ok(()) => session.refetch(),
            Err(err) => toasts.push(err.flash()),
        }
        data.refetch();
    };
    Effect::new(move |_| {
        if let Some(result) = actions.add.value().get() {
            after(result);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.remove.value().get() {
            after(result);
        }
    });

    view! {
        <RequireUser>
            <Transition fallback=|| ()>
                {move || Suspend::new(async move {
                    match data.await {
                        Ok(page) => view! { <ExclusionsView page actions/> }.into_any(),
                        Err(ExclusionError::NotFound) => view! { <NotFound/> }.into_any(),
                        Err(ExclusionError::Unauthorized) => ().into_any(),
                        Err(_) => view! { <ServerError/> }.into_any(),
                    }
                })}
            </Transition>
        </RequireUser>
    }
}

#[component]
fn ExclusionsView(page: ExclusionsPage, actions: Actions) -> impl IntoView {
    let i18n = use_i18n();
    let group_id = page.group_id;
    let notice = page
        .notice
        .clone()
        .map(|notice| view! { <NoticeBanner notice/> });
    let add_card = (!page.is_drawn
        && !matches!(&page.notice, Some(FeasibilityNotice::GroupTooLarge { .. })))
    .then(|| view! { <AddCard page=page.clone() action=actions.add/> });
    view! {
        <div class="d-flex flex-wrap justify-content-between align-items-start gap-3 mb-3">
            <div>
                <h1 class="h2">{i18n.t("exclusion.title")}</h1>
                <p class="mb-0">
                    {i18n.t("exclusion.forGroup")} " "
                    <a href=group_path(group_id)>{page.group_name.clone()}</a>
                </p>
            </div>
            <a href=group_path(group_id) class="btn btn-outline-secondary">
                {i18n.t("exclusion.backToGroup")}
            </a>
        </div>
        <p class="text-muted">{i18n.t("exclusion.privacy")}</p>
        {page
            .is_drawn
            .then(|| {
                view! { <div class="alert alert-info" role="status">{i18n.t("exclusion.locked")}</div> }
            })}
        {notice}
        <div class="row">
            <div class="col-lg-6">
                {add_card}
                <WhyCard/>
            </div>
            <div class="col-lg-6">
                <CurrentCard
                    group_id
                    rules=page.my_rules
                    editable=!page.is_drawn
                    action=actions.remove
                />
            </div>
        </div>
    }
}

/// The group cannot be drawn with the current rules. Names nobody and no rule.
#[component]
fn NoticeBanner(notice: FeasibilityNotice) -> impl IntoView {
    let i18n = use_i18n();
    match notice {
    FeasibilityNotice::GroupTooLarge { max } => view! {
        <div class="alert alert-warning" role="alert">{t!(i18n, "exclusion.warning.groupTooLarge", max = max)}</div>
    }.into_any(),
    FeasibilityNotice::DrawImpossible { no_receiver_for_me } => view! {
        <div class="alert alert-warning" role="alert">
            <p class="mb-0">{i18n.t("exclusion.warning.drawImpossible")}</p>
            {no_receiver_for_me
                .then(|| view! { <p class="mb-0 mt-2">{i18n.t("exclusion.warning.noReceiverLeft")}</p> })}
        </div>
    }.into_any(),
    }
}

#[component]
fn AddCard(page: ExclusionsPage, action: ServerAction<AddExclusion>) -> impl IntoView {
    let i18n = use_i18n();
    let count = page.my_rules.len();
    let max = page.max_rules;
    let body = if page.can_add {
        view! { <AddForm group_id=page.group_id candidates=page.candidates action/> }.into_any()
    } else if count >= usize::from(max) {
        view! { <p class="mb-0">{t!(i18n, "exclusion.addNew.limitReached", max = max)}</p> }
            .into_any()
    } else {
        view! { <p class="mb-0">{i18n.t("exclusion.addNew.noCandidates")}</p> }.into_any()
    };
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("exclusion.addNew.title")}</h2>
                <p>{i18n.t("exclusion.addNew.description")}</p>
                <p class="text-muted small">
                    {t!(i18n, "exclusion.addNew.limit", max = max, count = count)}
                </p>
                {body}
            </div>
        </div>
    }
}

/// The select and its submit button. The button is only disabled once JavaScript runs
/// (while nobody is selected or the call is pending); the server renders it enabled, so the
/// form works without JavaScript (the select is `required`).
#[component]
fn AddForm(
    group_id: i32,
    candidates: Vec<MemberRef>,
    action: ServerAction<AddExclusion>,
) -> impl IntoView {
    let i18n = use_i18n();
    let selected = RwSignal::new(String::new());
    let interactive = RwSignal::new(false);
    let select_ref = NodeRef::<leptos::html::Select>::new();
    // Effects only run in the browser, after hydration: the first render matches the server.
    // The select may already show a person (chosen before hydration, or restored by the
    // browser on reload / back), so the button starts from the element's value.
    Effect::new(move |_| {
        if let Some(select) = select_ref.get_untracked() {
            selected.set(select.value());
        }
        interactive.set(true);
    });
    let pending = action.pending();
    let disabled = move || interactive.get() && (selected.get().is_empty() || pending.get());
    let options = candidates
        .into_iter()
        .map(|member| view! { <option value=member.user_id>{member.name}</option> })
        .collect_view();
    view! {
        <ActionForm action>
            <input type="hidden" name="group_id" value=group_id/>
            <div class="mb-3">
                <label for="exclusion-target" class="form-label">
                    {i18n.t("exclusion.addNew.selectPerson")}
                </label>
                <select
                    id="exclusion-target"
                    name="excluded_user_id"
                    class="form-select"
                    required
                    node_ref=select_ref
                    on:change=move |ev| selected.set(event_target_value(&ev))
                >
                    <option value="">{i18n.t("exclusion.addNew.selectPlaceholder")}</option>
                    {options}
                </select>
            </div>
            <button type="submit" class="btn btn-primary" disabled=disabled>
                {i18n.t("exclusion.addNew.submit")}
            </button>
        </ActionForm>
    }
}

#[component]
fn WhyCard() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("exclusion.why.title")}</h2>
                <p class="mb-1">{i18n.t("exclusion.why.intro")}</p>
                <ul>
                    <li>{i18n.t("exclusion.why.reason1")}</li>
                    <li>{i18n.t("exclusion.why.reason2")}</li>
                    <li>{i18n.t("exclusion.why.reason3")}</li>
                </ul>
                <p class="small text-muted mb-0">{i18n.t("exclusion.why.note")}</p>
            </div>
        </div>
    }
}

/// My rules; with a Remove button each (behind a confirm) unless the group is drawn.
#[component]
fn CurrentCard(
    group_id: i32,
    rules: Vec<MemberRef>,
    editable: bool,
    action: ServerAction<RemoveExclusion>,
) -> impl IntoView {
    let i18n = use_i18n();
    let list = if rules.is_empty() {
        view! { <p class="mb-0">{i18n.t("exclusion.current.empty")}</p> }.into_any()
    } else {
        let rows = rules
            .into_iter()
            .map(|rule| {
                let label = t!(
                    i18n,
                    "wishlist.edit.itemAction",
                    action = i18n.t("exclusion.current.remove"),
                    title = rule.name.as_str()
                );
                let remove = editable.then(|| {
                    view! {
                        <ActionForm action>
                            <input type="hidden" name="group_id" value=group_id/>
                            <input type="hidden" name="excluded_user_id" value=rule.user_id/>
                            <ConfirmSubmit
                                label=i18n.t("exclusion.current.remove")
                                confirm=i18n.t("exclusion.current.removeConfirm")
                                class="btn btn-sm btn-outline-danger"
                                aria_label=label
                            />
                        </ActionForm>
                    }
                });
                view! {
                    <li class="list-group-item d-flex flex-wrap justify-content-between align-items-center gap-2">
                        <span class="text-break">{rule.name}</span>
                        {remove}
                    </li>
                }
            })
            .collect_view();
        view! { <ul class="list-group list-group-flush">{rows}</ul> }.into_any()
    };
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("exclusion.current.title")}</h2>
                {list}
            </div>
        </div>
    }
}

/// The group page's hook (spec: "Pages / routes"): a link to the caller's exclusions while
/// the group is not drawn, nothing after the draw.
#[component]
pub fn ExclusionsLink(group_id: i32, is_drawn: bool) -> impl IntoView {
    let i18n = use_i18n();
    (!is_drawn).then(|| {
        view! {
            <p class="mb-4">
                <a href=exclusions_path(group_id) class="btn btn-outline-primary">
                    {i18n.t("group.show.manageExclusions")}
                </a>
            </p>
        }
    })
}
