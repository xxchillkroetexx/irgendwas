//! `/auth/forgot-password`.

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use super::form::{AuthInput, ErrorAlert, last_error};
use crate::auth_api::{Field, RequestPasswordReset};
use crate::i18n::use_i18n;
use crate::paths::{safe_next, with_next};

#[component]
pub fn ForgotPasswordPage() -> impl IntoView {
    let i18n = use_i18n();
    let query = use_query_map();
    let next = Memo::new(move |_| query.read().get("next").as_deref().and_then(safe_next));
    let action = ServerAction::<RequestPasswordReset>::new();
    let error = last_error(action.value());
    let login_href = move || with_next("/auth/login", next.get().as_deref());

    view! {
        <div class="row justify-content-center">
            <div class="col-md-6 col-lg-5">
                <div class="card">
                    <div class="card-body">
                        <h1 class="h3 card-title mb-3">{i18n.t("auth.forgotPassword.title")}</h1>
                        <p class="text-muted">{i18n.t("auth.forgotPassword.description")}</p>
                        <ErrorAlert error/>
                        <ActionForm action>
                            <AuthInput
                                id="forgot-email"
                                name="email"
                                label=i18n.t("auth.forgotPassword.email")
                                kind="email"
                                autocomplete="email"
                                field=Field::Email
                                error
                            />
                            <button type="submit" class="btn btn-primary w-100">
                                {i18n.t("auth.forgotPassword.submit")}
                            </button>
                        </ActionForm>
                        <div class="mt-3 text-center">
                            <a href=login_href>{i18n.t("auth.forgotPassword.backToLogin")}</a>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
