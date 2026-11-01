//! Root component and HTML shell.

use leptos::prelude::*;
use leptos_meta::{MetaTags, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::components::Layout;
use crate::i18n::{provide_i18n, request_locale};
use crate::pages::{ErrorPage, HomePage, NotFound};
use crate::tz::use_timezone;

const BOOTSTRAP_CSS: &str =
    "https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/css/bootstrap.min.css";
const BOOTSTRAP_JS: &str =
    "https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/js/bootstrap.bundle.min.js";

/// Server-rendered document. `lang` and `data-timezone` carry the request locale and
/// `APP_TIMEZONE` to the client so hydration renders identical text.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    let locale = request_locale();
    let timezone = use_timezone();
    view! {
        <!DOCTYPE html>
        <html lang=locale.code() data-timezone=timezone.name()>
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <link rel="stylesheet" href=BOOTSTRAP_CSS/>
                <link rel="stylesheet" href="/assets/css/styles.css"/>
                <script defer src=BOOTSTRAP_JS></script>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let i18n = provide_i18n(request_locale());

    view! {
        <Title text=i18n.t("app.name")/>
        <Router>
            <Layout>
                <ErrorBoundary fallback=|errors| view! { <ErrorPage errors/> }>
                    <Routes fallback=|| view! { <NotFound/> }>
                        <Route path=path!("/") view=HomePage/>
                    </Routes>
                </ErrorBoundary>
            </Layout>
        </Router>
    }
}
