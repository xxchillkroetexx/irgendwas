//! The archive clock of a group (spec: group-archive, A4). Pure: no I/O, no feature gates,
//! used by the sweep and by `get_group` (the admin's archive date).
//!
//! A drawn group (`is_drawn` and `drawn_at` set) is archived [`ARCHIVE_AFTER_MONTHS_DRAWN`]
//! months after the later of `drawn_at` and `kept_active_at`; any other group
//! [`ARCHIVE_AFTER_MONTHS_NOT_DRAWN`] months after the latest of `created_at`,
//! `registration_deadline`, `draw_date` and `kept_active_at`. Months are calendar months in
//! UTC (the day is clamped to the month's last day). The warning is due
//! [`ARCHIVE_WARNING_DAYS`] days before the archive date.

use chrono::{DateTime, Duration, Months, Utc};

/// Months after the draw (or the last "keep active") until a drawn group is archived.
pub const ARCHIVE_AFTER_MONTHS_DRAWN: u32 = 6;
/// Months after the latest reference date until a group that is not drawn is archived.
pub const ARCHIVE_AFTER_MONTHS_NOT_DRAWN: u32 = 9;
/// Days before the archive date the admin gets the warning mail.
pub const ARCHIVE_WARNING_DAYS: i64 = 14;

/// The columns of a `groups` row the archive clock depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupClock {
    pub is_drawn: bool,
    pub drawn_at: Option<DateTime<Utc>>,
    /// `UNIX_EPOCH` when the (nullable) column is NULL.
    pub created_at: DateTime<Utc>,
    pub registration_deadline: Option<DateTime<Utc>>,
    pub draw_date: Option<DateTime<Utc>>,
    /// The admin's last restart of the clock (keep active, restore, edit, reset).
    pub kept_active_at: Option<DateTime<Utc>>,
}

/// `(warn_at, auto_archive_at)` of the group (A4). Drawn means both `is_drawn` and
/// `drawn_at` are set (draw D6); anything else counts as not drawn.
pub fn archive_dates(clock: &GroupClock) -> (DateTime<Utc>, DateTime<Utc>) {
    let (reference, months) = match clock.drawn_at.filter(|_| clock.is_drawn) {
        Some(drawn_at) => (
            drawn_at.max(clock.kept_active_at.unwrap_or(drawn_at)),
            ARCHIVE_AFTER_MONTHS_DRAWN,
        ),
        None => (
            [
                clock.registration_deadline,
                clock.draw_date,
                clock.kept_active_at,
            ]
            .into_iter()
            .flatten()
            .fold(clock.created_at, DateTime::max),
            ARCHIVE_AFTER_MONTHS_NOT_DRAWN,
        ),
    };
    let archive_at = reference
        .checked_add_months(Months::new(months))
        .unwrap_or(DateTime::<Utc>::MAX_UTC);
    let warn_at = archive_at
        .checked_sub_signed(Duration::days(ARCHIVE_WARNING_DAYS))
        .unwrap_or(archive_at);
    (warn_at, archive_at)
}
