//! The archive sweep (spec: group-archive A5, A6): archives groups past their archive date
//! and warns their admin 14 days before. Runs outside any request: it gets the pool, the
//! config and a [`MailTransport`] explicitly and never reads the Leptos context.
//!
//! One sweep reads the clock columns of every active group without locks and decides per
//! group in Rust ([`archive_dates`]). Each group that needs something is then handled in its
//! own short READ COMMITTED transaction that takes the group row `FOR UPDATE` and decides
//! again under the lock (restored, kept active, reset, edited, archived or warned meanwhile):
//!
//! 1. `now >= auto_archive_at`: archive (`archived_at = now`), no mail (U2, U6);
//! 2. else `now >= warn_at` and no warning for this archive date yet: record the warning
//!    (`archive_warned_at = now`), read the current admin, commit, **then** send the mail
//!    (no connection held during SMTP) and await its outcome. A failed delivery writes the
//!    previous value back, so the next sweep tries again while the window is open.
//!
//! A warning only counts for the archive date it was sent for (`archive_warned_at >=
//! warn_at`), so no reset is needed anywhere: every restart of the clock moves `warn_at`
//! past the old warning.
//!
//! Logs (A13): group ids, user ids, counts and error kinds only; never names, addresses or
//! mail texts.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use sqlx::{FromRow, MySql, MySqlPool, Transaction};
use tokio::task::JoinHandle;

use super::archive_mail;
use super::config::Config;
use super::db::{self, IsolationLevel};
use super::draw_worker::MailTransport;
use super::i18n::mail_i18n;
pub use crate::domain::archive::{
    ARCHIVE_AFTER_MONTHS_DRAWN, ARCHIVE_AFTER_MONTHS_NOT_DRAWN, ARCHIVE_WARNING_DAYS, GroupClock,
    archive_dates,
};

/// How often the server process sweeps (the first sweep runs right after startup).
pub const ARCHIVE_SWEEP_PERIOD: Duration = Duration::from_secs(3600);

/// What one sweep did: warning mails delivered, groups archived.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepResult {
    pub warned: usize,
    pub archived: usize,
}

/// The clock columns of a `groups` row (plus the warning state).
#[derive(FromRow)]
struct ClockRow {
    id: i32,
    is_drawn: bool,
    drawn_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    registration_deadline: Option<DateTime<Utc>>,
    draw_date: Option<DateTime<Utc>>,
    kept_active_at: Option<DateTime<Utc>>,
    archive_warned_at: Option<DateTime<Utc>>,
    archived_at: Option<DateTime<Utc>>,
}

/// What is due for a group at `now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Archive,
    Warn {
        auto_archive_at: DateTime<Utc>,
        previous: Option<DateTime<Utc>>,
    },
    Nothing,
}

impl ClockRow {
    fn step(&self, now: DateTime<Utc>) -> Step {
        if self.archived_at.is_some() {
            return Step::Nothing;
        }
        let (warn_at, auto_archive_at) = archive_dates(&GroupClock {
            is_drawn: self.is_drawn,
            drawn_at: self.drawn_at,
            created_at: self.created_at.unwrap_or(DateTime::UNIX_EPOCH),
            registration_deadline: self.registration_deadline,
            draw_date: self.draw_date,
            kept_active_at: self.kept_active_at,
        });
        if now >= auto_archive_at {
            Step::Archive
        } else if now >= warn_at && self.archive_warned_at.is_none_or(|at| at < warn_at) {
            Step::Warn {
                auto_archive_at,
                previous: self.archive_warned_at,
            }
        } else {
            Step::Nothing
        }
    }
}

/// The [`ClockRow`] columns of a `groups` select.
macro_rules! clock_columns {
    () => {
        "id, is_drawn, drawn_at, created_at, registration_deadline, draw_date, \
         kept_active_at, archive_warned_at, archived_at"
    };
}

/// The admin a warning goes to (the group's admin at the time of the warning, A7/AC8).
#[derive(FromRow)]
struct AdminRow {
    admin_id: i32,
    email: String,
    name: String,
    locale: Option<String>,
    group_name: String,
}

/// What [`handle`] did with one group.
enum Handled {
    Archived,
    Warned,
    Skipped,
}

/// Runs one sweep at `now` (spec A6). Never fails: a failure on one group is logged and the
/// sweep goes on with the next.
pub async fn run_archive_sweep(
    pool: &MySqlPool,
    config: &Config,
    mail: &dyn MailTransport,
    now: DateTime<Utc>,
) -> SweepResult {
    // TIMESTAMP columns keep whole seconds: compare and store `now` the same way, so the
    // conditional write-back after a failed mail matches the stored value.
    let now = Utc
        .timestamp_opt(now.timestamp(), 0)
        .single()
        .unwrap_or(now);
    let mut result = SweepResult::default();
    let rows = sqlx::query_as::<_, ClockRow>(concat!(
        "SELECT ",
        clock_columns!(),
        " FROM `groups` WHERE archived_at IS NULL ORDER BY id"
    ))
    .fetch_all(pool)
    .await;
    let rows = match rows {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!(error = %db::redacted(&err), "archive sweep: could not read the groups");
            return result;
        }
    };
    for row in rows {
        if row.step(now) == Step::Nothing {
            continue;
        }
        let group_id = row.id;
        match handle(pool, config, mail, group_id, now).await {
            Ok(Handled::Archived) => result.archived += 1,
            Ok(Handled::Warned) => result.warned += 1,
            Ok(Handled::Skipped) => {}
            Err(err) => {
                tracing::error!(group_id, error = %db::redacted(&err), "archive sweep: database error");
            }
        }
    }
    if result != SweepResult::default() {
        tracing::info!(
            warned = result.warned,
            archived = result.archived,
            "archive sweep done"
        );
    }
    result
}

/// Archives or warns one group, decided again under its row lock.
async fn handle(
    pool: &MySqlPool,
    config: &Config,
    mail: &dyn MailTransport,
    group_id: i32,
    now: DateTime<Utc>,
) -> Result<Handled, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let mut tx = db::begin_with(&mut conn, IsolationLevel::ReadCommitted).await?;
    let locked = sqlx::query_as::<_, ClockRow>(concat!(
        "SELECT ",
        clock_columns!(),
        " FROM `groups` WHERE id = ? FOR UPDATE"
    ))
    .bind(group_id)
    .fetch_optional(&mut *tx)
    .await?;
    // Deleted meanwhile: nothing to do.
    let Some(locked) = locked else {
        tx.rollback().await?;
        return Ok(Handled::Skipped);
    };
    match locked.step(now) {
        Step::Nothing => {
            tx.rollback().await?;
            Ok(Handled::Skipped)
        }
        Step::Archive => {
            let archived = sqlx::query(
                "UPDATE `groups` SET archived_at = ? WHERE id = ? AND archived_at IS NULL",
            )
            .bind(now)
            .bind(group_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            tx.commit().await?;
            if archived == 0 {
                return Ok(Handled::Skipped);
            }
            tracing::info!(group_id, "group archived by the archive sweep");
            Ok(Handled::Archived)
        }
        Step::Warn {
            auto_archive_at,
            previous,
        } => {
            let admin = record_warning(&mut tx, group_id, now).await?;
            tx.commit().await?;
            drop(conn);
            warn(
                pool,
                config,
                mail,
                group_id,
                admin,
                auto_archive_at,
                previous,
                now,
            )
            .await
        }
    }
}

/// Records the warning for this archive date and reads who gets it (inside the lock).
async fn record_warning(
    tx: &mut Transaction<'_, MySql>,
    group_id: i32,
    now: DateTime<Utc>,
) -> Result<AdminRow, sqlx::Error> {
    sqlx::query("UPDATE `groups` SET archive_warned_at = ? WHERE id = ?")
        .bind(now)
        .bind(group_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query_as::<_, AdminRow>(
        "SELECT u.id AS admin_id, u.email, u.name, u.locale, g.name AS group_name \
         FROM `groups` g JOIN users u ON u.id = g.admin_id WHERE g.id = ?",
    )
    .bind(group_id)
    .fetch_one(&mut **tx)
    .await
}

/// Sends the recorded warning; a failed delivery writes the previous value back (only if
/// nothing else changed the warning meanwhile), so the next sweep tries again.
#[allow(clippy::too_many_arguments)]
async fn warn(
    pool: &MySqlPool,
    config: &Config,
    mail: &dyn MailTransport,
    group_id: i32,
    admin: AdminRow,
    auto_archive_at: DateTime<Utc>,
    previous: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<Handled, sqlx::Error> {
    let i18n = mail_i18n(admin.locale.as_deref(), config);
    let message = archive_mail::archive_warning(
        &i18n,
        config,
        &admin.email,
        &admin.name,
        group_id,
        &admin.group_name,
        auto_archive_at,
    );
    let user_id = admin.admin_id;
    match mail.send(message).await {
        Ok(()) => {
            tracing::info!(group_id, user_id, "archive warning sent");
            Ok(Handled::Warned)
        }
        Err(err) => {
            tracing::warn!(
                group_id,
                user_id,
                error = err.kind(),
                "archive warning not delivered; retried by the next sweep"
            );
            sqlx::query(
                "UPDATE `groups` SET archive_warned_at = ? WHERE id = ? AND archive_warned_at = ?",
            )
            .bind(previous)
            .bind(group_id)
            .bind(now)
            .execute(pool)
            .await?;
            Ok(Handled::Skipped)
        }
    }
}

/// Runs [`run_archive_sweep`] at once and then every `period` (spec A5). Started by the
/// server process next to the session cleanup, never by `app_state` (tests have no
/// background sweeper). Each sweep runs in its own task: a panic ends only that sweep.
pub fn spawn_archive_sweep(
    pool: MySqlPool,
    config: Arc<Config>,
    mail: Arc<dyn MailTransport>,
    period: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let (pool, config, mail) = (pool.clone(), config.clone(), mail.clone());
            let sweep = tokio::spawn(async move {
                run_archive_sweep(&pool, &config, &*mail, Utc::now()).await;
            });
            if sweep.await.is_err() {
                tracing::error!("archive sweep panicked; the next one runs as scheduled");
            }
        }
    })
}
