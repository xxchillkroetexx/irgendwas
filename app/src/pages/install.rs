//! Platform installation instructions, available before and after login.

use leptos::prelude::*;

use crate::i18n::use_i18n;
use crate::t;

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
        <h2>{i18n.t("share.install.title")}</h2>
        <p>{i18n.t("share.install.support")}</p>
        <p>{t!(i18n, "share.iphone.help", link = i18n.t("share.title"))}</p>
        <a href="/wishes/shared">{i18n.t("share.title")}</a>
    }
}
