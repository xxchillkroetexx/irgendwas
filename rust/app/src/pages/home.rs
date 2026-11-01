//! Landing page (hero and login/register cards of `src/Views/home/index.php`).

use leptos::prelude::*;

use crate::i18n::use_i18n;

#[component]
pub fn HomePage() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="text-center mb-5">
            <h1 class="display-4">{i18n.t("home.title")}</h1>
            <p class="lead">{i18n.t("home.subtitle")}</p>
        </div>
        <div class="row justify-content-center">
            <div class="col-md-8">
                <div class="row">
                    <div class="col-md-6">
                        <div class="card mb-4">
                            <div class="card-body text-center">
                                <h5 class="card-title">{i18n.t("home.login.title")}</h5>
                                <p class="card-text">{i18n.t("home.login.description")}</p>
                                <a href="/auth/login" class="btn btn-primary">{i18n.t("auth.login")}</a>
                            </div>
                        </div>
                    </div>
                    <div class="col-md-6">
                        <div class="card mb-4">
                            <div class="card-body text-center">
                                <h5 class="card-title">{i18n.t("home.register.title")}</h5>
                                <p class="card-text">{i18n.t("home.register.description")}</p>
                                <a href="/auth/register" class="btn btn-secondary">
                                    {i18n.t("auth.register")}
                                </a>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
