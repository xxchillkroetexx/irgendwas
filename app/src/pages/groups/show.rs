//! `/groups/:id`: one group as the calling member sees it (spec: groups, UI notes "group
//! page"). Every mutation is an `ActionForm` (a plain form post without JavaScript) behind
//! a confirm dialog. Delete and leave navigate away (server redirect); regenerate, remove
//! and transfer stay and re-read the group and the flash.

use leptos::prelude::*;

use super::{GroupErrorView, SafeConfirmSubmit, route_group_id, when_logged_in};
use crate::auth::use_session_resource;
use crate::components::{PageLoading, RequireUser};
use crate::flash::{Toasts, use_toasts};
use crate::groups_api::{
    DeleteGroup, GroupDetail, GroupError, GroupOp, GroupStatus, Invitation, LeaveGroup, MemberView,
    RegenerateInvitation, RemoveMember, TransferAdmin, get_group,
};
use crate::i18n::use_i18n;
use crate::pages::draw::{DrawActions, DrawPanel, DrawPanelData, DrawResource, draw_resource};
use crate::pages::exclusions::ExclusionsLink;
use crate::pages::wishlists::{PanelData, PanelResource, WishlistPanel, panel_resource};
use crate::t;

type GroupResource = Resource<Result<GroupDetail, GroupError>>;

#[component]
pub fn GroupPage() -> impl IntoView {
    // Member emails and the invitation code: no cache keeps the page.
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    let id = route_group_id();
    let group: GroupResource = Resource::new(
        move || id.get(),
        |id| async move {
            match id {
                Some(id) => when_logged_in(get_group(id)).await,
                None => Err(GroupError::NotFound),
            }
        },
    );
    // The wishlists slice's card (its data is page-level like the group's).
    let panel = panel_resource(id);
    // The draw slice's panel: the caller's own assignment and, for the admin, the status.
    let draw = draw_resource(id, group);
    view! {
        <RequireUser>
            <GroupShow group panel draw/>
        </RequireUser>
    }
}

/// The page's server actions. Created outside the group `Transition`, so a refetch (which
/// re-renders the whole group) keeps them.
#[derive(Clone, Copy)]
struct Actions {
    regenerate: ServerAction<RegenerateInvitation>,
    delete: ServerAction<DeleteGroup>,
    leave: ServerAction<LeaveGroup>,
    remove: ServerAction<RemoveMember>,
    transfer: ServerAction<TransferAdmin>,
    draw: DrawActions,
}

#[component]
fn GroupShow(group: GroupResource, panel: PanelResource, draw: DrawResource) -> impl IntoView {
    let session = use_session_resource().0;
    let toasts = use_toasts();
    let actions = Actions {
        regenerate: ServerAction::new(),
        delete: ServerAction::new(),
        leave: ServerAction::new(),
        remove: ServerAction::new(),
        transfer: ServerAction::new(),
        draw: DrawActions::new(),
    };

    // Staying actions: the server stored the flash; read it and the new state.
    let after_stay = move |result: Result<(), GroupError>, op: GroupOp| {
        if let Err(err) = result {
            toasts.push(err.flash(op));
        } else {
            session.refetch();
        }
        group.refetch();
        panel.refetch();
        draw.refetch();
    };
    // The draw actions stay too; an error can carry two messages (DrawImpossible).
    let after_draw = move |result: Result<(), crate::draw_api::DrawApiError>| {
        match result {
            Ok(()) => session.refetch(),
            Err(err) => err.flashes().into_iter().for_each(|f| toasts.push(f)),
        }
        group.refetch();
        panel.refetch();
        draw.refetch();
    };
    Effect::new(move |_| {
        if let Some(result) = actions.draw.perform.value().get() {
            after_draw(result);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.draw.reset.value().get() {
            after_draw(result);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.draw.resend.value().get() {
            after_draw(result.map(|_| ()));
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.regenerate.value().get() {
            after_stay(result.map(|_| ()), GroupOp::Other);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.remove.value().get() {
            after_stay(result, GroupOp::Remove);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = actions.transfer.value().get() {
            after_stay(result, GroupOp::Other);
        }
    });
    // Delete and leave navigate on success (server redirect); only errors are left.
    Effect::new(move |_| {
        if let Some(Err(err)) = actions.delete.value().get() {
            toasts.push(err.flash(GroupOp::Other));
        }
    });
    Effect::new(move |_| {
        if let Some(Err(err)) = actions.leave.value().get() {
            toasts.push(err.flash(GroupOp::Leave));
            group.refetch();
        }
    });

    view! {
        <Transition fallback=|| view! { <PageLoading/> }>
            {move || Suspend::new(async move {
                // Both resources are awaited here, whatever the outcome, so the page is only
                // sent once both have resolved (no late data chunk).
                let panel = panel.await.ok();
                let draw = draw.await.ok();
                match group.await {
                    Ok(detail) => view! { <GroupDetailView detail actions panel draw/> }.into_any(),
                    Err(error) => view! { <GroupErrorView error/> }.into_any(),
                }
            })}
        </Transition>
    }
}

#[component]
fn GroupDetailView(
    detail: GroupDetail,
    actions: Actions,
    panel: Option<PanelData>,
    draw: Option<DrawPanelData>,
) -> impl IntoView {
    let i18n = use_i18n();
    let id = detail.id;
    let header_actions = if detail.is_admin {
        view! { <AdminActions id actions status=detail.status/> }.into_any()
    } else if detail.can_leave {
        view! {
            <ActionForm action=actions.leave>
                <input type="hidden" name="group_id" value=id/>
                <SafeConfirmSubmit
                    label=i18n.t("group.show.leaveGroup")
                    confirm=i18n.t("group.show.leaveGroupConfirm")
                    class="btn btn-outline-danger"
                />
            </ActionForm>
        }
        .into_any()
    } else {
        view! { <p class="text-muted mb-0">{i18n.t("group.show.cannotLeaveAfterDraw")}</p> }
            .into_any()
    };
    let invitation = detail
        .invitation
        .clone()
        .map(|invitation| view! { <InvitationCard invitation status=detail.status/> });
    let members = detail.members.clone();
    let (is_admin, is_drawn) = (detail.is_admin, detail.is_drawn);

    view! {
        <div class="d-flex flex-wrap justify-content-between align-items-start gap-3 mb-4">
            <div>
                <h1 class="h2">{detail.name.clone()}</h1>
                {detail
                    .description
                    .clone()
                    .map(|text| view! { <p class="lead mb-0" style="white-space: pre-line">{text}</p> })}
            </div>
            {header_actions}
        </div>
        <div class="group-detail-grid">
            <div class="group-draw">
                <DrawPanel group_id=id is_admin is_drawn data=draw actions=actions.draw/>
            </div>
            <div class="group-wishlist" data-testid="group-wishlist">
                {panel.map(|data| view! { <WishlistPanel group_id=id data/> })}
            </div>
            <div class="group-details" data-testid="group-details"><DetailsCard detail/></div>
            <div class="group-members" data-testid="group-members">
                <MembersCard group_id=id members is_admin is_drawn actions/>
            </div>
            <div class="group-invitation" data-testid="group-invitation">{invitation}</div>
            <div class="group-exclusions">
                <ExclusionsLink group_id=id is_drawn/>
            </div>
        </div>
    }
}

#[component]
fn AdminActions(id: i32, actions: Actions, status: GroupStatus) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="d-flex flex-wrap gap-2">
            <a href=format!("/groups/{id}/edit") class="btn btn-outline-primary">
                {i18n.t("group.show.editButton")}
            </a>
            {(status == GroupStatus::Open).then(|| view! {
                <ActionForm action=actions.regenerate>
                    <input type="hidden" name="group_id" value=id/>
                    <SafeConfirmSubmit
                        label=i18n.t("group.show.regenerateCode")
                        confirm=i18n.t("group.show.regenerateCodeConfirm")
                        class="btn btn-outline-secondary"
                    />
                </ActionForm>
            })}
            <ActionForm action=actions.delete>
                <input type="hidden" name="group_id" value=id/>
                <SafeConfirmSubmit
                    label=i18n.t("group.show.deleteGroup")
                    confirm=i18n.t("group.show.deleteGroupConfirm")
                    class="btn btn-outline-danger"
                />
            </ActionForm>
        </div>
    }
}

/// A stored date as a local date of the page, or a dash.
fn date_or_dash(i18n: &crate::i18n::I18n, date: Option<chrono::DateTime<chrono::Utc>>) -> String {
    date.map(|d| i18n.date(d))
        .unwrap_or_else(|| "–".to_string())
}

#[component]
fn DetailsCard(detail: GroupDetail) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("group.show.details")}</h2>
                <dl class="row mb-0">
                    <dt class="col-sm-5">{i18n.t("group.show.status")}</dt>
                    <dd class="col-sm-7">
                        <span class=detail.status.badge_class()>{i18n.t(detail.status.label_key())}</span>
                    </dd>
                    <dt class="col-sm-5">{i18n.t("group.adminLabel")}</dt>
                    <dd class="col-sm-7">{detail.admin.name}</dd>
                    <dt class="col-sm-5">{i18n.t("group.show.registrationDeadline")}</dt>
                    <dd class="col-sm-7">
                        <span>{date_or_dash(&i18n, detail.registration_deadline)}</span>
                    </dd>
                    <dt class="col-sm-5">{i18n.t("group.show.drawDate")}</dt>
                    <dd class="col-sm-7">
                        <span>{date_or_dash(&i18n, detail.draw_date)}</span>
                        <div class="form-text">{i18n.t("group.show.drawDateInfo")}</div>
                    </dd>
                    <dt class="col-sm-5">{i18n.t("group.show.createdOn")}</dt>
                    <dd class="col-sm-7">
                        <span>{i18n.date(detail.created_at)}</span>
                    </dd>
                </dl>
            </div>
        </div>
    }
}

/// Puts `text` on the clipboard and shows the "copied" toast once the browser has accepted
/// the write. Nothing happens if there is no clipboard (not a secure context) or the write
/// is rejected (e.g. no permission).
fn copy_to_clipboard(text: String, toasts: Toasts) {
    #[cfg(feature = "hydrate")]
    {
        let Some(origin) = toasts.current_path() else {
            return;
        };
        let clipboard = window().navigator().clipboard();
        if clipboard.is_undefined() {
            return;
        }
        let written = wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text));
        leptos::task::spawn_local(async move {
            if written.await.is_ok() {
                toasts.push_if_current(
                    &origin,
                    crate::flash::FlashMessage::success("group.show.copied"),
                );
            }
        });
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = (text, toasts);
    }
}

/// Admin only: the code (in a read-only field, with a copy button) and the link.
#[component]
fn InvitationCard(invitation: Invitation, status: GroupStatus) -> impl IntoView {
    let i18n = use_i18n();
    if status != GroupStatus::Open {
        return view! {
            <div class="card mb-4">
                <div class="card-body">
                    <h2 class="h5 card-title">{i18n.t("group.show.invitationCode")}</h2>
                    <p class="alert alert-warning mb-0" role="status">
                        {i18n.t("group.show.registrationClosedHint")}
                    </p>
                </div>
            </div>
        }
        .into_any();
    }
    let toasts = use_toasts();
    let code = invitation.code.clone();
    let on_copy = move |_| copy_to_clipboard(code.clone(), toasts);
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("group.show.invitationCode")}</h2>
                <p class="mb-2">{i18n.t("group.show.shareCode")}</p>
                <div class="input-group mb-3">
                    <input
                        type="text"
                        class="form-control font-monospace"
                        readonly
                        value=invitation.code
                        aria-label=i18n.t("group.show.invitationCode")
                    />
                    <button type="button" class="btn btn-outline-secondary" on:click=on_copy>
                        {i18n.t("group.show.copy")}
                    </button>
                </div>
                <p class="mb-1">{i18n.t("group.show.shareLink")}</p>
                <code class="d-block text-break user-select-all">{invitation.link}</code>
            </div>
        </div>
    }
    .into_any()
}

#[component]
fn MembersCard(
    group_id: i32,
    members: Vec<MemberView>,
    is_admin: bool,
    is_drawn: bool,
    actions: Actions,
) -> impl IntoView {
    let i18n = use_i18n();
    let count = members.len();
    let rows = members
        .into_iter()
        .map(|member| {
            let manage = (is_admin && !member.is_me).then(|| {
                let user_id = member.user_id;
                let transfer_aria = t!(i18n, "group.show.makeAdminFor", name = member.name.clone());
                let remove_aria = t!(i18n, "group.show.removeMemberFor", name = member.name.clone());
                view! {
                    <div class="d-flex flex-wrap gap-2">
                        <ActionForm action=actions.transfer>
                            <input type="hidden" name="group_id" value=group_id/>
                            <input type="hidden" name="user_id" value=user_id/>
                            <SafeConfirmSubmit
                                label=i18n.t("group.show.makeAdmin")
                                aria_label=transfer_aria
                                confirm=i18n.t("group.show.makeAdminConfirm")
                                class="btn btn-sm btn-outline-primary"
                            />
                        </ActionForm>
                        {(!is_drawn)
                            .then(|| {
                                view! {
                                    <ActionForm action=actions.remove>
                                        <input type="hidden" name="group_id" value=group_id/>
                                        <input type="hidden" name="user_id" value=user_id/>
                                        <SafeConfirmSubmit
                                            label=i18n.t("group.show.removeMember")
                                            aria_label=remove_aria
                                            confirm=i18n.t("group.show.removeMemberConfirm")
                                            class="btn btn-sm btn-outline-danger"
                                        />
                                    </ActionForm>
                                }
                            })}
                    </div>
                }
            });
            view! {
                <li class="list-group-item d-flex flex-wrap justify-content-between align-items-center gap-2">
                    <div>
                        <span class="fw-semibold">{member.name}</span>
                        {member
                            .is_admin
                            .then(|| view! { <span class="badge bg-primary ms-2">{i18n.t("group.admin")}</span> })}
                        {member
                            .is_me
                            .then(|| view! { <span class="badge bg-info text-dark ms-2">{i18n.t("group.show.you")}</span> })}
                        {member.email.map(|email| view! { <div class="small text-muted">{email}</div> })}
                    </div>
                    {manage}
                </li>
            }
        })
        .collect_view();
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{t!(i18n, "group.show.members", count = count)}</h2>
                <ul class="list-group list-group-flush">{rows}</ul>
            </div>
        </div>
    }
}
