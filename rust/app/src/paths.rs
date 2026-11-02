//! Shared validator for relative return paths (spec: i18n-foundation, `safe_return_path`).

/// Longest accepted return path, in bytes.
const MAX_LEN: usize = 512;

/// `Some(raw)` only for a safe, relative, same-site path: 1..=512 bytes, starts with `/`
/// whose next character is neither `/` nor `\`, no `\`, ASCII control characters or
/// spaces anywhere, and not `/api` or under `/api/`.
///
/// The caller redirects to exactly the returned string, never prefixed with a host.
pub fn safe_return_path(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.len() > MAX_LEN || !raw.starts_with('/') {
        return None;
    }
    if matches!(raw.as_bytes().get(1), Some(b'/' | b'\\')) {
        return None;
    }
    if raw
        .bytes()
        .any(|b| b == b'\\' || b == b' ' || b.is_ascii_control())
    {
        return None;
    }
    if raw == "/api" || raw.starts_with("/api/") {
        return None;
    }
    Some(raw.to_string())
}

/// Where a logged-in user lands by default (the account slice owns the page).
pub const DASHBOARD_PATH: &str = "/user/dashboard";

/// The profile page (account slice).
pub const PROFILE_PATH: &str = "/user/profile";

/// The page of a group.
pub fn group_path(group_id: i32) -> String {
    format!("/groups/{group_id}")
}

/// The edit page of the caller's own wishlist in a group (wishlists slice).
pub fn wishlist_edit_path(group_id: i32) -> String {
    format!("/groups/{group_id}/wishlist")
}

/// The read-only view of a member's wishlist in a group (wishlists slice; the draw slice
/// links here from the assignment cards).
pub fn wishlist_view_path(group_id: i32, user_id: i32) -> String {
    format!("/groups/{group_id}/wishlist/{user_id}")
}

/// [`safe_return_path`] plus: not `/auth` and not under `/auth/`, so a redirect never loops
/// into an auth page or a server fn endpoint (spec: auth, `safe_next`). `None` means the
/// caller falls back to [`DASHBOARD_PATH`].
pub fn safe_next(raw: &str) -> Option<String> {
    safe_return_path(raw).filter(|path| path != "/auth" && !path.starts_with("/auth/"))
}

/// Where to go after a login or registration: `safe_next(next)`, else the dashboard.
pub fn next_or_dashboard(next: Option<&str>) -> String {
    next.and_then(safe_next)
        .unwrap_or_else(|| DASHBOARD_PATH.to_string())
}

/// Percent-encodes everything except the RFC 3986 unreserved characters
/// (`A-Z a-z 0-9 - . _ ~`), for a query parameter value.
pub fn encode_query_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 3);
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `path` with `?next=<percent-encoded next>` appended when `next` is given.
pub fn with_next(path: &str, next: Option<&str>) -> String {
    match next {
        Some(next) => format!("{path}?next={}", encode_query_value(next)),
        None => path.to_string(),
    }
}

/// The login page that returns to `path_and_query` afterwards (protected-page guard).
pub fn login_url(path_and_query: &str) -> String {
    with_next("/auth/login", Some(path_and_query))
}
