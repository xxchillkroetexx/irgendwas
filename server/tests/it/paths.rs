//! `app::paths::safe_return_path` (i18n-foundation AC15).

use app::paths::safe_return_path;

#[test]
fn ac15_safe_paths_are_returned_unchanged() {
    for ok in [
        "/",
        "/groups/5?tab=a",
        "/auth/register",
        "/auth/reset-password/abc_DEF-123",
        "/join/ABCD2345",
    ] {
        assert_eq!(safe_return_path(ok), Some(ok.to_string()), "{ok:?}");
    }
}

#[test]
fn ac15_unsafe_paths_are_rejected() {
    let too_long = format!("/{}", "a".repeat(512));
    assert_eq!(too_long.len(), 513);
    for bad in [
        "",
        "groups",
        "//evil.example/",
        "https://evil.example/",
        "/\\evil.example",
        "/a\\b",
        "/api",
        "/api/set_locale",
        "/a b",
        "/a\r\nSet-Cookie: x=y",
        "/a\tb",
        "/a\nb",
        "/a\0b",
        too_long.as_str(),
    ] {
        assert_eq!(safe_return_path(bad), None, "{bad:?} must be rejected");
    }
}

#[test]
fn ac15_length_limit_is_512_bytes() {
    let longest = format!("/{}", "a".repeat(511));
    assert_eq!(longest.len(), 512);
    assert_eq!(safe_return_path(&longest), Some(longest.clone()));
}

#[test]
fn ac15_only_api_itself_and_its_subpaths_are_blocked() {
    // `/apiary` is ordinary (only `/api` itself and `/api/...` are blocked).
    assert_eq!(safe_return_path("/apiary"), Some("/apiary".to_string()));
    assert_eq!(
        safe_return_path("/apiary?x=/api/y"),
        Some("/apiary?x=/api/y".to_string())
    );
    assert_eq!(safe_return_path("/api/"), None);
    assert_eq!(safe_return_path("/api/x/y"), None);
    // Amended 2026-11-18 (#87 review): the check applies to the path part, so a query
    // or fragment does not hide `/api` (`/api?x` requests the path `/api`).
    assert_eq!(safe_return_path("/api?x"), None);
    assert_eq!(safe_return_path("/api#x"), None);
    assert_eq!(safe_return_path("/api/?x"), None);
}

/// Amended 2026-11-18 (#87 review): a browser resolves dot segments in the path after
/// validation, so `/x/../api/f` would land on `/api/f`.
#[test]
fn ac15_dot_segments_in_the_path_are_rejected() {
    for bad in [
        "/.",
        "/..",
        "/./",
        "/../",
        "/x/..",
        "/x/../api/set_locale",
        "/x/./groups",
        "/x/%2e%2e/api/f",
        "/x/%2E%2E/api/f",
        "/x/.%2e/api/f",
        "/x/%2E./api/f",
        "/x/%2e/groups",
        "/x/%2E",
        "/groups/..?x=1",
        "/groups/.#top",
    ] {
        assert_eq!(safe_return_path(bad), None, "{bad:?} must be rejected");
    }
}

#[test]
fn ac15_dots_that_are_no_dot_segment_stay_allowed() {
    for ok in [
        "/...",
        "/.well-known/x",
        "/x/..y",
        "/x/y..",
        "/a.b/c",
        "/x/%2e%2e%2e",
        "/x/%2fx",
        "/groups?next=/x/../api/f",
        "/groups?a=.&b=..",
        "/groups#/../api",
    ] {
        assert_eq!(safe_return_path(ok), Some(ok.to_string()), "{ok:?}");
    }
}
