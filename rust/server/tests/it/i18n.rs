//! Translation catalogs, lookup and locale resolution (`app::i18n`), pure parts.
//! Formatting, plurals and flash: `i18n_format.rs`. HTTP and DB: `i18n_locale.rs`.
//! Key coverage of the catalogs: `i18n_coverage.rs`.

use app::i18n::{
    Catalogs, I18n, Locale, accept_language, catalog, resolve_locale, translate, translate_with,
};
use app::t;
use chrono_tz::Europe::Berlin;

fn synthetic(en: &str, de: &str) -> &'static Catalogs {
    Box::leak(Box::new(Catalogs::from_json(en, de).unwrap()))
}

#[test]
fn embedded_catalogs_translate_kept_keys() {
    assert!(!catalog(Locale::En).is_empty());
    assert!(!catalog(Locale::De).is_empty());
    assert_eq!(translate(Locale::En, "auth.login"), "Login");
    assert_eq!(translate(Locale::De, "auth.login"), "Anmelden");
    // Unknown key: the key itself.
    assert_eq!(translate(Locale::De, "no.such.key"), "no.such.key");

    let i18n = I18n::new(Locale::En, Berlin);
    assert_eq!(t!(i18n, "nav.groups"), "My Groups");
    assert_eq!(
        I18n::new(Locale::De, Berlin).t("nav.groups"),
        "Meine Gruppen"
    );
}

#[test]
fn placeholders_and_fallback_chain_with_synthetic_catalogs() {
    let c = synthetic(
        r#"{"t.hello": "Hello {name}", "t.only_en": "Only English"}"#,
        r#"{"t.hello": "Hallo {name}"}"#,
    );
    let de = I18n::with_catalogs(Locale::De, Berlin, c);
    let en = I18n::with_catalogs(Locale::En, Berlin, c);

    assert_eq!(en.t_with("t.hello", &[("name", "Ada")]), "Hello Ada");
    assert_eq!(de.t_with("t.hello", &[("name", "Ada")]), "Hallo Ada");
    // current locale -> English -> the key
    assert_eq!(de.t("t.only_en"), "Only English");
    assert_eq!(de.t("t.nowhere"), "t.nowhere");
    assert_eq!(t!(de, "t.hello", name = "Ada"), "Hallo Ada");

    // The free functions use the embedded catalogs.
    assert_eq!(
        translate_with(Locale::En, "no.such.key", &[("name", "x")]),
        "no.such.key"
    );
}

#[test]
fn a_key_can_be_both_a_leaf_and_a_prefix() {
    let c = synthetic(
        r#"{"t.a": "Leaf", "t.a.b": "Nested"}"#,
        r#"{"t.a": "Blatt", "t.a.b": "Verschachtelt"}"#,
    );
    let en = I18n::with_catalogs(Locale::En, Berlin, c);
    let de = I18n::with_catalogs(Locale::De, Berlin, c);
    assert_eq!(en.t("t.a"), "Leaf");
    assert_eq!(en.t("t.a.b"), "Nested");
    assert_eq!(de.t("t.a"), "Blatt");
    assert_eq!(de.t("t.a.b"), "Verschachtelt");
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
fn locale_codes_and_cookie_header() {
    assert_eq!(Locale::from_code("de-DE"), Some(Locale::De));
    assert_eq!(Locale::from_code("EN_us"), Some(Locale::En));
    assert_eq!(Locale::from_code("fr"), None);
    assert_eq!(
        app::server::i18n::cookie_locale("a=1; locale=de; b=2"),
        Some(Locale::De)
    );
    assert_eq!(app::server::i18n::cookie_locale("xlocale=de"), None);
}

#[test]
fn ac9_accept_language_parsing() {
    let cases: [(&str, Option<Locale>); 10] = [
        ("fr-CH, fr;q=0.9, de;q=0.8, en;q=0.7", Some(Locale::De)),
        ("en-US,en;q=0.9", Some(Locale::En)),
        ("fr", None),
        ("de;q=0, en;q=0.5", Some(Locale::En)),
        ("de;q=abc", None),
        ("de;q=2", None),
        ("*", None),
        ("", None),
        ("en;q=0.5, de", Some(Locale::De)),
        // stable for ties: the first of equal weight wins
        ("de-AT;q=0.8, en-GB;q=0.8", Some(Locale::De)),
    ];
    for (header, expected) in cases {
        assert_eq!(accept_language(header), expected, "{header:?}");
    }
}

#[test]
fn ac9_accept_language_edge_cases() {
    // whitespace around parts is ignored
    assert_eq!(
        accept_language(" de ; q=0.8 , en ; q=0.9 "),
        Some(Locale::En)
    );
    // a negative or malformed q drops only that item
    assert_eq!(accept_language("de;q=-1, en;q=0.2"), Some(Locale::En));
    assert_eq!(accept_language("de;q=, en"), Some(Locale::En));
    // ties keep header order
    assert_eq!(accept_language("en;q=0.5, de;q=0.5"), Some(Locale::En));
    assert_eq!(accept_language("de;q=0.5, en;q=0.5"), Some(Locale::De));
    // a higher q beats an earlier item
    assert_eq!(accept_language("en;q=0.1, de;q=0.9"), Some(Locale::De));
}

#[test]
fn ac10_resolve_locale_order() {
    use Locale::{De, En};
    // session > cookie > Accept-Language > default
    assert_eq!(resolve_locale(Some("de"), Some("en"), Some("en"), En), De);
    assert_eq!(resolve_locale(None, Some("de"), Some("en"), En), De);
    assert_eq!(resolve_locale(None, None, Some("de"), En), De);
    assert_eq!(resolve_locale(None, None, None, De), De);
    assert_eq!(resolve_locale(None, None, None, En), En);
    // Accept-Language beats the default even when they differ
    assert_eq!(resolve_locale(None, None, Some("en-US,en;q=0.9"), De), En);
    // invalid values are skipped at every step
    assert_eq!(resolve_locale(Some("xx"), Some("de"), None, En), De);
    assert_eq!(resolve_locale(Some("xx"), Some("yy"), Some("de"), En), De);
    assert_eq!(resolve_locale(None, Some("zz"), Some("fr"), De), De);
    assert_eq!(resolve_locale(Some(""), None, Some("fr, *"), En), En);
}
