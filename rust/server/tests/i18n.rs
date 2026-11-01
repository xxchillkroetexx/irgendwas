//! Translation catalogs and lookup (`app::i18n`).

use app::i18n::{I18n, Locale, catalog, resolve_locale, translate, translate_with};
use app::t;

#[test]
fn catalogs_load_and_translate() {
    assert!(catalog(Locale::En).len() > 200);
    assert!(catalog(Locale::De).len() > 200);
    assert_eq!(translate(Locale::En, "auth.login"), "Login");
    assert_eq!(translate(Locale::De, "auth.login"), "Anmelden");
    // Keys that are both a leaf and a prefix coexist (flat keys).
    assert_eq!(translate(Locale::En, "auth.login.title"), "Login");
}

#[test]
fn placeholders_fallbacks_and_macro() {
    assert_eq!(
        translate_with(Locale::En, "user.dashboard.welcome", &[("name", "Ada")]),
        "Welcome, Ada!"
    );
    assert_eq!(
        translate_with(Locale::De, "group.show.members", &[("count", "3")]),
        "Gruppenmitglieder (3)"
    );
    // Unknown key → key itself.
    assert_eq!(translate(Locale::De, "no.such.key"), "no.such.key");

    let i18n = I18n::new(Locale::En);
    assert_eq!(
        t!(i18n, "group.show.members", count = 5),
        "Group Members (5)"
    );
    assert_eq!(t!(i18n, "nav.groups"), "My Groups");
}

#[test]
fn every_english_key_exists_in_german() {
    let de = catalog(Locale::De);
    let missing: Vec<_> = catalog(Locale::En)
        .keys()
        .filter(|k| !de.contains_key(*k))
        .collect();
    assert!(missing.is_empty(), "missing in de.json: {missing:?}");
}

#[test]
fn foundation_keys_exist_in_both_locales() {
    for key in [
        "nav.toggle",
        "common.close",
        "error.notFound.title",
        "error.notFound.message",
        "error.generic.title",
        "error.generic.message",
        "error.backHome",
        "error.unauthorized",
        "error.forbidden",
    ] {
        for locale in Locale::ALL {
            assert!(
                catalog(locale).contains_key(key),
                "{key} missing in {locale}"
            );
        }
    }
}

#[test]
fn locale_resolution() {
    assert_eq!(Locale::from_code("de-DE"), Some(Locale::De));
    assert_eq!(Locale::from_code("EN_us"), Some(Locale::En));
    assert_eq!(Locale::from_code("fr"), None);
    assert_eq!(
        resolve_locale(Some("de"), Some("en"), Locale::En),
        Locale::De
    );
    assert_eq!(
        resolve_locale(Some("xx"), Some("de"), Locale::En),
        Locale::De
    );
    assert_eq!(resolve_locale(None, None, Locale::De), Locale::De);
    assert_eq!(
        app::server::i18n::cookie_locale("a=1; locale=de; b=2"),
        Some(Locale::De)
    );
    assert_eq!(app::server::i18n::cookie_locale("xlocale=de"), None);
}
