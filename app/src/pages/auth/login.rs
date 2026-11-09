//! `/auth/login?next=<path>`.

use leptos::html::Input;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use super::form::{AuthInput, ErrorAlert, clear_on_error, last_error};
use crate::auth_api::{Field, Login};
use crate::components::RedirectIfLoggedIn;
use crate::components::guard::full_navigation;
use crate::i18n::use_i18n;
use crate::paths::{next_or_dashboard, safe_next, with_next};

#[component]
pub fn LoginPage() -> impl IntoView {
    let i18n = use_i18n();
    let query = use_query_map();
    let next = Memo::new(move |_| query.read().get("next"));
    let valid_next = Memo::new(move |_| next.get().as_deref().and_then(safe_next));
    let target = Signal::derive(move || next_or_dashboard(next.get().as_deref()));

    let action = ServerAction::<Login>::new();
    let error = last_error(action.value());
    let password = NodeRef::<Input>::new();
    clear_on_error(action.value(), vec![password]);
    // Success: load the target as a new document (the account language may differ from
    // the page's, and the locale is fixed per page load).
    Effect::new(move |_| {
        if matches!(action.value().get(), Some(Ok(()))) {
            full_navigation(&target.get_untracked());
        }
    });

    let register_href = move || with_next("/auth/register", valid_next.get().as_deref());
    let forgot_href = move || with_next("/auth/forgot-password", valid_next.get().as_deref());

    view! {
        <RedirectIfLoggedIn to=target>
            <div class="row justify-content-center">
                <div class="col-md-6 col-lg-5">
                    {move || {
                        valid_next
                            .get()
                            .map(|_| {
                                view! {
                                    <div class="alert alert-info" role="status">
                                        {i18n.t("auth.login.required")}
                                    </div>
                                }
                            })
                    }}
                    <div class="card">
                        <div class="card-body">
                            <h1 class="h3 card-title mb-4">{i18n.t("auth.login.title")}</h1>
                            <ErrorAlert error/>
                            <ActionForm action>
                                {move || {
                                    next.get().map(|next| view! { <input type="hidden" name="next" value=next/> })
                                }}
                                <AuthInput
                                    id="login-email"
                                    name="email"
                                    label=i18n.t("auth.login.email")
                                    kind="email"
                                    autocomplete="email"
                                    field=Field::Email
                                    error
                                />
                                <AuthInput
                                    id="login-password"
                                    name="password"
                                    label=i18n.t("auth.login.password")
                                    kind="password"
                                    autocomplete="current-password"
                                    field=Field::Password
                                    error
                                    node_ref=password
                                />
                                <button type="submit" class="btn btn-primary w-100">
                                    {i18n.t("auth.login.submit")}
                                </button>
                            </ActionForm>
                            <div class="mt-3 text-center">
                                <a href=forgot_href>{i18n.t("auth.login.forgotPassword")}</a>
                            </div>
                            <div class="mt-2 text-center">
                                {i18n.t("auth.login.noAccount")} " "
                                <a href=register_href>{i18n.t("auth.register")}</a>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </RedirectIfLoggedIn>
    }
}
