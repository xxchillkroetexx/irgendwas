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
    // `/apiary` and `/api?x` are ordinary (only `/api` itself and `/api/...` are blocked).
    assert_eq!(safe_return_path("/apiary"), Some("/apiary".to_string()));
    assert_eq!(safe_return_path("/api?x"), Some("/api?x".to_string()));
    assert_eq!(safe_return_path("/api/"), None);
    assert_eq!(safe_return_path("/api/x/y"), None);
}
