//! `/auth/register?next=<path>`: step 1 of the registration (email only).

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use super::form::{AuthInput, ErrorAlert, last_error};
use crate::auth_api::{Field, Register};
use crate::components::RedirectIfLoggedIn;
use crate::i18n::use_i18n;
use crate::paths::{DASHBOARD_PATH, safe_next, with_next};

#[component]
pub fn RegisterPage() -> impl IntoView {
    let i18n = use_i18n();
    let query = use_query_map();
    let next = Memo::new(move |_| query.read().get("next").as_deref().and_then(safe_next));

    let action = ServerAction::<Register>::new();
    let error = last_error(action.value());
    let done = move || matches!(action.value().get(), Some(Ok(())));
    let login_href = move || with_next("/auth/login", next.get().as_deref());

    view! {
        <RedirectIfLoggedIn to=DASHBOARD_PATH.to_string()/>
        <div class="row justify-content-center">
            <div class="col-md-6 col-lg-5">
                <div class="card">
                    <div class="card-body">
                        <h1 class="h3 card-title mb-3">{i18n.t("auth.register.title")}</h1>
                        <Show
                            when=done
                            fallback=move || {
                                view! {
                                    <p class="text-muted">{i18n.t("auth.register.description")}</p>
                                    <ErrorAlert error/>
                                    <ActionForm action>
                                        {move || {
                                            next.get().map(|next| view! { <input type="hidden" name="next" value=next/> })
                                        }}
                                        <AuthInput
                                            id="register-email"
                                            name="email"
                                            label=i18n.t("auth.register.email")
                                            kind="email"
                                            autocomplete="email"
                                            field=Field::Email
                                            error
                                        />
                                        <button type="submit" class="btn btn-primary w-100">
                                            {i18n.t("auth.register.submit")}
                                        </button>
                                    </ActionForm>
                                }
                            }
                        >
                            <div class="alert alert-success" role="status">
                                {i18n.t("flash.success.registration_instructions")}
                            </div>
                        </Show>
                        <div class="mt-3 text-center">
                            {i18n.t("auth.register.haveAccount")} " "
                            <a href=login_href>{i18n.t("auth.login")}</a>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
