//! Locale-aware display formatting (spec: i18n-foundation, "Formatting").
//!
//! Formats are fixed per locale (no ICU): en `10 Dec 2026` / `10 Dec 2026, 14:30`,
//! de `10.12.2026` / `10.12.2026 14:30`; integers grouped by thousands (en `,`, de `.`).
//! Dates are converted to the page's time zone first. Display formatting goes only through
//! this module (via [`I18n`]); slices never call chrono's `.format` themselves.

use std::borrow::Cow;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use crate::i18n::{I18n, Locale};
use crate::tz::to_local;

/// `10 Dec 2026` (en) / `10.12.2026` (de), in `tz`.
pub fn date(locale: Locale, tz: Tz, dt: DateTime<Utc>) -> String {
    let pattern = match locale {
        Locale::En => "%-d %b %Y",
        Locale::De => "%d.%m.%Y",
    };
    to_local(dt, tz).format(pattern).to_string()
}

/// `10 Dec 2026, 14:30` (en) / `10.12.2026 14:30` (de), in `tz`.
pub fn datetime(locale: Locale, tz: Tz, dt: DateTime<Utc>) -> String {
    let pattern = match locale {
        Locale::En => "%-d %b %Y, %H:%M",
        Locale::De => "%d.%m.%Y %H:%M",
    };
    to_local(dt, tz).format(pattern).to_string()
}

/// Integer grouped by thousands: `2,000` (en) / `2.000` (de).
pub fn number(locale: Locale, n: i128) -> String {
    let separator = match locale {
        Locale::En => ',',
        Locale::De => '.',
    };
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(digit);
    }
    out
}

/// A date argument for `t!` (rendered with `I18n::date`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDate(pub DateTime<Utc>);

/// A date-time argument for `t!` (rendered with `I18n::datetime`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime(pub DateTime<Utc>);

/// How a `t!` argument is turned into text.
///
/// Implemented for strings (unchanged), integers (grouped with [`I18n::number`]) and
/// [`LocalDate`] / [`LocalDateTime`]. Nothing else, so a bare `DateTime<Utc>` or another
/// `Display` type does not compile as an argument.
pub trait MessageArg {
    fn render(&self, i18n: &I18n) -> String;
}

impl MessageArg for &str {
    fn render(&self, _i18n: &I18n) -> String {
        (*self).to_string()
    }
}

impl MessageArg for String {
    fn render(&self, _i18n: &I18n) -> String {
        self.clone()
    }
}

impl MessageArg for &String {
    fn render(&self, _i18n: &I18n) -> String {
        (*self).clone()
    }
}

impl MessageArg for Cow<'_, str> {
    fn render(&self, _i18n: &I18n) -> String {
        self.to_string()
    }
}

macro_rules! integer_arg {
    ($($t:ty),*) => {$(
        impl MessageArg for $t {
            fn render(&self, i18n: &I18n) -> String {
                // Every primitive integer except u128 fits; saturate the u128 overflow.
                #[allow(clippy::unnecessary_fallible_conversions)]
                let n = i128::try_from(*self).unwrap_or(i128::MAX);
                i18n.number(n)
            }
        }
    )*};
}
integer_arg!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

impl MessageArg for LocalDate {
    fn render(&self, i18n: &I18n) -> String {
        i18n.date(self.0)
    }
}

impl MessageArg for LocalDateTime {
    fn render(&self, i18n: &I18n) -> String {
        i18n.datetime(self.0)
    }
}
