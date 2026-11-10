//! groups slice: creating a group, validation and the invitation code (AC1-AC8).
//! Server-rendered pages are in groups_pages.rs, the e2e journey in groups.spec.ts.

use crate::common::auth::*;
use crate::common::groups::*;
use app::AppError;
use app::groups_api::{GroupError, GroupField, GroupFieldError};
use app::server::invite::{
    CODE_ALPHABET, CODE_LEN, MAX_CODE_ATTEMPTS, generate_code, insert_group_with_code,
    normalize_code, replace_code,
};
use app::test_support::{self as ts, groups_snapshot};
use chrono::{TimeZone, Utc};
use sqlx::MySqlPool;

fn validation(err: &GroupError, field: GroupField, error: GroupFieldError) {
    match err {
        GroupError::Validation(list) => assert!(
            list.contains(&(field, error)),
            "expected {field:?}/{error:?} in {list:?}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}

// -------------------------------------------------------------------------- AC1

#[sqlx::test(migrations = "../migrations")]
async fn ac1_create_trims_the_name_and_makes_the_creator_admin_and_only_member(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    let reply = create_group(&mut alice, "  Family 2026  ", "A description", "", "").await;
    let id: i32 = reply.value();

    let row = ts::group_row(&pool, id).await;
    assert_eq!(row.name, "Family 2026");
    assert_eq!(row.description.as_deref(), Some("A description"));
    assert_eq!(row.admin_id, w.alice);
    assert_eq!(row.registration_deadline, None);
    assert_eq!(row.draw_date, None);
    assert!(row.is_drawn != Some(true));
    assert_eq!(ts::member_ids(&pool, id).await, vec![w.alice]);
    assert!(
        is_valid_code(&row.invitation_code),
        "{}",
        row.invitation_code
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac1_a_plain_form_post_ends_on_the_new_group_with_the_flash(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    // A form post without JavaScript asks for HTML.
    alice.headers.push(("accept".into(), "text/html".into()));

    let reply = create_group(&mut alice, "No JS Group", "", "", "").await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    let id: i32 = sqlx::query_scalar("SELECT id FROM `groups` WHERE name = 'No JS Group'")
        .fetch_one(&pool)
        .await
        .expect("the group was created");
    assert_eq!(reply.location(), Some(format!("/groups/{id}")));

    let page = alice.get(&format!("/groups/{id}")).await;
    assert_eq!(page.status, 200);
    assert!(
        html_contains(&page.body, &en("flash.success.group_created")),
        "the flash is rendered by the server"
    );
}

// -------------------------------------------------------------------------- AC2

#[sqlx::test(migrations = "../migrations")]
async fn ac2_an_empty_or_blank_name_creates_nothing(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let before = groups_snapshot(&pool).await;

    for name in ["", " ", "   ", "\t \n"] {
        let reply = create_group(&mut alice, name, "desc", "", "").await;
        assert_eq!(reply.status, 422, "{name:?}");
        validation(
            &reply.group_err(),
            GroupField::Name,
            GroupFieldError::NameRequired,
        );
    }
    assert_eq!(groups_snapshot(&pool).await, before);
}

// -------------------------------------------------------------------------- AC3

#[sqlx::test(migrations = "../migrations")]
async fn ac3_a_256_character_name_is_rejected_and_255_is_accepted(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let before = groups_snapshot(&pool).await;

    let reply = create_group(&mut alice, &repeat('a', 256), "", "", "").await;
    assert_eq!(reply.status, 422);
    validation(
        &reply.group_err(),
        GroupField::Name,
        GroupFieldError::NameTooLong,
    );
    assert_eq!(groups_snapshot(&pool).await, before, "nothing saved");

    let id: i32 = create_group(&mut alice, &repeat('a', 255), "", "", "")
        .await
        .value();
    assert_eq!(ts::group_row(&pool, id).await.name, repeat('a', 255));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac3_the_limit_counts_characters_after_trimming(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    // 255 two-byte characters are fine; padding spaces do not count.
    let name = repeat('ä', 255);
    let padded = format!("  {name}  ");
    let id: i32 = create_group(&mut alice, &padded, "", "", "").await.value();
    assert_eq!(ts::group_row(&pool, id).await.name, name);

    let reply = create_group(&mut alice, &repeat('ä', 256), "", "", "").await;
    validation(
        &reply.group_err(),
        GroupField::Name,
        GroupFieldError::NameTooLong,
    );
}

// -------------------------------------------------------------------------- AC4

#[sqlx::test(migrations = "../migrations")]
async fn ac4_a_2001_character_description_is_rejected_and_2000_is_accepted(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let before = groups_snapshot(&pool).await;

    let reply = create_group(&mut alice, "Long", &repeat('d', 2001), "", "").await;
    assert_eq!(reply.status, 422);
    validation(
        &reply.group_err(),
        GroupField::Description,
        GroupFieldError::DescriptionTooLong,
    );
    assert_eq!(groups_snapshot(&pool).await, before);

    let id: i32 = create_group(&mut alice, "Long", &repeat('d', 2000), "", "")
        .await
        .value();
    assert_eq!(
        ts::group_row(&pool, id).await.description,
        Some(repeat('d', 2000))
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac4_an_empty_description_is_stored_as_null_and_text_is_trimmed(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    for blank in ["", "   ", "\n \t"] {
        let id: i32 = create_group(&mut alice, "No desc", blank, "", "")
            .await
            .value();
        assert_eq!(
            ts::group_row(&pool, id).await.description,
            None,
            "{blank:?}"
        );
    }
    let id: i32 = create_group(&mut alice, "Desc", "  hello\nworld  ", "", "")
        .await
        .value();
    assert_eq!(
        ts::group_row(&pool, id).await.description.as_deref(),
        Some("hello\nworld")
    );
}

// -------------------------------------------------------------------------- AC5

#[sqlx::test(migrations = "../migrations")]
async fn ac5_dates_are_stored_as_utc_instants_of_the_berlin_wall_clock(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    // Winter (CET, UTC+1): 23:59:59 -> 22:59:59 UTC and 12:00:00 -> 11:00:00 UTC.
    let id: i32 = create_group(&mut alice, "Winter", "", "2026-12-10", "2026-12-12")
        .await
        .value();
    let row = ts::group_row(&pool, id).await;
    assert_eq!(
        row.registration_deadline,
        Some(Utc.with_ymd_and_hms(2026, 12, 10, 22, 59, 59).unwrap())
    );
    assert_eq!(
        row.draw_date,
        Some(Utc.with_ymd_and_hms(2026, 12, 12, 11, 0, 0).unwrap())
    );

    // Summer (CEST, UTC+2).
    let id: i32 = create_group(&mut alice, "Summer", "", "2026-07-10", "2026-07-12")
        .await
        .value();
    let row = ts::group_row(&pool, id).await;
    assert_eq!(
        row.registration_deadline,
        Some(Utc.with_ymd_and_hms(2026, 7, 10, 21, 59, 59).unwrap())
    );
    assert_eq!(
        row.draw_date,
        Some(Utc.with_ymd_and_hms(2026, 7, 12, 10, 0, 0).unwrap())
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_only_one_date_may_be_set(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    let id: i32 = create_group(&mut alice, "Deadline only", "", "2026-12-10", "")
        .await
        .value();
    let row = ts::group_row(&pool, id).await;
    assert!(row.registration_deadline.is_some());
    assert_eq!(row.draw_date, None);

    let id: i32 = create_group(&mut alice, "Draw only", "", "", "2026-12-12")
        .await
        .value();
    let row = ts::group_row(&pool, id).await;
    assert_eq!(row.registration_deadline, None);
    assert!(row.draw_date.is_some());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_the_stored_dates_follow_app_timezone(pool: MySqlPool) {
    let w = world_with(
        &pool,
        &[("APP_TIMEZONE", "America/New_York")],
        Opts::default(),
    )
    .await;
    let mut alice = w.alice().await;

    let id: i32 = create_group(&mut alice, "NY", "", "2026-12-10", "2026-12-12")
        .await
        .value();
    let row = ts::group_row(&pool, id).await;
    // EST is UTC-5.
    assert_eq!(
        row.registration_deadline,
        Some(Utc.with_ymd_and_hms(2026, 12, 11, 4, 59, 59).unwrap())
    );
    assert_eq!(
        row.draw_date,
        Some(Utc.with_ymd_and_hms(2026, 12, 12, 17, 0, 0).unwrap())
    );
}

// -------------------------------------------------------------------------- AC6

#[sqlx::test(migrations = "../migrations")]
async fn ac6_a_draw_date_before_the_deadline_saves_nothing(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let before = groups_snapshot(&pool).await;

    let reply = create_group(&mut alice, "Backwards", "", "2026-12-12", "2026-12-10").await;
    assert_eq!(reply.status, 422);
    validation(
        &reply.group_err(),
        GroupField::DrawDate,
        GroupFieldError::DrawBeforeDeadline,
    );
    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_the_same_day_is_allowed(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let id: i32 = create_group(&mut alice, "Same day", "", "2026-12-10", "2026-12-10")
        .await
        .value();
    let row = ts::group_row(&pool, id).await;
    assert!(row.draw_date.is_some() && row.registration_deadline.is_some());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_past_dates_are_allowed(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let id: i32 = create_group(&mut alice, "Old", "", "2020-01-01", "2020-01-02")
        .await
        .value();
    assert!(
        ts::group_row(&pool, id)
            .await
            .registration_deadline
            .is_some()
    );
}

// -------------------------------------------------------------------------- AC7

#[sqlx::test(migrations = "../migrations")]
async fn ac7_an_unparseable_date_is_reported_on_its_field_and_nothing_is_saved(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let before = groups_snapshot(&pool).await;

    for bad in [
        "abc",
        "2026-13-01",
        "2026-02-30",
        "10.12.2026",
        "2026-1-5",
        "2026-12-10T00:00",
        "2026-12-10 ",
        "12/10/2026",
    ] {
        let reply = create_group(&mut alice, "Bad date", "", bad, "").await;
        assert_eq!(reply.status, 422, "{bad:?}");
        validation(
            &reply.group_err(),
            GroupField::RegistrationDeadline,
            GroupFieldError::InvalidDate,
        );

        let reply = create_group(&mut alice, "Bad date", "", "", bad).await;
        validation(
            &reply.group_err(),
            GroupField::DrawDate,
            GroupFieldError::InvalidDate,
        );
    }
    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn group_dates_are_checked_against_timestamp_bounds_in_utc(pool: MySqlPool) {
    let w = world_with(
        &pool,
        &[("APP_TIMEZONE", "Pacific/Kiritimati")],
        Opts::default(),
    )
    .await;
    let mut alice = w.alice().await;
    let before = groups_snapshot(&pool).await;
    for (deadline, draw, field) in [
        ("1969-12-30", "", GroupField::RegistrationDeadline),
        ("", "1969-12-30", GroupField::DrawDate),
        ("2106-02-07", "", GroupField::RegistrationDeadline),
        ("", "2106-02-08", GroupField::DrawDate),
        ("2500-01-01", "", GroupField::RegistrationDeadline),
    ] {
        let response = create_group(&mut alice, "outside", "", deadline, draw).await;
        assert_eq!(response.status, 422, "{deadline} {draw}");
        validation(&response.group_err(), field, GroupFieldError::InvalidDate);
    }
    assert_eq!(groups_snapshot(&pool).await, before);

    // On the same local date the noon draw is valid, but the 23:59:59
    // deadline exceeds the limit after conversion from UTC+14.
    let id: i32 = create_group(&mut alice, "boundary", "", "", "2106-02-07")
        .await
        .value();
    assert_eq!(
        ts::group_row(&pool, id).await.draw_date,
        Some(Utc.with_ymd_and_hms(2106, 2, 6, 22, 0, 0).unwrap())
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac7_all_problems_are_returned_together(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    let reply = create_group(&mut alice, "  ", &repeat('d', 2001), "nope", "also nope").await;
    let GroupError::Validation(list) = reply.group_err() else {
        panic!("expected Validation");
    };
    for expected in [
        (GroupField::Name, GroupFieldError::NameRequired),
        (GroupField::Description, GroupFieldError::DescriptionTooLong),
        (
            GroupField::RegistrationDeadline,
            GroupFieldError::InvalidDate,
        ),
        (GroupField::DrawDate, GroupFieldError::InvalidDate),
    ] {
        assert!(list.contains(&expected), "{expected:?} missing in {list:?}");
    }
}

// -------------------------------------------------------------------------- AC8

#[sqlx::test(migrations = "../migrations")]
async fn ac8_codes_of_new_groups_have_the_right_format_and_are_unique(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    let mut codes = std::collections::BTreeSet::new();
    for i in 0..25 {
        let id: i32 = create_group(&mut alice, &format!("G{i}"), "", "", "")
            .await
            .value();
        let code = ts::group_code(&pool, id).await;
        assert!(is_valid_code(&code), "{code}");
        assert!(codes.insert(code), "duplicate code");
    }
}

#[test]
fn ac8_generated_codes_use_the_alphabet_and_the_full_range() {
    assert_eq!(CODE_ALPHABET, ALPHABET);
    assert_eq!(CODE_LEN, 8);
    let mut seen = std::collections::BTreeSet::new();
    let mut with_repeat = 0;
    for _ in 0..2000 {
        let code = generate_code();
        assert!(is_valid_code(&code), "{code}");
        seen.extend(code.chars());
        let distinct: std::collections::BTreeSet<char> = code.chars().collect();
        if distinct.len() < code.chars().count() {
            with_repeat += 1;
        }
    }
    assert_eq!(seen.len(), 32, "every symbol shows up in 16000 draws");
    assert!(
        with_repeat > 0,
        "characters may repeat (drawn with replacement, not shuffled)"
    );
}

#[test]
fn ac8_generated_codes_are_not_repeated() {
    let codes: std::collections::BTreeSet<String> = (0..2000).map(|_| generate_code()).collect();
    assert_eq!(codes.len(), 2000);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_a_forced_collision_retries_with_a_new_code(pool: MySqlPool) {
    let admin = ts::create_user(&pool, "Admin", "admin@example.com").await;
    let existing = ts::create_group_with_code(&pool, admin, "Existing", "AAAAAAAA").await;

    let mut offered = vec!["BBBBBBBB", "AAAAAAAA", "AAAAAAAA"];
    let mut calls = 0;
    let mut next = || {
        calls += 1;
        offered
            .pop()
            .expect("generator called too often")
            .to_string()
    };
    let mut tx = pool.begin().await.unwrap();
    let (id, code) = insert_group_with_code(&mut tx, "New", admin, &mut next)
        .await
        .expect("retry succeeds");
    tx.commit().await.unwrap();

    assert_eq!(calls, 3, "two collisions, then a fresh code");
    assert_eq!(code, "BBBBBBBB");
    assert_ne!(id, existing);
    let row = ts::group_row(&pool, id).await;
    assert_eq!(row.invitation_code, "BBBBBBBB");
    assert_eq!(row.admin_id, admin);
    assert_eq!(ts::member_ids(&pool, id).await, vec![admin]);
    assert_eq!(ts::group_code(&pool, existing).await, "AAAAAAAA");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_after_five_collisions_it_gives_up_with_an_internal_error(pool: MySqlPool) {
    let admin = ts::create_user(&pool, "Admin", "admin@example.com").await;
    ts::create_group_with_code(&pool, admin, "Existing", "AAAAAAAA").await;
    assert_eq!(MAX_CODE_ATTEMPTS, 5);

    let mut calls = 0;
    let mut next = || {
        calls += 1;
        "AAAAAAAA".to_string()
    };
    let mut tx = pool.begin().await.unwrap();
    let result = insert_group_with_code(&mut tx, "New", admin, &mut next).await;
    assert_eq!(result, Err(AppError::Internal));
    tx.rollback().await.unwrap();
    assert_eq!(calls, 5);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `groups`")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "no half-created group");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_replacing_a_code_retries_on_collision_too(pool: MySqlPool) {
    let admin = ts::create_user(&pool, "Admin", "admin@example.com").await;
    ts::create_group_with_code(&pool, admin, "Other", "AAAAAAAA").await;
    let group = ts::create_group_with_code(&pool, admin, "Mine", "CCCCCCCC").await;

    let mut offered = vec!["DDDDDDDD", "AAAAAAAA"];
    let mut next = || {
        offered
            .pop()
            .expect("generator called too often")
            .to_string()
    };
    let mut tx = pool.begin().await.unwrap();
    let code = replace_code(&mut tx, group, &mut next).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(code, "DDDDDDDD");
    assert_eq!(ts::group_code(&pool, group).await, "DDDDDDDD");

    let mut tx = pool.begin().await.unwrap();
    let mut always = || "AAAAAAAA".to_string();
    assert_eq!(
        replace_code(&mut tx, group, &mut always).await,
        Err(AppError::Internal)
    );
    tx.rollback().await.unwrap();
    assert_eq!(ts::group_code(&pool, group).await, "DDDDDDDD", "unchanged");
}

// ------------------------------------------------------------------ AC28 (unit)

#[test]
fn ac28_codes_are_normalised_by_trimming_removing_spaces_and_dashes_and_uppercasing() {
    assert_eq!(normalize_code("ABCD2345"), "ABCD2345");
    assert_eq!(normalize_code(" abcd-efgh "), "ABCDEFGH");
    assert_eq!(normalize_code("abcd efgh"), "ABCDEFGH");
    assert_eq!(normalize_code("\tab-cd-ef-gh\n"), "ABCDEFGH");
    assert_eq!(normalize_code("---"), "");
    assert_eq!(normalize_code("   "), "");
    assert_eq!(normalize_code(""), "");
}

#[test]
fn ac28_normalising_does_not_fold_accents() {
    // Lookups are exact on the normalised value: an accented lookalike stays different.
    assert_eq!(normalize_code("àbcdefgh"), "ÀBCDEFGH");
    assert_ne!(normalize_code("àbcdefgh"), "ABCDEFGH");
}
