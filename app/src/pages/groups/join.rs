//! `/join` (enter a code) and `/join/:code` (the invitation preview with a Join button;
//! spec: groups D11, UI notes). Opening a link never joins: only the button (a POST) does.

use leptos::ev::SubmitEvent;
use leptos::html::Input;
use leptos::prelude::*;
use leptos::server_fn::ServerFn;
use leptos_router::hooks::{use_navigate, use_params_map};

use super::{group_path, when_logged_in};
use crate::components::{ClientRedirect, RequireUser};
use crate::flash::{FlashMessage, use_toasts};
use crate::groups_api::{
    ClosedReason, GroupError, GroupOp, InvitationPreview, JoinGroup, OpenInvitation, join_path,
    normalize_invitation_code, preview_invitation,
};
use crate::i18n::use_i18n;
use crate::{t, t_plural};

#[component]
pub fn JoinPage() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <RequireUser>
            <div class="row justify-content-center">
                <div class="col-md-8 col-lg-6">
                    <div class="card">
                        <div class="card-body">
                            <h1 class="h3 card-title mb-3">{i18n.t("group.join.title")}</h1>
                            <p>{i18n.t("group.join.description")}</p>
                            <JoinCodeForm/>
                        </div>
                    </div>
                </div>
            </div>
        </RequireUser>
    }
}

/// The enter-code form. With JavaScript it goes straight to `/join/<normalised code>`;
/// without, it posts to `open_invitation`, which redirects there.
#[component]
fn JoinCodeForm() -> impl IntoView {
    let i18n = use_i18n();
    let input = NodeRef::<Input>::new();
    let error = RwSignal::new(None::<GroupError>);
    let navigate = use_navigate();
    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let raw = input
            .get_untracked()
            .map(|el| el.value())
            .unwrap_or_default();
        let code = normalize_invitation_code(&raw);
        if code.is_empty() {
            error.set(Some(GroupError::CodeRequired));
            return;
        }
        match join_path(&code) {
            Some(path) => {
                error.set(None);
                navigate(&path, Default::default());
            }
            None => error.set(Some(GroupError::InvalidCode)),
        }
    };
    view! {
        <form method="post" action=OpenInvitation::url() on:submit=on_submit>
            <div class="mb-3">
                <label for="join-code" class="form-label">{i18n.t("group.join.code")}</label>
                <input
                    type="text"
                    id="join-code"
                    name="code"
                    class=move || if error.get().is_some() { "form-control is-invalid" } else { "form-control" }
                    placeholder=i18n.t("group.join.codePlaceholder")
                    aria-describedby="join-code-help join-code-error"
                    aria-invalid=move || if error.get().is_some() { "true" } else { "false" }
                    autocomplete="off"
                    autocapitalize="characters"
                    node_ref=input
                />
                <div id="join-code-error" class="invalid-feedback" role="alert">
                    {move || error.get().map(|e| e.text(&i18n))}
                </div>
                <div id="join-code-help" class="form-text">{i18n.t("group.join.codeHelp")}</div>
            </div>
            <div class="d-flex gap-2">
                <button type="submit" class="btn btn-primary">{i18n.t("group.join.submit")}</button>
                <a href="/groups" class="btn btn-outline-secondary">{i18n.t("group.join.cancel")}</a>
            </div>
        </form>
    }
}

type PreviewResource = Resource<Result<InvitationPreview, GroupError>>;

#[component]
pub fn JoinPreviewPage() -> impl IntoView {
    let params = use_params_map();
    let code = Memo::new(move |_| {
        normalize_invitation_code(&params.read().get("code").unwrap_or_default())
    });
    let preview: PreviewResource = Resource::new_blocking(
        move || code.get(),
        |code| async move {
            let result = when_logged_in(preview_invitation(code)).await;
            // Decide before SSR sends its headers; existing members bypass the
            // confirmation page even after the draw or registration deadline.
            #[cfg(feature = "ssr")]
            if let Ok(preview) = &result
                && preview.already_member
            {
                crate::server::respond::redirect_page(&group_path(preview.group_id));
            }
            result
        },
    );
    view! {
        <RequireUser>
            <JoinPreview code preview/>
        </RequireUser>
    }
}

#[component]
fn JoinPreview(code: Memo<String>, preview: PreviewResource) -> impl IntoView {
    let i18n = use_i18n();
    let action = ServerAction::<JoinGroup>::new();
    let toasts = use_toasts();
    let navigate = use_navigate();
    // Success navigates by the server's redirect. An existing member goes to the group.
    Effect::new(move |_| {
        if let Some(Err(GroupError::AlreadyMember { group_id })) = action.value().get() {
            let path = group_path(group_id);
            // Shown on the group page (a plain `push` would stay behind on /join).
            toasts.push_for(&path, FlashMessage::info("flash.info.already_member"));
            navigate(&path, Default::default());
        }
    });
    let error = Signal::derive(move || {
        action
            .value()
            .get()
            .and_then(Result::err)
            .filter(|e| !matches!(e, GroupError::AlreadyMember { .. }))
    });

    view! {
        <div class="row justify-content-center">
            <div class="col-md-8 col-lg-6">
                <Transition fallback=|| ()>
                    {move || Suspend::new(async move {
                        match preview.await {
                            Ok(preview) if preview.already_member => {
                                view! { <ClientRedirect path=group_path(preview.group_id)/> }
                                    .into_any()
                            }
                            Ok(preview) => {
                                view! { <PreviewCard preview code=code.get_untracked() action error/> }
                                    .into_any()
                            }
                            Err(err @ (GroupError::InvalidCode | GroupError::CodeRequired)) => {
                                view! {
                                    <div class="card">
                                        <div class="card-body">
                                            <h1 class="h3 card-title mb-3">{i18n.t("group.join.title")}</h1>
                                            <div class="alert alert-danger" role="alert">
                                                {err.text_for(GroupOp::Join, &i18n)}
                                            </div>
                                            <JoinCodeForm/>
                                        </div>
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(GroupError::Unauthorized) => ().into_any(),
                            Err(err) => {
                                view! {
                                    <div class="alert alert-danger" role="alert">
                                        {err.text_for(GroupOp::Join, &i18n)}
                                    </div>
                                }
                                    .into_any()
                            }
                        }
                    })}
                </Transition>
            </div>
        </div>
    }
}

#[component]
fn PreviewCard(
    preview: InvitationPreview,
    code: String,
    action: ServerAction<JoinGroup>,
    error: Signal<Option<GroupError>>,
) -> impl IntoView {
    let i18n = use_i18n();
    let count = preview.member_count;
    let outcome = if preview.already_member {
        view! {
            <div class="alert alert-info" role="status">{i18n.t("flash.info.already_member")}</div>
            <a href=group_path(preview.group_id) class="btn btn-primary">{i18n.t("group.viewDetails")}</a>
        }
        .into_any()
    } else {
        match preview.closed {
            Some(ClosedReason::Drawn) => view! {
                <div class="alert alert-warning" role="alert">{i18n.t("flash.error.join_closed_drawn")}</div>
            }
            .into_any(),
            Some(ClosedReason::DeadlinePassed) => view! {
                <div class="alert alert-warning" role="alert">
                    {i18n.t("flash.error.registration_deadline_passed")}
                </div>
            }
            .into_any(),
            None => view! {
                {move || {
                    error
                        .get()
                        .map(|e| {
                            view! {
                                <div class="alert alert-danger" role="alert">
                                    {e.text_for(GroupOp::Join, &i18n)}
                                </div>
                            }
                        })
                }}
                <ActionForm action>
                    <input type="hidden" name="code" value=code/>
                    <button type="submit" class="btn btn-primary">{i18n.t("group.join.submit")}</button>
                </ActionForm>
            }
            .into_any(),
        }
    };
    view! {
        <div class="card">
            <div class="card-body">
                <h1 class="h3 card-title mb-3">
                    {t!(i18n, "group.join.previewTitle", name = preview.name)}
                </h1>
                <p class="mb-1">
                    {i18n.t("group.adminLabel")} ": " <strong>{preview.admin_name}</strong>
                </p>
                <p>
                    <span>{t_plural!(i18n, "group.memberCount", i64::from(count), count = count)}</span>
                </p>
                {outcome}
            </div>
        </div>
    }
}
