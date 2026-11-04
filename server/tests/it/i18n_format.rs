//! i18n-foundation: formatting, typed `t!` arguments, plurals, flash messages
//! (AC1 to AC8), date-input helpers (AC5).

use app::components::FlashAlert;
use app::flash::FlashMessage;
use app::format::{self, LocalDate, LocalDateTime};
use app::i18n::{Catalogs, I18n, Locale};
use app::tz::{self, TzError};
use app::{t, t_plural};
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::America::New_York;
use chrono_tz::Europe::Berlin;
use chrono_tz::Tz;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

/// Synthetic catalogs, leaked so `I18n` can hold them as `&'static`.
fn catalogs(en: &str, de: &str) -> &'static Catalogs {
    Box::leak(Box::new(
        Catalogs::from_json(en, de).expect("synthetic catalogs"),
    ))
}

fn i18n(locale: Locale, catalogs: &'static Catalogs) -> I18n {
    I18n::with_catalogs(locale, Berlin, catalogs)
}

// ---------------------------------------------------------------- formatting

#[test]
fn ac1_date_and_datetime_per_locale_in_berlin() {
    // (UTC instant, en date, en datetime, de date, de datetime)
    let table = [
        (
            "2026-12-01T10:00:00Z",
            "1 Dec 2026",
            "1 Dec 2026, 11:00",
            "01.12.2026",
            "01.12.2026 11:00",
        ),
        (
            "2026-07-05T08:05:00Z",
            "5 Jul 2026",
            "5 Jul 2026, 10:05",
            "05.07.2026",
            "05.07.2026 10:05",
        ),
        (
            "2026-12-31T23:30:00Z",
            "1 Jan 2027",
            "1 Jan 2027, 00:30",
            "01.01.2027",
            "01.01.2027 00:30",
        ),
        (
            "2026-12-10T22:59:59Z",
            "10 Dec 2026",
            "10 Dec 2026, 23:59",
            "10.12.2026",
            "10.12.2026 23:59",
        ),
    ];
    for (instant, en_date, en_datetime, de_date, de_datetime) in table {
        let dt = utc(instant);
        assert_eq!(format::date(Locale::En, Berlin, dt), en_date, "{instant}");
        assert_eq!(
            format::datetime(Locale::En, Berlin, dt),
            en_datetime,
            "{instant}"
        );
        assert_eq!(format::date(Locale::De, Berlin, dt), de_date, "{instant}");
        assert_eq!(
            format::datetime(Locale::De, Berlin, dt),
            de_datetime,
            "{instant}"
        );
    }
}

#[test]
fn ac2_i18n_formats_with_its_locale_and_tz() {
    let dt = utc("2026-12-01T10:00:00Z");
    assert_eq!(
        I18n::new(Locale::De, Berlin).datetime(dt),
        "01.12.2026 11:00"
    );
    assert_eq!(
        I18n::new(Locale::De, New_York).datetime(dt),
        "01.12.2026 05:00"
    );

    for locale in Locale::ALL {
        for tz in [Berlin, New_York, Tz::UTC] {
            let i18n = I18n::new(locale, tz);
            assert_eq!(i18n.locale(), locale);
            assert_eq!(i18n.tz(), tz);
            assert_eq!(i18n.date(dt), format::date(locale, tz, dt));
            assert_eq!(i18n.datetime(dt), format::datetime(locale, tz, dt));
            assert_eq!(i18n.number(1_234_567), format::number(locale, 1_234_567));
        }
    }
}

#[test]
fn ac3_number_groups_thousands() {
    let cases: [(i128, &str, &str); 5] = [
        (0, "0", "0"),
        (999, "999", "999"),
        (2000, "2,000", "2.000"),
        (1_234_567, "1,234,567", "1.234.567"),
        (-1234, "-1,234", "-1.234"),
    ];
    for (n, en, de) in cases {
        assert_eq!(format::number(Locale::En, n), en, "en {n}");
        assert_eq!(format::number(Locale::De, n), de, "de {n}");
    }
}

#[test]
fn ac4_t_macro_renders_typed_arguments() {
    let c = catalogs(
        r#"{"t.max": "Max {max}", "t.when": "On {date} at {time}", "t.hi": "Hi {name}"}"#,
        r#"{"t.max": "Höchstens {max}", "t.when": "Am {date} um {time}", "t.hi": "Hallo {name}"}"#,
    );
    let de = i18n(Locale::De, c);
    let en = i18n(Locale::En, c);
    let dt = utc("2026-12-01T10:00:00Z");

    assert_eq!(t!(de, "t.max", max = 2000_u32), "Höchstens 2.000");
    assert_eq!(t!(en, "t.max", max = 2000), "Max 2,000");
    assert_eq!(
        t!(de, "t.when", date = LocalDate(dt), time = LocalDateTime(dt)),
        "Am 01.12.2026 um 01.12.2026 11:00"
    );
    assert_eq!(
        t!(en, "t.when", date = LocalDate(dt), time = LocalDateTime(dt)),
        "On 1 Dec 2026 at 1 Dec 2026, 11:00"
    );

    // Strings are inserted unchanged (no grouping, no escaping).
    let owned = String::from("Ada & <Bob>");
    assert_eq!(t!(en, "t.hi", name = "Ada"), "Hi Ada");
    assert_eq!(t!(en, "t.hi", name = owned.clone()), "Hi Ada & <Bob>");
    assert_eq!(t!(de, "t.hi", name = &owned), "Hallo Ada & <Bob>");
}

#[test]
fn ac5_date_input_values() {
    assert_eq!(
        tz::to_date_input_value(utc("2026-12-10T22:59:59Z"), Berlin),
        "2026-12-10"
    );
    assert_eq!(
        tz::to_date_input_value(utc("2026-12-31T23:30:00Z"), Berlin),
        "2027-01-01"
    );

    let day = NaiveDate::from_ymd_opt(2026, 12, 10).unwrap();
    assert_eq!(tz::parse_date_input("2026-12-10"), Ok(day));
    assert_eq!(tz::parse_date_input(" 2026-12-10 "), Ok(day));
    for bad in [
        "",
        "10.12.2026",
        "2026-02-30",
        "2026-12-10T10:00",
        "2026-1-5x",
    ] {
        assert_eq!(
            tz::parse_date_input(bad),
            Err(TzError::Invalid),
            "{bad:?} must be rejected"
        );
    }
}

// ------------------------------------------------------------------- plurals

const ITEMS_EN: &str = r#"{"t.items.one": "{count} item", "t.items.other": "{count} items"}"#;
const ITEMS_DE: &str = r#"{"t.items.one": "{count} Artikel", "t.items.other": "{count} Artikel"}"#;

#[test]
fn ac6_t_plural_selects_one_or_other() {
    let c = catalogs(ITEMS_EN, ITEMS_DE);
    let en = i18n(Locale::En, c);
    let de = i18n(Locale::De, c);

    let cases: [(i64, &str); 5] = [
        (1, "1 item"),
        (0, "0 items"),
        (2, "2 items"),
        (-1, "-1 items"),
        (2000, "2,000 items"),
    ];
    for (n, expected) in cases {
        assert_eq!(t_plural!(en, "t.items", n, count = n), expected, "en {n}");
    }
    assert_eq!(t_plural!(de, "t.items", 1_i64, count = 1_i64), "1 Artikel");
    assert_eq!(
        t_plural!(de, "t.items", 2000_i64, count = 2000_i64),
        "2.000 Artikel"
    );
}

#[test]
fn ac7_t_plural_falls_back_to_english_then_to_the_key() {
    let c = catalogs(
        r#"{"t.x.one": "one thing", "t.x.other": "many things"}"#,
        "{}",
    );
    let de = i18n(Locale::De, c);
    let no_params: [(&str, &str); 0] = [];
    assert_eq!(de.t_plural("t.x", 1, &no_params), "one thing");
    assert_eq!(de.t_plural("t.x", 0, &no_params), "many things");
    assert_eq!(de.t_plural("t.x", 5, &no_params), "many things");

    assert_eq!(de.t_plural("t.none", 1, &no_params), "t.none.one");
    assert_eq!(de.t_plural("t.none", 0, &no_params), "t.none.other");
    assert_eq!(de.t_plural("t.none", 7, &no_params), "t.none.other");
}

// --------------------------------------------------------------------- flash

const FLASH_EN: &str = r#"{"t.flash": "{n} on {d} at {dt}", "t.hi": "{name}",
    "t.items.one": "{count} item", "t.items.other": "{count} items"}"#;
const FLASH_DE: &str = r#"{"t.flash": "{n} am {d} um {dt}", "t.hi": "{name}",
    "t.items.one": "{count} Artikel", "t.items.other": "{count} Artikel"}"#;

fn through_session(message: &FlashMessage) -> FlashMessage {
    let json = serde_json::to_string(message).expect("serialise flash");
    serde_json::from_str(&json).expect("deserialise flash")
}

#[test]
fn ac8_flash_renders_typed_args_in_the_readers_locale() {
    let c = catalogs(FLASH_EN, FLASH_DE);
    let t0 = utc("2026-12-01T10:00:00Z");
    let message = through_session(
        &FlashMessage::success("t.flash")
            .with_number("n", 2000)
            .with_date("d", t0)
            .with_datetime("dt", t0),
    );
    assert_eq!(
        message.text(&i18n(Locale::De, c)),
        "2.000 am 01.12.2026 um 01.12.2026 11:00"
    );
    assert_eq!(
        message.text(&i18n(Locale::En, c)),
        "2,000 on 1 Dec 2026 at 1 Dec 2026, 11:00"
    );
}

#[test]
fn ac8_flash_plural_picks_the_form() {
    let c = catalogs(FLASH_EN, FLASH_DE);
    let one = through_session(
        &FlashMessage::info("t.items")
            .with_plural(1)
            .with_number("count", 1),
    );
    assert_eq!(one.text(&i18n(Locale::En, c)), "1 item");
    assert_eq!(one.text(&i18n(Locale::De, c)), "1 Artikel");

    let many = through_session(
        &FlashMessage::info("t.items")
            .with_plural(3)
            .with_number("count", 3),
    );
    assert_eq!(many.text(&i18n(Locale::En, c)), "3 items");
}

#[test]
fn ac8_flash_text_param_is_literal_and_rendered_as_text() {
    let c = catalogs(FLASH_EN, FLASH_DE);
    let i18n = i18n(Locale::En, c);
    let message = through_session(&FlashMessage::warning("t.hi").with_param("name", "<b>x</b>"));
    assert_eq!(message.text(&i18n), "<b>x</b>");

    // `FlashAlert` escapes it: no <b> element reaches the HTML.
    let owner = Owner::new();
    let html = owner.with(|| {
        provide_context(i18n);
        view! { <FlashAlert message=message/> }.to_html()
    });
    assert!(
        html.contains("&lt;b&gt;x&lt;/b&gt;"),
        "text must be escaped: {html}"
    );
    assert!(!html.contains("<b>"), "no <b> element expected: {html}");
}
