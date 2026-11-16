//! group-archive (#61, docs/specs/group-archive.md): the archive clock, the sweep, manual
//! archive / restore / keep active, the read-only rules, the archive page and what members
//! may (not) see. Every test names the ACs it covers (`acN_...`).
//!
//! Time: the sweep is called directly with an explicit `now`; columns are set straight in the
//! DB where an AC gives times. Server fns that write `CURRENT_TIMESTAMP` (keep active,
//! restore, edit, reset, draw) use the real clock, so tests derive the following sweep times
//! from the value the DB stored.
//!
//! UI contract the tests rely on (the implementer must render these `data-testid`s; texts and
//! forms come from the spec):
//! - `archive-soon`: the admin's warning alert (`alert alert-warning`, `role="status"`),
//!   containing `group.show.archiveSoon` and the "Keep active" form
//!   (`<form ... action="/api/keep_group_active">`).
//! - `auto-archive-on`: the details card's `group.show.autoArchiveOn` line (admin only).
//! - `archived-notice`: the `alert alert-secondary` with `group.show.archivedNotice`.
//! - `archived-group`: one card per group on `/groups/archive`; `archive-empty`: the empty text.
//! - The no-JS forms are `<form method="post" action="/api/archive_group">` (also
//!   `restore_group`, `keep_group_active`) with a hidden `group_id`.
//! - Existing contracts stay: `my-assignment`, `assignment-receiver`, `draw-*` (draw.rs
//!   helpers), `group-invitation`, `group-wishlist`, `group-details`.
use std::sync::Arc;
use std::time::Duration;

use crate::common::auth::{Client, Reply, capture_logs, has_flash_key, html_contains, urlencode};
use crate::common::draw::{
    D, DrawReply, GROUP_NAME, MailKind, assert_only_own_assignment, block, delivery, draw_world,
    en_plural, kind, list_mine, perform, rename_group, resend, reset, testid_count, text_of,
};
use crate::common::exclusions::{ExReply, add_ex, page as ex_page, remove_ex};
use crate::common::groups::{
    APP_URL, CODE, GroupReply, de, de_with, en, en_with, get_group, join_group, leave_group,
    preview, regenerate_invitation, remove_member, transfer_admin, update_group,
};
use crate::common::wishlists::{
    WlReply, add_item, clear_wishlist, delete_item, get_my_wishlist, move_item, titles, update_item,
};
use crate::common::{post_from, send};
use app::domain::archive::{GroupClock, archive_dates};
use app::draw_api::DrawApiError;
use app::exclusions_api::ExclusionError;
use app::groups_api::{
    ArchivedGroupSummary, ClosedReason, GroupDetail, GroupError, GroupOp, GroupStatus, GroupSummary,
};
use app::i18n::{I18n, Locale, translate};
use app::server::archive_flow::{SweepResult, run_archive_sweep, spawn_archive_sweep};
use app::server::draw_worker::MailTransport;
use app::test_support::mailpit::Mail;
use app::test_support::{self as ts, test_config};
use app::wishlists_api::{CopySources, WishlistError, WishlistView};
use chrono::{DateTime, Duration as Dur, Months, TimeZone, Utc};
use sqlx::{MySql, MySqlPool, Transaction};
use tokio::task::JoinHandle;

// ------------------------------------------------------------------------------ helpers

fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap()
}

fn plus_months(t: DateTime<Utc>, n: u32) -> DateTime<Utc> {
    t.checked_add_months(Months::new(n)).unwrap()
}

fn minus_months(t: DateTime<Utc>, n: u32) -> DateTime<Utc> {
    t.checked_sub_months(Months::new(n)).unwrap()
}

/// The current time without sub-second part (what a TIMESTAMP column keeps).
fn now_secs() -> DateTime<Utc> {
    Utc.timestamp_opt(Utc::now().timestamp(), 0).unwrap()
}

fn assert_near_now(t: Option<DateTime<Utc>>, what: &str) {
    let t = t.unwrap_or_else(|| panic!("{what}: expected a timestamp, got NULL"));
    let drift = (Utc::now() - t).num_seconds().abs();
    assert!(drift < 120, "{what}: {t} is not the current time");
}

/// A date as the page / mail shows it in English / German (APP_TIMEZONE Europe/Berlin).
fn en_date(t: DateTime<Utc>) -> String {
    I18n::new(Locale::En, chrono_tz::Europe::Berlin).date(t)
}

fn de_date(t: DateTime<Utc>) -> String {
    I18n::new(Locale::De, chrono_tz::Europe::Berlin).date(t)
}

fn gp(g: i32) -> String {
    format!("/groups/{g}")
}

/// A browser doing a plain (no-JS) form post: `Accept: text/html`.
fn plain(c: &Client) -> Client {
    c.fork().header("accept", "text/html")
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_sweep(r: &SweepResult, warned: u64, archived: u64) {
    assert_eq!(
        (r.warned as u64, r.archived as u64),
        (warned, archived),
        "sweep result (warned, archived)"
    );
}

// -- server fn wrappers

async fn archive(c: &mut Client, g: i32) -> Reply {
    c.call("archive_group", &[("group_id", &g.to_string())])
        .await
}

async fn restore(c: &mut Client, g: i32) -> Reply {
    c.call("restore_group", &[("group_id", &g.to_string())])
        .await
}

async fn keep(c: &mut Client, g: i32) -> Reply {
    c.call("keep_group_active", &[("group_id", &g.to_string())])
        .await
}

async fn list_archived(c: &mut Client) -> Reply {
    c.call("list_archived_groups", &[]).await
}

async fn list_groups(c: &mut Client) -> Vec<GroupSummary> {
    c.call("list_my_groups", &[]).await.value()
}

// -- DB readers / writers

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct Stamps {
    archived_at: Option<DateTime<Utc>>,
    archive_warned_at: Option<DateTime<Utc>>,
    kept_active_at: Option<DateTime<Utc>>,
}

async fn stamps(pool: &MySqlPool, g: i32) -> Stamps {
    sqlx::query_as::<_, Stamps>(
        "SELECT archived_at, archive_warned_at, kept_active_at FROM `groups` WHERE id = ?",
    )
    .bind(g)
    .fetch_one(pool)
    .await
    .expect("read archive columns")
}

async fn set_stamps(
    pool: &MySqlPool,
    g: i32,
    archived_at: Option<DateTime<Utc>>,
    archive_warned_at: Option<DateTime<Utc>>,
    kept_active_at: Option<DateTime<Utc>>,
) {
    sqlx::query(
        "UPDATE `groups` SET archived_at = ?, archive_warned_at = ?, kept_active_at = ? \
         WHERE id = ?",
    )
    .bind(archived_at)
    .bind(archive_warned_at)
    .bind(kept_active_at)
    .bind(g)
    .execute(pool)
    .await
    .expect("set archive columns");
}

async fn set_archived_at(pool: &MySqlPool, g: i32, at: Option<DateTime<Utc>>) {
    sqlx::query("UPDATE `groups` SET archived_at = ? WHERE id = ?")
        .bind(at)
        .bind(g)
        .execute(pool)
        .await
        .expect("set archived_at");
}

async fn set_kept(pool: &MySqlPool, g: i32, at: Option<DateTime<Utc>>) {
    sqlx::query("UPDATE `groups` SET kept_active_at = ? WHERE id = ?")
        .bind(at)
        .bind(g)
        .execute(pool)
        .await
        .expect("set kept_active_at");
}

async fn set_drawn_state(pool: &MySqlPool, g: i32, drawn_at: Option<DateTime<Utc>>) {
    sqlx::query("UPDATE `groups` SET is_drawn = ?, drawn_at = ? WHERE id = ?")
        .bind(drawn_at.is_some())
        .bind(drawn_at)
        .bind(g)
        .execute(pool)
        .await
        .expect("set drawn state");
}

/// Marks `g` (members Alice, Bob, Carol) as drawn at `at` with the assignments
/// Alice -> Bob, Bob -> Carol, Carol -> Alice (no mails, no worker).
async fn make_drawn(d: &D, g: i32, at: DateTime<Utc>) {
    set_drawn_state(&d.pool, g, Some(at)).await;
    for (giver, receiver) in [(d.alice, d.bob), (d.bob, d.carol), (d.carol, d.alice)] {
        ts::add_assignment(&d.pool, g, giver, receiver).await;
    }
}

async fn item_ids(pool: &MySqlPool, g: i32, user: i32) -> Vec<i32> {
    sqlx::query_scalar(
        "SELECT i.id FROM wishlist_items i JOIN wishlists w ON w.id = i.wishlist_id \
         WHERE w.group_id = ? AND w.user_id = ? ORDER BY i.position, i.id",
    )
    .bind(g)
    .bind(user)
    .fetch_all(pool)
    .await
    .expect("item ids")
}

async fn invitation_code(pool: &MySqlPool, g: i32) -> String {
    ts::group_code(pool, g).await
}

// -- sweep and mail helpers

async fn sweep(d: &D, now: DateTime<Utc>) -> SweepResult {
    let config = test_config();
    run_archive_sweep(&d.pool, &config, &*d.transport, now).await
}

fn spawn_sweep(d: &D, now: DateTime<Utc>) -> JoinHandle<SweepResult> {
    let pool = d.pool.clone();
    let transport = d.transport.clone();
    let config = Arc::new(test_config());
    tokio::spawn(async move { run_archive_sweep(&pool, &config, &*transport, now).await })
}

/// Mails to `user` that are neither draw-result nor draw-reset mails.
async fn other_mails(d: &D, user: i32) -> Vec<Mail> {
    d.mails(user)
        .await
        .into_iter()
        .filter(|m| kind(m, GROUP_NAME) == MailKind::Other)
        .collect()
}

async fn all_mail_count(d: &D, user: i32) -> usize {
    d.mailpit.messages_for(d.email_of(user)).await.len()
}

fn warning_subject(group: &str, date: &str) -> String {
    en_with(
        "email.archiveWarning.subject",
        &[("group", group), ("date", date)],
    )
}

/// G (Family): Alice admin, Bob, Carol; drawn at `drawn_at` with three assignments.
async fn drawn_world(pool: &MySqlPool, drawn_at: DateTime<Utc>) -> D {
    let d = draw_world(pool).await;
    make_drawn(&d, d.g, drawn_at).await;
    d
}

/// G drawn, with wishlists and a rule, then archived by Alice through the server fn.
async fn archived_world(pool: &MySqlPool) -> D {
    let d = drawn_world(pool, utc(2026, 5, 14, 10, 0, 0)).await;
    ts::create_wishlist(pool, d.g, d.bob, &["Socks", "Book"]).await;
    ts::create_wishlist(pool, d.g, d.alice, &["Mug"]).await;
    ts::add_exclusion(pool, d.g, d.bob, d.alice).await;
    archive(&mut d.alice().await, d.g).await.group_ok();
    d
}

/// Sets `g` into its warning window right now: drawn `6 months - 7 days` ago, so the
/// archive date is in 7 days.
async fn into_window(d: &D, g: i32) -> DateTime<Utc> {
    let drawn_at = minus_months(now_secs() + Dur::days(7), 6);
    make_drawn(d, g, drawn_at).await;
    drawn_at
}

async fn hold_lock(pool: &MySqlPool, group: i32) -> Transaction<'static, MySql> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM `groups` WHERE id = ? FOR UPDATE")
        .bind(group)
        .fetch_one(&mut *tx)
        .await
        .expect("lock the group row");
    tx
}

async fn assert_waiting<T>(handle: &JoinHandle<T>, what: &str) {
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !handle.is_finished(),
        "{what} must wait for the group row lock"
    );
}

fn clock() -> GroupClock {
    GroupClock {
        is_drawn: false,
        drawn_at: None,
        created_at: utc(2026, 1, 1, 0, 0, 0),
        registration_deadline: None,
        draw_date: None,
        kept_active_at: None,
    }
}

// =========================================================================== AC1: migration

fn migration_sql() -> String {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../migrations");
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .expect("migrations dir")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_group_archive.sql"))
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one NNNN_group_archive.sql: {found:?}"
    );
    std::fs::read_to_string(found.pop().unwrap()).expect("read migration")
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct FullGroupRow {
    id: i32,
    name: String,
    description: Option<String>,
    admin_id: i32,
    invitation_code: String,
    registration_deadline: Option<DateTime<Utc>>,
    draw_date: Option<DateTime<Utc>>,
    is_drawn: bool,
    drawn_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

async fn full_rows(pool: &MySqlPool) -> Vec<FullGroupRow> {
    sqlx::query_as::<_, FullGroupRow>(
        "SELECT id, name, description, admin_id, invitation_code, registration_deadline, \
         draw_date, is_drawn, drawn_at, created_at, updated_at FROM `groups` ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("read groups")
}

/// The database is taken back to the schema before the migration (the three columns
/// dropped), filled with drawn and undrawn groups and everything that hangs off them, and the
/// migration file is applied to it. Independent of the migration's number.
#[sqlx::test(migrations = "../migrations")]
async fn ac1_the_migration_is_additive_keeps_every_row_and_backfills_undrawn_groups(
    pool: MySqlPool,
) {
    let sql = migration_sql();
    sqlx::query(
        "ALTER TABLE `groups` DROP COLUMN kept_active_at, DROP COLUMN archive_warned_at, \
         DROP COLUMN archived_at",
    )
    .execute(&pool)
    .await
    .expect("go back to the pre-migration schema");

    let alice = ts::create_user(&pool, "Alice", "alice-mig@example.test").await;
    let bob = ts::create_user(&pool, "Bob", "bob-mig@example.test").await;
    let carol = ts::create_user(&pool, "Carol", "carol-mig@example.test").await;
    let drawn = ts::create_group_with_code(&pool, alice, "Drawn", "DRWN2345").await;
    let undrawn = ts::create_group_with_code(&pool, alice, "Undrawn", "UNDR2345").await;
    let reset_one = ts::create_group_with_code(&pool, bob, "Reset", "RSET2345").await;
    for g in [drawn, undrawn] {
        ts::add_member(&pool, g, bob).await;
        ts::add_member(&pool, g, carol).await;
    }
    ts::add_member(&pool, reset_one, carol).await;
    set_drawn_state(&pool, drawn, Some(utc(2025, 12, 10, 18, 0, 0))).await;
    ts::add_assignment(&pool, drawn, alice, bob).await;
    ts::add_assignment(&pool, drawn, bob, carol).await;
    ts::add_assignment(&pool, drawn, carol, alice).await;
    ts::create_wishlist(&pool, drawn, bob, &["Socks", "Book"]).await;
    ts::create_wishlist(&pool, undrawn, carol, &["Mug"]).await;
    ts::add_exclusion(&pool, undrawn, alice, bob).await;
    ts::set_group_dates(
        &pool,
        undrawn,
        Some(utc(2025, 12, 1, 22, 59, 59)),
        Some(utc(2025, 12, 12, 11, 0, 0)),
    )
    .await;
    // Explicit updated_at values (an explicit assignment suppresses ON UPDATE).
    let stamps_in = [
        (drawn, utc(2025, 12, 10, 18, 0, 0)),
        (undrawn, utc(2026, 11, 3, 8, 30, 0)),
        (reset_one, utc(2026, 3, 4, 5, 6, 7)),
    ];
    for (g, at) in stamps_in {
        sqlx::query("UPDATE `groups` SET updated_at = ? WHERE id = ?")
            .bind(at)
            .bind(g)
            .execute(&pool)
            .await
            .unwrap();
    }

    let rows_before = full_rows(&pool).await;
    let snapshot_before = ts::groups_snapshot(&pool).await;
    assert_eq!(rows_before.len(), 3);

    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(&pool)
        .await
        .expect("the migration applies to a populated database");

    // Every existing value (updated_at included) and every child row is unchanged.
    assert_eq!(full_rows(&pool).await, rows_before);
    assert_eq!(ts::groups_snapshot(&pool).await, snapshot_before);

    // The columns: nullable TIMESTAMP, no default expression, no ON UPDATE.
    let columns: Vec<(String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT CAST(column_name AS CHAR), CAST(data_type AS CHAR), CAST(is_nullable AS CHAR), \
         CAST(column_default AS CHAR), CAST(extra AS CHAR) FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = 'groups' \
         AND column_name IN ('archived_at', 'archive_warned_at', 'kept_active_at') \
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(columns.len(), 3, "three new columns: {columns:?}");
    for (name, data_type, nullable, default, extra) in &columns {
        assert_eq!(data_type.to_lowercase(), "timestamp", "{name}");
        assert_eq!(nullable, "YES", "{name}");
        assert!(
            default
                .as_deref()
                .is_none_or(|d| d.eq_ignore_ascii_case("null")),
            "{name}: default {default:?}"
        );
        assert!(
            !extra.to_lowercase().contains("on update"),
            "{name}: extra {extra:?}"
        );
    }

    // Values: nothing archived or warned; groups that are not drawn (never drawn or reset)
    // start their clock at the migration (U10, revised 2026-11-16: a fresh start, not
    // updated_at, which the PHP import kept from the old app); drawn groups keep NULL.
    for g in [drawn, undrawn, reset_one] {
        let s = stamps(&pool, g).await;
        assert_eq!(s.archived_at, None, "group {g}");
        assert_eq!(s.archive_warned_at, None, "group {g}");
    }
    assert_eq!(stamps(&pool, drawn).await.kept_active_at, None);
    for g in [undrawn, reset_one] {
        assert_near_now(
            stamps(&pool, g).await.kept_active_at,
            &format!("kept_active_at of undrawn group {g}"),
        );
    }
}

// ================================================================== AC2, AC3: the pure dates

#[test]
fn ac2_a_drawn_group_is_archived_six_months_after_the_draw_and_warned_14_days_before() {
    let c = GroupClock {
        is_drawn: true,
        drawn_at: Some(utc(2026, 8, 31, 12, 0, 0)),
        ..clock()
    };
    let (warn, archive) = archive_dates(&c);
    assert_eq!(
        archive,
        utc(2027, 2, 28, 12, 0, 0),
        "clamped to the month's end"
    );
    assert_eq!(warn, utc(2027, 2, 14, 12, 0, 0));

    let c = GroupClock {
        is_drawn: true,
        drawn_at: Some(utc(2026, 5, 14, 10, 0, 0)),
        ..clock()
    };
    assert_eq!(
        archive_dates(&c),
        (utc(2026, 10, 31, 10, 0, 0), utc(2026, 11, 14, 10, 0, 0))
    );
}

#[test]
fn ac2_the_later_of_drawn_at_and_kept_active_at_is_the_reference() {
    let drawn = utc(2026, 5, 14, 10, 0, 0);
    let later = GroupClock {
        is_drawn: true,
        drawn_at: Some(drawn),
        kept_active_at: Some(utc(2026, 9, 1, 8, 0, 0)),
        ..clock()
    };
    assert_eq!(archive_dates(&later).1, utc(2027, 3, 1, 8, 0, 0));
    let earlier = GroupClock {
        kept_active_at: Some(utc(2026, 1, 1, 8, 0, 0)),
        ..later.clone()
    };
    assert_eq!(archive_dates(&earlier).1, utc(2026, 11, 14, 10, 0, 0));
    // The dates of a drawn group do not depend on created_at or the planned dates.
    let noisy = GroupClock {
        created_at: utc(2030, 1, 1, 0, 0, 0),
        registration_deadline: Some(utc(2031, 1, 1, 0, 0, 0)),
        draw_date: Some(utc(2032, 1, 1, 0, 0, 0)),
        kept_active_at: None,
        ..earlier
    };
    assert_eq!(archive_dates(&noisy).1, utc(2026, 11, 14, 10, 0, 0));
}

#[test]
fn ac3_an_undrawn_group_is_archived_nine_months_after_its_latest_reference() {
    let created = GroupClock {
        created_at: utc(2026, 5, 1, 10, 0, 0),
        ..clock()
    };
    assert_eq!(
        archive_dates(&created),
        (utc(2027, 1, 18, 10, 0, 0), utc(2027, 2, 1, 10, 0, 0))
    );
    let with_draw_date = GroupClock {
        draw_date: Some(utc(2026, 12, 12, 11, 0, 0)),
        ..created.clone()
    };
    assert_eq!(archive_dates(&with_draw_date).1, utc(2027, 9, 12, 11, 0, 0));
    let with_deadline = GroupClock {
        registration_deadline: Some(utc(2026, 12, 1, 22, 59, 59)),
        ..created.clone()
    };
    assert_eq!(archive_dates(&with_deadline).1, utc(2027, 9, 1, 22, 59, 59));
    let end_of_month = GroupClock {
        created_at: utc(2026, 5, 31, 12, 0, 0),
        ..clock()
    };
    assert_eq!(archive_dates(&end_of_month).1, utc(2027, 2, 28, 12, 0, 0));
}

#[test]
fn ac3_kept_active_at_and_every_date_count_when_they_are_the_latest() {
    let base = GroupClock {
        created_at: utc(2026, 5, 1, 10, 0, 0),
        registration_deadline: Some(utc(2026, 6, 1, 10, 0, 0)),
        draw_date: Some(utc(2026, 7, 1, 10, 0, 0)),
        kept_active_at: Some(utc(2026, 8, 1, 10, 0, 0)),
        ..clock()
    };
    assert_eq!(
        archive_dates(&base).1,
        utc(2027, 5, 1, 10, 0, 0),
        "kept_active_at"
    );
    let no_kept = GroupClock {
        kept_active_at: None,
        ..base.clone()
    };
    assert_eq!(
        archive_dates(&no_kept).1,
        utc(2027, 4, 1, 10, 0, 0),
        "draw_date"
    );
    let no_draw_date = GroupClock {
        draw_date: None,
        ..no_kept.clone()
    };
    assert_eq!(
        archive_dates(&no_draw_date).1,
        utc(2027, 3, 1, 10, 0, 0),
        "deadline"
    );
    let only_created = GroupClock {
        registration_deadline: None,
        ..no_draw_date
    };
    assert_eq!(
        archive_dates(&only_created).1,
        utc(2027, 2, 1, 10, 0, 0),
        "created_at"
    );
}

#[test]
fn ac3_is_drawn_without_drawn_at_counts_as_not_drawn() {
    let c = GroupClock {
        is_drawn: true,
        drawn_at: None,
        created_at: utc(2026, 5, 1, 10, 0, 0),
        ..clock()
    };
    assert_eq!(archive_dates(&c).1, utc(2027, 2, 1, 10, 0, 0));
    // And a group that is not drawn ignores a stale drawn_at.
    let c = GroupClock {
        is_drawn: false,
        drawn_at: Some(utc(2030, 1, 1, 0, 0, 0)),
        created_at: utc(2026, 5, 1, 10, 0, 0),
        ..clock()
    };
    assert_eq!(archive_dates(&c).1, utc(2027, 2, 1, 10, 0, 0));
}

/// Property check over many random clocks: the warning is always 14 days before the archive
/// date, and the archive date is N months after the latest reference.
#[test]
fn ac2_ac3_the_dates_follow_the_rules_for_many_random_clocks() {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let base = utc(2024, 1, 1, 0, 0, 0).timestamp();
    let span: u64 = 6 * 365 * 24 * 3600;
    fn pick(base: i64, span: u64, r: u64) -> DateTime<Utc> {
        Utc.timestamp_opt(base + (r % span) as i64, 0).unwrap()
    }
    for _ in 0..500 {
        let created_at = pick(base, span, next());
        let mut opt = || {
            let v = pick(base, span, next());
            (next() % 3 != 0).then_some(v)
        };
        let drawn_at = opt();
        let registration_deadline = opt();
        let draw_date = opt();
        let kept_active_at = opt();
        let is_drawn = next() % 2 == 0;
        let c = GroupClock {
            is_drawn,
            drawn_at,
            created_at,
            registration_deadline,
            draw_date,
            kept_active_at,
        };
        let (warn, archive) = archive_dates(&c);
        assert_eq!(archive - warn, Dur::days(14), "{c:?}");
        let latest = |values: &[Option<DateTime<Utc>>]| values.iter().flatten().max().copied();
        let expected = if is_drawn && drawn_at.is_some() {
            plus_months(latest(&[drawn_at, kept_active_at]).unwrap(), 6)
        } else {
            plus_months(
                latest(&[
                    Some(created_at),
                    registration_deadline,
                    draw_date,
                    kept_active_at,
                ])
                .unwrap(),
                9,
            )
        };
        assert_eq!(archive, expected, "{c:?}");
    }
}

// ============================================================================= AC4, AC5: writes

/// An old `kept_active_at`, so "unchanged" and "set to now" can be told apart.
fn old_kept() -> DateTime<Utc> {
    utc(2026, 1, 2, 3, 4, 5)
}

async fn assert_kept_unchanged(pool: &MySqlPool, g: i32, what: &str) {
    assert_eq!(
        stamps(pool, g).await.kept_active_at,
        Some(old_kept()),
        "{what} must not touch kept_active_at"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac4_keep_update_restore_and_reset_set_kept_active_at_to_now(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;

    // keep_group_active
    set_kept(&pool, d.g, Some(old_kept())).await;
    keep(&mut alice, d.g).await.group_ok();
    assert_near_now(stamps(&pool, d.g).await.kept_active_at, "keep_group_active");

    // update_group
    set_kept(&pool, d.g, Some(old_kept())).await;
    update_group(&mut alice, d.g, "Family", "", "", "")
        .await
        .group_ok();
    assert_near_now(stamps(&pool, d.g).await.kept_active_at, "update_group");

    // restore_group on an archived group
    archive(&mut alice, d.g).await.group_ok();
    set_kept(&pool, d.g, Some(old_kept())).await;
    restore(&mut alice, d.g).await.group_ok();
    assert_near_now(stamps(&pool, d.g).await.kept_active_at, "restore_group");

    // reset_draw
    make_drawn(&d, d.g, utc(2026, 5, 14, 10, 0, 0)).await;
    set_kept(&pool, d.g, Some(old_kept())).await;
    reset(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    assert_near_now(stamps(&pool, d.g).await.kept_active_at, "reset_draw");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac4_other_actions_leave_kept_active_at_unchanged(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut alice = d.alice().await;
    let mut bob = d.bob().await;
    set_kept(&pool, d.g, Some(old_kept())).await;
    join_group(&mut d.dave().await, CODE).await.group_ok();
    assert_kept_unchanged(&pool, d.g, "join_group").await;
    add_item(&mut bob, d.g, "Socks", "", "").await.wl_ok();
    assert_kept_unchanged(&pool, d.g, "add_item").await;
    add_ex(&mut bob, d.g, d.carol).await.ex_ok();
    assert_kept_unchanged(&pool, d.g, "add_exclusion").await;
    regenerate_invitation(&mut alice, d.g).await.group_ok();
    assert_kept_unchanged(&pool, d.g, "regenerate_invitation").await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    assert_kept_unchanged(&pool, d.g, "perform_draw").await;
    transfer_admin(&mut alice, d.g, d.bob).await.group_ok();
    assert_kept_unchanged(&pool, d.g, "transfer_admin").await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_editing_an_old_undrawn_group_restarts_its_clock_before_any_sweep(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let ten_months_ago = minus_months(now_secs(), 10);
    ts::set_group_created_at(&pool, d.g, ten_months_ago).await;
    set_kept(&pool, d.g, Some(ten_months_ago)).await;
    let mut alice = d.alice().await;

    update_group(&mut alice, d.g, "Family", "", "2020-01-01", "2020-01-02")
        .await
        .group_ok();

    let g: GroupDetail = get_group(&mut alice, d.g).await.value();
    let schedule = g.archive_schedule.expect("the admin sees the schedule");
    let expected = plus_months(Utc::now(), 9);
    assert!(
        (schedule.auto_archive_at - expected).num_seconds().abs() < 120,
        "nine months after the save, got {}",
        schedule.auto_archive_at
    );
    assert!(!schedule.archive_soon);
    let r = sweep(&d, Utc::now()).await;
    assert_sweep(&r, 0, 0);
    assert_eq!(stamps(&pool, d.g).await.archived_at, None);
    assert_eq!(all_mail_count(&d, d.alice).await, 0);
}

// ==================================================================== AC6, AC7, AC8: warn + archive

#[sqlx::test(migrations = "../migrations")]
async fn ac6_the_sweep_warns_once_at_the_warning_date_and_archives_at_the_archive_date(
    pool: MySqlPool,
) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;

    let r = sweep(&d, utc(2026, 10, 31, 9, 59, 59)).await;
    assert_sweep(&r, 0, 0);
    assert_eq!(all_mail_count(&d, d.alice).await, 0);
    assert_eq!(stamps(&pool, d.g).await.archive_warned_at, None);

    let warn_time = utc(2026, 10, 31, 10, 0, 0);
    let r = sweep(&d, warn_time).await;
    assert_sweep(&r, 1, 0);
    assert_eq!(all_mail_count(&d, d.alice).await, 1);
    let s = stamps(&pool, d.g).await;
    assert_eq!(s.archive_warned_at, Some(warn_time));
    assert_eq!(s.archived_at, None);

    let r = sweep(&d, utc(2026, 11, 1, 10, 0, 0)).await;
    assert_sweep(&r, 0, 0);
    assert_eq!(all_mail_count(&d, d.alice).await, 1, "no second warning");

    let r = sweep(&d, utc(2026, 11, 14, 9, 59, 59)).await;
    assert_sweep(&r, 0, 0);
    assert_eq!(stamps(&pool, d.g).await.archived_at, None);

    let archive_time = utc(2026, 11, 14, 10, 0, 0);
    let r = sweep(&d, archive_time).await;
    assert_sweep(&r, 0, 1);
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(archive_time));
    assert_eq!(
        all_mail_count(&d, d.alice).await,
        1,
        "no mail on the archive date itself"
    );
    for member in [d.bob, d.carol] {
        assert_eq!(all_mail_count(&d, member).await, 0, "members get no mail");
    }

    // An archived group is left alone by later sweeps.
    let r = sweep(&d, utc(2027, 6, 1, 10, 0, 0)).await;
    assert_sweep(&r, 0, 0);
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(archive_time));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_a_group_inside_its_window_is_archived_without_a_warning_when_the_sweep_is_late(
    pool: MySqlPool,
) {
    // The server was down for the whole window: archive, no warning (A6 step 1).
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let r = sweep(&d, utc(2026, 11, 20, 10, 0, 0)).await;
    assert_sweep(&r, 0, 1);
    assert_eq!(all_mail_count(&d, d.alice).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac7_the_warning_mail_goes_to_the_stored_address_and_has_the_agreed_content(
    pool: MySqlPool,
) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let r = sweep(&d, utc(2026, 10, 31, 10, 0, 0)).await;
    assert_sweep(&r, 1, 0);

    let mails = other_mails(&d, d.alice).await;
    assert_eq!(mails.len(), 1);
    let mail = &mails[0];
    assert_eq!(mail.subject, warning_subject(GROUP_NAME, "14 Nov 2026"));
    assert_eq!(
        mail.to[0].address.to_lowercase(),
        d.emails[0].to_lowercase()
    );
    assert!(!mail.text.trim().is_empty(), "a plain-text part");
    assert!(html_contains(
        &mail.html,
        &en_with("email.archiveWarning.greeting", &[("name", "Alice")])
    ));
    let body = en_with(
        "email.archiveWarning.body",
        &[("group", GROUP_NAME), ("date", "14 Nov 2026")],
    );
    for part in [&mail.html, &mail.text] {
        let part = norm(part);
        for (key, text) in [
            ("body", body.clone()),
            ("keep", en("email.archiveWarning.keep")),
            ("nothing", en("email.archiveWarning.nothing")),
        ] {
            assert!(
                html_contains(&part, &norm(&text)),
                "email.archiveWarning.{key} missing in {part}"
            );
        }
        assert!(
            part.contains(&format!("{APP_URL}/groups/{}", d.g)),
            "link to the group in {part}"
        );
    }
    assert!(
        norm(&mail.text).contains(&norm(&en("email.common.signature"))),
        "the signature in the text part"
    );
    assert!(html_contains(
        &mail.html,
        &en("email.archiveWarning.button")
    ));
    // The mail names no receiver of any assignment.
    for name in ["Bob", "Carol"] {
        assert!(
            !format!("{}\n{}\n{}", mail.subject, mail.text, mail.html).contains(name),
            "the mail mentions {name}"
        );
    }
    for member in [d.bob, d.carol] {
        assert_eq!(all_mail_count(&d, member).await, 0);
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac7_the_group_name_is_html_escaped_in_the_mail(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    rename_group(&pool, d.g, "<b>x</b>").await;
    sweep(&d, utc(2026, 10, 31, 10, 0, 0)).await;
    let mail = other_mails(&d, d.alice).await.remove(0);
    assert!(mail.html.contains("&lt;b&gt;x&lt;/b&gt;"), "{}", mail.html);
    assert!(!mail.html.contains("<b>x</b>"), "{}", mail.html);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac7_a_german_admin_gets_a_german_mail_with_a_german_date(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    ts::set_user_locale(&pool, d.alice, Some("de")).await;
    sweep(&d, utc(2026, 10, 31, 10, 0, 0)).await;
    let mail = other_mails(&d, d.alice).await.remove(0);
    assert_eq!(
        mail.subject,
        de_with(
            "email.archiveWarning.subject",
            &[("group", GROUP_NAME), ("date", "14.11.2026")]
        )
    );
    assert!(
        html_contains(
            &norm(&mail.html),
            &norm(&de("email.archiveWarning.nothing"))
        ),
        "{}",
        mail.html
    );
    assert!(mail.html.contains("lang=\"de\""), "{}", mail.html);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac7_the_date_is_the_local_date_in_app_timezone(pool: MySqlPool) {
    // 2026-11-14 23:30 UTC is already 15 Nov in Europe/Berlin.
    let d = drawn_world(&pool, utc(2026, 5, 14, 23, 30, 0)).await;
    sweep(&d, utc(2026, 10, 31, 23, 30, 0)).await;
    let mail = other_mails(&d, d.alice).await.remove(0);
    assert_eq!(mail.subject, warning_subject(GROUP_NAME, "15 Nov 2026"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_the_warning_goes_to_the_admin_at_the_time_of_the_warning(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    transfer_admin(&mut d.alice().await, d.g, d.bob)
        .await
        .group_ok();
    let r = sweep(&d, utc(2026, 10, 31, 10, 0, 0)).await;
    assert_sweep(&r, 1, 0);
    assert_eq!(
        all_mail_count(&d, d.alice).await,
        0,
        "the old admin gets nothing"
    );
    let mails = other_mails(&d, d.bob).await;
    assert_eq!(mails.len(), 1);
    assert!(html_contains(
        &mails[0].html,
        &en_with("email.archiveWarning.greeting", &[("name", "Bob")])
    ));
}

// ===================================================================== AC9: the deploy backlog

#[sqlx::test(migrations = "../migrations")]
async fn ac9_groups_past_their_archive_date_are_archived_at_once_without_a_mail(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2025, 12, 10, 18, 0, 0)).await;
    // An undrawn group whose reference is more than nine months back, no warning.
    let h = ts::create_group(&pool, d.alice, "Old and quiet").await;
    ts::set_group_created_at(&pool, h, utc(2025, 1, 1, 10, 0, 0)).await;

    let now = utc(2026, 11, 20, 10, 0, 0);
    let r = sweep(&d, now).await;
    assert_sweep(&r, 0, 2);
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(now));
    assert_eq!(stamps(&pool, h).await.archived_at, Some(now));
    assert_eq!(all_mail_count(&d, d.alice).await, 0);
    assert_eq!(stamps(&pool, d.g).await.archive_warned_at, None);
}

// ============================================================ AC10, AC11, AC12: restarting the clock

#[sqlx::test(migrations = "../migrations")]
async fn ac10_keep_active_moves_the_archive_date_and_the_next_warning(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    sweep(&d, utc(2026, 10, 31, 10, 0, 0)).await; // warned at W
    assert_eq!(other_mails(&d, d.alice).await.len(), 1);

    keep(&mut d.alice().await, d.g).await.group_ok();
    let k = stamps(&pool, d.g)
        .await
        .kept_active_at
        .expect("kept_active_at");

    let r = sweep(&d, utc(2026, 11, 14, 10, 0, 0)).await;
    assert_sweep(&r, 0, 0);
    assert_eq!(
        stamps(&pool, d.g).await.archived_at,
        None,
        "not at the old date"
    );

    let warn_at = plus_months(k, 6) - Dur::days(14);
    let r = sweep(&d, warn_at - Dur::seconds(1)).await;
    assert_sweep(&r, 0, 0);
    let r = sweep(&d, warn_at).await;
    assert_sweep(&r, 1, 0);
    assert_eq!(
        other_mails(&d, d.alice).await.len(),
        2,
        "exactly one new warning"
    );
    let r = sweep(&d, warn_at + Dur::days(1)).await;
    assert_sweep(&r, 0, 0);
    assert_eq!(other_mails(&d, d.alice).await.len(), 2);

    let archive_at = plus_months(k, 6);
    let r = sweep(&d, archive_at).await;
    assert_sweep(&r, 0, 1);
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(archive_at));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_a_restored_group_gets_a_fresh_period_and_a_new_warning(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let mut alice = d.alice().await;
    archive(&mut alice, d.g).await.group_ok();
    restore(&mut alice, d.g).await.group_ok();
    let r = stamps(&pool, d.g)
        .await
        .kept_active_at
        .expect("restore sets kept_active_at");

    let result = sweep(&d, r + Dur::hours(1)).await;
    assert_sweep(&result, 0, 0);
    assert_eq!(all_mail_count(&d, d.alice).await, 0);

    let warn_at = plus_months(r, 6) - Dur::days(14);
    assert_sweep(&sweep(&d, warn_at).await, 1, 0);
    assert_eq!(other_mails(&d, d.alice).await.len(), 1);
    let archive_at = plus_months(r, 6);
    assert_sweep(&sweep(&d, archive_at).await, 0, 1);
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(archive_at));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_resetting_the_draw_restarts_the_clock_with_nine_months(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    make_drawn(&d, d.g, minus_months(now_secs(), 7)).await;
    reset(&mut d.alice().await, d.g).await.dr_ok();
    d.idle().await;
    let x = stamps(&pool, d.g)
        .await
        .kept_active_at
        .expect("reset sets kept_active_at");

    // Drawn seven months ago would be due; after the reset nothing happens.
    assert_sweep(&sweep(&d, x + Dur::hours(1)).await, 0, 0);
    assert_eq!(stamps(&pool, d.g).await.archived_at, None);

    let before = other_mails(&d, d.alice).await.len();
    let warn_at = plus_months(x, 9) - Dur::days(14);
    assert_sweep(&sweep(&d, warn_at).await, 1, 0);
    assert_eq!(other_mails(&d, d.alice).await.len(), before + 1);
    let archive_at = plus_months(x, 9);
    assert_sweep(&sweep(&d, archive_at).await, 0, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_drawing_again_moves_the_clock_to_six_months_after_the_new_draw(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    make_drawn(&d, d.g, minus_months(now_secs(), 7)).await;
    let mut alice = d.alice().await;
    reset(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    perform(&mut alice, d.g).await.dr_ok();
    d.idle().await;
    let t = ts::group_row(&pool, d.g).await;
    assert_eq!(t.is_drawn, Some(true));
    let drawn_at: DateTime<Utc> = sqlx::query_scalar("SELECT drawn_at FROM `groups` WHERE id = ?")
        .bind(d.g)
        .fetch_one(&pool)
        .await
        .unwrap();

    let before = other_mails(&d, d.alice).await.len();
    // The archive date is at least six months after the draw (kept_active_at is not later).
    let warn_at = plus_months(drawn_at, 6) - Dur::days(14);
    assert_sweep(&sweep(&d, warn_at - Dur::days(1)).await, 0, 0);
    assert_sweep(&sweep(&d, warn_at).await, 1, 0);
    assert_eq!(other_mails(&d, d.alice).await.len(), before + 1);
    assert_sweep(&sweep(&d, plus_months(drawn_at, 6)).await, 0, 1);
}

// ================================================================ AC13: a group in preparation

#[sqlx::test(migrations = "../migrations")]
async fn ac13_a_group_in_preparation_is_not_archived_before_its_draw(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    ts::set_group_created_at(&pool, d.g, utc(2026, 5, 1, 10, 0, 0)).await;
    assert_sweep(&sweep(&d, utc(2026, 11, 1, 10, 0, 0)).await, 0, 0);
    assert_sweep(&sweep(&d, utc(2026, 12, 11, 10, 0, 0)).await, 0, 0);
    assert_eq!(all_mail_count(&d, d.alice).await, 0);

    // Drawn at 2026-12-12 11:00: the archive date becomes 2027-06-12 11:00.
    make_drawn(&d, d.g, utc(2026, 12, 12, 11, 0, 0)).await;
    assert_sweep(&sweep(&d, utc(2027, 5, 29, 10, 59, 59)).await, 0, 0);
    assert_sweep(&sweep(&d, utc(2027, 5, 29, 11, 0, 0)).await, 1, 0);
    assert_sweep(&sweep(&d, utc(2027, 6, 12, 10, 59, 59)).await, 0, 0);
    assert_sweep(&sweep(&d, utc(2027, 6, 12, 11, 0, 0)).await, 0, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_left_undrawn_the_group_is_warned_once_and_archived_after_nine_months(
    pool: MySqlPool,
) {
    let d = draw_world(&pool).await;
    ts::set_group_created_at(&pool, d.g, utc(2026, 5, 1, 10, 0, 0)).await;

    assert_sweep(&sweep(&d, utc(2027, 1, 18, 10, 0, 0)).await, 1, 0);
    let mails = other_mails(&d, d.alice).await;
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].subject, warning_subject(GROUP_NAME, "1 Feb 2027"));
    assert_sweep(&sweep(&d, utc(2027, 1, 20, 10, 0, 0)).await, 0, 0);
    assert_eq!(other_mails(&d, d.alice).await.len(), 1);

    let at = utc(2027, 2, 1, 10, 0, 0);
    assert_sweep(&sweep(&d, at).await, 0, 1);
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(at));
    assert_eq!(
        other_mails(&d, d.alice).await.len(),
        1,
        "no mail on the archive date"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_a_planned_draw_date_keeps_an_undrawn_group_active(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    ts::set_group_created_at(&pool, d.g, utc(2026, 5, 1, 10, 0, 0)).await;
    ts::set_group_dates(&pool, d.g, None, Some(utc(2026, 12, 12, 11, 0, 0))).await;
    assert_sweep(&sweep(&d, utc(2027, 1, 18, 10, 0, 0)).await, 0, 0);
    assert_sweep(&sweep(&d, utc(2027, 2, 1, 10, 0, 0)).await, 0, 0);
    assert_eq!(stamps(&pool, d.g).await.archived_at, None);
    assert_eq!(all_mail_count(&d, d.alice).await, 0);
}

// ========================================================== AC14, AC15, AC16, AC17: robustness

#[sqlx::test(migrations = "../migrations")]
async fn ac14_two_concurrent_sweeps_warn_once(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let now = utc(2026, 11, 1, 10, 0, 0);
    let (a, b) = tokio::join!(spawn_sweep(&d, now), spawn_sweep(&d, now));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.warned as u64 + b.warned as u64, 1, "one warning in total");
    assert_eq!(a.archived as u64 + b.archived as u64, 0);
    assert_eq!(all_mail_count(&d, d.alice).await, 1);
    assert_eq!(d.transport.attempts_to(&d.emails[0]), 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_two_concurrent_sweeps_archive_once_and_keep_the_first_value(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let first = utc(2026, 11, 20, 10, 0, 0);
    let second = first + Dur::seconds(1);
    let (a, b) = tokio::join!(spawn_sweep(&d, first), spawn_sweep(&d, second));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.archived as u64 + b.archived as u64, 1, "archived once");
    let archived_at = stamps(&pool, d.g).await.archived_at.expect("archived");
    assert!(
        archived_at == first || archived_at == second,
        "{archived_at}"
    );
    assert_sweep(&sweep(&d, second + Dur::days(1)).await, 0, 0);
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(archived_at));
    assert_eq!(all_mail_count(&d, d.alice).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_failed_warning_is_undone_and_retried_by_the_next_sweep(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    d.transport.fail(&d.emails[0]);
    let r = sweep(&d, utc(2026, 11, 1, 10, 0, 0)).await;
    assert_sweep(&r, 0, 0);
    assert!(
        d.transport.attempts_to(&d.emails[0]) >= 1,
        "a delivery was attempted"
    );
    assert_eq!(
        stamps(&pool, d.g).await.archive_warned_at,
        None,
        "back to NULL"
    );
    assert_eq!(all_mail_count(&d, d.alice).await, 0);

    d.transport.heal(&d.emails[0]);
    let r = sweep(&d, utc(2026, 11, 1, 11, 0, 0)).await;
    assert_sweep(&r, 1, 0);
    assert_eq!(other_mails(&d, d.alice).await.len(), 1);
    assert_sweep(&sweep(&d, utc(2026, 11, 1, 12, 0, 0)).await, 0, 0);
    assert_eq!(other_mails(&d, d.alice).await.len(), 1, "exactly one");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_the_previous_warning_value_is_written_back_on_failure(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    // A warning for an earlier archive date (before this window's warn_at).
    let previous = utc(2026, 3, 1, 10, 0, 0);
    set_stamps(&pool, d.g, None, Some(previous), None).await;
    d.transport.fail(&d.emails[0]);
    assert_sweep(&sweep(&d, utc(2026, 11, 1, 10, 0, 0)).await, 0, 0);
    assert_eq!(stamps(&pool, d.g).await.archive_warned_at, Some(previous));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_group_whose_warnings_always_failed_is_still_archived_on_time(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    d.transport.fail(&d.emails[0]);
    assert_sweep(&sweep(&d, utc(2026, 11, 1, 10, 0, 0)).await, 0, 0);
    assert_sweep(&sweep(&d, utc(2026, 11, 10, 10, 0, 0)).await, 0, 0);
    let at = utc(2026, 11, 14, 10, 0, 0);
    assert_sweep(&sweep(&d, at).await, 0, 1);
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(at));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_failing_group_does_not_stop_the_sweep_from_handling_another(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    // H: administered by Bob, in its window as well; due for a warning to Bob.
    let h = ts::create_group(&pool, d.bob, "Hobby").await;
    make_drawn_h(&d, h).await;
    d.transport.fail(&d.emails[0]); // Alice's group G fails ...
    let r = sweep(&d, utc(2026, 11, 1, 10, 0, 0)).await;
    assert_sweep(&r, 1, 0); // ... H's warning still goes out.
    assert_eq!(other_mails(&d, d.bob).await.len(), 1);
    assert_eq!(stamps(&pool, d.g).await.archive_warned_at, None);
    assert!(stamps(&pool, h).await.archive_warned_at.is_some());

    // And an archive of a third group is not held up by the failures either.
    let k = ts::create_group(&pool, d.carol, "Quiet").await;
    ts::set_group_created_at(&pool, k, utc(2025, 1, 1, 10, 0, 0)).await;
    assert_sweep(&sweep(&d, utc(2026, 11, 1, 11, 0, 0)).await, 0, 1);
    assert!(stamps(&pool, k).await.archived_at.is_some());
}

/// H of [`ac15_a_failing_group_does_not_stop_the_sweep_from_handling_another`]: drawn like G
/// (same window), members Bob only.
async fn make_drawn_h(d: &D, h: i32) {
    set_drawn_state(&d.pool, h, Some(utc(2026, 5, 14, 10, 0, 0))).await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac16_the_spawned_sweep_archives_at_once_and_survives_a_failing_mail(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    // G: long due.
    make_drawn(&d, d.g, utc(2025, 1, 1, 10, 0, 0)).await;
    // H: administered by Bob, inside its window right now; Bob's address fails.
    let h = ts::create_group(&pool, d.bob, "Hobby").await;
    let drawn_at = minus_months(now_secs() + Dur::days(7), 6);
    set_drawn_state(&pool, h, Some(drawn_at)).await;
    d.transport.fail(&d.emails[1]);

    let transport: Arc<dyn MailTransport> = d.transport.clone();
    let handle = spawn_archive_sweep(
        d.pool.clone(),
        Arc::new(test_config()),
        transport,
        Duration::from_millis(50),
    );
    let mut ok = false;
    for _ in 0..100 {
        if stamps(&pool, d.g).await.archived_at.is_some() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ok, "the first run happens at once and archives G");

    // The failing mail is retried on later ticks.
    let mut retried = false;
    for _ in 0..100 {
        if d.transport.attempts_to(&d.emails[1]) >= 2 {
            retried = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(retried, "the warning to Bob is attempted again");

    // The task still runs: a group that becomes due later is archived too.
    let k = ts::create_group(&pool, d.carol, "Late").await;
    ts::set_group_created_at(&pool, k, utc(2025, 1, 1, 10, 0, 0)).await;
    let mut ok = false;
    for _ in 0..100 {
        if stamps(&pool, k).await.archived_at.is_some() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ok, "the sweep keeps running after a failed mail");
    assert!(!handle.is_finished());
    handle.abort();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_the_logs_name_group_ids_and_counts_but_no_personal_data(pool: MySqlPool) {
    let (logs, _guard) = capture_logs();
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    rename_group(&pool, d.g, "Zebra Crossing Club").await;
    // H (admin Bob): its warning fails; K (admin Carol): long due, archived.
    let h = ts::create_group(&pool, d.bob, "Quokka Society").await;
    set_drawn_state(&pool, h, Some(utc(2026, 5, 14, 10, 0, 0))).await;
    let k = ts::create_group(&pool, d.carol, "Narwhal Guild").await;
    ts::set_group_created_at(&pool, k, utc(2025, 1, 1, 10, 0, 0)).await;
    d.transport.fail(&d.emails[1]);

    assert_sweep(&sweep(&d, utc(2026, 11, 1, 10, 0, 0)).await, 1, 1);

    let log = logs.contents();
    assert!(!log.trim().is_empty(), "the sweep logs");
    for id in [d.g, h, k] {
        assert!(
            log.contains(&id.to_string()),
            "group id {id} in the logs: {log}"
        );
    }
    for secret in [
        "Zebra Crossing Club",
        "Quokka Society",
        "Narwhal Guild",
        "Alice",
        "Bob",
        "Carol",
        d.emails[0].as_str(),
        d.emails[1].as_str(),
        d.emails[2].as_str(),
        "will be archived",
        "Keep active",
    ] {
        assert!(!log.contains(secret), "the logs contain {secret:?}: {log}");
    }
}

// ========================================================== AC18 - AC23: archive, restore, keep

async fn archive_check(pool: MySqlPool, drawn: bool) {
    let d = draw_world(&pool).await;
    if drawn {
        make_drawn(&d, d.g, utc(2026, 5, 14, 10, 0, 0)).await;
    }
    let mut alice = d.alice().await;
    let reply = plain(&alice)
        .call("archive_group", &[("group_id", &d.g.to_string())])
        .await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(gp(d.g)));
    assert_near_now(stamps(&pool, d.g).await.archived_at, "archived_at");

    let info = alice.session_info().await;
    assert!(
        has_flash_key(&info, "flash.success.group_archived"),
        "{:?}",
        info.flash
    );
    let page = alice.get(&gp(d.g)).await;
    assert_eq!(page.status, 200);
    assert_eq!(
        testid_count(&page.body, "archived-notice"),
        1,
        "the archived view"
    );
    assert!(html_contains(&page.body, &en("group.status.archived")));

    let mine = list_groups(&mut alice).await;
    assert!(mine.iter().all(|s| s.id != d.g), "gone from My groups");
    assert!(!alice.get("/groups").await.body.contains(GROUP_NAME));
    let archived: Vec<ArchivedGroupSummary> = list_archived(&mut alice).await.value();
    assert_eq!(archived.iter().map(|a| a.id).collect::<Vec<_>>(), vec![d.g]);
    assert!(alice.get("/groups/archive").await.body.contains(GROUP_NAME));
    for user in [d.alice, d.bob, d.carol] {
        assert_eq!(all_mail_count(&d, user).await, 0, "archiving sends no mail");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac18_the_admin_archives_a_drawn_group(pool: MySqlPool) {
    archive_check(pool, true).await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac18_the_admin_archives_a_group_that_is_not_drawn(pool: MySqlPool) {
    archive_check(pool, false).await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_only_the_admin_may_archive_restore_or_keep_active(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let fns = ["archive_group", "restore_group", "keep_group_active"];
    let g = d.g.to_string();
    for archived in [false, true] {
        set_archived_at(&pool, d.g, archived.then(|| utc(2026, 6, 1, 10, 0, 0))).await;
        let before = stamps(&pool, d.g).await;
        let snapshot = ts::groups_snapshot(&pool).await;
        for name in fns {
            let mut bob = d.bob().await;
            let r = bob.call(name, &[("group_id", &g)]).await;
            assert_eq!(r.group_err(), GroupError::Forbidden, "{name}: member");
            assert_eq!(r.status.as_u16(), 403);
            let r = d.dave().await.call(name, &[("group_id", &g)]).await;
            assert_eq!(r.group_err(), GroupError::NotFound, "{name}: outsider");
            let r = d.anon().call(name, &[("group_id", &g)]).await;
            assert_eq!(r.group_err(), GroupError::Unauthorized, "{name}: anonymous");
            assert_eq!(r.status.as_u16(), 401);
        }
        assert_eq!(stamps(&pool, d.g).await, before, "archived = {archived}");
        assert_eq!(ts::groups_snapshot(&pool).await, snapshot);
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_a_get_or_a_cross_origin_post_changes_nothing(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let mut alice = d.alice().await;
    let before = stamps(&pool, d.g).await;
    let cookie = format!(
        "{}={}",
        server::SESSION_COOKIE,
        alice.session_cookie().unwrap()
    );
    let body = format!("group_id={}", d.g);
    for name in ["archive_group", "restore_group", "keep_group_active"] {
        let r = alice.get(&format!("/api/{name}?group_id={}", d.g)).await;
        assert!(r.status.as_u16() >= 400, "GET {name}: {}", r.status);
        for origin in [Some("https://evil.example"), Some("null"), None] {
            let req = post_from(origin, &format!("/api/{name}"), Some(&cookie), &body);
            let res = send(&d.app, req).await;
            assert_eq!(res.status(), 403, "{name} from {origin:?}");
        }
    }
    assert_eq!(stamps(&pool, d.g).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac20_archive_and_restore_are_idempotent_and_keep_active_refuses_an_archived_group(
    pool: MySqlPool,
) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let mut alice = d.alice().await;
    let at = utc(2026, 6, 1, 10, 0, 0);
    set_archived_at(&pool, d.g, Some(at)).await;

    archive(&mut alice, d.g).await.group_ok();
    assert_eq!(stamps(&pool, d.g).await.archived_at, Some(at), "unchanged");
    let r = keep(&mut alice, d.g).await;
    assert_eq!(r.group_err(), GroupError::Archived);
    assert_eq!(r.status.as_u16(), 409);
    assert_eq!(stamps(&pool, d.g).await.kept_active_at, None);

    set_archived_at(&pool, d.g, None).await;
    restore(&mut alice, d.g).await.group_ok();
    let s = stamps(&pool, d.g).await;
    assert_eq!(
        (s.archived_at, s.kept_active_at),
        (None, None),
        "restoring an active group"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac21_restoring_brings_the_group_back_exactly_as_it_was(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    ts::create_wishlist(&pool, d.g, d.bob, &["Socks"]).await;
    ts::add_exclusion(&pool, d.g, d.bob, d.alice).await;
    let before = ts::groups_snapshot(&pool).await;
    let code = invitation_code(&pool, d.g).await;
    let mut alice = d.alice().await;
    archive(&mut alice, d.g).await.group_ok();

    let reply = plain(&alice)
        .call("restore_group", &[("group_id", &d.g.to_string())])
        .await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(gp(d.g)));
    let s = stamps(&pool, d.g).await;
    assert_eq!(s.archived_at, None);
    assert_near_now(s.kept_active_at, "kept_active_at");
    let info = alice.session_info().await;
    assert!(
        has_flash_key(&info, "flash.success.group_restored"),
        "{:?}",
        info.flash
    );

    assert!(list_groups(&mut alice).await.iter().any(|s| s.id == d.g));
    let archived: Vec<ArchivedGroupSummary> = list_archived(&mut alice).await.value();
    assert!(archived.is_empty());
    assert!(!alice.get("/groups/archive").await.body.contains(GROUP_NAME));

    assert_eq!(
        ts::groups_snapshot(&pool).await,
        before,
        "nothing else changed"
    );
    assert_eq!(invitation_code(&pool, d.g).await, code);
    // Writable again.
    update_group(&mut alice, d.g, "Family 2027", "", "", "")
        .await
        .group_ok();
    add_item(&mut d.bob().await, d.g, "Book", "", "")
        .await
        .wl_ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_the_admin_sees_the_warning_and_keeps_the_group_active(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let drawn_at = into_window(&d, d.g).await;
    let archive_on = plus_months(drawn_at, 6);
    let mut alice = d.alice().await;
    let mut bob = d.bob().await;

    let g: GroupDetail = get_group(&mut alice, d.g).await.value();
    let schedule = g.archive_schedule.expect("schedule for the admin");
    assert_eq!(schedule.auto_archive_at, archive_on);
    assert!(schedule.archive_soon, "inside the warning window");

    let body = alice.get(&gp(d.g)).await.body;
    let alert = block(&body, "archive-soon").expect("the warning alert");
    assert!(
        alert.contains("alert-warning") && alert.contains("role=\"status\""),
        "{alert}"
    );
    assert!(html_contains(
        &text_of(alert),
        &en_with("group.show.archiveSoon", &[("date", &en_date(archive_on))])
    ));
    assert!(
        alert.contains("action=\"/api/keep_group_active\""),
        "{alert}"
    );
    assert!(html_contains(alert, &en("group.show.keepActive")));

    // Bob sees neither the alert, nor the button, nor any date.
    let bob_body = bob.get(&gp(d.g)).await.body;
    assert_eq!(testid_count(&bob_body, "archive-soon"), 0);
    assert!(!html_contains(&bob_body, &en("group.show.keepActive")));
    assert!(!bob_body.contains(&en_date(archive_on)));

    // Keep active (plain post: the flash shows up on the group page).
    let reply = plain(&alice)
        .call("keep_group_active", &[("group_id", &d.g.to_string())])
        .await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(gp(d.g)));
    let k = stamps(&pool, d.g)
        .await
        .kept_active_at
        .expect("kept_active_at set");
    let info = alice.session_info().await;
    assert!(
        has_flash_key(&info, "flash.success.group_kept_active"),
        "{:?}",
        info.flash
    );
    let body = alice.get(&gp(d.g)).await.body;
    assert_eq!(testid_count(&body, "archive-soon"), 0, "the alert is gone");
    let line = block(&body, "auto-archive-on").expect("the date line");
    assert!(html_contains(
        &text_of(line),
        &en_with(
            "group.show.autoArchiveOn",
            &[("date", &en_date(plus_months(k, 6)))]
        )
    ));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_outside_the_window_the_admin_sees_only_the_date_line(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let drawn_at = minus_months(now_secs(), 1);
    make_drawn(&d, d.g, drawn_at).await;
    let body = d.alice().await.get(&gp(d.g)).await.body;
    assert_eq!(testid_count(&body, "archive-soon"), 0);
    assert!(!html_contains(&body, &en("group.show.keepActive")));
    let line = block(&body, "auto-archive-on").expect("the date line");
    assert!(html_contains(
        &text_of(line),
        &en_with(
            "group.show.autoArchiveOn",
            &[("date", &en_date(plus_months(drawn_at, 6)))]
        )
    ));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_without_javascript_the_buttons_are_plain_forms_that_redirect_back(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    into_window(&d, d.g).await;
    let mut alice = d.alice().await;
    let g = d.g.to_string();

    // The forms on the page are plain POST forms with the group id.
    let body = alice.get(&gp(d.g)).await.body;
    for action in ["archive_group", "keep_group_active"] {
        let at = body
            .find(&format!("action=\"/api/{action}\""))
            .unwrap_or_else(|| panic!("no form posting to {action}"));
        let form_start = body[..at].rfind("<form").expect("inside a form");
        let form_end = at + body[at..].find("</form>").expect("form end");
        let form = &body[form_start..form_end];
        assert!(
            form.contains("method=\"post\"") || form.contains("method=\"POST\""),
            "{form}"
        );
        assert!(
            form.contains("name=\"group_id\"") && form.contains(&format!("value=\"{g}\"")),
            "{form}"
        );
    }

    for (name, flash) in [
        ("keep_group_active", "flash.success.group_kept_active"),
        ("archive_group", "flash.success.group_archived"),
        ("restore_group", "flash.success.group_restored"),
    ] {
        let reply = plain(&alice).call(name, &[("group_id", &g)]).await;
        assert_eq!(reply.status, 302, "{name}: {}", reply.body);
        assert_eq!(reply.location(), Some(gp(d.g)), "{name}");
        let page = alice.get(&gp(d.g)).await;
        assert_eq!(page.status, 200);
        assert!(
            html_contains(&page.body, &en(flash)),
            "{name}: flash on the group page"
        );
    }
}

// ================================================================== AC24 - AC29: read-only

/// What must not change when a call is refused.
async fn snap(
    pool: &MySqlPool,
    groups: &[i32],
) -> (ts::GroupsSnapshot, Vec<Stamps>, Vec<Vec<(i32, bool, u8)>>) {
    let mut st = vec![];
    let mut del = vec![];
    for g in groups {
        st.push(stamps(pool, *g).await);
        del.push(delivery(pool, *g).await);
    }
    (ts::groups_snapshot(pool).await, st, del)
}

fn group_archived(r: &Reply) {
    assert_eq!(r.status.as_u16(), 409, "{}", r.body);
    assert_eq!(r.group_err(), GroupError::Archived);
}

fn draw_archived(r: &Reply) {
    assert_eq!(r.status.as_u16(), 409, "{}", r.body);
    assert_eq!(r.dr_err(), DrawApiError::Archived);
}

fn ex_archived(r: &Reply) {
    assert_eq!(r.status.as_u16(), 409, "{}", r.body);
    assert_eq!(r.ex_err(), ExclusionError::Archived);
}

fn wl_archived(r: &Reply) {
    assert_eq!(r.status.as_u16(), 409, "{}", r.body);
    assert_eq!(r.wl_err(), WishlistError::Archived);
}

struct Ro {
    d: D,
    /// Drawn, archived (Alice admin; Alice, Bob, Carol).
    g: i32,
    /// Not drawn, archived (Alice admin; Alice, Bob, Carol); Bob has a rule and a list.
    u: i32,
    /// Active (Bob admin; Bob, Alice); Bob has a list.
    h: i32,
}

async fn ro_world(pool: &MySqlPool) -> Ro {
    let d = drawn_world(pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let g = d.g;
    ts::create_wishlist(pool, g, d.bob, &["Socks", "Book"]).await;
    ts::add_exclusion(pool, g, d.bob, d.alice).await;
    let u = ts::create_group_with_code(pool, d.alice, "Undrawn", "UNDR2345").await;
    ts::add_member(pool, u, d.bob).await;
    ts::add_member(pool, u, d.carol).await;
    ts::create_wishlist(pool, u, d.bob, &["Cap", "Scarf"]).await;
    ts::add_exclusion(pool, u, d.bob, d.carol).await;
    let h = ts::create_group_with_code(pool, d.bob, "Hobby", "HOBB2345").await;
    ts::add_member(pool, h, d.alice).await;
    ts::create_wishlist(pool, h, d.bob, &["Kite"]).await;
    set_archived_at(pool, g, Some(utc(2026, 6, 1, 10, 0, 0))).await;
    set_archived_at(pool, u, Some(utc(2026, 6, 2, 10, 0, 0))).await;
    Ro { d, g, u, h }
}

async fn copy_items(c: &mut Client, group: i32, source: i32, ids: &[i32]) -> Reply {
    let mut values = vec![
        ("group_id".to_string(), group.to_string()),
        ("source_group_id".to_string(), source.to_string()),
    ];
    values.extend(
        ids.iter()
            .enumerate()
            .map(|(n, id)| (format!("item_ids[{n}]"), id.to_string())),
    );
    let refs: Vec<_> = values
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    c.call("copy_wishlist_items", &refs).await
}

#[sqlx::test(migrations = "../migrations")]
async fn ac24_the_admin_cannot_change_an_archived_group(pool: MySqlPool) {
    let w = ro_world(&pool).await;
    let d = &w.d;
    let mut alice = d.alice().await;
    let before = snap(&pool, &[w.g, w.u]).await;

    group_archived(&update_group(&mut alice, w.g, "New name", "x", "2030-01-01", "").await);
    group_archived(&regenerate_invitation(&mut alice, w.g).await);
    group_archived(&remove_member(&mut alice, w.g, d.bob).await);
    group_archived(&transfer_admin(&mut alice, w.g, d.bob).await);
    group_archived(&leave_group(&mut alice, w.g).await); // not AdminCannotLeave (A9)
    draw_archived(&reset(&mut alice, w.g).await);
    draw_archived(&resend(&mut alice, w.g).await);
    draw_archived(&perform(&mut alice, w.u).await);

    assert_eq!(snap(&pool, &[w.g, w.u]).await, before, "no row changed");
    assert_eq!(d.transport.attempts_total(), 0, "no mail was queued");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac24_a_member_cannot_change_an_archived_group(pool: MySqlPool) {
    let w = ro_world(&pool).await;
    let d = &w.d;
    let mut bob = d.bob().await;
    let before = snap(&pool, &[w.g, w.u, w.h]).await;
    let u_items = item_ids(&pool, w.u, d.bob).await;
    let h_items = item_ids(&pool, w.h, d.bob).await;

    group_archived(&leave_group(&mut bob, w.g).await);
    group_archived(&leave_group(&mut bob, w.u).await);
    ex_archived(&add_ex(&mut bob, w.u, d.alice).await);
    ex_archived(&add_ex(&mut bob, w.g, d.carol).await); // archived wins over AlreadyDrawn
    ex_archived(&remove_ex(&mut bob, w.u, d.carol).await);
    wl_archived(&add_item(&mut bob, w.u, "New", "", "").await);
    wl_archived(&update_item(&mut bob, u_items[0], "Renamed", "", "").await);
    wl_archived(&delete_item(&mut bob, u_items[0]).await);
    wl_archived(&move_item(&mut bob, u_items[1], "Up").await);
    wl_archived(&clear_wishlist(&mut bob, w.u, true).await);
    wl_archived(&copy_items(&mut bob, w.u, w.h, &h_items).await);

    // The share target's add_shared_wish into the archived group.
    let shared = format!(
        "/share-target?title=Gift&text={}",
        urlencode("https://shop.example/p")
    );
    let review = bob.get(&shared).await;
    assert_eq!(review.status.as_u16(), 303);
    let location = review.location().expect("review location");
    let id = location.split_once("id=").expect("draft id").1.to_string();
    let r = bob
        .call(
            "add_shared_wish",
            &[
                ("id", &id),
                ("group_id", &w.u.to_string()),
                ("title", "Gift"),
                ("description", ""),
                ("link", "https://shop.example/p"),
            ],
        )
        .await;
    wl_archived(&r);

    assert_eq!(
        snap(&pool, &[w.g, w.u, w.h]).await,
        before,
        "no row changed"
    );
}

#[test]
fn ac24_archived_errors_are_409_with_the_archived_message_keys() {
    assert_eq!(GroupError::Archived.status_code(), 409);
    assert_eq!(
        GroupError::Archived.message_key(GroupOp::Other),
        "flash.error.group_archived"
    );
    assert_eq!(
        GroupError::Archived.message_key(GroupOp::Leave),
        "flash.error.group_archived"
    );
    assert_eq!(
        GroupError::Archived.message_key(GroupOp::Join),
        "flash.error.join_closed_archived"
    );
    assert_eq!(ExclusionError::Archived.status_code(), 409);
    assert_eq!(
        ExclusionError::Archived.message_key(),
        "flash.error.group_archived"
    );
    assert_eq!(WishlistError::Archived.status_code(), 409);
    assert_eq!(
        WishlistError::Archived.message_key(),
        "flash.error.group_archived"
    );
    let flashes = DrawApiError::Archived.flashes();
    assert_eq!(flashes.len(), 1);
    assert_eq!(flashes[0].key, "flash.error.group_archived");
    en("flash.error.group_archived");
    de("flash.error.group_archived");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac24_a_plain_post_of_a_refused_call_comes_back_as_a_flash(pool: MySqlPool) {
    let w = ro_world(&pool).await;
    let mut bob = w.d.bob().await;
    let reply = plain(&bob)
        .call("leave_group", &[("group_id", &w.g.to_string())])
        .await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    let info = bob.session_info().await;
    assert!(
        has_flash_key(&info, "flash.error.group_archived"),
        "{:?}",
        info.flash
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac25_authorization_errors_win_over_archived(pool: MySqlPool) {
    let w = ro_world(&pool).await;
    let d = &w.d;
    let before = snap(&pool, &[w.g, w.u]).await;
    let r = update_group(&mut d.bob().await, w.g, "Mine", "", "", "").await;
    assert_eq!(r.group_err(), GroupError::Forbidden);
    let r = add_item(&mut d.dave().await, w.u, "Hello", "", "").await;
    assert_eq!(r.wl_err(), WishlistError::NotFound);
    let r = archive(&mut d.dave().await, w.g).await;
    assert_eq!(r.group_err(), GroupError::NotFound);
    let r = regenerate_invitation(&mut d.bob().await, w.g).await;
    assert_eq!(r.group_err(), GroupError::Forbidden);
    let r = add_ex(&mut d.dave().await, w.u, d.alice).await;
    assert_eq!(r.ex_err(), ExclusionError::NotFound);
    let r = reset(&mut d.bob().await, w.g).await;
    assert_eq!(r.dr_err(), DrawApiError::Forbidden);
    assert_eq!(snap(&pool, &[w.g, w.u]).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac26_the_admin_can_still_delete_an_archived_group(pool: MySqlPool) {
    let w = ro_world(&pool).await;
    let d = &w.d;
    let mut alice = d.alice().await;
    let r = plain(&alice)
        .call("delete_group", &[("group_id", &w.g.to_string())])
        .await;
    assert_eq!(r.status, 302, "{}", r.body);
    assert_eq!(r.location().as_deref(), Some("/groups"));
    assert!(ts::find_group(&pool, w.g).await.is_none());
    for sql in [
        "SELECT COUNT(*) FROM group_members WHERE group_id = ?",
        "SELECT COUNT(*) FROM wishlists WHERE group_id = ?",
        "SELECT COUNT(*) FROM exclusion_rules WHERE group_id = ?",
        "SELECT COUNT(*) FROM gift_assignments WHERE group_id = ?",
    ] {
        let n: i64 = sqlx::query_scalar(sql)
            .bind(w.g)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "{sql}");
    }
    let page = alice.get("/groups").await;
    assert!(html_contains(
        &page.body,
        &en("flash.success.group_deleted")
    ));
    // The other groups are untouched.
    assert!(ts::find_group(&pool, w.u).await.is_some());
    assert!(ts::find_group(&pool, w.h).await.is_some());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac27_nobody_can_join_an_archived_group(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    // Drawn and past its deadline as well: Archived is reported first.
    ts::set_group_dates(&pool, d.g, Some(utc(2020, 1, 1, 22, 59, 59)), None).await;
    set_archived_at(&pool, d.g, Some(utc(2026, 6, 1, 10, 0, 0))).await;
    let members = ts::member_ids(&pool, d.g).await;
    let mut dave = d.dave().await;

    let page = dave.get(&format!("/join/{CODE}")).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(
        &page.body,
        &en("flash.error.join_closed_archived")
    ));
    assert!(
        !page.body.contains("action=\"/api/join_group\""),
        "no Join button"
    );
    let p = preview(&mut dave, CODE).await;
    assert_eq!(p.closed, Some(ClosedReason::Archived));
    let r = join_group(&mut dave, CODE).await;
    assert_eq!(r.group_err(), GroupError::Archived);
    assert_eq!(r.status.as_u16(), 409);
    assert_eq!(
        ts::member_ids(&pool, d.g).await,
        members,
        "no membership added"
    );

    // A member opening the link is redirected to the group.
    let page = d.bob().await.get(&format!("/join/{CODE}")).await;
    assert_eq!(page.status, 302);
    assert_eq!(page.location(), Some(gp(d.g)));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac28_wishes_can_be_copied_from_an_archived_group_into_an_active_one(pool: MySqlPool) {
    let w = ro_world(&pool).await;
    let d = &w.d;
    let mut bob = d.bob().await;
    // Merged with #76: the copy section of the edit page reads `get_copy_sources`.
    let sources: CopySources = bob
        .call("get_copy_sources", &[("group_id", &w.h.to_string())])
        .await
        .wl_value();
    assert!(
        sources.sources.iter().any(|s| s.group.id == w.g),
        "archived G is a source: {sources:?}"
    );
    let section = bob.get(&format!("/groups/{}/wishlist?copy=1", w.h)).await;
    assert!(
        section.body.contains(GROUP_NAME),
        "the copy section lists G"
    );

    let ids = item_ids(&pool, w.g, d.bob).await;
    copy_items(&mut bob, w.h, w.g, &ids).await.wl_ok();
    assert_eq!(titles(&pool, w.h, d.bob).await, ["Kite", "Socks", "Book"]);
    assert_eq!(
        titles(&pool, w.g, d.bob).await,
        ["Socks", "Book"],
        "the source is unchanged"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac29_a_write_that_waits_for_the_archiving_transaction_sees_the_archive(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    ts::create_wishlist(&pool, d.g, d.bob, &["Socks"]).await;
    let mut bob = d.bob().await;
    let mut tx = hold_lock(&pool, d.g).await;
    sqlx::query("UPDATE `groups` SET archived_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(d.g)
        .execute(&mut *tx)
        .await
        .unwrap();

    let g = d.g;
    let add = tokio::spawn(async move { add_item(&mut bob, g, "Late", "", "").await });
    assert_waiting(&add, "add_item").await;
    tx.commit().await.unwrap();

    wl_archived(&add.await.unwrap());
    assert_eq!(
        titles(&pool, d.g, d.bob).await,
        ["Socks"],
        "no item written"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac29_archiving_waits_for_a_running_wishlist_write(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    let mut alice = d.alice().await;
    // What a wishlist write holds on the group row.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM `groups` WHERE id = ? LOCK IN SHARE MODE")
        .bind(d.g)
        .fetch_one(&mut *tx)
        .await
        .unwrap();

    let g = d.g;
    let archiving = tokio::spawn(async move { archive(&mut alice, g).await });
    assert_waiting(&archiving, "archive_group").await;
    assert_eq!(stamps(&pool, d.g).await.archived_at, None);
    tx.commit().await.unwrap();

    archiving.await.unwrap().group_ok();
    assert!(stamps(&pool, d.g).await.archived_at.is_some());
}

// ============================================================= AC30 - AC37: what people see

fn assert_absent(html: &str, texts: &[String], what: &str) {
    for text in texts {
        assert!(!html_contains(html, text), "{what}: unexpected {text:?}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac30_a_member_sees_the_archived_group_read_only(pool: MySqlPool) {
    let d = archived_world(&pool).await;
    let archived_at = stamps(&pool, d.g).await.archived_at.expect("archived");
    let mut bob = d.bob().await;

    let page = bob.get(&gp(d.g)).await;
    assert_eq!(page.status, 200);
    let body = &page.body;
    let notice = block(body, "archived-notice").expect("archived notice");
    assert!(
        notice.contains("alert-secondary") && notice.contains("role=\"status\""),
        "{notice}"
    );
    assert!(html_contains(
        &text_of(notice),
        &en_with(
            "group.show.archivedNotice",
            &[("date", &en_date(archived_at))]
        )
    ));
    assert!(html_contains(body, &en("group.status.archived")));
    assert!(
        body.contains("Alice") && body.contains("Carol"),
        "member names"
    );
    assert!(
        !body.contains(&d.emails[0]) && !body.contains(&d.emails[2]),
        "no emails"
    );

    // Own assignment only (Bob -> Carol).
    assert_eq!(testid_count(body, "my-assignment"), 1);
    assert_eq!(
        text_of(block(body, "assignment-receiver").unwrap()),
        "Carol"
    );
    // Wishlist card: view links, no edit button.
    let wishlist = block(body, "group-wishlist").expect("wishlist card");
    assert!(wishlist.contains(&format!("href=\"/groups/{}/wishlist/{}\"", d.g, d.alice)));
    assert!(wishlist.contains(&format!("href=\"/groups/{}/wishlist/{}\"", d.g, d.bob)));
    assert!(!html_contains(wishlist, &en("group.show.editWishlist")));
    assert!(!wishlist.contains(&format!("href=\"/groups/{}/wishlist\"", d.g)));

    assert_absent(
        body,
        &[
            en("group.show.leaveGroup"),
            en("group.show.cannotLeaveAfterDraw"),
            en("group.draw.pending"),
            CODE.to_string(),
        ],
        "member page",
    );
    assert!(
        !body.contains(&format!("/groups/{}/exclusions", d.g)),
        "no exclusions link"
    );
    assert_eq!(
        testid_count(body, "auto-archive-on"),
        0,
        "no archive-date line"
    );

    let g: GroupDetail = get_group(&mut bob, d.g).await.value();
    assert_eq!(g.status, GroupStatus::Archived);
    assert!(!g.can_leave);
    assert!(g.invitation.is_none());
    assert_eq!(g.archived_at, Some(archived_at));
    assert!(g.archive_schedule.is_none());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_the_admin_sees_emails_restore_and_delete_but_no_controls(pool: MySqlPool) {
    let d = archived_world(&pool).await;
    let mut alice = d.alice().await;
    let body = alice.get(&gp(d.g)).await.body;

    assert!(
        body.contains(&d.emails[1]) && body.contains(&d.emails[2]),
        "member emails"
    );
    assert!(html_contains(&body, &en("group.show.restoreGroup")));
    assert!(html_contains(&body, &en("group.show.deleteGroup")));
    assert!(body.contains("action=\"/api/restore_group\""));
    assert_absent(
        &body,
        &[
            en("group.show.editButton"),
            en("group.show.regenerateCode"),
            en("group.show.makeAdmin"),
            en("group.show.removeMember"),
            en("group.show.keepActive"),
            en("group.show.performDraw"),
            en("group.draw.notifications.resend"),
            en("group.show.archiveGroup"),
        ],
        "admin page",
    );
    for id in [
        "draw-perform",
        "draw-reset",
        "draw-resend",
        "draw-notifications",
        "archive-soon",
    ] {
        assert_eq!(testid_count(&body, id), 0, "{id}");
    }
    assert!(!body.contains(CODE), "no invitation card");

    let edit = alice.get(&format!("/groups/{}/edit", d.g)).await;
    assert_eq!(edit.status, 200);
    assert!(html_contains(&edit.body, &en("flash.error.group_archived")));
    assert!(
        !edit.body.contains("action=\"/api/update_group\""),
        "no form"
    );
    assert!(
        edit.body.contains(&format!("href=\"/groups/{}\"", d.g)),
        "link back"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac32_an_archived_group_page_carries_only_the_viewers_own_assignment(pool: MySqlPool) {
    let d = archived_world(&pool).await;
    // Alice -> Bob, Bob -> Carol, Carol -> Alice.
    for (who, own_receiver) in [(0, d.bob), (1, d.carol)] {
        let mut c = if who == 0 {
            d.alice().await
        } else {
            d.bob().await
        };
        let body = c.get(&gp(d.g)).await.body;
        assert_only_own_assignment(&body, own_receiver, &format!("viewer {who}"));
    }
    let mut alice = d.alice().await;
    let list = list_archived(&mut alice).await;
    assert!(list.status.is_success(), "{}", list.body);
    for word in ["receiver", "giver"] {
        assert!(
            !list.body.contains(word),
            "list_archived_groups mentions {word}"
        );
    }
    let page = alice.get("/groups/archive").await.body;
    for word in ["receiver_id", "receiver_name", "giver_id"] {
        assert!(!page.contains(word), "archive page mentions {word}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac33_the_archive_page_lists_my_archived_groups_newest_archive_first(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let g = d.g; // admin Alice; Alice, Bob, Carol; drawn; archived later
    let drawn_at = utc(2026, 5, 14, 10, 0, 0);
    make_drawn(&d, g, drawn_at).await;
    set_archived_at(&pool, g, Some(utc(2026, 6, 2, 10, 0, 0))).await;
    let h = ts::create_group_with_code(&pool, d.bob, "Hobby Club", "HOBB2345").await;
    ts::add_member(&pool, h, d.carol).await;
    set_archived_at(&pool, h, Some(utc(2026, 6, 1, 10, 0, 0))).await;
    let k = ts::create_group_with_code(&pool, d.alice, "Active Club", "ACTV2345").await;
    ts::add_member(&pool, k, d.bob).await;
    set_drawn_state(&pool, k, Some(utc(2026, 10, 1, 10, 0, 0))).await;
    ts::add_assignment(&pool, k, d.alice, d.bob).await;
    ts::add_assignment(&pool, k, d.bob, d.alice).await;
    let j = ts::create_group_with_code(&pool, d.dave, "Dave Secret", "DAVE2345").await;
    ts::add_member(&pool, j, d.carol).await;
    set_archived_at(&pool, j, Some(utc(2026, 6, 3, 10, 0, 0))).await;
    let mut bob = d.bob().await;

    let list: Vec<ArchivedGroupSummary> = list_archived(&mut bob).await.value();
    assert_eq!(list.iter().map(|a| a.id).collect::<Vec<_>>(), vec![g, h]);
    assert_eq!(
        (list[0].is_admin, list[1].is_admin),
        (false, true),
        "Bob is admin of H only"
    );
    assert_eq!(
        (list[0].admin_name.as_str(), list[1].admin_name.as_str()),
        ("Alice", "Bob")
    );
    assert_eq!((list[0].member_count, list[1].member_count), (3, 2));
    assert_eq!((list[0].drawn_at, list[1].drawn_at), (Some(drawn_at), None));
    assert_eq!(list[0].archived_at, utc(2026, 6, 2, 10, 0, 0));

    let body = bob.get("/groups/archive").await.body;
    let cards: Vec<&str> = body
        .split("data-testid=\"archived-group\"")
        .skip(1)
        .collect();
    assert_eq!(cards.len(), 2, "two cards");
    let (cg, ch) = (text_of(cards[0]), text_of(cards[1]));
    assert!(
        cg.contains(GROUP_NAME) && ch.contains("Hobby Club"),
        "newest archive first"
    );
    assert!(cg.contains("Alice") && ch.contains("Bob"), "admin names");
    assert!(cg.contains(&en_plural("group.memberCount", 3, &[("count", "3")])));
    assert!(ch.contains(&en_plural("group.memberCount", 2, &[("count", "2")])));
    assert!(cg.contains(&en_with(
        "group.draw.drawnOn",
        &[("date", &en_date(drawn_at))]
    )));
    let drawn_prefix = en("group.draw.drawnOn");
    let drawn_prefix = drawn_prefix.split("{date}").next().unwrap().trim();
    assert!(!ch.contains(drawn_prefix), "H was never drawn");
    assert!(cg.contains(&en_with(
        "group.archive.archivedOn",
        &[("date", &en_date(utc(2026, 6, 2, 10, 0, 0)))]
    )));
    assert!(cards[0].contains(&format!("href=\"/groups/{g}\"")));
    assert!(cards[1].contains(&format!("href=\"/groups/{h}\"")));
    let badge = en("group.admin");
    assert_eq!(
        ch.matches(&badge).count(),
        cg.matches(&badge).count() + 1,
        "the Admin badge only on Bob's own group"
    );
    assert!(!body.contains("Active Club") && !body.contains("Dave Secret"));

    // /groups: K only, plus a link to the archive.
    let groups = bob.get("/groups").await.body;
    assert!(groups.contains("Active Club"));
    assert!(!groups.contains(GROUP_NAME) && !groups.contains("Hobby Club"));
    assert!(
        groups.contains("href=\"/groups/archive\""),
        "link to the archive"
    );
    assert!(html_contains(&groups, &en("group.archive.title")));
    let mine = list_groups(&mut bob).await;
    assert_eq!(mine.iter().map(|s| s.id).collect::<Vec<_>>(), vec![k]);

    // Assignments: K's only (the dashboard card, U8).
    let assignments = list_mine(&mut bob).await;
    assert_eq!(
        assignments.iter().map(|a| a.group_id).collect::<Vec<_>>(),
        vec![k]
    );
    let dashboard = bob.get("/user/dashboard").await.body;
    assert!(!dashboard.contains(GROUP_NAME) && !dashboard.contains("Hobby Club"));

    // The share-target picker offers only K.
    let shared = format!(
        "/share-target?title=Gift&text={}",
        urlencode("https://shop.example/p")
    );
    let location = bob.get(&shared).await.location().expect("review location");
    let review = bob.get(&location).await;
    assert_eq!(review.status, 200);
    assert!(review.body.contains("Active Club"), "K is offered");
    assert!(!review.body.contains(GROUP_NAME) && !review.body.contains("Hobby Club"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac34_the_archive_page_of_a_user_without_archived_groups_is_empty_and_private(
    pool: MySqlPool,
) {
    let d = archived_world(&pool).await;
    let mut dave = d.dave().await;
    let page = dave.get("/groups/archive").await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("group.archive.empty")));
    assert!(!page.body.contains(GROUP_NAME));
    assert!(
        page.header("cache-control")
            .is_some_and(|v| v.contains("no-store")),
        "{:?}",
        page.header("cache-control")
    );
    // The page itself: title, intro, link back.
    assert!(html_contains(&page.body, &en("group.archive.title")));
    assert!(html_contains(&page.body, &en("group.archive.intro")));
    assert!(page.body.contains("href=\"/groups\""));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac34_a_logged_out_visitor_is_sent_to_login_and_back(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let mut visitor = d.anon();
    let res = visitor.get("/groups/archive").await;
    assert!(res.status.is_redirection(), "{}", res.status);
    assert_eq!(
        res.location(),
        Some(format!("/auth/login?next={}", urlencode("/groups/archive")))
    );
    let login = visitor
        .login_next(
            &d.emails[1],
            app::test_support::BOB_PASSWORD,
            "/groups/archive",
        )
        .await;
    login.ok();
    assert_eq!(login.location().as_deref(), Some("/groups/archive"));
    assert_eq!(visitor.get("/groups/archive").await.status, 200);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac35_the_wishlist_and_exclusion_pages_of_an_archived_group_are_read_only(pool: MySqlPool) {
    let d = archived_world(&pool).await;
    let mut bob = d.bob().await;

    let wishlist = bob.get(&format!("/groups/{}/wishlist", d.g)).await;
    assert_eq!(wishlist.status, 200);
    let body = &wishlist.body;
    assert!(html_contains(body, &en("flash.error.group_archived")));
    assert!(
        body.contains("Socks") && body.contains("Book"),
        "the items are listed"
    );
    for action in [
        "add_item",
        "update_item",
        "delete_item",
        "move_item",
        "clear_wishlist",
        "copy_wishlist_items",
    ] {
        assert!(
            !body.contains(&format!("action=\"/api/{action}\"")),
            "{action} form"
        );
    }
    let mine: WishlistView = get_my_wishlist(&mut bob, d.g).await.wl_value();
    assert!(mine.archived);
    assert_eq!(mine.items.len(), 2);

    let exclusions = bob.get(&format!("/groups/{}/exclusions", d.g)).await;
    assert_eq!(exclusions.status, 200);
    let body = &exclusions.body;
    assert!(html_contains(body, &en("flash.error.group_archived")));
    assert!(body.contains("Alice"), "Bob's rule (Alice) is listed");
    for action in ["add_exclusion", "remove_exclusion"] {
        assert!(
            !body.contains(&format!("action=\"/api/{action}\"")),
            "{action} form"
        );
    }
    let p = ex_page(&mut bob, d.g).await;
    assert!(p.archived);
    assert!(!p.can_add);
    assert!(p.candidates.is_empty());
    assert!(p.notice.is_none());
    assert_eq!(p.my_rules.len(), 1, "the rules stay visible");
}

/// AC28/AC35 after merging #76: an archived group's edit page has no copy section (nothing
/// can be copied into it), while an active list's page lists the archived list as a source,
/// to its owner only. Archived wins over the copy's own errors (A9).
#[sqlx::test(migrations = "../migrations")]
async fn ac28_ac35_no_copy_section_on_an_archived_list_and_archived_sources_are_the_owners(
    pool: MySqlPool,
) {
    let w = ro_world(&pool).await;
    let d = &w.d;
    let mut bob = d.bob().await;

    // Bob has non-empty lists in H (active) and U (archived): the archived page has no section.
    for g in [w.g, w.u] {
        let page = bob.get(&format!("/groups/{g}/wishlist?copy=1")).await;
        assert_eq!(page.status, 200);
        assert!(html_contains(&page.body, &en("flash.error.group_archived")));
        assert!(!page.body.contains("id=\"copy\""), "copy section on {g}");
        assert!(!page.body.contains("action=\"/api/copy_wishlist_items\""));
        assert!(!html_contains(&page.body, &en("wishlist.copy.title")));
    }
    // The active list's page has the section, listing both archived lists.
    let page = bob.get(&format!("/groups/{}/wishlist?copy=1", w.h)).await;
    assert!(page.body.contains("id=\"copy\""), "copy section on H");
    assert!(page.body.contains("action=\"/api/copy_wishlist_items\""));
    let sources: CopySources = bob
        .call("get_copy_sources", &[("group_id", &w.h.to_string())])
        .await
        .wl_value();
    let ids: Vec<i32> = sources.sources.iter().map(|s| s.group.id).collect();
    assert!(ids.contains(&w.g) && ids.contains(&w.u), "{ids:?}");

    // Alice is a member of H, G and U but has no list there: Bob's archived lists are not hers.
    let alice_sources: CopySources = d
        .alice()
        .await
        .call("get_copy_sources", &[("group_id", &w.h.to_string())])
        .await
        .wl_value();
    assert!(alice_sources.sources.is_empty(), "{alice_sources:?}");
    let page = d
        .alice()
        .await
        .get(&format!("/groups/{}/wishlist", w.h))
        .await;
    assert!(!page.body.contains("Socks") && !page.body.contains("Scarf"));

    // Copying a duplicate into archived U: Archived, not AlreadyOnList.
    let h_list: i32 =
        sqlx::query_scalar("SELECT id FROM wishlists WHERE group_id = ? AND user_id = ?")
            .bind(w.h)
            .bind(d.bob)
            .fetch_one(&pool)
            .await
            .unwrap();
    let cap = crate::common::wishlists::insert_item(&pool, h_list, "Cap", None, None).await;
    let before = snap(&pool, &[w.u, w.h]).await;
    wl_archived(&copy_items(&mut bob, w.u, w.h, &[cap]).await);
    // Without JavaScript: back to the archived page's `?copy=1` with the archived message.
    let reply = plain(&bob)
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &w.u.to_string()),
                ("source_group_id", &w.h.to_string()),
                ("item_ids[0]", &cap.to_string()),
            ],
        )
        .await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(
        reply.location(),
        Some(format!("/groups/{}/wishlist?copy=1", w.u))
    );
    let info = bob.session_info().await;
    assert!(
        has_flash_key(&info, "flash.error.group_archived"),
        "{:?}",
        info.flash
    );
    assert_eq!(snap(&pool, &[w.u, w.h]).await, before, "nothing written");
}

/// AC28 without JavaScript: a plain post copies from an archived source into an active list
/// and lands on the edit page with the count.
#[sqlx::test(migrations = "../migrations")]
async fn ac28_a_plain_post_copies_from_an_archived_source(pool: MySqlPool) {
    let w = ro_world(&pool).await;
    let d = &w.d;
    let mut bob = d.bob().await;
    let ids = item_ids(&pool, w.g, d.bob).await;
    let reply = plain(&bob)
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &w.h.to_string()),
                ("source_group_id", &w.g.to_string()),
                ("item_ids[0]", &ids[0].to_string()),
                ("item_ids[1]", &ids[1].to_string()),
            ],
        )
        .await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(format!("/groups/{}/wishlist", w.h)));
    assert_eq!(titles(&pool, w.h, d.bob).await, ["Kite", "Socks", "Book"]);
    assert_eq!(
        titles(&pool, w.g, d.bob).await,
        ["Socks", "Book"],
        "source unchanged"
    );
    let info = bob.session_info().await;
    assert!(
        has_flash_key(&info, "flash.success.wishes_copied"),
        "{:?}",
        info.flash
    );
}

/// AC35 (amended 2026-11-16, review of #61): the read-only view of the caller's own list in
/// an archived group offers no way to the edit page (no Edit button, no "add now").
#[sqlx::test(migrations = "../migrations")]
async fn ac35_the_own_list_view_of_an_archived_group_has_no_edit_links(pool: MySqlPool) {
    let d = drawn_world(&pool, utc(2026, 5, 14, 10, 0, 0)).await;
    ts::create_wishlist(&pool, d.g, d.bob, &["Socks"]).await;
    let edit_link = format!("href=\"/groups/{}/wishlist\"", d.g);
    let own = |c: i32| format!("/groups/{}/wishlist/{c}", d.g);

    // Active: the owner's view links to the edit page (Bob with items, Carol empty).
    let body = d.bob().await.get(&own(d.bob)).await.body;
    assert!(body.contains(&edit_link), "active, with items: {body}");
    let body = d.carol().await.get(&own(d.carol)).await.body;
    assert!(body.contains(&edit_link), "active, empty: {body}");
    assert!(html_contains(&body, &en("wishlist.view.addNow")));

    set_archived_at(&pool, d.g, Some(utc(2026, 6, 1, 10, 0, 0))).await;
    let page = d.bob().await.get(&own(d.bob)).await;
    assert_eq!(page.status, 200);
    assert!(page.body.contains("Socks"), "the items stay listed");
    assert!(!page.body.contains(&edit_link), "archived, with items");
    assert!(!html_contains(&page.body, &en("wishlist.edit.title")));
    let page = d.carol().await.get(&own(d.carol)).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("wishlist.view.empty")));
    assert!(!page.body.contains(&edit_link), "archived, empty");
    assert!(!html_contains(&page.body, &en("wishlist.view.addNow")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac36_the_admin_sees_the_archive_date_in_both_locales(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    make_drawn(&d, d.g, utc(2026, 12, 10, 10, 0, 0)).await;
    let mut alice = d.alice().await;
    let mut alice_de = d.german("alice").await;

    let g: GroupDetail = get_group(&mut alice, d.g).await.value();
    assert_eq!(
        g.archive_schedule.unwrap().auto_archive_at,
        utc(2027, 6, 10, 10, 0, 0)
    );
    let line = |html: &str| text_of(block(html, "auto-archive-on").expect("date line"));
    let body = alice.get(&gp(d.g)).await.body;
    assert!(line(&body).contains(&en_with(
        "group.show.autoArchiveOn",
        &[("date", "10 Jun 2027")]
    )));
    let body = alice_de.get(&gp(d.g)).await.body;
    assert!(line(&body).contains(&de_with(
        "group.show.autoArchiveOn",
        &[("date", "10.06.2027")]
    )));

    set_kept(&pool, d.g, Some(utc(2027, 7, 1, 10, 0, 0))).await;
    let body = alice.get(&gp(d.g)).await.body;
    assert!(line(&body).contains(&en_with(
        "group.show.autoArchiveOn",
        &[("date", "1 Jan 2028")]
    )));
    let body = alice_de.get(&gp(d.g)).await.body;
    assert!(line(&body).contains(&de_with(
        "group.show.autoArchiveOn",
        &[("date", "01.01.2028")]
    )));

    // An undrawn group created 2026-11-01 10:00 without dates.
    let h = ts::create_group(&pool, d.alice, "Fresh").await;
    ts::set_group_created_at(&pool, h, utc(2026, 11, 1, 10, 0, 0)).await;
    let body = alice.get(&gp(h)).await.body;
    assert!(line(&body).contains(&en_with(
        "group.show.autoArchiveOn",
        &[("date", "1 Aug 2027")]
    )));
    let body = alice_de.get(&gp(h)).await.body;
    assert!(line(&body).contains(&de_with(
        "group.show.autoArchiveOn",
        &[("date", "01.08.2027")]
    )));
}

async fn assert_member_sees_no_schedule(d: &D, drawn_at: DateTime<Utc>, what: &str) {
    let auto = plus_months(drawn_at, 6);
    for (label, mut bob) in [("en", d.bob().await), ("de", d.german("bob").await)] {
        let g: GroupDetail = get_group(&mut bob, d.g).await.value();
        assert!(g.archive_schedule.is_none(), "{what}/{label}: get_group");
        let body = bob.get(&gp(d.g)).await.body;
        let mut forbidden = vec![
            en_date(auto),
            de_date(auto),
            auto.format("%Y-%m-%d").to_string(),
            auto.format("%d.%m.%Y").to_string(),
            "group.show.autoArchiveOn".to_string(),
            "group.show.archiveSoon".to_string(),
            en("group.show.keepActive"),
            de("group.show.keepActive"),
        ];
        for key in ["group.show.autoArchiveOn", "group.show.archiveSoon"] {
            for locale in [Locale::En, Locale::De] {
                let template = translate(locale, key);
                let prefix = template.split("{date}").next().unwrap().to_string();
                forbidden.push(prefix);
            }
        }
        for text in &forbidden {
            assert!(
                !body.contains(text.as_str()),
                "{what}/{label}: the page contains {text:?}"
            );
        }
        assert_eq!(testid_count(&body, "archive-soon"), 0);
        assert_eq!(testid_count(&body, "auto-archive-on"), 0);
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac37_members_never_see_the_archive_date_outside_the_warning_window(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let drawn_at = utc(2026, 12, 10, 10, 0, 0);
    make_drawn(&d, d.g, drawn_at).await;
    assert_member_sees_no_schedule(&d, drawn_at, "outside the window").await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac37_members_never_see_the_archive_date_inside_the_warning_window(pool: MySqlPool) {
    let d = draw_world(&pool).await;
    let drawn_at = into_window(&d, d.g).await;
    let detail: GroupDetail = get_group(&mut d.alice().await, d.g).await.value();
    assert!(
        detail
            .archive_schedule
            .expect("admin schedule")
            .archive_soon,
        "the fixture is inside its window"
    );
    assert_member_sees_no_schedule(&d, drawn_at, "inside the window").await;
}

// ============================================================================== AC38: i18n

const NEW_KEYS: [&str; 24] = [
    "group.status.archived",
    "group.archive.title",
    "group.archive.intro",
    "group.archive.empty",
    "group.archive.archivedOn",
    "group.show.archivedNotice",
    "group.show.autoArchiveOn",
    "group.show.archiveSoon",
    "group.show.keepActive",
    "group.show.archiveGroup",
    "group.show.archiveGroupConfirm",
    "group.show.restoreGroup",
    "group.show.restoreGroupConfirm",
    "flash.success.group_archived",
    "flash.success.group_restored",
    "flash.success.group_kept_active",
    "flash.error.group_archived",
    "flash.error.join_closed_archived",
    "email.archiveWarning.subject",
    "email.archiveWarning.greeting",
    "email.archiveWarning.body",
    "email.archiveWarning.keep",
    "email.archiveWarning.nothing",
    "email.archiveWarning.button",
];

fn placeholders(text: &str) -> Vec<String> {
    let mut found: Vec<String> = text
        .split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_string()))
        .collect();
    found.sort();
    found
}

#[test]
fn ac38_every_new_key_exists_in_both_catalogs_with_the_same_placeholders() {
    for key in NEW_KEYS {
        let en_text = en(key);
        let de_text = de(key);
        assert!(
            !en_text.trim().is_empty() && !de_text.trim().is_empty(),
            "{key}"
        );
        assert_eq!(placeholders(&en_text), placeholders(&de_text), "{key}");
    }
    assert_eq!(placeholders(&en("group.show.archiveSoon")), ["date"]);
    assert_eq!(
        placeholders(&en("email.archiveWarning.body")),
        ["date", "group"]
    );
    assert_eq!(
        placeholders(&en("email.archiveWarning.subject")),
        ["date", "group"]
    );
    assert_eq!(placeholders(&en("email.archiveWarning.greeting")), ["name"]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac38_the_archive_pages_the_alert_the_buttons_and_the_flashes_render_in_german(
    pool: MySqlPool,
) {
    let d = draw_world(&pool).await;
    let drawn_at = into_window(&d, d.g).await;
    let mut alice = d.german("alice").await;
    let g = d.g.to_string();

    // Active page inside the window: alert, Keep active, Archive group.
    let body = alice.get(&gp(d.g)).await.body;
    let alert = block(&body, "archive-soon").expect("alert");
    assert!(html_contains(
        &text_of(alert),
        &de_with(
            "group.show.archiveSoon",
            &[("date", &de_date(plus_months(drawn_at, 6)))]
        )
    ));
    assert!(html_contains(alert, &de("group.show.keepActive")));
    assert!(html_contains(&body, &de("group.show.archiveGroup")));

    // Keep active, archive: flashes and the archived view in German.
    for (name, flash) in [
        ("keep_group_active", "flash.success.group_kept_active"),
        ("archive_group", "flash.success.group_archived"),
    ] {
        plain(&alice).call(name, &[("group_id", &g)]).await;
        let page = alice.get(&gp(d.g)).await.body;
        assert!(html_contains(&page, &de(flash)), "{name}");
    }
    let archived_at = stamps(&pool, d.g).await.archived_at.expect("archived");
    let body = alice.get(&gp(d.g)).await.body;
    assert!(html_contains(
        &text_of(block(&body, "archived-notice").expect("notice")),
        &de_with(
            "group.show.archivedNotice",
            &[("date", &de_date(archived_at))]
        )
    ));
    assert!(html_contains(&body, &de("group.status.archived")));
    assert!(html_contains(&body, &de("group.show.restoreGroup")));

    // The archive page.
    let page = alice.get("/groups/archive").await.body;
    for key in ["group.archive.title", "group.archive.intro"] {
        assert!(html_contains(&page, &de(key)), "{key}");
    }
    assert!(html_contains(
        &page,
        &de_with(
            "group.archive.archivedOn",
            &[("date", &de_date(archived_at))]
        )
    ));
    assert!(!html_contains(&page, &en("group.archive.title")));

    // Restore, and the empty archive.
    plain(&alice)
        .call("restore_group", &[("group_id", &g)])
        .await;
    let page = alice.get(&gp(d.g)).await.body;
    assert!(html_contains(&page, &de("flash.success.group_restored")));
    let page = alice.get("/groups/archive").await.body;
    assert!(html_contains(&page, &de("group.archive.empty")));

    // The join page of an archived group.
    set_archived_at(&pool, d.g, Some(utc(2026, 6, 1, 10, 0, 0))).await;
    let mut dave = d.german("dave").await;
    let page = dave.get(&format!("/join/{CODE}")).await.body;
    assert!(html_contains(
        &page,
        &de("flash.error.join_closed_archived")
    ));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac38_the_archive_texts_render_in_english_for_an_english_session(pool: MySqlPool) {
    let d = archived_world(&pool).await;
    let mut alice = d.alice().await;
    let page = alice.get("/groups/archive").await.body;
    assert!(html_contains(&page, &en("group.archive.title")));
    assert!(html_contains(&page, &en("group.archive.intro")));
    assert!(!page.contains("group.archive."), "no raw keys: {page}");
    let body = alice.get(&gp(d.g)).await.body;
    assert!(!body.contains("group.show.archive") && !body.contains("group.status.archived"));
}
