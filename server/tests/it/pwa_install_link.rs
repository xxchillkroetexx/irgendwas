//! Issue #74: install links are marked so the installed app (standalone display mode, or
//! `navigator.standalone` on older iOS Safari) can hide them without changing SSR output.
use crate::common::auth::{Client, Opts, router};
use app::test_support::{ALICE_PASSWORD, create_alice};
use sqlx::MySqlPool;

/// Every `<a href="/install" ...>` in `body` carries the `install-link` class.
fn assert_install_links_marked(body: &str, page: &str) {
    let links: Vec<&str> = body
        .match_indices("<a ")
        .map(|(start, _)| &body[start..start + body[start..].find('>').unwrap()])
        .filter(|tag| tag.contains("href=\"/install\""))
        .collect();
    assert!(!links.is_empty(), "{page}: no install link rendered");
    for tag in links {
        assert!(
            tag.contains("class=\"install-link\""),
            "{page}: unmarked install link {tag}"
        );
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn install_links_are_marked_and_the_standalone_marker_runs_in_head(pool: MySqlPool) {
    create_alice(&pool, "alice@example.test").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);

    let home = client.get("/").await;
    assert!(home.status.is_success());
    assert_install_links_marked(&home.body, "/");
    let head = &home.body[..home.body.find("</head>").unwrap()];
    assert!(head.contains("navigator.standalone===true"));
    assert!(head.contains("setAttribute('data-standalone','')"));

    // The install page itself stays reachable by URL.
    let install = client.get("/install").await;
    assert!(install.status.is_success());

    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    let share = client.get("/wishes/shared").await;
    assert!(share.status.is_success(), "{}", share.status);
    assert_install_links_marked(&share.body, "/wishes/shared");
    assert_eq!(share.body.matches("href=\"/install\"").count(), 2);
}
