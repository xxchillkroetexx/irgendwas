//! `/user/dashboard` (spec: account, "UI notes"): welcome, subtitle and the tips card. The
//! groups column (slice groups) and the assignments column (slice draw) join the row once
//! those slices exist; until then they are not rendered at all. The default landing page
//! after login.

use leptos::prelude::*;

use crate::auth::use_session_resource;
use crate::components::RequireUser;
use crate::i18n::use_i18n;
use crate::t;

#[component]
pub fn DashboardPage() -> impl IntoView {
    view! {
        <RequireUser>
            <Dashboard/>
        </RequireUser>
    }
}

#[component]
fn Dashboard() -> impl IntoView {
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
        </div>
    }
}
