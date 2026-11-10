//! Page guards (spec: auth, "Protected-page guard").
//!
//! Each guard is a *blocking* resource, so during SSR its decision is made before the
//! response head is sent: a redirect becomes a real `302` with `Location`. On the client
//! (navigation inside the app) the same decision navigates with the router.
//!
//! ```ignore
//! view! { <RequireUser>{|| view! { <h1>"..."</h1> }}</RequireUser> }
//! ```

use leptos::prelude::*;
use leptos_router::hooks::use_location;
use serde::{Deserialize, Serialize};

use crate::auth::get_current_user;
use crate::error::AppError;
use crate::i18n::use_i18n;
use crate::paths::login_url;

/// Navigates on the client. Renders nothing; on the server it does nothing (the SSR
/// response already redirected).
#[component]
pub fn ClientRedirect(#[prop(into)] path: String) -> impl IntoView {
    #[cfg(not(feature = "ssr"))]
    {
        let navigate = leptos_router::hooks::use_navigate();
        navigate(
            &path,
            leptos_router::NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    }
    #[cfg(feature = "ssr")]
    let _ = path;
}

/// Loads `path` as a new document (needed when the page language may change, which is
/// fixed per page load). Client only; a no-op on the server.
pub fn full_navigation(path: &str) {
    #[cfg(feature = "hydrate")]
    {
        if let Err(err) = window().location().set_href(path) {
            leptos::logging::error!("navigation failed: {err:?}");
        }
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = path;
}

/// Path and query of the current location (`/a/b?x=1`), the same on server and client.
pub fn current_path_and_query() -> Memo<String> {
    let location = use_location();
    Memo::new(move |_| {
        let path = location.pathname.get();
        let search = location.search.get();
        if search.is_empty() {
            path
        } else {
            format!("{path}?{search}")
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Access {
    Allowed,
    Redirect(String),
    Failed,
}

/// Visible feedback while a protected page checks the current login.
#[component]
pub fn PageLoading() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="d-flex align-items-center gap-2 py-4 text-muted" role="status">
            <span class="spinner-border spinner-border-sm" aria-hidden="true"></span>
            <span>{i18n.t("common.loading")}</span>
        </div>
    }
}

/// Redirects during SSR (302) when `redirect` is given.
fn apply_redirect(redirect: Option<&str>) {
    #[cfg(feature = "ssr")]
    if let Some(path) = redirect {
        crate::server::respond::redirect_page(path);
    }
    #[cfg(not(feature = "ssr"))]
    let _ = redirect;
}

/// Shows `children` only to a logged-in user; anonymous visitors are sent to
/// `/auth/login?next=<this page>`.
#[component]
pub fn RequireUser(children: ChildrenFn) -> impl IntoView {
    let here = current_path_and_query();
    let location = use_location();
    let access = Resource::new_blocking(
        // Query-only navigation is still the same protected page. Re-check on a
        // path change, but keep its live query for the login return URL below.
        move || location.pathname.get(),
        move |_| async move {
            let access = match get_current_user().await {
                Ok(Some(_)) => Access::Allowed,
                Ok(None) | Err(AppError::Unauthorized) => match here.try_get_untracked() {
                    Some(here) => Access::Redirect(login_url(&here)),
                    // The page was removed while its access request was in flight.
                    None => Access::Failed,
                },
                Err(_) => Access::Failed,
            };
            if let Access::Redirect(to) = &access {
                apply_redirect(Some(to));
            }
            access
        },
    );
    let i18n = use_i18n();
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=|| view! { <PageLoading/> }>
            {move || Suspend::new(async move {
                match access.await {
                    Access::Allowed => children.read_value()().into_any(),
                    Access::Redirect(to) => view! { <ClientRedirect path=to/> }.into_any(),
                    Access::Failed => {
                        view! {
                            <div class="alert alert-danger" role="alert">
                                {i18n.t("error.generic.message")}
                            </div>
                        }
                            .into_any()
                    }
                }
            })}
        </Suspense>
    }
}

/// For pages that make no sense when logged in (landing, login, register): shows
/// `children` only once the check found no logged-in user; a logged-in visitor is sent to
/// `to` without ever seeing them (on the client, a navigation to such a page shows
/// `PageLoading` while the check runs instead of flashing the logged-out content).
#[component]
pub fn RedirectIfLoggedIn(#[prop(into)] to: Signal<String>, children: ChildrenFn) -> impl IntoView {
    let location = use_location();
    let target = Resource::new_blocking(
        move || location.pathname.get(),
        move |_| async move {
            let logged_in = matches!(get_current_user().await, Ok(Some(_)));
            // Reading after await must tolerate route disposal. If still present,
            // use the latest target, including any query-only change meanwhile.
            let redirect = if logged_in {
                to.try_get_untracked()
            } else {
                None
            };
            apply_redirect(redirect.as_deref());
            redirect
        },
    );
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=|| view! { <PageLoading/> }>
            {move || Suspend::new(async move {
                match target.await {
                    Some(to) => view! { <ClientRedirect path=to/> }.into_any(),
                    None => children.read_value()().into_any(),
                }
            })}
        </Suspense>
    }
}
