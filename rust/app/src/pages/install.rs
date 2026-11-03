//! Platform installation instructions, available before and after login.

use leptos::prelude::*;

use crate::i18n::use_i18n;

#[component]
pub fn InstallPage() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <h1>{i18n.t("app.install.title")}</h1>
        <p>{i18n.t("app.install.intro")}</p>
        <h2>{i18n.t("app.install.iphone.title")}</h2>
        <p>{i18n.t("app.install.iphone.steps")}</p>
        <h2>{i18n.t("app.install.android.title")}</h2>
        <p>{i18n.t("app.install.android.steps")}</p>
        <h2>{i18n.t("app.install.desktop.title")}</h2>
        <p>{i18n.t("app.install.desktop.steps")}</p>
        <p>{i18n.t("app.install.help")}</p>
    }
}
