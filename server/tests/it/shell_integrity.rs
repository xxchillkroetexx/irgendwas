//! Issue #71: every stylesheet and script the shell loads from another origin carries
//! Subresource Integrity, on public, token-bearing and error pages alike.
use crate::common::auth::{Client, Opts, router};
use app::test_support::{create_alice, set_reset_token};
use sqlx::MySqlPool;

/// The `<link ...>` and `<script ...>` start tags in `body`.
fn resource_tags(body: &str) -> Vec<&str> {
    ["<link ", "<script "]
        .into_iter()
        .flat_map(|open| body.match_indices(open))
        .map(|(start, _)| &body[start..start + body[start..].find('>').unwrap() + 1])
        .collect()
}

/// The value of `name="..."` in `tag`.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

fn is_external(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://") || url.starts_with("//")
}

/// Asserts that every external stylesheet and script in `body` has a SHA-384 integrity
/// hash and `crossorigin="anonymous"`; returns how many there are.
fn assert_external_resources_pinned(body: &str, page: &str) -> usize {
    let mut external = 0;
    for tag in resource_tags(body) {
        let url = if tag.starts_with("<script") {
            attr(tag, "src")
        } else {
            attr(tag, "href")
        };
        let Some(url) = url.filter(|url| is_external(url)) else {
            continue;
        };
        external += 1;
        let integrity = attr(tag, "integrity").unwrap_or_default();
        assert!(
            integrity.starts_with("sha384-") && integrity.len() == "sha384-".len() + 64,
            "{page}: {url} without a SHA-384 integrity hash: {tag}"
        );
        assert_eq!(
            attr(tag, "crossorigin"),
            Some("anonymous"),
            "{page}: {url} without crossorigin=\"anonymous\": {tag}"
        );
    }
    external
}

#[sqlx::test(migrations = "../migrations")]
async fn every_external_stylesheet_and_script_carries_an_integrity_hash(pool: MySqlPool) {
    let alice = create_alice(&pool, "alice@example.test").await;
    let token = "T".repeat(43);
    set_reset_token(&pool, alice, &token, 1800).await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);

    let reset = format!("/auth/reset-password/{token}");
    for page in [
        "/",
        "/auth/login",
        "/auth/register",
        reset.as_str(),
        "/no-such-page",
    ] {
        let reply = client.get(page).await;
        assert!(reply.body.contains("</head>"), "{page}: {}", reply.status);
        // Bootstrap's stylesheet and script bundle.
        assert_eq!(
            assert_external_resources_pinned(&reply.body, page),
            2,
            "{page}"
        );
    }
}

#[test]
fn an_unpinned_external_resource_is_detected() {
    let unpinned = r#"<head><script defer src="https://cdn.example/x.js"></script></head>"#;
    let result = std::panic::catch_unwind(|| assert_external_resources_pinned(unpinned, "t"));
    assert!(result.is_err());

    let local = r#"<link rel="stylesheet" href="/assets/css/styles.css"><script src="/pkg/a.js">"#;
    assert_eq!(assert_external_resources_pinned(local, "t"), 0);
}
