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
