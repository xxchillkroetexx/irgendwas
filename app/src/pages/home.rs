//! Landing page (spec: i18n-polish "Landing page"):
//! hero, login/register cards, the invitation-code link, "How it works" and the features.
//! Logged-in visitors are sent to the dashboard.

use leptos::prelude::*;

use crate::components::RedirectIfLoggedIn;
use crate::i18n::use_i18n;
use crate::paths::DASHBOARD_PATH;

#[component]
pub fn HomePage() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <RedirectIfLoggedIn to=DASHBOARD_PATH.to_string()/>
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

                // "/join" is protected: anonymous visitors go through login (next=/join).
                <p class="text-center">
                    <span>{i18n.t("home.join.text")}</span>
                    " "
                    <a href="/join">{i18n.t("group.joinButton")}</a>
                </p>

                <div class="mt-5">
                    <h2 class="text-center mb-4">{i18n.t("home.howItWorks.title")}</h2>
                    <div class="row">
                        <Step
                            icon=StepIcon::People
                            title=i18n.t("home.howItWorks.step1.title")
                            description=i18n.t("home.howItWorks.step1.description")
                        />
                        <Step
                            icon=StepIcon::ListCheck
                            title=i18n.t("home.howItWorks.step2.title")
                            description=i18n.t("home.howItWorks.step2.description")
                        />
                        <Step
                            icon=StepIcon::Gift
                            title=i18n.t("home.howItWorks.step3.title")
                            description=i18n.t("home.howItWorks.step3.description")
                        />
                    </div>
                </div>

                <div class="mt-5 text-center">
                    <h2>{i18n.t("home.features.title")}</h2>
                    <div class="row mt-3">
                        <Feature text=i18n.t("home.features.multipleGroups")/>
                        <Feature text=i18n.t("home.features.emailNotifications")/>
                        <Feature text=i18n.t("home.features.exclusionRules")/>
                        <Feature text=i18n.t("home.features.prioritizedWishlists")/>
                        <Feature text=i18n.t("home.features.secretAssignments")/>
                    </div>
                </div>
            </div>
        </div>
    }
}

/// The Bootstrap icons of the three "How it works" steps.
#[derive(Clone, Copy)]
enum StepIcon {
    People,
    ListCheck,
    Gift,
}

impl StepIcon {
    fn class(self) -> &'static str {
        match self {
            StepIcon::People => "bi bi-people",
            StepIcon::ListCheck => "bi bi-list-check",
            StepIcon::Gift => "bi bi-gift",
        }
    }

    /// Only the list icon uses the even-odd rule (as in the Bootstrap icon set).
    fn fill_rule(self) -> Option<&'static str> {
        matches!(self, StepIcon::ListCheck).then_some("evenodd")
    }

    fn path(self) -> &'static str {
        match self {
            StepIcon::People => {
                "M15 14s1 0 1-1-1-4-5-4-5 3-5 4 1 1 1 1h8Zm-7.978-1A.261.261 0 0 1 7 12.996c.001-.264.167-1.03.76-1.72C8.312 10.629 9.282 10 11 10c1.717 0 2.687.63 3.24 1.276.593.69.758 1.457.76 1.72l-.008.002a.274.274 0 0 1-.014.002H7.022ZM11 7a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm3-2a3 3 0 1 1-6 0 3 3 0 0 1 6 0ZM6.936 9.28a5.88 5.88 0 0 0-1.23-.247A7.35 7.35 0 0 0 5 9c-4 0-5 3-5 4 0 .667.333 1 1 1h4.216A2.238 2.238 0 0 1 5 13c0-1.01.377-2.042 1.09-2.904.243-.294.526-.569.846-.816ZM4.92 10A5.493 5.493 0 0 0 4 13H1c0-.26.164-1.03.76-1.724.545-.636 1.492-1.256 3.16-1.275ZM1.5 5.5a3 3 0 1 1 6 0 3 3 0 0 1-6 0Zm3-2a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z"
            }
            StepIcon::ListCheck => {
                "M5 11.5a.5.5 0 0 1 .5-.5h9a.5.5 0 0 1 0 1h-9a.5.5 0 0 1-.5-.5zm0-4a.5.5 0 0 1 .5-.5h9a.5.5 0 0 1 0 1h-9a.5.5 0 0 1-.5-.5zm0-4a.5.5 0 0 1 .5-.5h9a.5.5 0 0 1 0 1h-9a.5.5 0 0 1-.5-.5zM3.854 2.146a.5.5 0 0 1 0 .708l-1.5 1.5a.5.5 0 0 1-.708 0l-.5-.5a.5.5 0 1 1 .708-.708L2 3.293l1.146-1.147a.5.5 0 0 1 .708 0zm0 4a.5.5 0 0 1 0 .708l-1.5 1.5a.5.5 0 0 1-.708 0l-.5-.5a.5.5 0 1 1 .708-.708L2 7.293l1.146-1.147a.5.5 0 0 1 .708 0zm0 4a.5.5 0 0 1 0 .708l-1.5 1.5a.5.5 0 0 1-.708 0l-.5-.5a.5.5 0 0 1 .708-.708l.146.147 1.146-1.147a.5.5 0 0 1 .708 0z"
            }
            StepIcon::Gift => {
                "M3 2.5a2.5 2.5 0 0 1 5 0 2.5 2.5 0 0 1 5 0v.006c0 .07 0 .27-.038.494H15a1 1 0 0 1 1 1v2a1 1 0 0 1-1 1v7.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 1 14.5V7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h2.038A2.968 2.968 0 0 1 3 2.506V2.5zm1.068.5H7v-.5a1.5 1.5 0 1 0-3 0c0 .085.002.274.045.43a.522.522 0 0 0 .023.07zM9 3h2.932a.56.56 0 0 0 .023-.07c.043-.156.045-.345.045-.43a1.5 1.5 0 0 0-3 0V3zM1 4v2h6V4H1zm8 0v2h6V4H9zm5 3H9v8h4.5a.5.5 0 0 0 .5-.5V7zm-7 8V7H2v7.5a.5.5 0 0 0 .5.5H7z"
            }
        }
    }
}

/// One "How it works" card (icon, title, description).
#[component]
fn Step(icon: StepIcon, title: String, description: String) -> impl IntoView {
    view! {
        <div class="col-md-4 mb-3">
            <div class="card h-100">
                <div class="card-body text-center">
                    <div class="mb-3">
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="48"
                            height="48"
                            fill="currentColor"
                            class=icon.class()
                            viewBox="0 0 16 16"
                            aria-hidden="true"
                        >
                            <path fill-rule=icon.fill_rule() d=icon.path()></path>
                        </svg>
                    </div>
                    <h5 class="card-title">{title}</h5>
                    <p class="card-text">{description}</p>
                </div>
            </div>
        </div>
    }
}

/// One feature with the green check icon.
#[component]
fn Feature(text: String) -> impl IntoView {
    view! {
        <div class="col-md-6 col-lg-4 mb-3">
            <div class="d-flex align-items-center">
                <div class="me-3">
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        fill="currentColor"
                        class="bi bi-check-circle-fill text-success"
                        viewBox="0 0 16 16"
                        aria-hidden="true"
                    >
                        <path d="M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0zm-3.97-3.03a.75.75 0 0 0-1.08.022L7.477 9.417 5.384 7.323a.75.75 0 0 0-1.06 1.06L6.97 11.03a.75.75 0 0 0 1.079-.02l3.992-4.99a.75.75 0 0 0-.01-1.05z"></path>
                    </svg>
                </div>
                <div class="text-start">{text}</div>
            </div>
        </div>
    }
}
