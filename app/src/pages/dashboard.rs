//! `/user/dashboard` (spec: account, "UI notes"; dashboard): welcome, subtitle, the tips
//! card and one card per active group of the caller. The default landing page after login.
//! Personal data: never cached (draw S4). No assignment data (dashboard AC3): a recipient is
//! shown only on the group page.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::use_session_resource;
use crate::components::RequireUser;
#[cfg(not(feature = "ssr"))]
use crate::groups_api::list_my_groups;
use crate::groups_api::{GroupError, GroupStatus, GroupSummary};
use crate::i18n::use_i18n;
use crate::pages::groups::{GroupBadges, when_logged_in};
use crate::paths::group_path;
use crate::t;

/// What a card shows of a group. The resource maps the summaries to this, so nothing else
/// (description, admin name) ends up in the hydration data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DashboardGroup {
    id: i32,
    name: String,
    is_admin: bool,
    status: GroupStatus,
}

impl From<GroupSummary> for DashboardGroup {
    fn from(group: GroupSummary) -> Self {
        DashboardGroup {
            id: group.id,
            name: group.name,
            is_admin: group.is_admin,
            status: group.status,
        }
    }
}

type GroupsResource = Resource<Result<Vec<DashboardGroup>, GroupError>>;

#[component]
pub fn DashboardPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    // Page level (never inside the guard's async children).
    let groups: GroupsResource = Resource::new(
        || (),
        |_| async move {
            when_logged_in(fetch_my_groups())
                .await
                .map(|list| list.into_iter().map(DashboardGroup::from).collect())
        },
    );
    view! {
        <RequireUser>
            <Dashboard groups/>
        </RequireUser>
    }
}

// During SSR the page calls the fn body directly: a failure leaves the group section out
// instead of setting the status of the whole page (the default page after login). In the
// browser it is the server fn.

#[cfg(feature = "ssr")]
async fn fetch_my_groups() -> Result<Vec<GroupSummary>, GroupError> {
    crate::server::groups_flow::list_my_groups().await
}

#[cfg(not(feature = "ssr"))]
async fn fetch_my_groups() -> Result<Vec<GroupSummary>, GroupError> {
    list_my_groups().await
}

#[component]
fn Dashboard(groups: GroupsResource) -> impl IntoView {
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
                        // An error (logged by the flow) leaves the section out.
                        groups.await.ok().map(|list| view! { <DashboardGroups list/> })
                    })}
                </Transition>
            </div>
        </div>
    }
}

/// The "My groups" section: the cards, or the empty state.
#[component]
fn DashboardGroups(list: Vec<DashboardGroup>) -> impl IntoView {
    let i18n = use_i18n();
    let content = if list.is_empty() {
        view! {
            <div data-testid="dashboard-groups-empty">
                <p class="mb-1">{i18n.t("group.emptyMessage")}</p>
                <p class="text-muted mb-0">{i18n.t("group.emptyMessageAction")}</p>
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class="row">
                {list.into_iter().map(|group| view! { <DashboardGroupCard group/> }).collect_view()}
            </div>
        }
        .into_any()
    };
    view! {
        <section data-testid="dashboard-groups">
            <h2 class="h4">{i18n.t("group.title")}</h2>
            {content}
        </section>
    }
}

#[component]
fn DashboardGroupCard(group: DashboardGroup) -> impl IntoView {
    view! {
        <div class="col-md-6 mb-3" data-testid="dashboard-group">
            <div class="card h-100">
                <div class="card-body">
                    <h3 class="h5 card-title"><a href=group_path(group.id)>{group.name}</a></h3>
                    <GroupBadges is_admin=group.is_admin status=group.status/>
                </div>
            </div>
        </div>
    }
}
