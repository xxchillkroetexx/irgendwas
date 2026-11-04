//! groups slice: editing, deleting and regenerating the code (AC17-AC26).

use crate::common::auth::*;
use crate::common::groups::*;
use crate::common::{post_from, send};
use app::groups_api::{GroupError, GroupField, GroupFieldError};
use app::test_support::{self as ts, groups_snapshot};
use chrono::{TimeZone, Utc};
use sqlx::MySqlPool;

fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, mi, s).unwrap()
}

async fn count(pool: &MySqlPool, sql: &'static str, id: i32) -> i64 {
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ------------------------------------------------------------------------- AC17

#[sqlx::test(migrations = "../migrations")]
async fn ac17_the_admin_edits_name_description_and_deadline(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    update_group(
        &mut alice,
        w.g,
        "  Family 2027 ",
        " New description ",
        "2027-01-05",
        "",
    )
    .await
    .group_ok();

    let row = ts::group_row(&pool, w.g).await;
    assert_eq!(row.name, "Family 2027");
    assert_eq!(row.description.as_deref(), Some("New description"));
    assert_eq!(row.registration_deadline, Some(utc(2027, 1, 5, 22, 59, 59)));
    assert_eq!(row.draw_date, None);
    // Everything else stays.
    assert_eq!(row.admin_id, w.alice);
    assert_eq!(row.invitation_code, CODE);
    assert_eq!(ts::member_ids(&pool, w.g).await, vec![w.alice, w.bob]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_a_plain_form_post_ends_on_the_group_with_the_flash(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    alice.headers.push(("accept".into(), "text/html".into()));

    let reply = update_group(&mut alice, w.g, "Renamed", "", "", "").await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(format!("/groups/{}", w.g)));
    let page = alice.get(&format!("/groups/{}", w.g)).await;
    assert!(html_contains(
        &page.body,
        &en("flash.success.group_updated")
    ));
    assert!(page.body.contains("Renamed"));
}

// ------------------------------------------------------------------------- AC18

#[sqlx::test(migrations = "../migrations")]
async fn ac18_empty_date_fields_clear_both_dates(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_group_dates(
        &pool,
        w.g,
        Some(utc(2026, 12, 10, 22, 59, 59)),
        Some(utc(2026, 12, 12, 11, 0, 0)),
    )
    .await;
    let mut alice = w.alice().await;

    update_group(&mut alice, w.g, "Family", "", "", "")
        .await
        .group_ok();
    let row = ts::group_row(&pool, w.g).await;
    assert_eq!(row.registration_deadline, None);
    assert_eq!(row.draw_date, None);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac18_clearing_one_date_keeps_the_other(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_group_dates(
        &pool,
        w.g,
        Some(utc(2026, 12, 10, 22, 59, 59)),
        Some(utc(2026, 12, 12, 11, 0, 0)),
    )
    .await;
    let mut alice = w.alice().await;

    update_group(&mut alice, w.g, "Family", "", "2026-12-10", "")
        .await
        .group_ok();
    let row = ts::group_row(&pool, w.g).await;
    assert_eq!(
        row.registration_deadline,
        Some(utc(2026, 12, 10, 22, 59, 59))
    );
    assert_eq!(row.draw_date, None);
}

// ------------------------------------------------------------------------- AC20

#[sqlx::test(migrations = "../migrations")]
async fn ac20_a_member_gets_forbidden_and_a_non_member_not_found(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut bob = w.bob().await;
    let mut dave = w.dave().await;

    let reply = update_group(&mut bob, w.g, "Hijacked", "x", "2030-01-01", "").await;
    assert_eq!(reply.status, 403);
    assert_eq!(reply.group_err(), GroupError::Forbidden);

    let reply = update_group(&mut dave, w.g, "Hijacked", "x", "2030-01-01", "").await;
    assert_eq!(reply.status, 404);
    assert_eq!(reply.group_err(), GroupError::NotFound);

    let reply = update_group(&mut dave, 999_999, "Hijacked", "x", "", "").await;
    assert_eq!(reply.group_err(), GroupError::NotFound);

    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac20_the_edit_page_is_a_403_for_a_member_and_a_404_for_anyone_else(pool: MySqlPool) {
    let w = world(&pool).await;
    let url = format!("/groups/{}/edit", w.g);
    let mut bob = w.bob().await;
    let mut dave = w.dave().await;
    dave.session_info().await; // takes the "logged in" flash, so both pages render alike

    let member = bob.get(&url).await;
    assert_eq!(member.status, 403);
    assert!(
        !member.body.contains(CODE),
        "no invitation code on the 403 page"
    );
    assert!(
        !html_contains(&member.body, &en("group.form.updateButton")),
        "no form"
    );

    let outsider = dave.get(&url).await;
    let missing = dave.get("/groups/999999/edit").await;
    assert_eq!(outsider.status, 404);
    assert_eq!(missing.status, 404);
    assert_eq!(
        comparable(&outsider.body, &url),
        comparable(&missing.body, "/groups/999999/edit")
    );
    assert!(!outsider.body.contains("Family"));

    assert_eq!(w.alice().await.get(&url).await.status, 200);
}

// ------------------------------------------------------------------------- AC21

#[sqlx::test(migrations = "../migrations")]
async fn ac21_edit_validation_follows_create_and_leaves_the_row_unchanged(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_group_dates(&pool, w.g, Some(utc(2026, 12, 10, 22, 59, 59)), None).await;
    let before = groups_snapshot(&pool).await;
    let mut alice = w.alice().await;

    let cases: Vec<(String, String, String, String, GroupField, GroupFieldError)> = vec![
        (
            "  ".into(),
            "".into(),
            "".into(),
            "".into(),
            GroupField::Name,
            GroupFieldError::NameRequired,
        ),
        (
            repeat('a', 256),
            "".into(),
            "".into(),
            "".into(),
            GroupField::Name,
            GroupFieldError::NameTooLong,
        ),
        (
            "N".into(),
            repeat('d', 2001),
            "".into(),
            "".into(),
            GroupField::Description,
            GroupFieldError::DescriptionTooLong,
        ),
        (
            "N".into(),
            "".into(),
            "2026-12-12".into(),
            "2026-12-10".into(),
            GroupField::DrawDate,
            GroupFieldError::DrawBeforeDeadline,
        ),
        (
            "N".into(),
            "".into(),
            "garbage".into(),
            "".into(),
            GroupField::RegistrationDeadline,
            GroupFieldError::InvalidDate,
        ),
        (
            "N".into(),
            "".into(),
            "".into(),
            "2026-02-30".into(),
            GroupField::DrawDate,
            GroupFieldError::InvalidDate,
        ),
    ];
    for (name, description, deadline, draw, field, error) in cases {
        let reply = update_group(&mut alice, w.g, &name, &description, &deadline, &draw).await;
        assert_eq!(reply.status, 422, "{field:?}/{error:?}");
        match reply.group_err() {
            GroupError::Validation(list) => {
                assert!(
                    list.contains(&(field, error)),
                    "{field:?}/{error:?} in {list:?}"
                )
            }
            other => panic!("expected Validation, got {other:?}"),
        }
        assert_eq!(groups_snapshot(&pool).await, before, "{field:?}/{error:?}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac21_an_old_group_with_past_dates_can_still_be_edited(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::set_group_dates(
        &pool,
        w.g,
        Some(utc(2019, 1, 1, 22, 59, 59)),
        Some(utc(2019, 1, 2, 11, 0, 0)),
    )
    .await;
    let mut alice = w.alice().await;
    update_group(&mut alice, w.g, "Renamed", "", "2019-01-01", "2019-01-02")
        .await
        .group_ok();
    assert_eq!(ts::group_row(&pool, w.g).await.name, "Renamed");
}

// ------------------------------------------------------------------------- AC22

#[sqlx::test(migrations = "../migrations")]
async fn ac22_delete_removes_the_group_and_every_child_row(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_member(&pool, w.g, w.carol).await;
    ts::create_wishlist(&pool, w.g, w.alice, &["Socks", "Book"]).await;
    ts::create_wishlist(&pool, w.g, w.bob, &["Tea"]).await;
    ts::add_exclusion(&pool, w.g, w.alice, w.bob).await;
    ts::add_exclusion(&pool, w.g, w.bob, w.carol).await;
    ts::add_assignment(&pool, w.g, w.alice, w.bob).await;
    ts::add_assignment(&pool, w.g, w.bob, w.carol).await;
    ts::add_assignment(&pool, w.g, w.carol, w.alice).await;
    ts::set_drawn(&pool, w.g).await;

    // A second group with the same people must survive untouched.
    let other = ts::create_group_with_code(&pool, w.alice, "Other", "OTHR2345").await;
    ts::add_member(&pool, other, w.bob).await;
    ts::create_wishlist(&pool, other, w.bob, &["Keep"]).await;
    ts::add_exclusion(&pool, other, w.alice, w.bob).await;
    ts::add_assignment(&pool, other, w.alice, w.bob).await;
    let other_before = (
        count(
            &pool,
            "SELECT COUNT(*) FROM group_members WHERE group_id = ?",
            other,
        )
        .await,
        count(
            &pool,
            "SELECT COUNT(*) FROM wishlists WHERE group_id = ?",
            other,
        )
        .await,
        count(
            &pool,
            "SELECT COUNT(*) FROM exclusion_rules WHERE group_id = ?",
            other,
        )
        .await,
        count(
            &pool,
            "SELECT COUNT(*) FROM gift_assignments WHERE group_id = ?",
            other,
        )
        .await,
    );

    let mut alice = w.alice().await;
    delete_group(&mut alice, w.g).await.group_ok();

    assert!(ts::find_group(&pool, w.g).await.is_none());
    for (table, sql) in [
        (
            "group_members",
            "SELECT COUNT(*) FROM group_members WHERE group_id = ?",
        ),
        (
            "wishlists",
            "SELECT COUNT(*) FROM wishlists WHERE group_id = ?",
        ),
        (
            "exclusion_rules",
            "SELECT COUNT(*) FROM exclusion_rules WHERE group_id = ?",
        ),
        (
            "gift_assignments",
            "SELECT COUNT(*) FROM gift_assignments WHERE group_id = ?",
        ),
    ] {
        assert_eq!(count(&pool, sql, w.g).await, 0, "{table}");
    }
    let items: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wishlist_items i JOIN wishlists w ON w.id = i.wishlist_id \
         WHERE w.group_id = ?",
    )
    .bind(w.g)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(items, 0, "wishlist_items");
    let orphans: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wishlist_items WHERE wishlist_id NOT IN (SELECT id FROM wishlists)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphans, 0);

    let other_after = (
        count(
            &pool,
            "SELECT COUNT(*) FROM group_members WHERE group_id = ?",
            other,
        )
        .await,
        count(
            &pool,
            "SELECT COUNT(*) FROM wishlists WHERE group_id = ?",
            other,
        )
        .await,
        count(
            &pool,
            "SELECT COUNT(*) FROM exclusion_rules WHERE group_id = ?",
            other,
        )
        .await,
        count(
            &pool,
            "SELECT COUNT(*) FROM gift_assignments WHERE group_id = ?",
            other,
        )
        .await,
    );
    assert_eq!(other_before, other_after);
    // The users are still there.
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users, 4);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_a_plain_form_post_ends_on_the_list_with_the_flash(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    alice.headers.push(("accept".into(), "text/html".into()));

    let reply = delete_group(&mut alice, w.g).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location().as_deref(), Some("/groups"));
    let page = alice.get("/groups").await;
    assert!(html_contains(
        &page.body,
        &en("flash.success.group_deleted")
    ));
    assert!(!page.body.contains("Family"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_a_deleted_group_is_gone_for_everyone(pool: MySqlPool) {
    let w = world(&pool).await;
    delete_group(&mut w.alice().await, w.g).await.group_ok();
    let mut bob = w.bob().await;
    assert_eq!(bob.get(&format!("/groups/{}", w.g)).await.status, 404);
    assert_eq!(
        get_group(&mut bob, w.g).await.group_err(),
        GroupError::NotFound
    );
    assert_eq!(
        join_group(&mut w.carol().await, CODE).await.group_err(),
        GroupError::InvalidCode
    );
    // Deleting twice: the admin no longer finds it.
    assert_eq!(
        delete_group(&mut w.alice().await, w.g).await.group_err(),
        GroupError::NotFound
    );
}

// ------------------------------------------------------------------------- AC23

#[sqlx::test(migrations = "../migrations")]
async fn ac23_only_the_admin_may_delete(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;

    let reply = delete_group(&mut w.bob().await, w.g).await;
    assert_eq!(reply.status, 403);
    assert_eq!(reply.group_err(), GroupError::Forbidden);
    let reply = delete_group(&mut w.dave().await, w.g).await;
    assert_eq!(reply.status, 404);
    assert_eq!(reply.group_err(), GroupError::NotFound);
    let reply = delete_group(&mut w.dave().await, 999_999).await;
    assert_eq!(reply.group_err(), GroupError::NotFound);

    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_the_admin_may_delete_a_drawn_group(pool: MySqlPool) {
    let w = world(&pool).await;
    ts::add_assignment(&pool, w.g, w.alice, w.bob).await;
    ts::add_assignment(&pool, w.g, w.bob, w.alice).await;
    ts::set_drawn(&pool, w.g).await;
    delete_group(&mut w.alice().await, w.g).await.group_ok();
    assert!(ts::find_group(&pool, w.g).await.is_none());
}

// ------------------------------------------------------------------------- AC24

#[sqlx::test(migrations = "../migrations")]
async fn ac24_a_get_never_deletes(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut alice = w.alice().await;

    for path in [
        format!("/api/delete_group?group_id={}", w.g),
        format!("/groups/{}/delete", w.g),
        format!("/groups/delete/{}", w.g),
        format!("/api/leave_group?group_id={}", w.g),
        format!("/api/regenerate_invitation?group_id={}", w.g),
        format!("/groups/{}/regenerate-invitation", w.g),
    ] {
        let reply = alice.get(&path).await;
        assert!(
            reply.status.is_client_error(),
            "{path} answered {}",
            reply.status
        );
    }
    assert_eq!(groups_snapshot(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac24_a_cross_origin_or_origin_less_post_never_deletes(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;
    let mut alice = w.alice().await;
    let cookie = format!(
        "{}={}",
        server::SESSION_COOKIE,
        alice.session_cookie().unwrap()
    );
    let body = form(&[("group_id", &w.g.to_string())]);

    for origin in [Some("https://evil.example"), Some("null"), None] {
        let req = post_from(origin, "/api/delete_group", Some(&cookie), &body);
        let res = send(&w.app, req).await;
        assert_eq!(res.status(), 403, "{origin:?}");
    }
    assert_eq!(groups_snapshot(&pool).await, before);

    // The same call from the right origin works, so the 403s were the origin check.
    delete_group(&mut alice, w.g).await.group_ok();
}

// ------------------------------------------------------------------------- AC25

#[sqlx::test(migrations = "../migrations")]
async fn ac25_regenerating_replaces_the_code_and_invalidates_the_old_one(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;

    let new: String = regenerate_invitation(&mut alice, w.g).await.value();
    assert!(is_valid_code(&new), "{new}");
    assert_ne!(new, CODE);
    assert_eq!(ts::group_code(&pool, w.g).await, new);

    let d = detail(&mut alice, w.g).await;
    let inv = d.invitation.unwrap();
    assert_eq!(inv.code, new);
    assert_eq!(inv.link, format!("{APP_URL}/join/{new}"));
    let page = alice.get(&format!("/groups/{}", w.g)).await;
    assert!(page.body.contains(&new) && !page.body.contains(CODE));

    // The old code is dead at once, the new one works.
    let mut carol = w.carol().await;
    assert_eq!(
        preview_invitation(&mut carol, CODE).await.group_err(),
        GroupError::InvalidCode
    );
    assert_eq!(
        join_group(&mut carol, CODE).await.group_err(),
        GroupError::InvalidCode
    );
    let old_page = carol.get(&format!("/join/{CODE}")).await;
    assert!(html_contains(
        &old_page.body,
        &en("flash.error.invalid_invitation_code")
    ));
    assert_eq!(join_group(&mut carol, &new).await.value::<i32>(), w.g);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac25_members_who_joined_stay_after_a_regeneration(pool: MySqlPool) {
    let w = world(&pool).await;
    regenerate_invitation(&mut w.alice().await, w.g)
        .await
        .group_ok();
    assert_eq!(ts::member_ids(&pool, w.g).await, vec![w.alice, w.bob]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac25_every_regeneration_gives_a_new_valid_code(pool: MySqlPool) {
    let w = world(&pool).await;
    let mut alice = w.alice().await;
    let mut seen = std::collections::BTreeSet::from([CODE.to_string()]);
    for _ in 0..10 {
        let code: String = regenerate_invitation(&mut alice, w.g).await.value();
        assert!(is_valid_code(&code));
        assert!(seen.insert(code), "a code repeated");
    }
}

// ------------------------------------------------------------------------- AC26

#[sqlx::test(migrations = "../migrations")]
async fn ac26_only_the_admin_may_regenerate(pool: MySqlPool) {
    let w = world(&pool).await;
    let before = groups_snapshot(&pool).await;

    let reply = regenerate_invitation(&mut w.bob().await, w.g).await;
    assert_eq!(reply.status, 403);
    assert_eq!(reply.group_err(), GroupError::Forbidden);
    let reply = regenerate_invitation(&mut w.dave().await, w.g).await;
    assert_eq!(reply.status, 404);
    assert_eq!(reply.group_err(), GroupError::NotFound);

    assert_eq!(ts::group_code(&pool, w.g).await, CODE);
    assert_eq!(groups_snapshot(&pool).await, before);
}
