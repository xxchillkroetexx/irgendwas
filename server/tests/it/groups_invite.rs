//! groups slice: joining through an invitation code, the preview, the rate limit on wrong
//! codes and the byte-wise code lookup (AC27-AC34, AC45, AC46).

use crate::common::auth::*;
use crate::common::groups::*;
use app::groups_api::{ClosedReason, GroupError};
use app::i18n::{I18n, Locale};
use app::test_support::{self as ts, ManualClock, groups_snapshot};
use chrono::{Duration, Utc};
use sqlx::MySqlPool;

fn berlin() -> chrono_tz::Tz {
    chrono_tz::Europe::Berlin
}

fn berlin_date(offset_days: i64) -> String {
    (Utc::now().with_timezone(&berlin()) + Duration::days(offset_days))
        .format("%Y-%m-%d")
        .to_string()
}

async fn member_count(pool: &MySqlPool, group: i32) -> usize {
    ts::member_ids(pool, group).await.len()
}

#[sqlx::test(migrations = "../migrations")]
async fn legacy_invitation_links_redirect_to_the_normal_flow_without_joining(pool: MySqlPool) {
    let w = world(&pool).await;
    let target = format!("/join/{CODE}");
    let legacy = format!("/groups/join?code={CODE}");
    let mut visitor = w.anon();

    let redirect = visitor.get(&legacy).await;
    assert_eq!(redirect.location().as_deref(), Some(target.as_str()));
    assert!(redirect.status.is_redirection());
    assert_eq!(
        redirect.header("cache-control").as_deref(),
        Some("no-store")
    );
    assert_eq!(
        redirect.header("referrer-policy").as_deref(),
        Some("same-origin")
    );
    let login_page = visitor.get(&target).await;
    assert_eq!(
        login_page.location(),
        Some(format!("/auth/login?next={}", urlencode(&target)))
    );
    visitor
        .login_next(&w.emails[2], app::test_support::CAROL_PASSWORD, &target)
        .await
        .ok();
    let preview = visitor.get(&target).await;
    assert_eq!(preview.status, 200);
    assert!(html_contains(&preview.body, &en("group.join.submit")));
    assert!(!ts::member_ids(&pool, w.g).await.contains(&w.carol));

    let mut member = w.bob().await;
    let redirect = member.get(&legacy).await;
    assert_eq!(redirect.location().as_deref(), Some(target.as_str()));
    let group = member.get(&target).await;
    assert_eq!(group.location(), Some(format!("/groups/{}", w.g)));
    assert_eq!(member_count(&pool, w.g).await, 2);
}

#[sqlx::test(migrations = "../migrations")]
async fn legacy_invitation_redirect_accepts_normalized_codes_and_rejects_unsafe_values(
    pool: MySqlPool,
) {
    let w = world(&pool).await;
    let mut visitor = w.anon();
    let normalized = visitor.get("/groups/join?code=abcd-2345").await;
    assert_eq!(normalized.location().as_deref(), Some("/join/ABCD2345"));

    for path in [
        "/groups/join",
        "/groups/join?code=",
        "/groups/join?code=%2F%2Fevil.example",
        "/groups/join?code=..%2Fadmin",
        "/groups/join?code=ABCD2345%0D%0ALocation%3A%20https%3A%2F%2Fevil.example",
    ] {
        let redirect = visitor.get(path).await;
        assert_eq!(redirect.location().as_deref(), Some("/join"), "{path}");
    }
    assert_eq!(member_count(&pool, w.g).await, 2);
}

// ------------------------------------------------------------------------- AC27

#[sqlx::test(migrations = "../migrations")]
async fn ac27_the_preview_shows_the_group_and_does_not_join(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;

    let p = preview(&mut carol, CODE).await;
    assert_eq!(p.group_id, w.g);
    assert_eq!(p.name, "Family");
    assert_eq!(p.admin_name, "Alice");
    assert_eq!(p.member_count, 2);
    assert!(!p.already_member);
    assert_eq!(p.closed, None);

    let page = carol.get(&format!("/join/{CODE}")).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(
        &page.body,
        &en_with("group.join.previewTitle", &[("name", "Family")])
    ));
    assert!(page.body.contains("Alice"));
    assert!(page.body.contains("2 members"));
    assert!(
        html_contains(&page.body, &en("group.join.submit")),
        "the Join button"
    );

    assert_eq!(
        ts::member_ids(&pool, w.g).await,
        vec![w.alice, w.bob],
        "opening the link (GET) and previewing never join"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac27_joining_makes_carol_a_member(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    assert_eq!(
        get_group(&mut carol, w.g).await.status,
        404,
        "not a member yet"
    );

    let id: i32 = join_group(&mut carol, CODE).await.value();
    assert_eq!(id, w.g);
    assert_eq!(
        ts::member_ids(&pool, w.g).await,
        vec![w.alice, w.bob, w.carol]
    );

    let page = carol.get(&format!("/groups/{}", w.g)).await;
    assert_eq!(page.status, 200);
    let d = detail(&mut carol, w.g).await;
    assert!(!d.is_admin && d.can_leave);
    assert_eq!(d.members.len(), 3);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac27_a_plain_form_post_ends_on_the_group_with_the_flash(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    carol.headers.push(("accept".into(), "text/html".into()));

    let reply = join_group(&mut carol, CODE).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(format!("/groups/{}", w.g)));
    let page = carol.get(&format!("/groups/{}", w.g)).await;
    assert!(html_contains(&page.body, &en("flash.success.group_joined")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac27_the_preview_of_a_one_member_group_says_1_member(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::create_group_with_code(&pool, w.dave, "Solo", "SOLO2345").await;
    let mut carol = w.carol().await;
    let page = carol.get("/join/SOLO2345").await;
    assert_eq!(page.status, 200);
    assert!(page.body.contains("1 member") && !page.body.contains("1 members"));
}

// ------------------------------------------------------------------------- AC28

#[sqlx::test(migrations = "../migrations")]
async fn ac28_codes_are_accepted_lowercase_with_spaces_or_dashes(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_invitation_code(&pool, w.g, "ABCDEFGH").await;
    let mut carol = w.carol().await;

    for typed in [
        " abcd-efgh ",
        "abcdefgh",
        "ABCD EFGH",
        "abcd-efgh\n",
        "A-B-C-D-E-F-G-H",
    ] {
        let p = preview(&mut carol, typed).await;
        assert_eq!(p.group_id, w.g, "{typed:?}");
    }
    let page = carol.get("/join/abcd-efgh").await;
    assert_eq!(page.status, 200);
    assert!(html_contains(
        &page.body,
        &en_with("group.join.previewTitle", &[("name", "Family")])
    ));

    assert_eq!(
        join_group(&mut carol, " abcd-efgh ").await.value::<i32>(),
        w.g
    );
    assert!(ts::member_ids(&pool, w.g).await.contains(&w.carol));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac28_the_enter_code_page_has_the_form_and_says_not_case_sensitive(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    let page = carol.get("/join").await;
    assert_eq!(page.status, 200);
    for key in [
        "group.join.title",
        "group.join.description",
        "group.join.code",
        "group.join.submit",
        "group.join.cancel",
    ] {
        assert!(html_contains(&page.body, &en(key)), "{key}");
    }
    assert!(en("group.join.codeHelp").contains("not case-sensitive"));
    assert!(html_contains(&page.body, &en("group.join.codeHelp")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac28_the_enter_code_page_prefills_nothing_from_the_query(pool: MySqlPool) {
    // The old `?code=` link format is gone; only /join/{CODE} exists.
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    let page = carol.get(&format!("/join?code={CODE}")).await;
    assert_eq!(page.status, 200);
    // The code input stays empty: no value attribute (or an empty one).
    let input = page
        .body
        .split("<input")
        .skip(1)
        .map(|t| t.split('>').next().unwrap_or_default())
        .find(|t| t.contains("name=\"code\"") || t.contains("id=\"code\""))
        .expect("code input");
    assert!(
        !input.contains("value=") || input.contains("value=\"\""),
        "{input}"
    );
    // CODE appears only inside the switch forms' `next` values (the requester's own URL).
    assert!(!without_switch_next(&page.body, &format!("/join?code={CODE}")).contains(CODE));
    assert!(
        ts::member_ids(&pool, w.g).await.len() == 2,
        "a GET never joins"
    );
}

// ------------------------------------------------------------------------- AC29

#[sqlx::test(migrations = "../migrations")]
async fn ac29_an_empty_code_is_required(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut carol = w.carol().await;
    for empty in ["", "   ", "- -", "\t"] {
        assert_eq!(
            join_group(&mut carol, empty).await.group_err(),
            GroupError::CodeRequired,
            "{empty:?}"
        );
        assert_eq!(
            preview_invitation(&mut carol, empty).await.group_err(),
            GroupError::CodeRequired,
            "{empty:?}"
        );
    }
    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac29_an_unknown_code_is_invalid_on_the_page_and_in_both_fns(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut carol = w.carol().await;

    let reply = join_group(&mut carol, "XXXXXXXX").await;
    assert_eq!(reply.group_err(), GroupError::InvalidCode);
    assert!(reply.status.is_client_error(), "{}", reply.status);
    assert_eq!(
        preview_invitation(&mut carol, "XXXXXXXX").await.group_err(),
        GroupError::InvalidCode
    );

    let page = carol.get("/join/XXXXXXXX").await;
    assert!(html_contains(
        &page.body,
        &en("flash.error.invalid_invitation_code")
    ));
    assert!(
        html_contains(&page.body, &en("group.join.code")),
        "the enter-code form is shown again"
    );
    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac29_odd_input_is_just_an_invalid_code(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;
    let long = repeat('A', 10_000);
    for odd in [
        "'; DROP TABLE `groups`; --",
        "%",
        "ABCD234%",
        "ABCD234_",
        "ABCD2345XYZ",
        "ABCD234",
        "ÄÖÜ",
        long.as_str(),
    ] {
        let reply = join_group(&mut carol, odd).await;
        assert_eq!(reply.group_err(), GroupError::InvalidCode, "{odd:.20}");
    }
    assert_eq!(member_count(&pool, w.g).await, 2);
}

// ------------------------------------------------------------------------- AC30

#[sqlx::test(migrations = "../migrations")]
async fn ac30_an_existing_member_redirects_and_gets_no_second_row(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut bob = w.bob().await;

    let p = preview(&mut bob, CODE).await;
    assert!(p.already_member);
    let page = bob.get(&format!("/join/{CODE}")).await;
    assert_eq!(page.status, 302);
    assert_eq!(page.location(), Some(format!("/groups/{}", w.g)));
    assert!(!page.body.contains("action=\"/api/join_group\""));

    let reply = join_group(&mut bob, CODE).await;
    assert_eq!(
        reply.group_err(),
        GroupError::AlreadyMember { group_id: w.g }
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = ? AND user_id = ?")
            .bind(w.g)
            .bind(w.bob)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);

    // The admin is a member too.
    let reply = join_group(&mut w.alice().await, CODE).await;
    assert_eq!(
        reply.group_err(),
        GroupError::AlreadyMember { group_id: w.g }
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn issue59_member_redirect_wins_after_draw_or_deadline_without_writes(pool: MySqlPool) {
    let w = world(&pool).await;
    sqlx::query("UPDATE `groups` SET is_drawn=TRUE, registration_deadline='2020-01-01' WHERE id=?")
        .bind(w.g)
        .execute(&pool)
        .await
        .unwrap();
    let before = groups_snapshot(&pool).await;
    for mut member in [w.alice().await, w.bob().await] {
        let page = member.get(&format!("/join/{CODE}")).await;
        assert_eq!(page.status, 302);
        assert_eq!(page.location(), Some(format!("/groups/{}", w.g)));
        assert_eq!(member.get(&format!("/groups/{}", w.g)).await.status, 200);
    }
    assert_eq!(
        groups_snapshot(&pool).await,
        before,
        "opening an invitation never writes membership or group data"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn issue59_existing_member_is_rechecked_after_login(pool: MySqlPool) {
    let w = world(&pool).await;
    let path = format!("/join/{CODE}");
    let mut visitor = w.anon();
    assert_eq!(
        visitor.get(&path).await.location(),
        Some(format!("/auth/login?next={}", urlencode(&path)))
    );
    let login = visitor
        .login_next(&w.emails[1], app::test_support::BOB_PASSWORD, &path)
        .await;
    login.ok();
    assert_eq!(login.location(), Some(path.clone()));
    let before = ts::member_ids(&pool, w.g).await;
    let page = visitor.get(&path).await;
    assert_eq!(page.status, 302);
    assert_eq!(page.location(), Some(format!("/groups/{}", w.g)));
    assert_eq!(ts::member_ids(&pool, w.g).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac30_the_same_user_joining_twice_at_once_ends_with_one_row(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut a = w.carol().await;
    let mut b = a.fork();
    let (ra, rb) = tokio::join!(join_group(&mut a, CODE), join_group(&mut b, CODE));
    let results = [ra.group_result::<i32>(), rb.group_result::<i32>()];
    assert!(results.iter().any(|r| r == &Ok(w.g)), "{results:?}");
    for r in &results {
        assert!(
            matches!(r, Ok(_) | Err(GroupError::AlreadyMember { .. })),
            "{r:?}"
        );
    }
    assert_eq!(
        ts::member_ids(&pool, w.g).await,
        vec![w.alice, w.bob, w.carol]
    );
}

// ------------------------------------------------------------------------- AC31

#[sqlx::test(migrations = "../migrations")]
async fn ac31_after_the_deadline_nobody_can_join(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    update_group(&mut alice, w.g, "Family", "", &berlin_date(-1), "")
        .await
        .group_ok();
    let mut carol = w.carol().await;

    let p = preview(&mut carol, CODE).await;
    assert_eq!(p.closed, Some(ClosedReason::DeadlinePassed));
    let page = carol.get(&format!("/join/{CODE}")).await;
    assert!(html_contains(
        &page.body,
        &en("flash.error.registration_deadline_passed")
    ));
    assert!(
        !html_contains(&page.body, &en("group.join.submit")),
        "no Join button"
    );

    assert_eq!(
        join_group(&mut carol, CODE).await.group_err(),
        GroupError::RegistrationClosed
    );
    assert_eq!(member_count(&pool, w.g).await, 2);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_the_deadline_is_exact_not_an_hour_or_two_late(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut carol = w.carol().await;

    // The PHP app read Berlin wall-clock as UTC and closed 1-2 h late.
    ts::set_group_dates(&pool, w.g, Some(Utc::now() - Duration::minutes(90)), None).await;
    assert_eq!(
        join_group(&mut carol, CODE).await.group_err(),
        GroupError::RegistrationClosed
    );
    ts::set_group_dates(&pool, w.g, Some(Utc::now() - Duration::minutes(5)), None).await;
    assert_eq!(
        join_group(&mut carol, CODE).await.group_err(),
        GroupError::RegistrationClosed
    );
    ts::set_group_dates(&pool, w.g, Some(Utc::now() + Duration::minutes(90)), None).await;
    assert_eq!(join_group(&mut carol, CODE).await.value::<i32>(), w.g);
}

// ------------------------------------------------------------------------- AC32

#[sqlx::test(migrations = "../migrations")]
async fn ac32_a_deadline_of_today_still_allows_joining_all_day(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    // Stored as today 23:59:59 Berlin: open for the rest of the local day.
    update_group(&mut alice, w.g, "Family", "", &berlin_date(0), "")
        .await
        .group_ok();
    let mut carol = w.carol().await;

    let p = preview(&mut carol, CODE).await;
    assert_eq!(p.closed, None);
    assert_eq!(join_group(&mut carol, CODE).await.value::<i32>(), w.g);
    assert!(ts::member_ids(&pool, w.g).await.contains(&w.carol));
}

// ------------------------------------------------------------------------- AC33

#[sqlx::test(migrations = "../migrations")]
async fn ac33_nobody_joins_a_drawn_group(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_assignment(&pool, w.g, w.alice, w.bob).await;
    ts::add_assignment(&pool, w.g, w.bob, w.alice).await;
    ts::set_drawn(&pool, w.g).await;
    let before = groups_snapshot(&pool).await;
    let mut carol = w.carol().await;

    let p = preview(&mut carol, CODE).await;
    assert_eq!(p.closed, Some(ClosedReason::Drawn));
    let page = carol.get(&format!("/join/{CODE}")).await;
    assert!(html_contains(
        &page.body,
        &en("flash.error.join_closed_drawn")
    ));
    assert!(
        !html_contains(&page.body, &en("group.join.submit")),
        "no Join button"
    );

    assert_eq!(
        join_group(&mut carol, CODE).await.group_err(),
        GroupError::AlreadyDrawn
    );
    assert_eq!(groups_snapshot(&pool).await, before);
}

// ------------------------------------------------------------------------- AC34

#[sqlx::test(migrations = "../migrations")]
async fn ac34_after_login_the_invite_link_shows_the_preview_and_does_not_join(pool: MySqlPool) {
    let w = world(&pool).await;
    let path = format!("/join/{CODE}");
    let mut visitor = w.anon();

    let res = visitor.get(&path).await;
    assert_eq!(
        res.location(),
        Some(format!("/auth/login?next={}", urlencode(&path)))
    );
    let login = visitor
        .login_next(&w.emails[2], app::test_support::CAROL_PASSWORD, &path)
        .await;
    login.ok();
    assert_eq!(login.location().as_deref(), Some(path.as_str()));

    let page = visitor.get(&path).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("group.join.submit")));
    assert!(
        !ts::member_ids(&pool, w.g).await.contains(&w.carol),
        "no automatic join"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac34_registering_through_the_invite_link_lands_on_the_preview(pool: MySqlPool) {
    let w = world(&pool).await;
    let path = format!("/join/{CODE}");
    let token = "e".repeat(43);
    let email = "eve.invited@example.com";
    ts::insert_pending_registration(&pool, email, &token, Some(&path), 3600).await;

    let mut eve = w.anon();
    let reply = eve
        .complete(&token, "Eve", "evepassword1", "evepassword1")
        .await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some(path.as_str()));

    let page = eve.get(&path).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("group.join.submit")));
    let eve_id: i32 = sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
        .bind(email)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        !ts::member_ids(&pool, w.g).await.contains(&eve_id),
        "no automatic join"
    );
}

// ------------------------------------------------------------------------- AC45

fn minutes_of(err: GroupError) -> u32 {
    match err {
        GroupError::TooManyAttempts { minutes } => minutes,
        other => panic!("expected TooManyAttempts, got {other:?}"),
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac45_the_eleventh_call_after_ten_invalid_codes_is_throttled(pool: MySqlPool) {
    let clock = ManualClock::new();
    let w = world_with(&pool, &[], Opts::clock(&clock)).await;
    let mut carol = w.carol().await;

    // Ten invalid codes, through both fns.
    for i in 0..10 {
        let code = format!("BAD{i}BAD{i}");
        let reply = if i % 2 == 0 {
            preview_invitation(&mut carol, &code).await
        } else {
            join_group(&mut carol, &code).await
        };
        assert_eq!(reply.group_err(), GroupError::InvalidCode, "attempt {i}");
    }
    for reply in [
        join_group(&mut carol, CODE).await,
        preview_invitation(&mut carol, CODE).await,
    ] {
        assert_eq!(reply.status, 429);
        let minutes = minutes_of(reply.group_err());
        assert!((1..=10).contains(&minutes), "minutes = {minutes}");
    }
    assert!(
        !ts::member_ids(&pool, w.g).await.contains(&w.carol),
        "no membership"
    );

    // The limit follows the user, not the browser or address.
    let mut other_browser = Client::new(&w.app).peer("198.51.100.77");
    other_browser
        .login(&w.emails[2], app::test_support::CAROL_PASSWORD)
        .await
        .ok();
    assert!(matches!(
        join_group(&mut other_browser, CODE).await.group_err(),
        GroupError::TooManyAttempts { .. }
    ));

    // Another user is unaffected, even from the same address.
    assert_eq!(
        join_group(&mut w.dave().await, CODE).await.value::<i32>(),
        w.g
    );

    // After the window Carol can join again.
    clock.advance(Duration::minutes(11));
    assert_eq!(join_group(&mut carol, CODE).await.value::<i32>(), w.g);
    assert!(ts::member_ids(&pool, w.g).await.contains(&w.carol));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac45_valid_codes_do_not_count(pool: MySqlPool) {
    let clock = ManualClock::new();
    let w = world_with(&pool, &[], Opts::clock(&clock)).await;
    ts::create_group_with_code(&pool, w.dave, "Other", "OTHR2345").await;
    let mut carol = w.carol().await;

    for _ in 0..15 {
        assert_eq!(preview(&mut carol, CODE).await.group_id, w.g);
        assert_eq!(preview(&mut carol, "othr-2345").await.name, "Other");
    }
    for i in 0..10 {
        assert_eq!(
            preview_invitation(&mut carol, &format!("NOPE{i}NOPE"))
                .await
                .group_err(),
            GroupError::InvalidCode,
            "invalid #{}",
            i + 1
        );
    }
    assert!(matches!(
        preview_invitation(&mut carol, CODE).await.group_err(),
        GroupError::TooManyAttempts { .. }
    ));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac45_nine_invalid_codes_do_not_block(pool: MySqlPool) {
    let clock = ManualClock::new();
    let w = world_with(&pool, &[], Opts::clock(&clock)).await;
    let mut carol = w.carol().await;
    for i in 0..9 {
        join_group(&mut carol, &format!("NOPE{i}NOPE"))
            .await
            .group_err();
    }
    assert_eq!(join_group(&mut carol, CODE).await.value::<i32>(), w.g);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac45_the_page_throttles_too(pool: MySqlPool) {
    let clock = ManualClock::new();
    let w = world_with(&pool, &[], Opts::clock(&clock)).await;
    let mut carol = w.carol().await;
    for i in 0..10 {
        preview_invitation(&mut carol, &format!("NOPE{i}NOPE"))
            .await
            .group_err();
    }
    let page = carol.get(&format!("/join/{CODE}")).await;
    assert!(
        !html_contains(&page.body, &en("group.join.submit")),
        "no Join button"
    );
    assert!(
        !page.body.contains("Family"),
        "the valid code is not previewed"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac45_the_window_is_ten_minutes_from_the_first_invalid_code(pool: MySqlPool) {
    let clock = ManualClock::new();
    let w = world_with(&pool, &[], Opts::clock(&clock)).await;
    let mut carol = w.carol().await;
    for i in 0..10 {
        join_group(&mut carol, &format!("NOPE{i}NOPE"))
            .await
            .group_err();
    }
    clock.advance(Duration::minutes(9));
    let minutes = minutes_of(join_group(&mut carol, CODE).await.group_err());
    assert_eq!(minutes, 1, "one minute left");
    clock.advance(Duration::minutes(2));
    assert_eq!(join_group(&mut carol, CODE).await.value::<i32>(), w.g);
}

// ------------------------------------------------------------------------- AC46

#[test]
fn ac46_the_throttle_message_uses_the_plural_forms() {
    let en_i18n = I18n::new(Locale::En, berlin());
    let de_i18n = I18n::new(Locale::De, berlin());

    let one = GroupError::TooManyAttempts { minutes: 1 };
    let en1 = one.text(&en_i18n);
    assert!(
        en1.contains("1 minute") && !en1.contains("1 minutes"),
        "{en1}"
    );
    let de1 = one.text(&de_i18n);
    assert!(
        de1.contains("1 Minute") && !de1.contains("1 Minuten"),
        "{de1}"
    );

    let five = GroupError::TooManyAttempts { minutes: 5 };
    assert!(five.text(&en_i18n).contains("5 minutes"));
    assert!(five.text(&de_i18n).contains("5 Minuten"));
}

#[test]
fn ac46_the_catalog_has_plural_member_counts() {
    assert_eq!(
        en_with("group.memberCount.one", &[("count", "1")]),
        "1 member"
    );
    assert_eq!(
        en_with("group.memberCount.other", &[("count", "2")]),
        "2 members"
    );
    assert_eq!(
        de_with("group.memberCount.one", &[("count", "1")]),
        "1 Mitglied"
    );
    assert_eq!(
        de_with("group.memberCount.other", &[("count", "2")]),
        "2 Mitglieder"
    );
}

// ------------------------------------------------------- byte-wise code lookup

#[sqlx::test(migrations = "../migrations")]
async fn codes_are_stored_with_a_binary_collation(pool: MySqlPool) {
    let collation: String = sqlx::query_scalar(
        "SELECT COLLATION_NAME FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'groups' \
         AND COLUMN_NAME = 'invitation_code'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(collation, "utf8mb4_bin");
}

#[sqlx::test(migrations = "../migrations")]
async fn lookalike_codes_are_different_codes_and_can_coexist(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_invitation_code(&pool, w.g, "ABCDEFGH").await;
    // With the default utf8mb4_unicode_ci these would violate the UNIQUE index.
    let lower = ts::create_group_with_code(&pool, w.dave, "Lower", "abcdefgh").await;
    let accent = ts::create_group_with_code(&pool, w.dave, "Accent", "ÀBCDEFGH").await;
    assert_ne!(lower, accent);
    let mut carol = w.carol().await;

    // Normalisation uppercases, so only the exact upper-case code is reachable.
    assert_eq!(preview(&mut carol, "abcdefgh").await.group_id, w.g);
    assert_eq!(preview(&mut carol, "ABCDEFGH").await.group_id, w.g);
    // The accent variant finds its own group, never G.
    assert_eq!(preview(&mut carol, "àbcdefgh").await.group_id, accent);
    assert_eq!(preview(&mut carol, "ÀBCDEFGH").await.group_id, accent);
}

#[sqlx::test(migrations = "../migrations")]
async fn accent_and_case_variants_of_a_stored_code_do_not_match(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_invitation_code(&pool, w.g, "ABCDEFGH").await;
    let before = groups_snapshot(&pool).await;
    let mut carol = w.carol().await;

    for variant in [
        "ÁBCDEFGH",
        "ÀBCDEFGH",
        "ABCDEFGH\u{0301}",
        "ＡＢＣＤＥＦＧＨ",
        "ABCDEFGHI",
    ] {
        assert_eq!(
            join_group(&mut carol, variant).await.group_err(),
            GroupError::InvalidCode,
            "{variant}"
        );
    }
    // A code stored in lower case can never be reached (lookups are upper case).
    ts::set_invitation_code(&pool, w.g, "abcdefgh").await;
    for typed in ["abcdefgh", "ABCDEFGH"] {
        assert_eq!(
            preview_invitation(&mut carol, typed).await.group_err(),
            GroupError::InvalidCode,
            "{typed}"
        );
    }
    ts::set_invitation_code(&pool, w.g, "ABCDEFGH").await;
    assert_eq!(groups_snapshot(&pool).await, before);
}
