//! Root component and HTML shell.

use leptos::prelude::*;
use leptos_meta::{MetaTags, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::components::Layout;
use crate::i18n::{provide_i18n, request_locale};
use crate::pages::SharedWishPage;
use crate::pages::auth::{
    ForgotPasswordPage, LoginPage, RegisterConfirmPage, RegisterPage, ResetPasswordPage,
};
use crate::pages::exclusions::ExclusionsManagePage;
use crate::pages::groups::{
    GroupCreatePage, GroupEditPage, GroupPage, GroupsPage, JoinPage, JoinPreviewPage,
};
use crate::pages::wishlists::{WishlistEditPage, WishlistViewPage};
use crate::pages::{DashboardPage, ErrorPage, HomePage, NotFound, ProfilePage, VerifyEmailPage};
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
                <link rel="manifest" href="/manifest.webmanifest"/>
                <link rel="apple-touch-icon" sizes="180x180" href="/assets/icons/apple-touch-icon.png"/>
                <meta name="theme-color" content="#dc3545"/>
                <meta name="apple-mobile-web-app-capable" content="yes"/>
                <meta name="apple-mobile-web-app-status-bar-style" content="default"/>
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
    let i18n = provide_i18n(request_locale(), use_timezone());

    view! {
        <Title text=i18n.t("app.name")/>
        <Router>
            <Layout>
                <ErrorBoundary fallback=|errors| view! { <ErrorPage errors/> }>
                    // `Async`: the whole page is rendered before the response starts. Out-of-order
                    // streaming would leave every pending Suspense (navbar, flash messages,
                    // guarded pages) in `<template>` chunks that only JavaScript swaps in, and
                    // the pages must work without JavaScript (account AC41, AC44).
                    <Routes fallback=|| view! { <NotFound/> }>
                        <Route path=path!("/") view=HomePage ssr=SsrMode::Async/>
                        <Route path=path!("/wishes/shared") view=SharedWishPage ssr=SsrMode::Async/>
                        <Route path=path!("/install") view=crate::pages::InstallPage ssr=SsrMode::Async/>
                        <Route path=path!("/auth/login") view=LoginPage ssr=SsrMode::Async/>
                        <Route path=path!("/auth/register") view=RegisterPage ssr=SsrMode::Async/>
                        <Route path=path!("/auth/register/confirm/:token") view=RegisterConfirmPage ssr=SsrMode::Async/>
                        <Route path=path!("/auth/forgot-password") view=ForgotPasswordPage ssr=SsrMode::Async/>
                        <Route path=path!("/auth/reset-password/:token") view=ResetPasswordPage ssr=SsrMode::Async/>
                        <Route path=path!("/user/dashboard") view=DashboardPage ssr=SsrMode::Async/>
                        <Route path=path!("/wishlists") view=crate::pages::wishlist::WishlistsPage ssr=SsrMode::Async/>
                        <Route path=path!("/groups/:id/wishlist") view=crate::pages::wishlist::WishlistPage ssr=SsrMode::Async/>
                        <Route path=path!("/user/profile") view=ProfilePage ssr=SsrMode::Async/>
                        <Route path=path!("/user/verify-email/:token") view=VerifyEmailPage ssr=SsrMode::Async/>
                        <Route path=path!("/groups") view=GroupsPage ssr=SsrMode::Async/>
                        <Route path=path!("/groups/new") view=GroupCreatePage ssr=SsrMode::Async/>
                        <Route path=path!("/groups/:id") view=GroupPage ssr=SsrMode::Async/>
                        <Route path=path!("/groups/:id/edit") view=GroupEditPage ssr=SsrMode::Async/>
                        <Route path=path!("/groups/:id/wishlist") view=WishlistEditPage ssr=SsrMode::Async/>
                        <Route path=path!("/groups/:id/exclusions") view=ExclusionsManagePage ssr=SsrMode::Async/>
                        <Route path=path!("/groups/:id/wishlist/:user_id") view=WishlistViewPage ssr=SsrMode::Async/>
                        <Route path=path!("/join") view=JoinPage ssr=SsrMode::Async/>
                        <Route path=path!("/join/:code") view=JoinPreviewPage ssr=SsrMode::Async/>
                    </Routes>
                </ErrorBoundary>
            </Layout>
        </Router>
    }
}
