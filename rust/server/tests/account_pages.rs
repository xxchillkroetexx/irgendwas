//! account slice: the dashboard shell and the profile page as the server renders them
//! (AC6-AC9, AC39). Interactions (forms, nav refresh) are in the e2e spec.

mod common;

use app::account_api::ProfileView;
use app::i18n::{I18n, Locale, translate, translate_with};
use app::test_support::{
    ALICE_PASSWORD, TEST_PASSWORD, create_user, set_pending_email, set_pending_email_at, user_row,
};
use chrono::{TimeZone, Utc};
use common::account::*;
use common::auth::*;
use sqlx::MySqlPool;

const DASHBOARD: &str = "/user/dashboard";
const PROFILE: &str = "/user/profile";

fn berlin() -> chrono_tz::Tz {
    chrono_tz::Europe::Berlin
}

// -------------------------------------------------------------------------- AC6

#[sqlx::test(migrations = "../migrations")]
async fn ac6_dashboard_shows_the_escaped_name_subtitle_and_tips(pool: MySqlPool) {
    create_user(&pool, "<b>Ann</b>", "ann@example.com").await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login("ann@example.com", TEST_PASSWORD).await.ok();

    let page = c.get(DASHBOARD).await;
    assert_eq!(page.status, 200);
    let welcome = translate_with(
        Locale::En,
        "user.dashboard.welcome",
        &[("name", "<b>Ann</b>")],
    );
    assert_eq!(welcome, "Welcome, <b>Ann</b>!");
    assert!(
        html_contains(&page.body, &welcome),
        "heading shows the name as literal text"
    );
    assert!(
        !page.body.contains("<b>Ann</b>"),
        "the name must not become an element"
    );
    for key in [
        "user.dashboard.subtitle",
        "user.dashboard.tips.title",
        "user.dashboard.tips.createWishlist",
        "user.dashboard.tips.inviteFriends",
        "user.dashboard.tips.setExclusions",
    ] {
        assert!(html_contains(&page.body, &en(key)), "{key} is shown");
    }
}

#[test]
fn ac6_the_invite_tip_describes_the_real_feature() {
    assert_eq!(
        translate(Locale::En, "user.dashboard.tips.inviteFriends"),
        "Share your group's invitation code with friends and family"
    );
    assert_eq!(
        translate(Locale::De, "user.dashboard.tips.inviteFriends"),
        "Teilen Sie den Einladungscode Ihrer Gruppe mit Freunden und Familie"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_dashboard_has_no_placeholder_for_the_later_slices(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let page = w.c.get(DASHBOARD).await;
    assert_eq!(page.status, 200);
    // Until the groups and draw slices land, their columns are not rendered at all.
    for text in [
        "You aren't a member of any Secret Santa groups yet.",
        "You don't have any gift assignments yet.",
    ] {
        assert!(!html_contains(&page.body, text), "no placeholder: {text}");
    }
    assert!(!page.body.contains("user.dashboard."), "no raw keys");
}

// -------------------------------------------------------------------------- AC7

#[sqlx::test(migrations = "../migrations")]
async fn ac7_dashboard_follows_the_session_locale(pool: MySqlPool) {
    let w = alice_world_with(&pool, &[], Opts::default()).await;
    let mut german = Client::new(&w.app).with_cookie("locale", "de");
    german.login(&w.email, ALICE_PASSWORD).await.ok();

    let page = german.get(DASHBOARD).await;
    assert_eq!(page.status, 200);
    assert_eq!(lang_of(&page.body), "de");
    let welcome = translate_with(Locale::De, "user.dashboard.welcome", &[("name", "Alice")]);
    assert!(html_contains(&page.body, &welcome), "{welcome}");
    for key in [
        "user.dashboard.subtitle",
        "user.dashboard.tips.title",
        "user.dashboard.tips.createWishlist",
        "user.dashboard.tips.inviteFriends",
        "user.dashboard.tips.setExclusions",
    ] {
        let german_text = de(key);
        assert_ne!(german_text, en(key), "{key} is translated");
        assert!(html_contains(&page.body, &german_text), "{key} in German");
    }
    assert!(!page.body.contains(&en("user.dashboard.subtitle")));
}

// -------------------------------------------------------------------------- AC8

#[sqlx::test(migrations = "../migrations")]
async fn ac8_profile_shows_name_and_email_and_no_pending_box(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let page = w.c.get(PROFILE).await;
    assert_eq!(page.status, 200);
    assert!(page.body.contains("Alice"), "current name");
    assert!(page.body.contains(&w.email), "current email");
    assert!(
        !html_contains(&page.body, &en("user.profile.changeEmail.cancel")),
        "no cancel button without a pending change"
    );
    assert!(
        !page.body.contains("Pending verification"),
        "no pending box"
    );
    for key in [
        "user.profile.emailChangeNotice",
        "user.profile.changeEmail.title",
        "user.profile.changePassword.title",
        "user.profile.changePassword.otherSessionsNotice",
        "user.profile.backToDashboard",
    ] {
        assert!(html_contains(&page.body, &en(key)), "{key} is shown");
    }
    assert!(
        !page
            .body
            .contains(&en("user.profile.emailPendingVerification")),
        "template not rendered raw"
    );

    // get_profile agrees.
    let profile = get_profile(&mut w.c)
        .await
        .account_result::<ProfileView>()
        .unwrap();
    assert_eq!(
        profile,
        ProfileView {
            name: "Alice".into(),
            email: w.email.clone(),
            pending_email: None,
            pending_expires_at: None,
        }
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_profile_escapes_the_name(pool: MySqlPool) {
    create_user(&pool, "\"><script>alert(1)</script>", "xss@example.com").await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login("xss@example.com", TEST_PASSWORD).await.ok();
    let page = c.get(PROFILE).await;
    assert_eq!(page.status, 200);
    assert!(!page.body.contains("<script>alert(1)"), "name is escaped");
}

// -------------------------------------------------------------------------- AC9

#[sqlx::test(migrations = "../migrations")]
async fn ac9_profile_shows_a_live_pending_change(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "new@example.com", &tok('p'), 3600).await;
    let expires = user_row(&pool, w.id)
        .await
        .email_verification_expires
        .expect("expiry");

    let page = w.c.get(PROFILE).await;
    assert_eq!(page.status, 200);
    let pending = en_with(
        "user.profile.emailPendingVerification",
        &[("email", "new@example.com")],
    );
    assert!(html_contains(&page.body, &pending), "{pending}");
    let expiry = en_with(
        "user.profile.emailPendingExpires",
        &[("date", &I18n::new(Locale::En, berlin()).datetime(expires))],
    );
    assert!(html_contains(&page.body, &expiry), "{expiry}");
    assert!(
        html_contains(&page.body, &en("user.profile.changeEmail.cancel")),
        "cancel button"
    );
    assert!(
        page.body.contains(&w.email),
        "the current address is still shown"
    );

    let profile = get_profile(&mut w.c)
        .await
        .account_result::<ProfileView>()
        .unwrap();
    assert_eq!(profile.pending_email.as_deref(), Some("new@example.com"));
    assert_eq!(profile.email, w.email);
    let reported = profile.pending_expires_at.expect("expiry");
    assert!(
        (reported - expires).num_seconds().abs() <= 1,
        "{reported} vs {expires}"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_an_expired_pending_change_is_not_shown(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "new@example.com", &tok('p'), -3600).await;

    let page = w.c.get(PROFILE).await;
    assert_eq!(page.status, 200);
    assert!(!page.body.contains("new@example.com"), "no pending box");
    assert!(!html_contains(
        &page.body,
        &en("user.profile.changeEmail.cancel")
    ));
    assert!(html_contains(
        &page.body,
        &en("user.profile.emailChangeNotice")
    ));

    let profile = get_profile(&mut w.c)
        .await
        .account_result::<ProfileView>()
        .unwrap();
    assert_eq!(profile.pending_email, None);
    assert_eq!(profile.pending_expires_at, None);
    assert_eq!(profile.email, w.email);
}

// ------------------------------------------------------------------------- AC39

#[sqlx::test(migrations = "../migrations")]
async fn ac39_pending_expiry_is_formatted_in_the_page_locale_and_time_zone(pool: MySqlPool) {
    let w = alice_world_with(&pool, &[], Opts::default()).await;
    let expires = Utc.with_ymd_and_hms(2099, 12, 1, 10, 0, 0).unwrap();
    set_pending_email_at(&pool, w.id, "new@example.com", &tok('p'), expires).await;

    let mut english = Client::new(&w.app).with_cookie("locale", "en");
    english.login(&w.email, ALICE_PASSWORD).await.ok();
    let page = english.get(PROFILE).await;
    assert_eq!(lang_of(&page.body), "en");
    assert!(
        html_contains(&page.body, "1 Dec 2099, 11:00"),
        "English, Europe/Berlin"
    );

    let mut german = Client::new(&w.app).with_cookie("locale", "de");
    german.login(&w.email, ALICE_PASSWORD).await.ok();
    let page = german.get(PROFILE).await;
    assert_eq!(lang_of(&page.body), "de");
    assert!(html_contains(&page.body, "01.12.2099 11:00"), "German");
    assert!(!page.body.contains("1 Dec 2099"));
}

// ------------------------------------------------------------------ AC41, AC42

#[sqlx::test(migrations = "../migrations")]
async fn ac41_the_verify_page_keeps_its_url_to_itself_but_sends_an_origin(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "new@example.com", &tok('v'), 3600).await;
    let mut anon = Client::new(&w.app);
    let path = format!("{VERIFY}{}", tok('v'));

    for (who, client) in [("anonymous", &mut anon), ("logged in", &mut w.c)] {
        let page = client.get(&path).await;
        assert_eq!(page.status, 200, "{who}");
        // `same-origin`: the URL is never sent to another site, but a no-JS form POST still
        // carries the real Origin (with `no-referrer` it would be `Origin: null` -> 403).
        assert_eq!(
            page.header("referrer-policy").as_deref(),
            Some("same-origin"),
            "{who}"
        );
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac42_profile_and_verify_pages_are_not_cached(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    set_pending_email(&pool, w.id, "new@example.com", &tok('v'), 3600).await;
    let mut anon = Client::new(&w.app);

    let profile = w.c.get(PROFILE).await;
    assert_eq!(profile.status, 200);
    let verify_path = format!("{VERIFY}{}", tok('v'));
    let verify_anon = anon.get(&verify_path).await;
    let verify_user = w.c.get(&verify_path).await;
    for (what, page) in [
        ("profile", profile),
        ("verify (anonymous)", verify_anon),
        ("verify (logged in)", verify_user),
    ] {
        assert!(
            page.header("cache-control")
                .unwrap_or_default()
                .contains("no-store"),
            "{what}: {:?}",
            page.header("cache-control")
        );
    }
}
