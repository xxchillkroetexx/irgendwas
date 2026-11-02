//! Page chrome: navbar, flash area, footer (port of `src/Views/layouts/main.php`).

use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::auth::{CurrentUser, SessionResource, get_session_info};
use crate::auth_api::Logout;
use crate::components::FlashMessages;
use crate::components::guard::full_navigation;
use crate::flash::{FlashMessage, Toasts, use_toasts};
use crate::i18n::{Locale, use_i18n};

/// Wraps every page. Owns the session resource (current user + flash messages), which is
/// re-fetched on every navigation so that redirects after server fns show fresh state.
#[component]
pub fn Layout(children: Children) -> impl IntoView {
    let location = use_location();
    // Blocking: the server waits for it before sending the response head, so taking the
    // flash messages out of the session is persisted with that response.
    // The pages only need the user's id and name: the email address stays out of the
    // serialized page data (a page shows it only where it is content, e.g. the profile).
    let session = Resource::new_blocking(
        move || (location.pathname.get(), location.search.get()),
        |_| async move {
            get_session_info().await.map(|mut info| {
                if let Some(user) = info.user.as_mut() {
                    user.email.clear();
                }
                info
            })
        },
    );
    provide_context(SessionResource(session));
    provide_context(Toasts(RwSignal::new(Vec::new())));

    view! {
        <header>
            <NavBar/>
        </header>
        <main class="container my-4">
            <FlashMessages/>
            {children()}
        </main>
        <Footer/>
    }
}

#[component]
pub fn NavBar() -> impl IntoView {
    let i18n = use_i18n();
    let SessionResource(session) = expect_context::<SessionResource>();
    // Mobile menu, driven by Leptos instead of Bootstrap's collapse JS (the e2e smoke test
    // uses it to prove hydration). Closed on the server and on every navigation.
    let menu_open = RwSignal::new(false);
    let location = use_location();
    Effect::new(move |_| {
        location.pathname.track();
        menu_open.set(false);
    });

    view! {
        <nav class="navbar navbar-expand-lg navbar-dark bg-primary">
            <div class="container">
                <a class="navbar-brand" href="/">{i18n.t("app.name")}</a>
                <button
                    class="navbar-toggler"
                    type="button"
                    aria-controls="navbarNav"
                    aria-expanded=move || if menu_open.get() { "true" } else { "false" }
                    aria-label=i18n.t("nav.toggle")
                    on:click=move |_| menu_open.update(|open| *open = !*open)
                >
                    <span class="navbar-toggler-icon"></span>
                </button>
                <div class="collapse navbar-collapse" class:show=move || menu_open.get() id="navbarNav">
                    <Transition fallback=|| ()>
                        {move || Suspend::new(async move {
                            let user = session.await.ok().and_then(|info| info.user);
                            view! { <NavLinks user/> }
                        })}
                    </Transition>
                </div>
            </div>
        </nav>
    }
}

#[component]
fn NavLinks(user: Option<CurrentUser>) -> impl IntoView {
    let i18n = use_i18n();
    let locale = i18n.locale();

    let main_links = user.is_some().then(|| {
        view! {
            <li class="nav-item">
                <a class="nav-link" href="/user/dashboard">{i18n.t("nav.dashboard")}</a>
            </li>
            <li class="nav-item">
                <a class="nav-link" href="/groups">{i18n.t("nav.groups")}</a>
            </li>
        }
    });

    let account_links = match user {
        Some(user) => view! {
            <li class="nav-item dropdown">
                <a
                    class="nav-link dropdown-toggle"
                    href="#"
                    id="userDropdown"
                    role="button"
                    data-bs-toggle="dropdown"
                    aria-expanded="false"
                >
                    {user.name}
                </a>
                <ul class="dropdown-menu dropdown-menu-end" aria-labelledby="userDropdown">
                    <li>
                        <a class="dropdown-item" href="/user/profile">{i18n.t("nav.profile")}</a>
                    </li>
                    <li>
                        <hr class="dropdown-divider"/>
                    </li>
                    <li>
                        <LogoutButton/>
                    </li>
                </ul>
            </li>
        }
        .into_any(),
        None => view! {
            <li class="nav-item">
                <a class="nav-link" href="/auth/login">{i18n.t("auth.login")}</a>
            </li>
            <li class="nav-item">
                <a class="nav-link" href="/auth/register">{i18n.t("auth.register")}</a>
            </li>
        }
        .into_any(),
    };

    let languages = Locale::ALL
        .into_iter()
        .map(|l| {
            view! {
                <li>
                    // Full page load: the locale is fixed per page load.
                    <a class="dropdown-item" href=format!("/language/{}", l.code()) rel="external">
                        {l.native_name()}
                    </a>
                </li>
            }
        })
        .collect_view();

    view! {
        <ul class="navbar-nav me-auto">{main_links}</ul>
        <ul class="navbar-nav">
            {account_links}
            <li class="nav-item dropdown">
                <a
                    class="nav-link dropdown-toggle"
                    href="#"
                    id="languageDropdown"
                    role="button"
                    data-bs-toggle="dropdown"
                    aria-expanded="false"
                >
                    {locale.code().to_uppercase()}
                </a>
                <ul class="dropdown-menu dropdown-menu-end" aria-labelledby="languageDropdown">
                    {languages}
                </ul>
            </li>
        </ul>
    }
}

/// Logout is a POST (the `logout` server fn). Afterwards the start page is loaded as a new
/// document, so every piece of per-user state is gone.
#[component]
fn LogoutButton() -> impl IntoView {
    let i18n = use_i18n();
    let action = ServerAction::<Logout>::new();
    let toasts = use_toasts();
    Effect::new(move |_| match action.value().get() {
        Some(Ok(())) => full_navigation("/"),
        Some(Err(err)) => toasts.push(FlashMessage::error(err.message_key())),
        None => {}
    });
    view! {
        <ActionForm action>
            <button type="submit" class="dropdown-item">{i18n.t("auth.logout")}</button>
        </ActionForm>
    }
}

#[component]
pub fn Footer() -> impl IntoView {
    let i18n = use_i18n();
    // No year here: rendering the current date would differ between server and client.
    view! {
        <footer class="mt-auto py-3 bg-light">
            <div class="container text-center">
                <p class="mb-0">"© " {i18n.t("footer.copyright")}</p>
            </div>
        </footer>
    }
}
