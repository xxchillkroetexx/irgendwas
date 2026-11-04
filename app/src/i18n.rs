//! Internationalisation.
//!
//! The catalogs in `locales/{en,de}.json` are a *flat* map of
//! dotted keys (`"auth.login.title"`) to strings with `{name}` placeholders. Keys stay flat
//! on purpose: some keys are both a leaf and a prefix (`auth.login` and `auth.login.title`),
//! which a nested loader (e.g. `leptos_i18n`) cannot represent without renaming keys.
//!
//! The catalogs are embedded with `include_str!`, so server and client resolve exactly the
//! same strings (no hydration mismatch). Lookup falls back to English, then to the key.
//!
//! Locale of a request: session value `locale`, then cookie `locale`, then
//! `Accept-Language`, then `APP_LOCALE` (resolved by the server middleware with
//! [`resolve_locale`]). The server writes it into
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
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// Picks the first valid locale of: session value, cookie value, `Accept-Language`, default.
///
/// Invalid values are skipped, not fatal.
pub fn resolve_locale(
    session: Option<&str>,
    cookie: Option<&str>,
    accept_language_header: Option<&str>,
    default: Locale,
) -> Locale {
    session
        .and_then(Locale::from_code)
        .or_else(|| cookie.and_then(Locale::from_code))
        .or_else(|| accept_language_header.and_then(accept_language))
        .unwrap_or(default)
}

/// First supported locale of an `Accept-Language` header value.
///
/// Items are `tag[;q=v]`; `q` defaults to 1, a malformed `q` or one outside `0..=1` drops
/// the item, `q=0` and `*` are ignored. Items are ordered by `q` (descending, stable for
/// ties) and the first tag [`Locale::from_code`] understands wins.
pub fn accept_language(header: &str) -> Option<Locale> {
    let mut items: Vec<(f64, &str)> = header
        .split(',')
        .filter_map(|item| {
            let mut parts = item.split(';').map(str::trim);
            let tag = parts.next()?;
            if tag.is_empty() || tag == "*" {
                return None;
            }
            let mut q = 1.0;
            for param in parts {
                let Some((name, value)) = param.split_once('=') else {
                    continue;
                };
                if name.trim().eq_ignore_ascii_case("q") {
                    q = value.trim().parse::<f64>().ok()?;
                }
            }
            if !(q > 0.0 && q <= 1.0) {
                return None;
            }
            Some((q, tag))
        })
        .collect();
    // `sort_by` is stable: equal weights keep header order.
    items.sort_by(|a, b| b.0.total_cmp(&a.0));
    items
        .into_iter()
        .find_map(|(_, tag)| Locale::from_code(tag))
}

type Catalog = HashMap<String, String>;

/// One flat catalog per locale: the embedded ones, or synthetic ones for tests.
#[derive(PartialEq, Eq)]
pub struct Catalogs {
    en: Catalog,
    de: Catalog,
}

impl std::fmt::Debug for Catalogs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalogs")
            .field("en", &self.en.len())
            .field("de", &self.de.len())
            .finish()
    }
}

static EMBEDDED: LazyLock<Catalogs> = LazyLock::new(|| {
    Catalogs::from_json(
        include_str!("../../locales/en.json"),
        include_str!("../../locales/de.json"),
    )
    .unwrap_or_else(|err| panic!("locales/{{en,de}}.json is invalid: {err}"))
});

impl Catalogs {
    /// The catalogs compiled into the binary (`locales/{en,de}.json`).
    pub fn embedded() -> &'static Catalogs {
        &EMBEDDED
    }

    pub fn from_json(en: &str, de: &str) -> Result<Catalogs, serde_json::Error> {
        Ok(Catalogs {
            en: serde_json::from_str(en)?,
            de: serde_json::from_str(de)?,
        })
    }

    pub fn catalog(&self, locale: Locale) -> &Catalog {
        match locale {
            Locale::En => &self.en,
            Locale::De => &self.de,
        }
    }

    /// `key` in `locale`, else in English.
    fn lookup(&self, locale: Locale, key: &str) -> Option<&str> {
        self.catalog(locale)
            .get(key)
            .or_else(|| self.en.get(key))
            .map(String::as_str)
    }

    /// `key` in `locale`, else in English, else the key itself.
    fn translate(&self, locale: Locale, key: &str) -> String {
        self.lookup(locale, key).unwrap_or(key).to_string()
    }
}

/// Replaces every `{name}` placeholder in `message`.
fn fill<K: AsRef<str>, V: AsRef<str>>(mut message: String, params: &[(K, V)]) -> String {
    for (name, value) in params {
        message = message.replace(&format!("{{{}}}", name.as_ref()), value.as_ref());
    }
    message
}

/// Raw embedded catalog of a locale (for tests and tooling).
pub fn catalog(locale: Locale) -> &'static HashMap<String, String> {
    Catalogs::embedded().catalog(locale)
}

/// Translates `key` with the embedded catalogs; falls back to English, then to the key.
pub fn translate(locale: Locale, key: &str) -> String {
    Catalogs::embedded().translate(locale, key)
}

/// Translates `key` and replaces every `{name}` placeholder with its value.
pub fn translate_with<K, V>(locale: Locale, key: &str, params: &[(K, V)]) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fill(translate(locale, key), params)
}

/// Translator handed to components through context: locale, time zone and catalogs of
/// the page. Cheap to copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct I18n {
    locale: Locale,
    tz: chrono_tz::Tz,
    catalogs: &'static Catalogs,
}

impl I18n {
    pub fn new(locale: Locale, tz: chrono_tz::Tz) -> Self {
        Self::with_catalogs(locale, tz, Catalogs::embedded())
    }

    /// Meant for tests: synthetic catalogs (e.g. `Box::leak(Box::new(Catalogs::from_json(..)))`).
    pub fn with_catalogs(locale: Locale, tz: chrono_tz::Tz, catalogs: &'static Catalogs) -> Self {
        Self {
            locale,
            tz,
            catalogs,
        }
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn tz(&self) -> chrono_tz::Tz {
        self.tz
    }

    /// Translates `key`: current locale, then English, then the key itself.
    pub fn t(&self, key: &str) -> String {
        self.catalogs.translate(self.locale, key)
    }

    /// [`I18n::t`] plus `{name}` placeholders.
    pub fn t_with<K: AsRef<str>, V: AsRef<str>>(&self, key: &str, params: &[(K, V)]) -> String {
        fill(self.t(key), params)
    }

    /// `<base>.one` if `n == 1`, else `<base>.other`, each with the usual fallback.
    /// `n` only selects the form; placeholders come from `params`.
    pub fn t_plural<K: AsRef<str>, V: AsRef<str>>(
        &self,
        base: &str,
        n: i64,
        params: &[(K, V)],
    ) -> String {
        let form = if n == 1 { "one" } else { "other" };
        self.t_with(&format!("{base}.{form}"), params)
    }

    /// [`crate::format::date`] with this locale and time zone.
    pub fn date(&self, dt: chrono::DateTime<chrono::Utc>) -> String {
        crate::format::date(self.locale, self.tz, dt)
    }

    /// [`crate::format::datetime`] with this locale and time zone.
    pub fn datetime(&self, dt: chrono::DateTime<chrono::Utc>) -> String {
        crate::format::datetime(self.locale, self.tz, dt)
    }

    /// [`crate::format::number`] with this locale.
    pub fn number(&self, n: i128) -> String {
        crate::format::number(self.locale, n)
    }
}

/// `t!(i18n, "key")` or `t!(i18n, "key", name = value, count = n)`.
#[macro_export]
macro_rules! t {
    ($i18n:expr, $key:expr) => {
        $i18n.t($key)
    };
    ($i18n:expr, $key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $i18n.t_with(
            $key,
            &[$((stringify!($name), $crate::format::MessageArg::render(&($value), &$i18n))),+],
        )
    };
}

/// `t_plural!(i18n, "base", n, count = n)`: `<base>.one` if `n == 1`, else `<base>.other`.
#[macro_export]
macro_rules! t_plural {
    ($i18n:expr, $base:expr, $n:expr) => {
        $i18n.t_plural::<&str, &str>($base, $n, &[])
    };
    ($i18n:expr, $base:expr, $n:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $i18n.t_plural(
            $base,
            $n,
            &[$((stringify!($name), $crate::format::MessageArg::render(&($value), &$i18n))),+],
        )
    };
}

/// Puts the translator for `locale` into the reactive context.
pub fn provide_i18n(locale: Locale, tz: chrono_tz::Tz) -> I18n {
    let i18n = I18n::new(locale, tz);
    provide_context(i18n);
    i18n
}

/// The translator of the current page. Falls back to the request locale if no context exists.
pub fn use_i18n() -> I18n {
    use_context::<I18n>().unwrap_or_else(|| I18n::new(request_locale(), crate::tz::use_timezone()))
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
