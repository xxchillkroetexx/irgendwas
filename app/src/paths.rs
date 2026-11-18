//! Shared validator for relative return paths (spec: i18n-foundation, `safe_return_path`).

/// Longest accepted return path, in bytes.
const MAX_LEN: usize = 512;

/// The segments a browser removes or resolves while parsing a URL path (WHATWG URL
/// standard: single-dot and double-dot path segments, `%2e` in any case).
const DOT_SEGMENTS: [&str; 6] = [".", "..", "%2e", ".%2e", "%2e.", "%2e%2e"];

fn is_dot_segment(segment: &str) -> bool {
    DOT_SEGMENTS
        .iter()
        .any(|dot| segment.eq_ignore_ascii_case(dot))
}

/// The path part of a relative URL: everything before the first `?` or `#`. Browsers
/// normalize only this part; query and fragment stay as they are.
fn path_part(raw: &str) -> &str {
    raw.split(['?', '#']).next().unwrap_or(raw)
}

/// Whether the path part of `raw` is `prefix` or lies under `prefix/`.
fn path_is_under(raw: &str, prefix: &str) -> bool {
    let path = path_part(raw);
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// `Some(raw)` only for a safe, relative, same-site path: 1..=512 bytes, starts with `/`
/// whose next character is neither `/` nor `\`, no `\`, ASCII control characters or
/// spaces anywhere, no dot segment in the path part (`.`, `..` or a percent-encoded form
/// such as `%2e` or `.%2E`), and a path part that is not `/api` or under `/api/`. The
/// path part is everything before the first `?` or `#`.
///
/// Dot segments are rejected because the browser resolves them after this check:
/// `/x/../api/f` would land on `/api/f` (#87 review). Without them, the path a browser
/// requests is the path part checked here.
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
    if path_part(raw).split('/').any(is_dot_segment) {
        return None;
    }
    if path_is_under(raw, "/api") {
        return None;
    }
    Some(raw.to_string())
}

/// Where a logged-in user lands by default (the account slice owns the page).
pub const DASHBOARD_PATH: &str = "/user/dashboard";

/// The profile page (account slice).
pub const PROFILE_PATH: &str = "/user/profile";

/// The list of the caller's active groups.
pub const GROUPS_PATH: &str = "/groups";

/// The list of the caller's archived groups (group-archive).
pub const ARCHIVE_PATH: &str = "/groups/archive";

/// The page of a group.
pub fn group_path(group_id: i32) -> String {
    format!("/groups/{group_id}")
}

/// The edit page of the caller's own wishlist in a group (wishlists slice).
pub fn wishlist_edit_path(group_id: i32) -> String {
    format!("/groups/{group_id}/wishlist")
}

/// The element id of an item's row on the edit page, also the fragment of its Edit and
/// Cancel links (wishlists, #69).
pub fn wishlist_item_anchor(item_id: i32) -> String {
    format!("item-{item_id}")
}

/// The edit page with the copy section open (`?copy=1`, copy-wishes). No `#copy` fragment:
/// without JavaScript the flash is not sticky and would scroll out of view.
pub fn wishlist_copy_open_path(group_id: i32) -> String {
    format!("{}?copy=1", wishlist_edit_path(group_id))
}

/// The edit page with an item's inline form open (`?edit=`), at the item's row.
pub fn wishlist_item_edit_path(group_id: i32, item_id: i32) -> String {
    format!(
        "{}?edit={item_id}#{}",
        wishlist_edit_path(group_id),
        wishlist_item_anchor(item_id)
    )
}

/// The edit page at an item's row (Cancel of the inline form).
pub fn wishlist_item_path(group_id: i32, item_id: i32) -> String {
    format!(
        "{}#{}",
        wishlist_edit_path(group_id),
        wishlist_item_anchor(item_id)
    )
}

/// The read-only view of a member's wishlist in a group (wishlists slice; the draw slice
/// links here from the assignment cards).
pub fn wishlist_view_path(group_id: i32, user_id: i32) -> String {
    format!("/groups/{group_id}/wishlist/{user_id}")
}

/// The caller's exclusion rules in a group (exclusions slice).
pub fn exclusions_path(group_id: i32) -> String {
    format!("/groups/{group_id}/exclusions")
}

/// [`safe_return_path`] plus: the path part is not `/auth` and not under `/auth/`, so a
/// redirect never loops into an auth page or a server fn endpoint (spec: auth,
/// `safe_next`). `None` means the caller falls back to [`DASHBOARD_PATH`].
pub fn safe_next(raw: &str) -> Option<String> {
    safe_return_path(raw).filter(|path| !path_is_under(path, "/auth"))
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
