//! exclusions slice: the page and `get_exclusions` (AC1, AC2, AC4, AC15, AC16, AC17,
//! AC19 pages, AC20 page, AC31).

use crate::common::auth::*;
use crate::common::exclusions::*;
use crate::common::groups::*;
use app::exclusions_api::FeasibilityNotice;
use app::test_support::{self as ts, BOB_PASSWORD};
use sqlx::MySqlPool;

const KEYS_NEW: &[&str] = &[
    "exclusion.privacy",
    "exclusion.addNew.limit",
    "exclusion.addNew.limitReached",
    "exclusion.addNew.noCandidates",
    "exclusion.locked",
    "exclusion.warning.drawImpossible",
    "exclusion.warning.noReceiverLeft",
    "flash.success.exclusion_added",
    "flash.success.exclusion_removed",
    "flash.error.exclusion_self",
    "flash.error.exclusion_limit_reached",
    "flash.error.exclusion_would_block_draw",
    "flash.error.exclusions_locked",
    "exclusion.addNew.description",
    "exclusion.why.note",
];

const KEYS_REUSED: &[&str] = &[
    "exclusion.title",
    "exclusion.forGroup",
    "exclusion.backToGroup",
    "exclusion.addNew.title",
    "exclusion.addNew.selectPerson",
    "exclusion.addNew.selectPlaceholder",
    "exclusion.addNew.submit",
    "exclusion.why.title",
    "exclusion.why.intro",
    "exclusion.why.reason1",
    "exclusion.why.reason2",
    "exclusion.why.reason3",
    "exclusion.current.title",
    "exclusion.current.empty",
    "exclusion.current.remove",
    "exclusion.current.removeConfirm",
    "group.show.manageExclusions",
    "flash.error.user_not_in_group",
];

// ------------------------------------------------------------------------ AC1

#[sqlx::test(migrations = "../migrations")]
async fn ac1_an_empty_page_shows_the_empty_state_the_privacy_line_the_limit_and_the_select(
    pool: MySqlPool,
) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    let html = bob.get(&ex_path(x.g())).await;
    assert_eq!(html.status, 200);
    let body = &html.body;
    assert!(html_contains(body, &en("exclusion.title")));
    assert!(html_contains(body, &en("exclusion.current.empty")));
    assert!(html_contains(body, &en("exclusion.privacy")));
    assert!(html_contains(
        body,
        &en_with("exclusion.addNew.limit", &[("max", "5"), ("count", "0")])
    ));
    assert!(html_contains(body, "Family"), "the group name");
    assert!(
        body.contains(&format!("href=\"/groups/{}\"", x.g())),
        "links back to the group"
    );
    assert_eq!(
        select_options(body),
        [
            (x.alice().to_string(), "Alice".to_string()),
            (x.carol().to_string(), "Carol".to_string()),
            (x.dave().to_string(), "Dave".to_string()),
        ]
    );
    // No notice, and no raw catalog keys.
    assert!(!html_contains(
        body,
        &en("exclusion.warning.drawImpossible")
    ));
    assert!(!html_contains(body, &en("exclusion.locked")));
    for key in KEYS_NEW.iter().chain(KEYS_REUSED) {
        assert!(!body.contains(key), "raw key {key} in the page");
    }
    // Never an email.
    for email in &x.w.emails {
        assert!(!body.contains(email.as_str()), "{email} on the page");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac1_get_exclusions_describes_the_empty_state(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    let p = page(&mut bob, x.g()).await;
    assert_eq!((p.group_id, p.group_name.as_str()), (x.g(), "Family"));
    assert!(!p.is_drawn);
    assert!(p.my_rules.is_empty());
    assert_eq!(
        p.candidates.iter().map(|m| m.user_id).collect::<Vec<_>>(),
        [x.alice(), x.carol(), x.dave()]
    );
    assert_eq!(p.max_rules, 5);
    assert!(p.can_add);
    assert!(p.notice.is_none());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac1_lists_are_ordered_by_name_then_id(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let aaron = ts::create_user(&pool, "Aaron", &ts::mailpit::unique_email("aaron")).await;
    let carol2 = ts::create_user(&pool, "Carol", &ts::mailpit::unique_email("carol2")).await;
    ts::add_member(&pool, x.g(), carol2).await;
    ts::add_member(&pool, x.g(), aaron).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), carol2).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), aaron).await;
    let mut bob = x.w.bob().await;
    let p = page(&mut bob, x.g()).await;
    assert_eq!(
        p.my_rules.iter().map(|m| m.user_id).collect::<Vec<_>>(),
        [aaron, x.carol(), carol2]
    );
    assert_eq!(
        p.candidates.iter().map(|m| m.user_id).collect::<Vec<_>>(),
        [x.alice(), x.dave()]
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac1_nobody_left_to_exclude_shows_the_no_candidates_text(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let g3 = x.g3(&pool).await;
    ts::add_exclusion(&pool, g3, x.bob(), x.alice()).await;
    ts::add_exclusion(&pool, g3, x.bob(), x.carol()).await;
    let mut bob = x.w.bob().await;
    let p = page(&mut bob, g3).await;
    assert!(p.candidates.is_empty());
    assert!(!p.can_add);
    let html = bob.get(&ex_path(g3)).await;
    assert!(html_contains(
        &html.body,
        &en("exclusion.addNew.noCandidates")
    ));
    assert!(select_options(&html.body).is_empty());
}

// ------------------------------------------------------------------------ AC2

#[sqlx::test(migrations = "../migrations")]
async fn ac2_every_member_sees_the_link_on_the_group_page_until_the_draw(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let href = format!("href=\"{}\"", ex_path(x.g()));
    for (who, mut c) in [
        ("alice", x.w.alice().await),
        ("bob", x.w.bob().await),
        ("carol", x.w.carol().await),
        ("dave", x.w.dave().await),
    ] {
        let html = c.get(&format!("/groups/{}", x.g())).await;
        assert_eq!(html.status, 200, "{who}");
        assert!(html.body.contains(&href), "{who}: link missing");
        assert!(
            html_contains(&html.body, &en("group.show.manageExclusions")),
            "{who}: link text"
        );
    }
    ts::set_drawn(&pool, x.g()).await;
    for (who, mut c) in [("alice", x.w.alice().await), ("bob", x.w.bob().await)] {
        let html = c.get(&format!("/groups/{}", x.g())).await;
        assert_eq!(html.status, 200, "{who}");
        assert!(!html.body.contains(&href), "{who}: link after the draw");
    }
}

// ------------------------------------------------------------------------ AC4

#[sqlx::test(migrations = "../migrations")]
async fn ac4_the_excluded_person_sees_nothing_of_it(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let mut carol = x.w.carol().await;
    let p = page(&mut carol, x.g()).await;
    assert!(p.my_rules.is_empty());
    assert!(p.candidates.iter().any(|m| m.user_id == x.bob()));
    assert!(p.notice.is_none());
    let html = carol.get(&ex_path(x.g())).await;
    assert!(html_contains(&html.body, &en("exclusion.current.empty")));
    assert!(
        select_options(&html.body)
            .iter()
            .any(|(v, l)| *v == x.bob().to_string() && l == "Bob")
    );
}

// ------------------------------------------------------------------------ AC15

async fn ac15_world(pool: &MySqlPool) -> X {
    let x = x_world(pool).await;
    ts::add_exclusion(pool, x.g(), x.bob(), x.alice()).await;
    ts::add_exclusion(pool, x.g(), x.bob(), x.carol()).await;
    x
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_leaver_can_make_the_group_infeasible_and_the_pages_say_so(pool: MySqlPool) {
    let x = ac15_world(&pool).await;
    let mut bob = x.w.bob().await;
    let mut alice = x.w.alice().await;
    let mut carol = x.w.carol().await;
    // Before: feasible, no notice anywhere.
    for c in [&mut bob, &mut alice, &mut carol] {
        assert!(page(c, x.g()).await.notice.is_none());
    }

    let mut dave = x.w.dave().await;
    leave_group(&mut dave, x.g()).await.group_ok();

    let p = page(&mut bob, x.g()).await;
    assert_eq!(
        p.notice,
        Some(FeasibilityNotice::DrawImpossible {
            no_receiver_for_me: true
        })
    );
    for (who, c) in [("alice", &mut alice), ("carol", &mut carol)] {
        let p = page(c, x.g()).await;
        assert_eq!(
            p.notice,
            Some(FeasibilityNotice::DrawImpossible {
                no_receiver_for_me: false
            }),
            "{who}"
        );
    }

    let impossible = en("exclusion.warning.drawImpossible");
    let no_receiver = en("exclusion.warning.noReceiverLeft");
    let html = bob.get(&ex_path(x.g())).await;
    assert!(html_contains(&html.body, &impossible));
    assert!(html_contains(&html.body, &no_receiver));
    for (who, c) in [("alice", &mut alice), ("carol", &mut carol)] {
        let html = c.get(&ex_path(x.g())).await;
        assert!(html_contains(&html.body, &impossible), "{who}");
        assert!(!html_contains(&html.body, &no_receiver), "{who}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_the_notice_names_nobody_and_no_rule(pool: MySqlPool) {
    let x = ac15_world(&pool).await;
    let mut dave = x.w.dave().await;
    leave_group(&mut dave, x.g()).await.group_ok();
    let mut alice = x.w.alice().await;
    let p = page(&mut alice, x.g()).await;
    // Alice's data holds her own (empty) rules and the notice, nothing about Bob's rules.
    assert!(p.my_rules.is_empty());
    assert_eq!(
        p.notice,
        Some(FeasibilityNotice::DrawImpossible {
            no_receiver_for_me: false
        })
    );
    let html = alice.get(&ex_path(x.g())).await;
    assert!(!html.body.contains("no_receiver_for_me"), "no internals");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_removing_a_rule_clears_the_notice_for_everybody(pool: MySqlPool) {
    let x = ac15_world(&pool).await;
    let mut dave = x.w.dave().await;
    leave_group(&mut dave, x.g()).await.group_ok();
    let mut bob = x.w.bob().await;
    remove_ex(&mut bob, x.g(), x.alice()).await.ex_ok();
    for (who, mut c) in [
        ("alice", x.w.alice().await),
        ("bob", x.w.bob().await),
        ("carol", x.w.carol().await),
    ] {
        assert!(page(&mut c, x.g()).await.notice.is_none(), "{who}");
        let html = c.get(&ex_path(x.g())).await;
        assert!(
            !html_contains(&html.body, &en("exclusion.warning.drawImpossible")),
            "{who}"
        );
        assert!(
            !html_contains(&html.body, &en("exclusion.warning.noReceiverLeft")),
            "{who}"
        );
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_removed_member_can_break_it_too(pool: MySqlPool) {
    let x = ac15_world(&pool).await;
    let mut alice = x.w.alice().await; // admin
    remove_member(&mut alice, x.g(), x.dave()).await.group_ok();
    let mut bob = x.w.bob().await;
    assert_eq!(
        page(&mut bob, x.g()).await.notice,
        Some(FeasibilityNotice::DrawImpossible {
            no_receiver_for_me: true
        })
    );
}

// ------------------------------------------------------------------------ AC16

#[sqlx::test(migrations = "../migrations")]
async fn ac16_after_the_draw_there_is_no_notice_and_no_candidate(pool: MySqlPool) {
    let x = ac15_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.dave()).await; // Bob cannot draw anybody
    ts::set_drawn(&pool, x.g()).await;
    let mut bob = x.w.bob().await;
    let p = page(&mut bob, x.g()).await;
    assert!(p.is_drawn);
    assert!(p.notice.is_none());
    assert!(p.candidates.is_empty());
    assert!(!p.can_add);
    assert_eq!(p.my_rules.len(), 3);
    let html = bob.get(&ex_path(x.g())).await;
    assert!(!html_contains(
        &html.body,
        &en("exclusion.warning.drawImpossible")
    ));
}

// ------------------------------------------------------------------------ AC17

#[sqlx::test(migrations = "../migrations")]
async fn ac17_the_drawn_page_is_read_only(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::set_drawn(&pool, x.g()).await;
    let mut bob = x.w.bob().await;
    let html = bob.get(&ex_path(x.g())).await;
    assert_eq!(html.status, 200);
    let body = &html.body;
    assert!(html_contains(body, &en("exclusion.locked")));
    assert!(body.contains("Carol"), "the rule is listed");
    assert!(!html_contains(body, &en("exclusion.addNew.title")));
    assert!(!html_contains(body, &en("exclusion.addNew.submit")));
    assert!(select_options(body).is_empty());
    assert!(!html_contains(body, &en("exclusion.current.remove")));
    assert!(!html_contains(
        body,
        &en("exclusion.warning.drawImpossible")
    ));
    // Nobody else is named: no candidates, no assignment data.
    assert!(!body.contains("Dave"));
}

// ------------------------------------------------------------------------ AC19

#[sqlx::test(migrations = "../migrations")]
async fn ac19_a_non_member_a_missing_group_and_a_bad_id_give_the_same_404_page(pool: MySqlPool) {
    let x = x_world(&pool).await;
    // Positive control.
    let mut bob = x.w.bob().await;
    assert_eq!(bob.get(&ex_path(x.g())).await.status, 200);

    let mut eve = x.eve_client().await;
    eve.session_info().await; // takes the "logged in" flash so the pages render alike
    let foreign = eve.get(&ex_path(x.g())).await;
    let missing = eve.get(&ex_path(999_999)).await;
    assert_eq!(foreign.status, 404);
    assert_eq!(missing.status, 404);
    assert_eq!(without_nonce(&foreign.body), without_nonce(&missing.body));
    for bad in ["abc", "0", "-1", "1.5", "99999999999999999999"] {
        let page = eve.get(&format!("/groups/{bad}/exclusions")).await;
        assert_eq!(page.status, 404, "{bad}");
        assert_eq!(
            without_nonce(&page.body),
            without_nonce(&missing.body),
            "{bad}"
        );
        let mut member = x.w.bob().await;
        member.session_info().await;
        assert_eq!(
            member
                .get(&format!("/groups/{bad}/exclusions"))
                .await
                .status,
            404,
            "{bad} as a member"
        );
    }
    for secret in ["Family", "Alice", "Bob", "Carol", "Dave"] {
        assert!(!foreign.body.contains(secret), "404 leaks {secret}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_the_old_exclusions_urls_are_gone(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    for path in [
        format!("/exclusions/{}", x.g()),
        format!("/exclusions/{}/remove/{}", x.g(), x.carol()),
    ] {
        assert_eq!(bob.get(&path).await.status, 404, "{path}");
    }
}

// ------------------------------------------------------------------------ AC20

#[sqlx::test(migrations = "../migrations")]
async fn ac20_a_logged_out_visitor_is_sent_to_login_and_back(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let path = ex_path(x.g());
    let mut visitor = x.w.anon();
    let res = visitor.get(&path).await;
    assert!(res.status.is_redirection(), "{}", res.status);
    assert_eq!(
        res.location().unwrap(),
        format!("/auth/login?next={}", urlencode(&path))
    );
    assert!(!res.body.contains("Carol"), "no data in the redirect");
    let login = visitor
        .login_next(&x.w.emails[1], BOB_PASSWORD, &path)
        .await;
    login.ok();
    assert_eq!(login.location().as_deref(), Some(path.as_str()));
    let landed = visitor.get(&path).await;
    assert_eq!(landed.status, 200);
    assert!(landed.body.contains("Carol"));
}

// ------------------------------------------------------------------------ AC31

#[sqlx::test(migrations = "../migrations")]
async fn ac31_new_and_reworded_keys_exist_in_both_catalogs_with_the_specified_text(
    _pool: MySqlPool,
) {
    for key in KEYS_NEW.iter().chain(KEYS_REUSED) {
        assert!(!en(key).is_empty(), "{key} en");
        assert!(!de(key).is_empty(), "{key} de");
    }
    assert_eq!(
        en("exclusion.addNew.description"),
        "Choose people you should not draw. This only works one way: they can still draw you."
    );
    assert_eq!(
        de("exclusion.addNew.description"),
        "Wählen Sie Personen, die Sie nicht ziehen sollen. Das gilt nur in eine Richtung: Diese Personen können Sie trotzdem ziehen."
    );
    assert_eq!(
        en("exclusion.why.note"),
        "Note: Too many exclusions can make a draw impossible. You will be told if an exclusion would do that."
    );
    assert_eq!(
        de("exclusion.why.note"),
        "Hinweis: Zu viele Ausschlüsse können eine Ziehung unmöglich machen. Sie werden informiert, wenn ein Ausschluss das bewirken würde."
    );
    assert_eq!(
        en("exclusion.privacy"),
        "Only you can see your exclusions. Other members and the group admin cannot."
    );
    assert_eq!(
        de("exclusion.privacy"),
        "Nur Sie sehen Ihre Ausschlüsse. Andere Mitglieder und der Gruppenadmin sehen sie nicht."
    );
    assert_eq!(
        en_with("exclusion.addNew.limit", &[("max", "5"), ("count", "2")]),
        "You can exclude up to 5 people (2 used)."
    );
    assert_eq!(
        de_with("exclusion.addNew.limit", &[("max", "5"), ("count", "2")]),
        "Sie können bis zu 5 Personen ausschließen (2 verwendet)."
    );
    assert_eq!(en("flash.success.exclusion_added"), "Exclusion added.");
    assert_eq!(
        de("flash.error.exclusions_locked"),
        "Ausschlüsse können nach der Ziehung nicht mehr geändert werden."
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_the_german_page_has_german_text_only(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let mut bob = x.w.german(&x.w.emails[1], BOB_PASSWORD).await;
    let html = bob.get(&ex_path(x.g())).await;
    assert_eq!(html.status, 200);
    let body = &html.body;
    assert_eq!(lang_of(body), "de");
    for key in [
        "exclusion.title",
        "exclusion.privacy",
        "exclusion.addNew.title",
        "exclusion.addNew.description",
        "exclusion.why.title",
        "exclusion.why.note",
        "exclusion.current.title",
    ] {
        assert!(html_contains(body, &de(key)), "German text of {key}");
        if en(key) != de(key) {
            assert!(!html_contains(body, &en(key)), "English text of {key}");
        }
    }
    assert!(html_contains(
        body,
        &de_with("exclusion.addNew.limit", &[("max", "5"), ("count", "1")])
    ));
    // The "Note:" prefix is part of the key text only (PHP printed it twice).
    assert!(!body.contains("Note:"), "English prefix on the German page");
    assert!(!body.contains("Hinweis: Hinweis:"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_the_note_has_no_hard_coded_prefix_in_english(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    let html = bob.get(&ex_path(x.g())).await;
    let note = en("exclusion.why.note");
    assert_eq!(html.body.matches(&note).count(), 1);
    assert!(!html.body.contains("<strong>Note:"));
    assert!(!html.body.contains("Note: Note:"));
    assert!(!html.body.contains("Note:</strong>"));
}
