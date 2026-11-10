//! i18n-polish: the landing page (AC13 to AC15) and the catalog texts (AC16).
//! Docs: docs/specs/i18n-polish.md. The real-tree key scan itself is
//! `i18n_coverage::ac16_real_tree_has_no_findings`; this file adds the spec's table.

use std::collections::BTreeMap;

use crate::common::auth::*;
use crate::common::groups::{de, en};
use app::test_support::{ALICE_PASSWORD, create_alice};
use axum::http::StatusCode;
use sqlx::MySqlPool;

const ALICE: &str = "alice@example.com";

fn app(pool: &MySqlPool) -> axum::Router {
    router(pool, &[("APP_LOCALE", "en")], Opts::default())
}

/// Asserts that every text occurs in `html`, in the given order. Returns the position of the last.
fn assert_in_order(html: &str, texts: &[String]) {
    let mut from = 0;
    for text in texts {
        let hit = [
            text.clone(),
            text.replace('&', "&amp;").replace('\'', "&#x27;"),
            text.replace('\'', "&#39;"),
        ]
        .iter()
        .filter_map(|t| html[from..].find(t.as_str()))
        .min();
        match hit {
            Some(at) => from += at + 1,
            None => panic!("{text:?} missing or out of order (searched from byte {from})"),
        }
    }
}

fn anchor_text<'a>(html: &'a str, href: &str) -> Vec<&'a str> {
    let needle = format!("href=\"{href}\"");
    html.match_indices(&needle)
        .filter_map(|(at, _)| {
            let rest = &html[at..];
            let open = rest.find('>')?;
            let close = rest.find("</a>")?;
            Some(rest[open + 1..close].trim())
        })
        .collect()
}

fn landing_texts(t: fn(&str) -> String) -> Vec<String> {
    let mut v: Vec<String> = [
        "home.title",
        "home.subtitle",
        "home.login.title",
        "home.login.description",
        "home.register.title",
        "home.register.description",
        "home.join.text",
        "home.howItWorks.title",
        "home.howItWorks.step1.title",
        "home.howItWorks.step1.description",
        "home.howItWorks.step2.title",
        "home.howItWorks.step2.description",
        "home.howItWorks.step3.title",
        "home.howItWorks.step3.description",
        "home.features.title",
        "home.features.multipleGroups",
        "home.features.emailNotifications",
        "home.features.exclusionRules",
        "home.features.prioritizedWishlists",
        "home.features.secretAssignments",
    ]
    .iter()
    .map(|k| t(k))
    .collect();
    v.shrink_to_fit();
    v
}

// --------------------------------------------------------------------- AC13

#[sqlx::test(migrations = "../migrations")]
async fn ac13_the_landing_page_shows_everything_in_order_in_english(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let page = c.get("/").await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(lang_of(&page.body), "en");
    assert_in_order(&page.body, &landing_texts(en));
    assert!(page.body.contains("3. Draw Names"));
    assert!(page.body.contains("Got an invitation code?"));
    assert!(
        page.body
            .contains("Assignments stay secret, even from the admin")
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_the_landing_page_shows_everything_in_order_in_german(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app).with_cookie("locale", "de");
    let page = c.get("/").await;
    assert_eq!(lang_of(&page.body), "de");
    assert_in_order(&page.body, &landing_texts(de));
    assert!(page.body.contains("3. Wichtel ziehen"));
    assert!(page.body.contains("Sie haben einen Einladungscode?"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_links_and_icons(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let html = c.get("/").await.body;

    // Cards: login/register buttons (the nav has links to the same pages as well).
    assert!(
        anchor_text(&html, "/auth/login")
            .iter()
            .any(|t| *t == en("auth.login"))
    );
    assert!(
        anchor_text(&html, "/auth/register")
            .iter()
            .any(|t| *t == en("auth.register"))
    );
    // The invitation code line links to /join with the group.joinButton text.
    let join = anchor_text(&html, "/join");
    assert_eq!(
        join,
        [en("group.joinButton")],
        "exactly one /join link, labelled group.joinButton"
    );

    // Three step icons plus five feature check icons: markup only.
    assert!(
        html.matches("<svg").count() >= 8,
        "svg icons: {}",
        html.matches("<svg").count()
    );
    // Responsive features grid.
    assert!(html.contains("col-md-6 col-lg-4"));
    // Three "how it works" cards and five features, by their texts appearing once each.
    for key in [
        "home.howItWorks.step1.title",
        "home.howItWorks.step2.title",
        "home.howItWorks.step3.title",
        "home.features.multipleGroups",
        "home.features.emailNotifications",
        "home.features.exclusionRules",
        "home.features.prioritizedWishlists",
        "home.features.secretAssignments",
    ] {
        assert_eq!(
            html.matches(&en(key)).count(),
            1,
            "{key} appears exactly once"
        );
    }
    // The old dropdown is gone from the page.
    assert!(!html.contains("languageDropdown") && !html.contains("/language/"));
    // The footer is unchanged: no year.
    assert!(html.contains("©"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_the_page_is_fully_rendered_without_streaming_placeholders(pool: MySqlPool) {
    // SsrMode::Async: everything is in the first response, no out-of-order chunks (no-JS).
    let app = app(&pool);
    let mut c = Client::new(&app);
    let html = c.get("/").await.body;
    assert!(
        !html.contains("<template id=\"") && !html.contains("<!--pending"),
        "streamed chunks"
    );
    assert!(html.contains(&en("home.features.secretAssignments")));
    assert!(
        html.contains(&en("auth.login")),
        "nav links are in the first response"
    );
}

// --------------------------------------------------------------------- AC14

#[sqlx::test(migrations = "../migrations")]
async fn ac14_the_join_link_goes_through_login_and_back_to_join(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    let anon = c.get("/join").await;
    assert!(anon.status.is_redirection(), "got {}", anon.status);
    let location = anon.location().expect("location");
    assert_eq!(location, "/auth/login?next=%2Fjoin");

    let login = c.login_next(ALICE, ALICE_PASSWORD, "/join").await;
    login.ok();
    assert_eq!(login.location().as_deref(), Some("/join"));
    assert_eq!(c.get("/join").await.status, StatusCode::OK);
}

// --------------------------------------------------------------------- AC15

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_logged_in_visitor_is_redirected_from_the_landing_page(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    let res = c.get("/").await;
    assert_eq!(res.status, StatusCode::FOUND);
    assert_eq!(res.location().as_deref(), Some("/user/dashboard"));
}

#[sqlx::test(migrations = "../migrations")]
async fn brand_link_targets_home_or_dashboard_for_the_current_session(pool: MySqlPool) {
    fn assert_brand_href(html: &str, href: &str) {
        let class = html.find("class=\"navbar-brand\"").expect("brand link");
        let start = html[..class].rfind("<a ").expect("brand anchor");
        let end = class + html[class..].find('>').expect("end of brand anchor");
        let anchor = &html[start..end];
        assert!(anchor.contains(&format!("href=\"{href}\"")), "{anchor}");
    }

    create_alice(&pool, ALICE).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    let anonymous = c.get("/").await;
    assert_brand_href(&anonymous.body, "/");

    c.login(ALICE, ALICE_PASSWORD).await.ok();
    let authenticated = c.get("/groups").await;
    assert_eq!(authenticated.status, StatusCode::OK);
    assert_brand_href(&authenticated.body, "/user/dashboard");
}

// --------------------------------------------------------------------- AC16

/// (key, en, de) of the spec's tables.
const SPEC_TEXTS: &[(&str, &str, &str)] = &[
    (
        "language.switchTo",
        "Switch language to {language}",
        "Sprache wechseln zu {language}",
    ),
    (
        "validation.locale_unsupported",
        "This language is not supported.",
        "Diese Sprache wird nicht unterstützt.",
    ),
    (
        "home.join.text",
        "Got an invitation code?",
        "Sie haben einen Einladungscode?",
    ),
    (
        "home.features.secretAssignments",
        "Assignments stay secret, even from the admin",
        "Zuweisungen bleiben geheim, auch vor dem Admin",
    ),
    (
        "home.howItWorks.step1.description",
        "Create a group and share its invitation link with family and friends.",
        "Erstellen Sie eine Gruppe und teilen Sie den Einladungslink mit Familie und Freunden.",
    ),
    (
        "home.howItWorks.step3.title",
        "3. Draw Names",
        "3. Wichtel ziehen",
    ),
    (
        "home.howItWorks.step3.description",
        "When everyone has joined, the admin starts the draw. Everyone gets their recipient by email, and exclusion rules are respected.",
        "Sobald alle beigetreten sind, startet der Admin die Ziehung. Jeder erhält seinen Wichtelpartner per E-Mail, und Ausschlussregeln werden berücksichtigt.",
    ),
];

/// Copied unchanged from the PHP catalogs: they must exist (and be non-empty) in both.
const COPIED_KEYS: &[&str] = &[
    "home.howItWorks.title",
    "home.howItWorks.step1.title",
    "home.howItWorks.step2.title",
    "home.howItWorks.step2.description",
    "home.features.title",
    "home.features.multipleGroups",
    "home.features.emailNotifications",
    "home.features.exclusionRules",
    "home.features.prioritizedWishlists",
    "group.joinButton",
];

fn catalog(name: &str) -> BTreeMap<String, String> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../locales/{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn ac16_the_spec_tables_exist_in_both_catalogs_with_the_given_texts() {
    let (en, de) = (catalog("en"), catalog("de"));
    for (key, en_text, de_text) in SPEC_TEXTS {
        assert_eq!(en.get(*key).map(String::as_str), Some(*en_text), "en {key}");
        assert_eq!(de.get(*key).map(String::as_str), Some(*de_text), "de {key}");
    }
    for key in COPIED_KEYS {
        for (name, cat) in [("en", &en), ("de", &de)] {
            assert!(
                cat.get(*key).is_some_and(|t| !t.trim().is_empty()),
                "{name} {key} missing"
            );
        }
    }
}

#[test]
fn ac16_the_php_texts_with_wrong_claims_are_not_carried_over() {
    let en = catalog("en");
    for key in [
        "home.howItWorks.step1.description",
        "home.howItWorks.step3.description",
    ] {
        let text = en.get(key).expect(key);
        assert!(!text.contains("randomly assigns"), "{key}: {text}");
        assert!(!text.contains("inviting participants"), "{key}: {text}");
    }
}

#[test]
fn ac16_the_catalogs_have_no_keys_for_the_removed_dropdown_or_unused_home_texts() {
    // `home.features.prioritizedWishlists` is now shown; nothing about the old label remains.
    let en = catalog("en");
    assert!(
        en.keys()
            .all(|k| !k.starts_with("language.") || k == "language.switchTo")
    );
}
