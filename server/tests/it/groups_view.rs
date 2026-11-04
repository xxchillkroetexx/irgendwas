//! groups slice: the list and the group page as the server renders them, and `get_group`
//! (AC5 display, AC9-AC16, AC19 form, AC46, AC47). Interactions are in groups.spec.ts.

use crate::common::auth::*;
use crate::common::groups::*;
use app::groups_api::{GroupDetail, GroupError, GroupStatus};
use app::test_support::{self as ts, groups_snapshot};
use chrono::{TimeZone, Utc};
use sqlx::MySqlPool;

fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, mi, s).unwrap()
}

fn pos(html: &str, needle: &str) -> usize {
    html.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found in the page"))
}

// -------------------------------------------------------------------------- AC9

#[sqlx::test(migrations = "../migrations")]
async fn ac9_the_list_shows_my_groups_newest_first_with_badges_counts_and_admins(pool: MySqlPool) {
    let w = world(&pool).await;
    // H: admin Carol, Alice joined; created after G (higher id, same second).
    let h = ts::create_group_with_code(&pool, w.carol, "Office Party", "HHHH2345").await;
    ts::add_member(&pool, h, w.alice).await;
    ts::add_member(&pool, h, w.dave).await;
    // K: Alice is not in it.
    ts::create_group_with_code(&pool, w.dave, "Secret Club", "KKKK2345").await;
    let mut alice = w.alice().await;

    let list = summaries(&mut alice).await;
    assert_eq!(
        list.iter().map(|g| g.id).collect::<Vec<_>>(),
        vec![h, w.g],
        "H (created later) before G, K absent"
    );
    let (hs, gs) = (&list[0], &list[1]);
    assert!(!hs.is_admin && gs.is_admin, "Admin badge only on G");
    assert_eq!(
        (hs.admin_name.as_str(), gs.admin_name.as_str()),
        ("Carol", "Alice")
    );
    assert_eq!((hs.member_count, gs.member_count), (3, 2));
    assert_eq!(gs.name, "Family");

    let page = alice.get("/groups").await;
    assert_eq!(page.status, 200);
    assert!(
        pos(&page.body, "Office Party") < pos(&page.body, "Family"),
        "H is listed before G"
    );
    assert!(!page.body.contains("Secret Club"), "no foreign group");
    assert!(html_contains(&page.body, &en("group.createButton")));
    assert!(html_contains(&page.body, &en("group.joinButton")));
    assert!(!html_contains(&page.body, &en("group.emptyMessage")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_the_order_is_created_at_descending_then_id_descending(pool: MySqlPool) {
    let w = world(&pool).await;
    let second = ts::create_group_with_code(&pool, w.alice, "Second", "SSSS2345").await;
    let third = ts::create_group_with_code(&pool, w.alice, "Third", "TTTT2345").await;
    let older = ts::create_group_with_code(&pool, w.alice, "Older", "OOOO2345").await;
    ts::set_group_created_at(&pool, w.g, utc(2026, 3, 1, 10, 0, 0)).await;
    ts::set_group_created_at(&pool, second, utc(2026, 2, 1, 10, 0, 0)).await;
    ts::set_group_created_at(&pool, third, utc(2026, 2, 1, 10, 0, 0)).await;
    ts::set_group_created_at(&pool, older, utc(2025, 1, 1, 10, 0, 0)).await;
    let mut alice = w.alice().await;

    let ids: Vec<i32> = summaries(&mut alice).await.iter().map(|g| g.id).collect();
    assert_eq!(ids, vec![w.g, third, second, older]);
}

// ------------------------------------------------------------------------- AC10

#[sqlx::test(migrations = "../migrations")]
async fn ac10_without_groups_the_list_shows_the_empty_state(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut dave = w.dave().await;

    assert!(summaries(&mut dave).await.is_empty());
    let page = dave.get("/groups").await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("group.emptyMessage")));
    assert!(html_contains(&page.body, &en("group.emptyMessageAction")));
    assert!(html_contains(&page.body, &en("group.createButton")));
    assert!(html_contains(&page.body, &en("group.joinButton")));
    assert!(page.body.contains("href=\"/groups/new\""));
    assert!(page.body.contains("href=\"/join\""));
    assert!(!page.body.contains("Family"), "G is not listed");
}

// ------------------------------------------------------------------------- AC11

#[sqlx::test(migrations = "../migrations")]
async fn ac11_a_member_sees_names_but_no_emails_code_or_admin_actions(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut bob = w.bob().await;

    let d = detail(&mut bob, w.g).await;
    assert_eq!(d.name, "Family");
    assert_eq!(d.status, GroupStatus::Open);
    assert_eq!(d.admin.name, "Alice");
    assert!(!d.is_admin);
    assert!(d.invitation.is_none(), "no code for a member");
    assert!(d.can_leave);
    assert_eq!(d.members.len(), 2);
    for m in &d.members {
        assert_eq!(m.email, None, "no emails for a member");
        assert_eq!(m.is_me, m.user_id == w.bob);
        assert_eq!(m.is_admin, m.user_id == w.alice);
    }

    let page = bob.get(&format!("/groups/{}", w.g)).await;
    assert_eq!(page.status, 200);
    for text in ["Family", "Alice", "Bob"] {
        assert!(page.body.contains(text), "{text}");
    }
    assert!(html_contains(&page.body, &en("group.show.you")));
    assert!(html_contains(&page.body, &en("group.show.leaveGroup")));
    assert!(!page.body.contains(&w.emails[0]), "Alice's email");
    assert!(!page.body.contains(CODE), "the invitation code");
    assert!(!page.body.contains(&format!("/join/{CODE}")));
    for key in [
        "group.show.editButton",
        "group.show.regenerateCode",
        "group.show.deleteGroup",
        "group.show.invitationCode",
        "group.show.makeAdmin",
        "group.show.removeMember",
    ] {
        assert!(
            !html_contains(&page.body, &en(key)),
            "{key} is for the admin only"
        );
    }
}

// ------------------------------------------------------------------------- AC12

#[sqlx::test(migrations = "../migrations")]
async fn ac12_the_admin_sees_emails_code_link_and_actions_but_no_leave_button(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    let d = detail(&mut alice, w.g).await;
    assert!(d.is_admin && !d.can_leave);
    let inv = d.invitation.expect("the admin gets the invitation");
    assert_eq!(inv.code, CODE);
    assert_eq!(inv.link, format!("{APP_URL}/join/{CODE}"));
    let emails: Vec<_> = d.members.iter().map(|m| m.email.clone()).collect();
    assert!(emails.contains(&Some(w.emails[0].clone())));
    assert!(emails.contains(&Some(w.emails[1].clone())));

    let page = alice.get(&format!("/groups/{}", w.g)).await;
    assert_eq!(page.status, 200);
    assert!(page.body.contains(&w.emails[0]) && page.body.contains(&w.emails[1]));
    assert!(page.body.contains(CODE));
    assert!(page.body.contains(&format!("{APP_URL}/join/{CODE}")));
    for key in [
        "group.show.editButton",
        "group.show.regenerateCode",
        "group.show.deleteGroup",
        "group.show.invitationCode",
        "group.show.makeAdmin",
        "group.show.removeMember",
    ] {
        assert!(html_contains(&page.body, &en(key)), "{key}");
    }
    assert!(!html_contains(&page.body, &en("group.show.leaveGroup")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_the_invite_link_comes_from_app_url_never_from_the_host_header(pool: MySqlPool) {
    let w = world_with(
        &pool,
        &[("APP_URL", "https://santa.example.org")],
        Opts::default(),
    )
    .await;
    let mut alice = Client::new(&w.app)
        .origin("https://santa.example.org")
        .header("host", "evil.example");
    alice
        .login(&w.emails[0], app::test_support::ALICE_PASSWORD)
        .await
        .ok();

    let d = detail(&mut alice, w.g).await;
    assert_eq!(
        d.invitation.unwrap().link,
        format!("https://santa.example.org/join/{CODE}")
    );
    let page = alice.get(&format!("/groups/{}", w.g)).await;
    assert!(
        page.body
            .contains(&format!("https://santa.example.org/join/{CODE}"))
    );
    assert!(!page.body.contains("evil.example"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_names_and_descriptions_are_escaped(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let id: i32 = create_group(
        &mut alice,
        "<script>alert(1)</script>",
        "line one\n<img src=x onerror=alert(2)>",
        "",
        "",
    )
    .await
    .value();

    let page = alice.get(&format!("/groups/{id}")).await;
    assert_eq!(page.status, 200);
    assert!(!page.body.contains("<script>alert(1)</script>"));
    assert!(!page.body.contains("<img src=x"));
    assert!(
        html_contains(&page.body, "<script>alert(1)</script>"),
        "shown as text"
    );
}

// ------------------------------------------------------------------------- AC13

#[sqlx::test(migrations = "../migrations")]
async fn ac13_a_non_member_and_a_missing_group_give_the_same_404(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut dave = w.dave().await;
    dave.session_info().await; // takes the "logged in" flash, so both pages render alike

    let foreign = dave.get(&format!("/groups/{}", w.g)).await;
    let missing = dave.get("/groups/999999").await;
    assert_eq!(foreign.status, 404);
    assert_eq!(missing.status, 404);
    assert_eq!(
        comparable(&foreign.body, &format!("/groups/{}", w.g)),
        comparable(&missing.body, "/groups/999999"),
        "the two 404 pages are identical"
    );
    assert!(!foreign.body.contains("Family"), "no group data leaks");
    assert!(!foreign.body.contains("Alice"));
    assert!(!foreign.body.contains(CODE));

    let a = get_group(&mut dave, w.g).await;
    let b = get_group(&mut dave, 999_999).await;
    assert_eq!(a.status, 404);
    assert_eq!(b.status, 404);
    assert_eq!(a.group_err(), GroupError::NotFound);
    assert_eq!(b.group_err(), GroupError::NotFound);
    assert_eq!(a.body, b.body);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_an_id_that_is_not_a_positive_integer_gives_the_404_page(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    for path in [
        "/groups/abc",
        "/groups/0",
        "/groups/-1",
        "/groups/1.5",
        "/groups/99999999999999999999",
        "/groups/abc/edit",
        "/groups/0/edit",
    ] {
        assert_eq!(alice.get(path).await.status, 404, "{path}");
    }
    for id in ["0", "-1"] {
        let reply = alice.call("get_group", &[("group_id", id)]).await;
        assert_eq!(reply.group_err(), GroupError::NotFound, "{id}");
    }
}

// ------------------------------------------------------------------------- AC14

#[sqlx::test(migrations = "../migrations")]
async fn ac14_a_logged_out_visitor_is_sent_to_login_and_back(pool: MySqlPool) {
    let w = world(&pool).await;
    let paths = [
        "/groups".to_string(),
        "/groups/new".to_string(),
        format!("/groups/{}", w.g),
        format!("/groups/{}/edit", w.g),
        "/join".to_string(),
        format!("/join/{CODE}"),
    ];
    for path in paths {
        let mut visitor = w.anon();
        let res = visitor.get(&path).await;
        assert!(res.status.is_redirection(), "{path}: {}", res.status);
        let location = res.location().expect("Location");
        assert_eq!(
            location,
            format!("/auth/login?next={}", urlencode(&path)),
            "{path}"
        );
        assert!(
            !res.body.contains(CODE) || path.contains(CODE),
            "no data in the redirect"
        );

        let next = urldecode(location.split_once("next=").unwrap().1);
        let login = visitor
            .login_next(&w.emails[1], app::test_support::BOB_PASSWORD, &next)
            .await;
        login.ok();
        assert_eq!(login.location().as_deref(), Some(path.as_str()));
    }
    // Bob lands on the group page itself.
    let mut bob = w.bob().await;
    assert_eq!(bob.get(&format!("/groups/{}", w.g)).await.status, 200);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_every_server_fn_without_a_session_is_unauthorized_and_changes_nothing(
    pool: MySqlPool,
) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut anon = w.anon();
    let g = w.g;

    let replies = vec![
        ("list_my_groups", list_my_groups(&mut anon).await),
        ("get_group", get_group(&mut anon, g).await),
        (
            "create_group",
            create_group(&mut anon, "X", "", "", "").await,
        ),
        (
            "update_group",
            update_group(&mut anon, g, "X", "", "", "").await,
        ),
        ("delete_group", delete_group(&mut anon, g).await),
        (
            "regenerate_invitation",
            regenerate_invitation(&mut anon, g).await,
        ),
        (
            "preview_invitation",
            preview_invitation(&mut anon, CODE).await,
        ),
        ("join_group", join_group(&mut anon, CODE).await),
        ("leave_group", leave_group(&mut anon, g).await),
        ("remove_member", remove_member(&mut anon, g, w.bob).await),
        ("transfer_admin", transfer_admin(&mut anon, g, w.bob).await),
    ];
    for (name, reply) in replies {
        assert_eq!(reply.status, 401, "{name}");
        assert_eq!(reply.group_err(), GroupError::Unauthorized, "{name}");
    }
    assert_eq!(groups_snapshot(&pool).await, before);
}

// ------------------------------------------------------------------------- AC15

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_past_deadline_closes_registration_and_a_draw_wins_over_it(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut bob = w.bob().await;

    assert_eq!(detail(&mut bob, w.g).await.status, GroupStatus::Open);
    ts::set_group_dates(&pool, w.g, Some(utc(2020, 1, 1, 22, 59, 59)), None).await;
    assert_eq!(
        detail(&mut bob, w.g).await.status,
        GroupStatus::RegistrationClosed
    );
    assert_eq!(
        summaries(&mut bob).await[0].status,
        GroupStatus::RegistrationClosed
    );

    ts::set_drawn(&pool, w.g).await;
    assert_eq!(detail(&mut bob, w.g).await.status, GroupStatus::Drawn);
    assert_eq!(summaries(&mut bob).await[0].status, GroupStatus::Drawn);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_future_deadline_keeps_the_group_open(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_group_dates(
        &pool,
        w.g,
        Some(Utc::now() + chrono::Duration::days(30)),
        None,
    )
    .await;
    let mut bob = w.bob().await;
    assert_eq!(detail(&mut bob, w.g).await.status, GroupStatus::Open);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_an_explicitly_undrawn_group_is_shown_as_not_drawn(pool: MySqlPool) {
    let w = world(&pool).await;
    sqlx::query("UPDATE `groups` SET is_drawn = FALSE, drawn_at = NULL WHERE id = ?")
        .bind(w.g)
        .execute(&pool)
        .await
        .unwrap();
    let mut bob = w.bob().await;
    let d = detail(&mut bob, w.g).await;
    assert_eq!(d.status, GroupStatus::Open);
    assert!(!d.is_drawn && d.can_leave);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_the_pages_show_the_status(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut bob = w.bob().await;
    let page_url = format!("/groups/{}", w.g);

    for (setup, label) in [
        (0, "group.status.open"),
        (1, "group.status.registrationClosed"),
        (2, "group.status.drawn"),
    ] {
        match setup {
            1 => ts::set_group_dates(&pool, w.g, Some(utc(2020, 1, 1, 22, 59, 59)), None).await,
            2 => ts::set_drawn(&pool, w.g).await,
            _ => {}
        }
        for url in ["/groups", page_url.as_str()] {
            let page = bob.get(url).await;
            assert!(html_contains(&page.body, &en(label)), "{url}: {label}");
        }
    }
}

// ------------------------------------------------------------------------- AC16

#[sqlx::test(migrations = "../migrations")]
async fn ac16_get_group_never_contains_assignment_data(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_member(&pool, w.g, w.carol).await;
    ts::add_assignment(&pool, w.g, w.alice, w.bob).await;
    ts::add_assignment(&pool, w.g, w.bob, w.carol).await;
    ts::add_assignment(&pool, w.g, w.carol, w.alice).await;
    ts::set_drawn(&pool, w.g).await;
    crate::common::draw::set_drawn_at(&pool, w.g, "2026-11-01 10:00:00").await;

    for (who, mut c) in [
        (w.alice, w.alice().await),
        (w.bob, w.bob().await),
        (w.carol, w.carol().await),
    ] {
        let own = match who {
            x if x == w.alice => w.bob,
            x if x == w.bob => w.carol,
            _ => w.alice,
        };
        let reply = get_group(&mut c, w.g).await;
        let d: GroupDetail = reply.value();
        assert!(d.is_drawn);
        let json = reply.body.to_lowercase();
        for word in ["assign", "giver", "receiver", "gives_to", "recipient"] {
            assert!(!json.contains(word), "{word} in {json}");
        }
        let page = c.get(&format!("/groups/{}", w.g)).await;
        assert_eq!(page.status, 200);
        // The page carries the draw panel: only the caller's own assignment, nobody else's.
        crate::common::draw::assert_only_own_assignment(&page.body, own, "group page");
    }
}

// ------------------------------------------------------------------- AC5 display

#[sqlx::test(migrations = "../migrations")]
async fn ac5_ac47_the_details_show_the_dates_in_the_page_locale(pool: MySqlPool) {
    let w = world(&pool).await;
    // 2026-12-10 23:59:59 and 2026-12-12 12:00:00 Berlin; created 2026-11-01 Berlin.
    ts::set_group_dates(
        &pool,
        w.g,
        Some(utc(2026, 12, 10, 22, 59, 59)),
        Some(utc(2026, 12, 12, 11, 0, 0)),
    )
    .await;
    ts::set_group_created_at(&pool, w.g, utc(2026, 11, 1, 11, 0, 0)).await;
    let url = format!("/groups/{}", w.g);

    let mut english = Client::new(&w.app).with_cookie("locale", "en");
    english
        .login(&w.emails[1], app::test_support::BOB_PASSWORD)
        .await
        .ok();
    let page = english.get(&url).await;
    assert_eq!(lang_of(&page.body), "en");
    for text in ["10 Dec 2026", "12 Dec 2026", "1 Nov 2026"] {
        assert!(html_contains(&page.body, text), "{text}");
    }
    assert!(!page.body.contains("10.12.2026"));

    let mut german = w
        .german(&w.emails[1], app::test_support::BOB_PASSWORD)
        .await;
    let page = german.get(&url).await;
    assert_eq!(lang_of(&page.body), "de");
    for text in ["10.12.2026", "12.12.2026", "01.11.2026"] {
        assert!(html_contains(&page.body, text), "{text}");
    }
    assert!(!page.body.contains("10 Dec 2026"));
    assert!(html_contains(&page.body, &de("group.show.drawDateInfo")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac47_the_dates_are_shown_in_app_timezone_not_utc(pool: MySqlPool) {
    // 2026-12-10 23:30 UTC is already the 11th in Berlin.
    let w = world(&pool).await;
    ts::set_group_dates(&pool, w.g, Some(utc(2026, 12, 10, 23, 30, 0)), None).await;
    let mut bob = w.bob().await;
    let page = bob.get(&format!("/groups/{}", w.g)).await;
    assert!(html_contains(&page.body, "11 Dec 2026"));
    assert!(!page.body.contains("10 Dec 2026"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac47_the_edit_form_holds_iso_dates_in_both_locales(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_group_dates(
        &pool,
        w.g,
        Some(utc(2026, 12, 10, 22, 59, 59)),
        Some(utc(2026, 12, 12, 11, 0, 0)),
    )
    .await;
    let url = format!("/groups/{}/edit", w.g);

    let mut english = w.alice().await;
    let mut german = w
        .german(&w.emails[0], app::test_support::ALICE_PASSWORD)
        .await;
    for (c, lang) in [(&mut english, "en"), (&mut german, "de")] {
        let page = c.get(&url).await;
        assert_eq!(page.status, 200, "{lang}");
        assert_eq!(lang_of(&page.body), lang);
        assert!(page.body.contains("2026-12-10"), "{lang}: deadline input");
        assert!(page.body.contains("2026-12-12"), "{lang}: draw date input");
        assert!(!page.body.contains("2026-12-11"));
    }
}

// ------------------------------------------------------------------ AC19 (form)

#[sqlx::test(migrations = "../migrations")]
async fn ac19_a_past_deadline_is_prefilled_and_saving_it_unchanged_keeps_the_instant(
    pool: MySqlPool,
) {
    let w = world(&pool).await;
    let stored = utc(2025, 12, 1, 22, 59, 59);
    ts::set_group_dates(&pool, w.g, Some(stored), None).await;
    let mut alice = w.alice().await;

    let page = alice.get(&format!("/groups/{}/edit", w.g)).await;
    assert_eq!(page.status, 200);
    assert!(page.body.contains("2025-12-01"), "the deadline field");

    update_group(&mut alice, w.g, "Family", "", "2025-12-01", "")
        .await
        .group_ok();
    assert_eq!(
        ts::group_row(&pool, w.g).await.registration_deadline,
        Some(stored)
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_the_edit_form_is_prefilled_with_the_current_values(pool: MySqlPool) {
    let w = world(&pool).await;
    sqlx::query("UPDATE `groups` SET name = 'Fam <1>', description = 'About \"us\"' WHERE id = ?")
        .bind(w.g)
        .execute(&pool)
        .await
        .unwrap();
    let mut alice = w.alice().await;
    let page = alice.get(&format!("/groups/{}/edit", w.g)).await;
    assert!(html_contains(&page.body, "Fam <1>"));
    assert!(html_contains(&page.body, "About \"us\""));
    assert!(html_contains(&page.body, &en("group.edit.title")));
    assert!(html_contains(&page.body, &en("group.form.updateButton")));
    assert!(html_contains(&page.body, &en("group.form.cancel")));
}

// ------------------------------------------------------------------ AC46 (plural)

#[sqlx::test(migrations = "../migrations")]
async fn ac46_the_list_pluralises_the_member_count_in_both_locales(pool: MySqlPool) {
    let w = world(&pool).await;
    // Carol is alone in her group, Alice's G has two members.
    ts::create_group_with_code(&pool, w.carol, "Solo", "SOLO2345").await;
    ts::add_member(
        &pool,
        ts::create_group_with_code(&pool, w.carol, "Pair", "PAIR2345").await,
        w.dave,
    )
    .await;
    let mut carol = w.carol().await;
    let page = carol.get("/groups").await;
    assert!(page.body.contains("1 member") && !page.body.contains("1 members"));
    assert!(page.body.contains("2 members"));

    let mut german = w
        .german(&w.emails[2], app::test_support::CAROL_PASSWORD)
        .await;
    let page = german.get("/groups").await;
    assert!(page.body.contains("1 Mitglied") && !page.body.contains("1 Mitglieder"));
    assert!(page.body.contains("2 Mitglieder"));
}
