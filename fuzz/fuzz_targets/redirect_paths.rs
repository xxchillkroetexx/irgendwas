#![no_main]

use libfuzzer_sys::fuzz_target;
use url::Url;

// Compile the actual production module, without the rest of the Leptos app. This keeps
// the fuzz build small while ensuring that a change to the validator is exercised here.
#[allow(dead_code)]
#[path = "../../app/src/paths.rs"]
mod paths;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    // Bare mutations often do not begin with `/`; also exercise them as path tails.
    let prefixed = format!("/{input}");
    for candidate in [input.as_ref(), prefixed.as_str()] {
        let accepted = paths::safe_return_path(candidate);
        let next = paths::safe_next(candidate);
        if let Some(path) = &accepted {
            assert_eq!(path, candidate, "validation must not rewrite a path");
            // A browser resolving the accepted path must not leave this origin.
            let base = Url::parse("https://example.invalid/base").unwrap();
            let resolved = base.join(path).expect("accepted path must parse as a URL");
            assert_eq!(resolved.origin(), base.origin(), "redirect left the origin");
        }
        if let Some(path) = &next {
            assert_eq!(accepted.as_ref(), Some(path), "safe_next must be a subset");
        }
        let expected = next.as_deref().unwrap_or(paths::DASHBOARD_PATH);
        assert_eq!(paths::next_or_dashboard(Some(candidate)), expected);

        // Encoding an arbitrary UTF-8 value for `?next` must round-trip as one value,
        // including `&`, `=`, `+`, `%`, non-ASCII text, and controls.
        let query = format!("next={}", paths::encode_query_value(candidate));
        let fields: Vec<_> = url::form_urlencoded::parse(query.as_bytes()).collect();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "next");
        assert_eq!(fields[0].1, candidate);
    }
});
