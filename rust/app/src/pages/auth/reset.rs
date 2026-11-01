//! `/auth/reset-password/:token`.

use leptos::html::Input;
use leptos::prelude::*;

use super::form::{AuthInput, ErrorAlert, clear_on_error, last_error};
use super::token_page::{after_layout_flash, invalid_link, route_token, token_page_headers};
use crate::auth::use_session_resource;
use crate::auth_api::{AuthError, Field, ResetPassword, check_reset_token};
use crate::components::ClientRedirect;
use crate::flash::{FlashMessage, Toasts};
use crate::i18n::use_i18n;

const INVALID_TARGET: &str = "/auth/forgot-password";

#[component]
pub fn ResetPasswordPage() -> impl IntoView {
    token_page_headers();
    let i18n = use_i18n();
    let token = route_token();
    let layout = use_session_resource();
    let toasts = use_context::<Toasts>();
    let valid = Resource::new_blocking(
        move || token.get(),
        move |token| async move {
            after_layout_flash(layout).await;
            let result = check_reset_token(token).await;
            if matches!(result, Err(AuthError::InvalidResetToken)) {
                invalid_link(
                    toasts,
                    FlashMessage::error("flash.error.reset_token_expired"),
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
                        match valid.await {
                            Ok(()) => view! { <ResetForm token=token.get_untracked()/> }.into_any(),
                            Err(AuthError::InvalidResetToken) => {
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
fn ResetForm(token: String) -> impl IntoView {
    let i18n = use_i18n();
    let action = ServerAction::<ResetPassword>::new();
    let error = last_error(action.value());
    let password = NodeRef::<Input>::new();
    let confirm = NodeRef::<Input>::new();
    clear_on_error(action.value(), vec![password, confirm]);

    view! {
        <div class="card">
            <div class="card-body">
                <h1 class="h3 card-title mb-3">{i18n.t("auth.resetPassword.title")}</h1>
                <ErrorAlert error/>
                <ActionForm action>
                    <input type="hidden" name="token" value=token/>
                    <AuthInput
                        id="reset-password"
                        name="password"
                        label=i18n.t("auth.resetPassword.newPassword")
                        kind="password"
                        autocomplete="new-password"
                        field=Field::Password
                        error
                        node_ref=password
                        help=i18n.t("auth.resetPassword.passwordRequirements")
                    />
                    <AuthInput
                        id="reset-password-confirm"
                        name="password_confirm"
                        label=i18n.t("auth.resetPassword.confirmPassword")
                        kind="password"
                        autocomplete="new-password"
                        field=Field::PasswordConfirm
                        error
                        node_ref=confirm
                    />
                    <button type="submit" class="btn btn-primary w-100">
                        {i18n.t("auth.resetPassword.submit")}
                    </button>
                </ActionForm>
                <div class="mt-3 text-center">
                    <a href="/auth/login">{i18n.t("auth.resetPassword.backToLogin")}</a>
                </div>
            </div>
        </div>
    }
}
