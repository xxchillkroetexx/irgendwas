//! `/user/profile` (spec: account, "UI notes"): three independent cards for the name, the
//! email address (with the pending change) and the password. Each card shows the result of
//! its own last submission; nothing is auto-dismissed. The forms are `ActionForm`s, so they
//! also post without JavaScript.

use leptos::html::Input;
use leptos::prelude::*;

use crate::account_api::{
    AccountError, CancelEmailChange, ChangePassword, ProfileView, RequestEmailChange, UpdateName,
    get_profile,
};
use crate::auth::{get_current_user, use_session_resource};
use crate::components::RequireUser;
use crate::format::LocalDateTime;
use crate::i18n::use_i18n;
use crate::paths::DASHBOARD_PATH;
use crate::t;

/// The outcome of a card's last submission.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Notice {
    /// i18n key of the success message.
    Success(&'static str),
    Error(AccountError),
}

impl Notice {
    fn of(result: &Result<(), AccountError>, success_key: &'static str) -> Self {
        match result {
            Ok(()) => Notice::Success(success_key),
            Err(err) => Notice::Error(err.clone()),
        }
    }
}

#[component]
fn CardNotice(notice: RwSignal<Option<Notice>>) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        notice.get().map(|notice| match notice {
            Notice::Success(key) => view! {
                <div class="alert alert-success" role="status">{i18n.t(key)}</div>
            }
            .into_any(),
            Notice::Error(err) => view! {
                <div class="alert alert-danger" role="alert">{err.text(&i18n)}</div>
            }
            .into_any(),
        })
    }
}

/// Empties the given inputs (client only; the effect runs after hydration).
fn clear(inputs: &[NodeRef<Input>]) {
    for input in inputs {
        if let Some(el) = input.get_untracked() {
            el.set_value("");
        }
    }
}

type ProfileResource = Resource<Result<ProfileView, AccountError>>;

#[component]
pub fn ProfilePage() -> impl IntoView {
    // Personal data: no cache keeps the page (AC42).
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    // Created here, synchronously, and not inside the guard's async children: the
    // serialized-data id of a resource depends on creation order, which must be the same on
    // server and client. Anonymous visitors are redirected by the guard; `get_profile` is
    // not called for them (its error status would leak into the redirect response).
    let profile: ProfileResource = Resource::new(
        || (),
        |_| async move {
            match get_current_user().await {
                Ok(Some(_)) => get_profile().await,
                _ => Err(AccountError::Unauthorized),
            }
        },
    );
    view! {
        <RequireUser>
            <Profile profile/>
        </RequireUser>
    }
}

#[component]
fn Profile(profile: ProfileResource) -> impl IntoView {
    let i18n = use_i18n();
    let session = use_session_resource().0;

    // Actions, messages and input refs live here, outside the profile `Transition`, so a
    // refetch (which re-renders the name and email cards) keeps them.
    let name_action = ServerAction::<UpdateName>::new();
    let email_action = ServerAction::<RequestEmailChange>::new();
    let cancel_action = ServerAction::<CancelEmailChange>::new();
    let password_action = ServerAction::<ChangePassword>::new();
    let name_notice = RwSignal::new(None::<Notice>);
    let email_notice = RwSignal::new(None::<Notice>);
    let password_notice = RwSignal::new(None::<Notice>);
    let new_email = NodeRef::<Input>::new();
    let email_password = NodeRef::<Input>::new();
    let current_password = NodeRef::<Input>::new();
    let new_password = NodeRef::<Input>::new();
    let confirm_password = NodeRef::<Input>::new();

    Effect::new(move |_| {
        if let Some(result) = name_action.value().get() {
            name_notice.set(Some(Notice::of(&result, "flash.success.profile_updated")));
            if result.is_ok() {
                // The navbar shows the name from the layout's session resource.
                session.refetch();
                profile.refetch();
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = email_action.value().get() {
            email_notice.set(Some(Notice::of(
                &result,
                "flash.success.email_verification_sent",
            )));
            clear(&[email_password]);
            if result.is_ok() {
                clear(&[new_email]);
                profile.refetch();
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = cancel_action.value().get() {
            email_notice.set(Some(Notice::of(
                &result,
                "flash.success.email_change_cancelled",
            )));
            if result.is_ok() {
                profile.refetch();
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = password_action.value().get() {
            password_notice.set(Some(match &result {
                Ok(changed) => Notice::Success(changed.message_key()),
                Err(error) => Notice::Error(error.clone()),
            }));
            clear(&[current_password, new_password, confirm_password]);
            if result.is_ok() {
                // Changing credentials also cancels a pending email change.
                profile.refetch();
            }
        }
    });

    view! {
        <div class="row justify-content-center">
            <div class="col-lg-8">
                <h1 class="h2 mb-4">{i18n.t("user.profile.title")}</h1>
                <Transition fallback=|| ()>
                    {move || Suspend::new(async move {
                        match profile.await {
                            Ok(view) => {
                                view! {
                                    <NameCard name=view.name.clone() action=name_action notice=name_notice/>
                                    <EmailCard
                                        profile=view
                                        action=email_action
                                        cancel=cancel_action
                                        notice=email_notice
                                        new_email
                                        password=email_password
                                    />
                                }
                                    .into_any()
                            }
                            Err(err) => {
                                view! {
                                    <div class="alert alert-danger" role="alert">{err.text(&i18n)}</div>
                                }
                                    .into_any()
                            }
                        }
                    })}
                </Transition>
                <PasswordCard
                    action=password_action
                    notice=password_notice
                    current=current_password
                    new=new_password
                    confirm=confirm_password
                />
                <a href=DASHBOARD_PATH>{i18n.t("user.profile.backToDashboard")}</a>
            </div>
        </div>
    }
}

#[component]
fn NameCard(
    name: String,
    action: ServerAction<UpdateName>,
    notice: RwSignal<Option<Notice>>,
) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <CardNotice notice/>
                <ActionForm action>
                    <div class="mb-3">
                        <label for="profile-name" class="form-label">{i18n.t("user.profile.name")}</label>
                        <input
                            type="text"
                            id="profile-name"
                            name="name"
                            class="form-control"
                            autocomplete="name"
                            value=name
                            required
                        />
                    </div>
                    <button type="submit" class="btn btn-primary">
                        {i18n.t("user.profile.updateProfile")}
                    </button>
                </ActionForm>
            </div>
        </div>
    }
}

#[component]
fn EmailCard(
    profile: ProfileView,
    action: ServerAction<RequestEmailChange>,
    cancel: ServerAction<CancelEmailChange>,
    notice: RwSignal<Option<Notice>>,
    new_email: NodeRef<Input>,
    password: NodeRef<Input>,
) -> impl IntoView {
    let i18n = use_i18n();
    let pending = profile.pending_email.zip(profile.pending_expires_at).map(|(email, expires)| {
        view! {
            <div class="alert alert-info" role="status">
                <p class="mb-1">{t!(i18n, "user.profile.emailPendingVerification", email = email)}</p>
                <p class="mb-2">
                    {t!(i18n, "user.profile.emailPendingExpires", date = LocalDateTime(expires))}
                </p>
                <ActionForm action=cancel>
                    <button type="submit" class="btn btn-sm btn-outline-secondary">
                        {i18n.t("user.profile.changeEmail.cancel")}
                    </button>
                </ActionForm>
            </div>
        }
    });
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("user.profile.changeEmail.title")}</h2>
                <p>
                    <span class="text-muted">{i18n.t("user.profile.email")}</span>
                    " "
                    <strong>{profile.email}</strong>
                </p>
                {pending}
                <CardNotice notice/>
                <ActionForm action>
                    <div class="mb-3">
                        <label for="email-new" class="form-label">
                            {i18n.t("user.profile.changeEmail.new")}
                        </label>
                        // Not type=email: the server's rules and messages apply, not the
                        // browser's.
                        <input
                            type="text"
                            inputmode="email"
                            id="email-new"
                            name="new_email"
                            class="form-control"
                            autocomplete="email"
                            node_ref=new_email
                            required
                        />
                    </div>
                    <div class="mb-3">
                        <label for="email-current-password" class="form-label">
                            {i18n.t("user.profile.changeEmail.currentPassword")}
                        </label>
                        <input
                            type="password"
                            id="email-current-password"
                            name="current_password"
                            class="form-control"
                            autocomplete="current-password"
                            node_ref=password
                            required
                        />
                    </div>
                    <p class="form-text">{i18n.t("user.profile.emailChangeNotice")}</p>
                    <button type="submit" class="btn btn-primary">
                        {i18n.t("user.profile.changeEmail.submit")}
                    </button>
                </ActionForm>
            </div>
        </div>
    }
}

#[component]
fn PasswordCard(
    action: ServerAction<ChangePassword>,
    notice: RwSignal<Option<Notice>>,
    current: NodeRef<Input>,
    new: NodeRef<Input>,
    confirm: NodeRef<Input>,
) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="card mb-4">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("user.profile.changePassword.title")}</h2>
                <CardNotice notice/>
                <ActionForm action>
                    <div class="mb-3">
                        <label for="password-current" class="form-label">
                            {i18n.t("user.profile.changePassword.current")}
                        </label>
                        <input
                            type="password"
                            id="password-current"
                            name="current_password"
                            class="form-control"
                            autocomplete="current-password"
                            node_ref=current
                            required
                        />
                    </div>
                    <div class="mb-3">
                        <label for="password-new" class="form-label">
                            {i18n.t("user.profile.changePassword.new")}
                        </label>
                        <input
                            type="password"
                            id="password-new"
                            name="new_password"
                            class="form-control"
                            autocomplete="new-password"
                            node_ref=new
                            required
                        />
                    </div>
                    <div class="mb-3">
                        <label for="password-confirm" class="form-label">
                            {i18n.t("user.profile.changePassword.confirm")}
                        </label>
                        <input
                            type="password"
                            id="password-confirm"
                            name="new_password_confirm"
                            class="form-control"
                            autocomplete="new-password"
                            node_ref=confirm
                            required
                        />
                    </div>
                    <p class="form-text">{i18n.t("user.profile.changePassword.otherSessionsNotice")}</p>
                    <button type="submit" class="btn btn-primary">
                        {i18n.t("user.profile.changePassword.submit")}
                    </button>
                </ActionForm>
            </div>
        </div>
    }
}
