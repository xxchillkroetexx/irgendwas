#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use url::Url;

// Compile the actual production module, without the rest of the Leptos app. This keeps
// the fuzz build small while ensuring that a change to the validator is exercised here.
#[allow(dead_code)]
#[path = "../../app/src/paths.rs"]
mod paths;

/// The page a redirect starts from; the WHATWG parser resolves accepted paths against it,
/// as a browser does with a `Location` header.
static BASE: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://example.invalid/groups/5/wishlist").unwrap());

/// Whether the resolved `path` is `prefix` itself or lies under `prefix/`.
fn is_under(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The URL a browser requests when redirected to the accepted `path`.
fn resolve(path: &str) -> Url {
    let resolved = BASE
        .join(path)
        .expect("an accepted path must parse as a URL");
    // Catches a regression in the `//` and `/\` rules (scheme-relative redirects).
    assert_eq!(resolved.origin(), BASE.origin(), "{path:?} left the origin");
    resolved
}

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    // Bare mutations often do not begin with `/`; also exercise them as path tails.
    let prefixed = format!("/{input}");
    for candidate in [input.as_ref(), prefixed.as_str()] {
        // The oracle is the path after the parser's normalization (dot segments,
        // percent-encoded dots), not the raw string the validator inspects.
        if let Some(path) = paths::safe_return_path(candidate) {
            let resolved = resolve(&path);
            assert!(
                !is_under(resolved.path(), "/api"),
                "safe_return_path accepted {path:?}, which resolves to {resolved}"
            );
        }
        if let Some(path) = paths::safe_next(candidate) {
            let resolved = resolve(&path);
            assert!(
                !is_under(resolved.path(), "/api") && !is_under(resolved.path(), "/auth"),
                "safe_next accepted {path:?}, which resolves to {resolved}"
            );
        }

        // Encoding an arbitrary UTF-8 value for `?next` must round-trip as one value,
        // including `&`, `=`, `+`, `%`, non-ASCII text, and controls.
        let query = format!("next={}", paths::encode_query_value(candidate));
        let mut fields = url::form_urlencoded::parse(query.as_bytes());
        let (name, value) = fields.next().expect("one field");
        assert_eq!(name, "next");
        assert_eq!(value, candidate);
        assert!(fields.next().is_none(), "the value split into fields");
    }
});
