//! Time zone helpers.
//!
//! The database connection runs in UTC and every timestamp is a `DateTime<Utc>`.
//! Conversion to and from `APP_TIMEZONE` (default `Europe/Berlin`) happens only at the UI
//! edge: for display, and for `<input type="datetime-local">` values.
//!
//! The server writes the configured zone into `<html data-timezone>`; the client reads it
//! back, so server and client render identical wall-clock times.

use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

/// Default zone when `APP_TIMEZONE` is not set.
pub const DEFAULT_TIMEZONE: Tz = chrono_tz::Europe::Berlin;

/// Format used by `<input type="datetime-local">`.
pub const INPUT_FORMAT: &str = "%Y-%m-%dT%H:%M";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TzError {
    #[error("invalid date/time value")]
    Invalid,
    #[error("this local time does not exist (daylight saving time gap)")]
    Nonexistent,
}

/// UTC instant → wall-clock time in `tz`.
pub fn to_local(dt: DateTime<Utc>, tz: Tz) -> DateTime<Tz> {
    dt.with_timezone(&tz)
}

/// Formats a UTC instant in `tz` with a chrono format string.
pub fn format_local(dt: DateTime<Utc>, tz: Tz, fmt: &str) -> String {
    to_local(dt, tz).format(fmt).to_string()
}

/// Value for `<input type="datetime-local">` (`YYYY-MM-DDTHH:MM`) in `tz`.
pub fn to_input_value(dt: DateTime<Utc>, tz: Tz) -> String {
    format_local(dt, tz, INPUT_FORMAT)
}

/// Format used by `<input type="date">`.
pub const DATE_INPUT_FORMAT: &str = "%Y-%m-%d";

/// Value for `<input type="date">` (`YYYY-MM-DD`, the local date in `tz`).
pub fn to_date_input_value(dt: DateTime<Utc>, tz: Tz) -> String {
    format_local(dt, tz, DATE_INPUT_FORMAT)
}

/// Parses a `YYYY-MM-DD` date input value (trimmed; a real calendar date).
pub fn parse_date_input(value: &str) -> Result<NaiveDate, TzError> {
    let value = value.trim();
    // chrono's `%m` / `%d` also accept one digit; an input value always has two.
    let shaped = value.len() == 10
        && value.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    if !shaped {
        return Err(TzError::Invalid);
    }
    NaiveDate::parse_from_str(value, DATE_INPUT_FORMAT).map_err(|_| TzError::Invalid)
}

/// Wall-clock time in `tz` → UTC instant.
///
/// Ambiguous times (the repeated hour when DST ends) resolve to the earlier instant;
/// times inside the DST gap are rejected.
pub fn from_local(naive: NaiveDateTime, tz: Tz) -> Result<DateTime<Utc>, TzError> {
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(earliest, _) => Ok(earliest.with_timezone(&Utc)),
        LocalResult::None => Err(TzError::Nonexistent),
    }
}

/// Parses a `datetime-local` value (`YYYY-MM-DDTHH:MM`, seconds optional) in `tz`.
pub fn parse_input_value(value: &str, tz: Tz) -> Result<DateTime<Utc>, TzError> {
    let value = value.trim();
    let naive = NaiveDateTime::parse_from_str(value, INPUT_FORMAT)
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| TzError::Invalid)?;
    from_local(naive, tz)
}

/// Time zone of the current page load (`APP_TIMEZONE` on the server, `<html data-timezone>`
/// on the client).
pub fn use_timezone() -> Tz {
    #[cfg(feature = "ssr")]
    {
        leptos::prelude::use_context::<crate::server::AppState>()
            .map(|s| s.config.app_timezone)
            .unwrap_or(DEFAULT_TIMEZONE)
    }
    #[cfg(not(feature = "ssr"))]
    {
        crate::components::document_attribute("data-timezone")
            .and_then(|name| name.parse::<Tz>().ok())
            .unwrap_or(DEFAULT_TIMEZONE)
    }
}
