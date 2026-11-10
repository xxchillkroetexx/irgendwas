//! Install links are marked consistently in SSR and the iOS marker runs in the head.
use crate::common::auth::{Client, Opts, router};
use app::test_support::{ALICE_PASSWORD, create_alice};
use sqlx::MySqlPool;

fn assert_install_links_marked(body: &str) {
    let links: Vec<&str> = body
        .match_indices("<a ")
        .map(|(start, _)| &body[start..start + body[start..].find('>').unwrap()])
        .filter(|tag| tag.contains("href=\"/install\""))
        .collect();
    assert!(!links.is_empty());
    for tag in links {
        assert!(tag.contains("class=\"browser-install-link\""), "{tag}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn installation_links_are_marked_and_ios_detection_is_in_head(pool: MySqlPool) {
    create_alice(&pool, "alice@example.test").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    let home = client.get("/").await;
    assert!(home.status.is_success());
    assert_install_links_marked(&home.body);
    let head = &home.body[..home.body.find("</head>").unwrap()];
    assert!(head.contains("navigator.standalone===true"));
    assert!(head.contains("setAttribute('data-standalone','')"));
    assert!(client.get("/install").await.status.is_success());
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    let share = client.get("/wishes/shared").await;
    assert!(share.status.is_success());
    assert_install_links_marked(&share.body);
    assert_eq!(share.body.matches("href=\"/install\"").count(), 2);
}
