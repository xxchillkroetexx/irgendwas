//! UTC ↔ APP_TIMEZONE conversion (`app::tz`).

use app::tz::{TzError, format_local, parse_input_value, to_input_value};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::Berlin;

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[test]
fn display_and_input_follow_berlin_dst() {
    // Winter: UTC+1, summer: UTC+2.
    assert_eq!(
        to_input_value(utc("2026-12-24T17:00:00Z"), Berlin),
        "2026-12-24T18:00"
    );
    assert_eq!(
        to_input_value(utc("2026-07-01T17:00:00Z"), Berlin),
        "2026-07-01T19:00"
    );
    assert_eq!(
        format_local(utc("2026-12-24T17:00:00Z"), Berlin, "%d.%m.%Y %H:%M"),
        "24.12.2026 18:00"
    );

    assert_eq!(
        parse_input_value("2026-12-24T18:00", Berlin),
        Ok(utc("2026-12-24T17:00:00Z"))
    );
    assert_eq!(
        parse_input_value("2026-07-01T19:00:00", Berlin),
        Ok(utc("2026-07-01T17:00:00Z"))
    );
}

#[test]
fn dst_edge_cases() {
    // 2026-03-29 02:30 does not exist in Berlin.
    assert_eq!(
        parse_input_value("2026-03-29T02:30", Berlin),
        Err(TzError::Nonexistent)
    );
    // 2026-10-25 02:30 exists twice; the earlier (CEST, UTC+2) wins.
    assert_eq!(
        parse_input_value("2026-10-25T02:30", Berlin),
        Ok(utc("2026-10-25T00:30:00Z"))
    );
    assert_eq!(parse_input_value("tomorrow", Berlin), Err(TzError::Invalid));
}
