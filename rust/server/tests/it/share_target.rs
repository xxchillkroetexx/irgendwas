//! Issue #58: receiving is read-only for wishlists and drafts survive login safely.
use crate::common;

use crate::common::auth::{Client, Opts, router, urlencode};
use app::share_api::{ShareDraft, ShareInput, prepare_share};
use app::test_support::{ALICE_PASSWORD, create_alice, create_group};
use sqlx::MySqlPool;

#[test]
fn extracts_urls_from_android_text_and_rejects_active_or_credentialed_urls() {
    let draft = prepare_share(ShareInput {
        title: "A present".into(),
        text: "Look at https://shop.example/product?a=1&b=2".into(),
        url: String::new(),
    });
    assert_eq!(draft.title, "A present");
    assert_eq!(draft.link, "https://shop.example/product?a=1&b=2");
    assert!(draft.description.starts_with("Look at"));
    for url in [
        "javascript:alert(1)",
        "data:text/html,x",
        "https://user:secret@example.test",
    ] {
        assert!(
            prepare_share(ShareInput {
                url: url.into(),
                ..Default::default()
            })
            .link
            .is_empty()
        );
    }
    assert_eq!(
        prepare_share(ShareInput {
            title: "https://shop.example/title".into(),
            ..Default::default()
        })
        .link,
        "https://shop.example/title"
    );
}

async fn receive(client: &mut Client, title: &str, link: &str) -> String {
    let reply = client
        .get(&format!(
            "/share-target?title={}&text={}",
            urlencode(title),
            urlencode(link)
        ))
        .await;
    assert_eq!(reply.status.as_u16(), 303);
    assert_eq!(reply.header("cache-control").as_deref(), Some("no-store"));
    let path = reply.location().unwrap();
    assert!(path.len() < 100);
    assert!(!path.contains("shop.example"));
    path
}

async fn count(pool: &MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM wishlist_items")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn long_share_survives_login_and_only_confirmed_post_creates_a_wish(pool: MySqlPool) {
    let alice = create_alice(&pool, "alice@example.test").await;
    let group = create_group(&pool, alice, "Family").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    let link = format!("https://shop.example/product?tracking={}", "x".repeat(1000));
    let review = receive(&mut client, "Birthday present", &link).await;
    let login = client.get(&review).await;
    assert_eq!(login.status.as_u16(), 302);
    assert_eq!(login.location().unwrap(), app::paths::login_url(&review));
    assert_eq!(count(&pool).await, 0);
    client
        .login_next("alice@example.test", ALICE_PASSWORD, &review)
        .await
        .ok();
    let page = client.get(&review).await;
    assert!(page.status.is_success());
    assert!(page.body.contains("Birthday present"));
    assert!(page.body.contains(&link));
    assert!(page.body.contains("Family"));
    let id = review.split_once("id=").unwrap().1;
    let reply = client
        .call(
            "add_shared_wish",
            &[
                ("id", id),
                ("group_id", &group.to_string()),
                ("title", "Birthday present"),
                ("description", "details"),
                ("link", &link),
            ],
        )
        .await;
    assert!(reply.status.is_success(), "{}", reply.body);
    assert_eq!(count(&pool).await, 1);
    assert_eq!(
        reply.location().as_deref(),
        Some(format!("/groups/{group}/wishlist").as_str())
    );
    let consumed = client.call("get_shared_draft", &[("id", id)]).await;
    assert_eq!(consumed.status.as_u16(), 404);
}

#[sqlx::test(migrations = "../migrations")]
async fn drafts_are_session_private_and_invalid_edits_are_retained(pool: MySqlPool) {
    let alice = create_alice(&pool, "alice@example.test").await;
    let own = create_group(&pool, alice, "Own").await;
    let other = app::test_support::create_bob(&pool, "bob@example.test").await;
    let forbidden = create_group(&pool, other, "Other").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    let review = receive(&mut client, "Shared", "https://shop.example/product").await;
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    let id = review.split_once("id=").unwrap().1;
    let mut another = Client::new(&app);
    another
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    assert_eq!(
        another
            .call("get_shared_draft", &[("id", id)])
            .await
            .status
            .as_u16(),
        404
    );
    let reply = client
        .call(
            "add_shared_wish",
            &[
                ("id", id),
                ("group_id", &forbidden.to_string()),
                ("title", "Edited"),
                ("description", "preserve me"),
                ("link", "https://shop.example/product"),
            ],
        )
        .await;
    assert_eq!(reply.status.as_u16(), 404);
    assert_eq!(count(&pool).await, 0);
    let draft = client.call("get_shared_draft", &[("id", id)]).await;
    let draft: ShareDraft = serde_json::from_str(&draft.body).unwrap();
    assert_eq!(draft.title, "Edited");
    assert_eq!(draft.description, "preserve me");
    let reply = client
        .call(
            "add_shared_wish",
            &[
                ("id", id),
                ("group_id", &own.to_string()),
                ("title", "Edited"),
                ("description", "preserve me"),
                ("link", "javascript:alert(1)"),
            ],
        )
        .await;
    assert_eq!(reply.status.as_u16(), 422);
    assert_eq!(count(&pool).await, 0);
    client.call("logout", &[]).await.ok();
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    assert_eq!(
        client
            .call("get_shared_draft", &[("id", id)])
            .await
            .status
            .as_u16(),
        404
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn oversized_intake_is_rejected_and_pending_drafts_are_bounded(pool: MySqlPool) {
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    assert_eq!(
        client
            .get(&format!("/share-target?text={}", "x".repeat(16_385)))
            .await
            .status
            .as_u16(),
        422
    );
    let first = receive(&mut client, "first", "https://shop.example/first").await;
    for _ in 0..5 {
        receive(&mut client, "other", "https://shop.example/other").await;
    }
    create_alice(&pool, "alice@example.test").await;
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    let id = first.split_once("id=").unwrap().1;
    assert_eq!(
        client
            .call("get_shared_draft", &[("id", id)])
            .await
            .status
            .as_u16(),
        404
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn plain_forms_keep_edits_through_login_and_validation_then_confirm(pool: MySqlPool) {
    let alice = create_alice(&pool, "alice@example.test").await;
    let group = create_group(&pool, alice, "Family").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app).header("accept", "text/html");
    let reply = client
        .call(
            "add_shared_wish",
            &[
                ("id", ""),
                ("group_id", &group.to_string()),
                ("title", "Manual pasted wish"),
                ("description", "Details"),
                ("link", "https://shop.example/pasted"),
            ],
        )
        .await;
    assert_eq!(reply.status.as_u16(), 302);
    let login = reply.location().unwrap();
    assert!(login.starts_with("/auth/login?next="));
    let review = common::auth::urldecode(login.split_once("next=").unwrap().1);
    client.headers.clear();
    client
        .login_next("alice@example.test", ALICE_PASSWORD, &review)
        .await
        .ok();
    let page = client.get(&review).await;
    assert!(page.body.contains("Manual pasted wish"));
    assert!(page.body.contains("https://shop.example/pasted"));
    assert!(page.body.contains("/api/add_shared_wish"));
    let id = review.split_once("id=").unwrap().1;
    client.headers.push(("accept".into(), "text/html".into()));
    let rejected = client
        .call(
            "add_shared_wish",
            &[
                ("id", id),
                ("group_id", &group.to_string()),
                ("title", "Correct this link"),
                ("description", "Keep me"),
                ("link", "javascript:bad"),
            ],
        )
        .await;
    assert_eq!(rejected.status.as_u16(), 302);
    assert_eq!(rejected.location().as_deref(), Some(review.as_str()));
    let page = client.get(&review).await;
    assert!(page.body.contains("Correct this link"));
    assert!(page.body.contains("javascript:bad")); // escaped input value, never an href
    assert_eq!(count(&pool).await, 0);
    let accepted = client
        .call(
            "add_shared_wish",
            &[
                ("id", id),
                ("group_id", &group.to_string()),
                ("title", "Corrected"),
                ("description", "Keep me"),
                ("link", "https://shop.example/pasted"),
            ],
        )
        .await;
    assert_eq!(accepted.status.as_u16(), 302);
    assert_eq!(
        accepted.location().as_deref(),
        Some(format!("/groups/{group}/wishlist").as_str())
    );
    assert_eq!(count(&pool).await, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn intake_preserves_new_share_when_old_authentication_is_invalidated(pool: MySqlPool) {
    let alice = create_alice(&pool, "alice@example.test").await;
    let group = create_group(&pool, alice, "Family").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    sqlx::query("UPDATE users SET session_epoch=session_epoch+1 WHERE id=?")
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    let review = receive(
        &mut client,
        "Keep through expired auth",
        "https://shop.example/new",
    )
    .await;
    assert_eq!(client.get(&review).await.status.as_u16(), 302);
    client
        .login_next("alice@example.test", ALICE_PASSWORD, &review)
        .await
        .ok();
    let page = client.get(&review).await;
    assert!(page.body.contains("Keep through expired auth"));
    assert!(page.body.contains("https://shop.example/new"));
    let id = review.split_once("id=").unwrap().1;
    sqlx::query("UPDATE users SET session_epoch=session_epoch+1 WHERE id=?")
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    client.headers.push(("accept".into(), "text/html".into()));
    let post = client
        .call(
            "add_shared_wish",
            &[
                ("id", id),
                ("group_id", &group.to_string()),
                ("title", "Edited after timeout"),
                ("description", "keep"),
                ("link", "https://shop.example/new"),
            ],
        )
        .await;
    assert_eq!(post.status.as_u16(), 302);
    let next = common::auth::urldecode(post.location().unwrap().split_once("next=").unwrap().1);
    client.headers.clear();
    client
        .login_next("alice@example.test", ALICE_PASSWORD, &next)
        .await
        .ok();
    assert!(
        client
            .get(&next)
            .await
            .body
            .contains("Edited after timeout")
    );
    assert_eq!(count(&pool).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn shared_description_cannot_escape_the_review_textarea(pool: MySqlPool) {
    let alice = create_alice(&pool, "alice@example.test").await;
    create_group(&pool, alice, "Family").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    let attack = "</textarea><script id=share-injected>alert(1)</script>";
    let review = receive(&mut client, "Safe title", attack).await;
    let page = client.get(&review).await;
    assert_eq!(page.status.as_u16(), 200);
    assert!(
        page.body
            .contains("&lt;/textarea&gt;&lt;script id=share-injected&gt;")
    );
    assert!(!page.body.contains("<script id=share-injected>"));
}
