//! `/user/dashboard` (spec: account, "UI notes"): welcome, subtitle, the tips card and the
//! caller's group cards. The default landing page
//! after login. Personal data: never cached (draw S4).

use leptos::prelude::*;

use crate::auth::use_session_resource;
use crate::components::RequireUser;
use crate::groups_api::{GroupError, GroupSummary, list_my_groups};
use crate::i18n::use_i18n;
use crate::pages::groups::{GroupErrorView, when_logged_in};
use crate::paths::group_path;
use crate::t;

#[component]
pub fn DashboardPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    // Page level (never inside the guard's async children).
    let groups: Resource<Result<Vec<GroupSummary>, GroupError>> =
        Resource::new(|| (), |_| when_logged_in(list_my_groups()));
    view! {
        <RequireUser>
            <Dashboard groups/>
        </RequireUser>
    }
}

#[component]
fn Dashboard(groups: Resource<Result<Vec<GroupSummary>, GroupError>>) -> impl IntoView {
    let i18n = use_i18n();
    let session = use_session_resource().0;

    view! {
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                let name = session
                    .await
                    .ok()
                    .and_then(|info| info.user)
                    .map(|user| user.name)
                    .unwrap_or_default();
                view! { <h1 class="h2 mb-1">{t!(i18n, "user.dashboard.welcome", name = name)}</h1> }
            })}
        </Transition>
        <p class="lead text-muted mb-4">{i18n.t("user.dashboard.subtitle")}</p>
        <div class="row">
            <div class="col-lg-4 mb-4">
                <div class="card h-100">
                    <div class="card-body">
                        <h2 class="h5 card-title">{i18n.t("user.dashboard.tips.title")}</h2>
                        <ul class="mb-0">
                            <li>{i18n.t("user.dashboard.tips.createWishlist")}</li>
                            <li>{i18n.t("user.dashboard.tips.inviteFriends")}</li>
                            <li>{i18n.t("user.dashboard.tips.setExclusions")}</li>
                        </ul>
                    </div>
                </div>
            </div>
            <div class="col-lg-8 mb-4">
                <Transition fallback=|| ()>
                    {move || Suspend::new(async move {
                        match groups.await {
                            Ok(list) if list.is_empty() => view! {
                                <section data-testid="dashboard-groups">
                                    <h2 class="h4">{i18n.t("group.title")}</h2>
                                    <p data-testid="dashboard-groups-empty">{i18n.t("group.emptyMessage")}</p>
                                </section>
                            }.into_any(),
                            Ok(list) => view! {
                                <section data-testid="dashboard-groups">
                                    <h2 class="h4">{i18n.t("group.title")}</h2>
                                    <div class="row">
                                        {list.into_iter().map(|group| view! { <DashboardGroupCard group/> }).collect_view()}
                                    </div>
                                </section>
                            }.into_any(),
                            Err(error) => view! { <GroupErrorView error/> }.into_any(),
                        }
                    })}
                </Transition>
            </div>
        </div>
    }
}

#[component]
fn DashboardGroupCard(group: GroupSummary) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="col-md-6 mb-3" data-testid="dashboard-group">
            <div class="card h-100">
                <div class="card-body">
                    <h3 class="h5 card-title"><a href=group_path(group.id)>{group.name}</a></h3>
                    <div class="d-flex flex-wrap gap-1">
                        {group.is_admin.then(|| view! { <span class="badge bg-primary">{i18n.t("group.admin")}</span> })}
                        <span class=group.status.badge_class()>{i18n.t(group.status.label_key())}</span>
                    </div>
                </div>
            </div>
        </div>
    }
}
