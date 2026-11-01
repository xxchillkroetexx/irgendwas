//! Internationalisation.
//!
//! The catalogs in `rust/locales/{en,de}.json` are the PHP message files: a *flat* map of
//! dotted keys (`"auth.login.title"`) to strings with `{name}` placeholders. Keys stay flat
//! on purpose: some keys are both a leaf and a prefix (`auth.login` and `auth.login.title`),
//! which a nested loader (e.g. `leptos_i18n`) cannot represent without renaming keys.
//!
//! The catalogs are embedded with `include_str!`, so server and client resolve exactly the
//! same strings (no hydration mismatch). Lookup falls back to English, then to the key.
//!
//! Locale of a request: session value `locale`, then cookie `locale`, then `APP_LOCALE`
//! (resolved by the server middleware, see `server::i18n`). The server writes it into
//! `<html lang>`, and the client reads it back from there during hydration. The locale is
//! fixed for one page load; switching language is a full navigation.
//!
//! Usage in components:
//! ```ignore
//! let i18n = use_i18n();
//! view! { <h1>{i18n.t("home.title")}</h1> <p>{t!(i18n, "user.dashboard.welcome", name = user.name)}</p> }
//! ```

use std::collections::HashMap;
use std::sync::LazyLock;

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Session key (and cookie name) that stores the chosen locale.
pub const LOCALE_KEY: &str = "locale";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Locale {
    #[default]
    En,
    De,
}

impl Locale {
    pub const ALL: [Locale; 2] = [Locale::En, Locale::De];

    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::De => "de",
        }
    }

    /// Native display name, for the language switcher.
    pub fn native_name(self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::De => "Deutsch",
        }
    }

    /// Parses `en`, `de`, `de-DE`, `de_AT`, ... (case-insensitive).
    pub fn from_code(code: &str) -> Option<Self> {
        let primary = code
            .trim()
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Locale::En),
            "de" => Some(Locale::De),
            _ => None,
        }
    }

    fn catalog(self) -> &'static HashMap<String, String> {
        match self {
            Locale::En => &EN,
            Locale::De => &DE,
        }
    }
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// Picks the first valid locale of: session value, cookie value, configured default.
pub fn resolve_locale(session: Option<&str>, cookie: Option<&str>, default: Locale) -> Locale {
    session
        .and_then(Locale::from_code)
        .or_else(|| cookie.and_then(Locale::from_code))
        .unwrap_or(default)
}

fn parse_catalog(name: &str, json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).unwrap_or_else(|err| panic!("locales/{name}.json is invalid: {err}"))
}

static EN: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| parse_catalog("en", include_str!("../../locales/en.json")));
static DE: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| parse_catalog("de", include_str!("../../locales/de.json")));

/// Raw catalog of a locale (for tests and tooling).
pub fn catalog(locale: Locale) -> &'static HashMap<String, String> {
    locale.catalog()
}

/// Translates `key`; falls back to English, then to the key itself.
pub fn translate(locale: Locale, key: &str) -> String {
    locale
        .catalog()
        .get(key)
        .or_else(|| Locale::En.catalog().get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Translates `key` and replaces every `{name}` placeholder with its value.
pub fn translate_with<K, V>(locale: Locale, key: &str, params: &[(K, V)]) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut message = translate(locale, key);
    for (name, value) in params {
        message = message.replace(&format!("{{{}}}", name.as_ref()), value.as_ref());
    }
    message
}

/// Translator handed to components through context. Cheap to copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct I18n {
    locale: Locale,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        Self { locale }
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn t(&self, key: &str) -> String {
        translate(self.locale, key)
    }

    pub fn t_with<K: AsRef<str>, V: AsRef<str>>(&self, key: &str, params: &[(K, V)]) -> String {
        translate_with(self.locale, key, params)
    }
}

/// `t!(i18n, "key")` or `t!(i18n, "key", name = value, count = n)`.
#[macro_export]
macro_rules! t {
    ($i18n:expr, $key:expr) => {
        $i18n.t($key)
    };
    ($i18n:expr, $key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $i18n.t_with($key, &[$((stringify!($name), ($value).to_string())),+])
    };
}

/// Puts the translator for `locale` into the reactive context.
pub fn provide_i18n(locale: Locale) -> I18n {
    let i18n = I18n::new(locale);
    provide_context(i18n);
    i18n
}

/// The translator of the current page. Falls back to the request locale if no context exists.
pub fn use_i18n() -> I18n {
    use_context::<I18n>().unwrap_or_else(|| I18n::new(request_locale()))
}

/// Locale resolved by the server middleware for the current request (request extension).
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestLocale(pub Locale);

/// Locale of the current page load.
///
/// Server: the [`RequestLocale`] request extension, else `APP_LOCALE`.
/// Client: the `lang` attribute the server rendered on `<html>`.
pub fn request_locale() -> Locale {
    #[cfg(feature = "ssr")]
    {
        use_context::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<RequestLocale>().map(|l| l.0))
            .or_else(|| use_context::<crate::server::AppState>().map(|s| s.config.app_locale))
            .unwrap_or_default()
    }
    #[cfg(not(feature = "ssr"))]
    {
        crate::components::document_attribute("lang")
            .and_then(|lang| Locale::from_code(&lang))
            .unwrap_or_default()
    }
}
