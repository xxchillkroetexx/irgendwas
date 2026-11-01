//! `/user/dashboard`: placeholder until the account slice builds the real page. It is the
//! default landing page after login and the protected page the auth guard is tested on.

use leptos::prelude::*;

use crate::components::RequireUser;
use crate::i18n::use_i18n;

#[component]
pub fn DashboardPage() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <RequireUser>
            <h1 class="h3">{i18n.t("nav.dashboard")}</h1>
        </RequireUser>
    }
}
