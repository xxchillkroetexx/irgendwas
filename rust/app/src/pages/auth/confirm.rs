//! `/auth/register/confirm/:token`: step 2 of the registration (name and password).

use leptos::html::Input;
use leptos::prelude::*;

use super::form::{AuthInput, ErrorAlert, clear_on_error, last_error};
use super::token_page::{after_layout_flash, invalid_link, route_token, token_page_headers};
use crate::auth::use_session_resource;
use crate::auth_api::{AuthError, CompleteRegistration, Field, get_pending_registration};
use crate::components::ClientRedirect;
use crate::flash::{FlashMessage, Toasts};
use crate::i18n::use_i18n;
use crate::t;

const INVALID_TARGET: &str = "/auth/register";

#[component]
pub fn RegisterConfirmPage() -> impl IntoView {
    token_page_headers();
    let i18n = use_i18n();
    let token = route_token();
    let layout = use_session_resource();
    let toasts = use_context::<Toasts>();
    let pending = Resource::new_blocking(
        move || token.get(),
        move |token| async move {
            after_layout_flash(layout).await;
            let result = get_pending_registration(token).await;
            if matches!(result, Err(AuthError::InvalidRegistrationToken)) {
                invalid_link(
                    toasts,
                    FlashMessage::error("flash.error.registration_link_invalid"),
                    INVALID_TARGET,
                )
                .await;
            }
            result
        },
    );

    view! {
        <div class="row justify-content-center">
            <div class="col-md-6 col-lg-5">
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match pending.await {
                            Ok(registration) => {
                                view! { <ConfirmForm email=registration.email token=token.get_untracked()/> }
                                    .into_any()
                            }
                            Err(AuthError::InvalidRegistrationToken) => {
                                view! { <ClientRedirect path=INVALID_TARGET/> }.into_any()
                            }
                            Err(err) => {
                                view! {
                                    <div class="alert alert-danger" role="alert">{err.text(&i18n)}</div>
                                }
                                    .into_any()
                            }
                        }
                    })}
                </Suspense>
            </div>
        </div>
    }
}

#[component]
fn ConfirmForm(email: String, token: String) -> impl IntoView {
    let i18n = use_i18n();
    let action = ServerAction::<CompleteRegistration>::new();
    let error = last_error(action.value());
    let password = NodeRef::<Input>::new();
    let confirm = NodeRef::<Input>::new();
    clear_on_error(action.value(), vec![password, confirm]);

    view! {
        <div class="card">
            <div class="card-body">
                <h1 class="h3 card-title mb-3">{i18n.t("auth.registerConfirm.title")}</h1>
                <p>{t!(i18n, "auth.registerConfirm.intro", email = email)}</p>
                <ErrorAlert error/>
                <ActionForm action>
                    <input type="hidden" name="token" value=token/>
                    <AuthInput
                        id="register-name"
                        name="name"
                        label=i18n.t("auth.register.name")
                        kind="text"
                        autocomplete="name"
                        field=Field::Name
                        error
                    />
                    <AuthInput
                        id="register-password"
                        name="password"
                        label=i18n.t("auth.register.password")
                        kind="password"
                        autocomplete="new-password"
                        field=Field::Password
                        error
                        node_ref=password
                        help=i18n.t("auth.register.passwordRequirements")
                    />
                    <AuthInput
                        id="register-password-confirm"
                        name="password_confirm"
                        label=i18n.t("auth.register.confirmPassword")
                        kind="password"
                        autocomplete="new-password"
                        field=Field::PasswordConfirm
                        error
                        node_ref=confirm
                    />
                    <button type="submit" class="btn btn-primary w-100">
                        {i18n.t("auth.registerConfirm.submit")}
                    </button>
                </ActionForm>
            </div>
        </div>
    }
}
