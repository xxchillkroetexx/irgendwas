//! 404 and generic error pages. Both set the HTTP status during SSR.

use leptos::prelude::*;

use crate::error::AppError;
use crate::i18n::use_i18n;

/// Sets the HTTP status of the page being rendered (SSR only; a no-op on the client).
#[cfg(feature = "ssr")]
pub(crate) fn set_status(code: u16) {
    if let (Some(res), Ok(status)) = (
        use_context::<leptos_axum::ResponseOptions>(),
        axum::http::StatusCode::from_u16(code),
    ) {
        res.set_status(status);
    }
}

#[cfg(not(feature = "ssr"))]
pub(crate) fn set_status(_code: u16) {}

#[component]
pub fn NotFound() -> impl IntoView {
    set_status(404);
    let i18n = use_i18n();
    view! {
        <div class="text-center py-5">
            <h1 class="display-5">{i18n.t("error.notFound.title")}</h1>
            <p class="lead">{i18n.t("error.notFound.message")}</p>
            <a href="/" class="btn btn-primary">{i18n.t("error.backHome")}</a>
        </div>
    }
}

/// 403 page: logged in, but not allowed (e.g. a member on an admin-only page).
#[component]
pub fn Forbidden() -> impl IntoView {
    set_status(403);
    let i18n = use_i18n();
    view! {
        <div class="text-center py-5">
            <h1 class="display-5">{i18n.t("error.generic.title")}</h1>
            <p class="lead">{i18n.t("error.forbidden")}</p>
            <a href="/" class="btn btn-primary">{i18n.t("error.backHome")}</a>
        </div>
    }
}

/// 500 page for an unexpected server fn error (details are only logged on the server).
#[component]
pub fn ServerError() -> impl IntoView {
    set_status(500);
    let i18n = use_i18n();
    view! {
        <div class="text-center py-5">
            <h1 class="display-5">{i18n.t("error.generic.title")}</h1>
            <p class="lead">{i18n.t("error.generic.message")}</p>
            <a href="/" class="btn btn-primary">{i18n.t("error.backHome")}</a>
        </div>
    }
}

/// Fallback of the layout's `ErrorBoundary`. Shows a translated message for [`AppError`]s
/// and a generic one for anything else (details are never shown to users).
#[component]
pub fn ErrorPage(errors: ArcRwSignal<Errors>) -> impl IntoView {
    let i18n = use_i18n();
    let first: Option<AppError> = errors
        .get_untracked()
        .into_iter()
        .find_map(|(_, err)| err.downcast_ref::<AppError>().cloned());
    let (status, key) = match &first {
        Some(err) => (err.status_code(), err.message_key().to_string()),
        None => (500, "error.generic.message".to_string()),
    };
    set_status(status);
    let title_key = if status == 404 {
        "error.notFound.title"
    } else {
        "error.generic.title"
    };
    view! {
        <div class="text-center py-5">
            <h1 class="display-5">{i18n.t(title_key)}</h1>
            <p class="lead">{i18n.t(&key)}</p>
            <a href="/" class="btn btn-primary" rel="external">{i18n.t("error.backHome")}</a>
        </div>
    }
}
