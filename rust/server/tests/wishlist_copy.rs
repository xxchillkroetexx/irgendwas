//! Issue #57: authorization, atomic bounds, source preservation and progressive HTML.
mod common;
use app::{
    AppError,
    test_support::*,
    wishlist_api::{WishlistSource, WishlistView},
};
use common::auth::{Client, Opts, router};
use sqlx::MySqlPool;

async fn add(client: &mut Client, group: i32, title: &str) {
    let reply = client
        .call(
            "add_wishlist_item",
            &[
                ("group_id", &group.to_string()),
                ("title", title),
                ("description", "Some details"),
                ("link", "shop.example/product"),
            ],
        )
        .await;
    assert!(reply.status.is_success(), "{}", reply.body);
}
async fn list(client: &mut Client, group: i32) -> WishlistView {
    let reply = client
        .call("get_my_wishlist", &[("group_id", &group.to_string())])
        .await;
    assert!(reply.status.is_success(), "{}", reply.body);
    serde_json::from_str(&reply.body).unwrap()
}
fn err(reply: &common::auth::Reply) -> AppError {
    serde_json::from_str(&reply.body).unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn copies_selected_own_items_preserving_source_and_order(pool: MySqlPool) {
    let alice = create_alice(&pool, "alice@example.test").await;
    let old = create_group(&pool, alice, "Old").await;
    let new = create_group(&pool, alice, "New").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    for title in ["First", "Skip", "Last"] {
        add(&mut client, old, title).await;
    }
    let original = list(&mut client, old).await;
    let before: String =
        sqlx::query_scalar("SELECT CAST(updated_at AS CHAR) FROM wishlists WHERE group_id=?")
            .bind(old)
            .fetch_one(&pool)
            .await
            .unwrap();
    let page = client
        .get(&format!("/groups/{new}/wishlist?source={old}"))
        .await;
    assert!(page.status.is_success(), "{}", page.body);
    assert!(page.body.contains("item_ids[2]"));
    let reply = client
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &new.to_string()),
                ("source_group_id", &old.to_string()),
                ("item_ids[2]", &original.items[2].id.to_string()),
                ("item_ids[0]", &original.items[0].id.to_string()),
            ],
        )
        .await;
    assert!(reply.status.is_success(), "{}", reply.body);
    let copied = list(&mut client, new).await;
    assert_eq!(
        copied
            .items
            .iter()
            .map(|i| i.title.as_str())
            .collect::<Vec<_>>(),
        vec!["First", "Last"]
    );
    for item in &copied.items {
        assert!(!original.items.iter().any(|old| old.id == item.id));
        assert_eq!(item.description.as_deref(), Some("Some details"));
        assert_eq!(item.link.as_deref(), Some("https://shop.example/product"));
    }
    assert_eq!(list(&mut client, old).await, original);
    let after: String =
        sqlx::query_scalar("SELECT CAST(updated_at AS CHAR) FROM wishlists WHERE group_id=?")
            .bind(old)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wishlists WHERE group_id=?")
        .bind(new)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn rejects_foreign_ids_and_groups_without_partial_writes(pool: MySqlPool) {
    let alice = create_alice(&pool, "alice@example.test").await;
    let bob = create_bob(&pool, "bob@example.test").await;
    let source = create_group(&pool, alice, "Source").await;
    let target = create_group(&pool, alice, "Target").await;
    let foreign = create_group(&pool, bob, "Foreign").await;
    add_member(&pool, source, bob).await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    add(&mut client, source, "Own").await;
    let own = list(&mut client, source).await.items[0].id;
    let reply = client
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &target.to_string()),
                ("source_group_id", &source.to_string()),
                ("item_ids[0]", &own.to_string()),
                ("item_ids[1]", "99999"),
            ],
        )
        .await;
    assert_eq!(err(&reply), AppError::NotFound);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wishlists WHERE group_id=?")
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let reply = client
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &foreign.to_string()),
                ("source_group_id", &source.to_string()),
                ("item_ids[0]", &own.to_string()),
            ],
        )
        .await;
    assert_eq!(err(&reply), AppError::NotFound);
    let mut other = Client::new(&app);
    other.login("bob@example.test", BOB_PASSWORD).await.ok();
    let reply = other
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &foreign.to_string()),
                ("source_group_id", &source.to_string()),
                ("item_ids[0]", &own.to_string()),
            ],
        )
        .await;
    assert_eq!(err(&reply), AppError::NotFound);
    let reply = Client::new(&app)
        .call("get_my_wishlist", &[("group_id", &source.to_string())])
        .await;
    assert_eq!(err(&reply), AppError::Unauthorized);
    sqlx::query("DELETE FROM group_members WHERE user_id=? AND group_id=?")
        .bind(bob)
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    let reply = other
        .call("get_my_wishlist", &[("group_id", &source.to_string())])
        .await;
    assert_eq!(err(&reply), AppError::NotFound);
}

#[sqlx::test(migrations = "../migrations")]
async fn caps_copy_atomically_and_keeps_drawn_sources_available(pool: MySqlPool) {
    let user = create_alice(&pool, "alice@example.test").await;
    let source = create_group(&pool, user, "Historical").await;
    let target = create_group(&pool, user, "Current").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    add(&mut client, source, "One").await;
    add(&mut client, source, "Two").await;
    for _ in 0..49 {
        add(&mut client, target, "Existing").await;
    }
    sqlx::query("UPDATE `groups` SET is_drawn=TRUE WHERE id=?")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    let reply = client
        .call("get_wishlist_sources", &[("group_id", &target.to_string())])
        .await;
    let sources: Vec<WishlistSource> = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(sources[0].group_id, source);
    let original = list(&mut client, source).await;
    let reply = client
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &target.to_string()),
                ("source_group_id", &source.to_string()),
                ("item_ids[0]", &original.items[0].id.to_string()),
                ("item_ids[1]", &original.items[1].id.to_string()),
            ],
        )
        .await;
    assert_eq!(
        err(&reply),
        AppError::Validation("validation.wishlist.too_many_items".into())
    );
    assert_eq!(list(&mut client, target).await.items.len(), 49);
    assert_eq!(list(&mut client, source).await, original);
}

#[test]
fn item_validation_rejects_active_links_and_limits_unicode() {
    use app::wishlist_api::validate_item;
    assert!(validate_item(" ", "", "").is_err());
    assert!(validate_item(&"🎁".repeat(255), "", "example.com").is_ok());
    assert!(validate_item(&"🎁".repeat(256), "", "").is_err());
    for link in [
        "javascript://alert(1)",
        "data://x",
        "https://user:pass@example.com",
        "https://",
    ] {
        assert!(validate_item("Wish", "", link).is_err(), "{link}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn concurrent_opposite_copies_finish_without_deadlock(pool: MySqlPool) {
    let user = create_alice(&pool, "alice@example.test").await;
    let a = create_group(&pool, user, "A").await;
    let b = create_group(&pool, user, "B").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    add(&mut client, a, "From A").await;
    add(&mut client, b, "From B").await;
    let ai = list(&mut client, a).await.items[0].id;
    let bi = list(&mut client, b).await.items[0].id;
    let mut second = client.fork();
    let a = a.to_string();
    let b = b.to_string();
    let ai = ai.to_string();
    let bi = bi.to_string();
    let fields_one = [
        ("group_id", a.as_str()),
        ("source_group_id", b.as_str()),
        ("item_ids[0]", bi.as_str()),
    ];
    let fields_two = [
        ("group_id", b.as_str()),
        ("source_group_id", a.as_str()),
        ("item_ids[0]", ai.as_str()),
    ];
    let (one, two) = tokio::join!(
        client.call("copy_wishlist_items", &fields_one),
        second.call("copy_wishlist_items", &fields_two)
    );
    assert!(one.status.is_success(), "{}", one.body);
    assert!(two.status.is_success(), "{}", two.body);
    assert_eq!(list(&mut client, a.parse().unwrap()).await.items.len(), 2);
    assert_eq!(list(&mut client, b.parse().unwrap()).await.items.len(), 2);
}

#[sqlx::test(migrations = "../migrations")]
async fn html_copy_redirects_with_flash_and_empty_selection_has_no_side_effect(pool: MySqlPool) {
    let user = create_alice(&pool, "alice@example.test").await;
    let source = create_group(&pool, user, "Source").await;
    let target = create_group(&pool, user, "Target").await;
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    client
        .login("alice@example.test", ALICE_PASSWORD)
        .await
        .ok();
    add(&mut client, source, "Wish").await;
    let item = list(&mut client, source).await.items[0].id;
    client.headers.push(("accept".into(), "text/html".into()));
    let empty = client
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &target.to_string()),
                ("source_group_id", &source.to_string()),
            ],
        )
        .await;
    assert_eq!(empty.status, axum::http::StatusCode::FOUND);
    assert_eq!(empty.location(), Some(format!("/groups/{target}/wishlist")));
    client.headers.clear();
    let info = client.session_info().await;
    assert!(
        info.flash
            .iter()
            .any(|f| f.key == "validation.wishlist.selection_required")
    );
    assert!(list(&mut client, target).await.items.is_empty());
    client.headers.push(("accept".into(), "text/html".into()));
    let reply = client
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &target.to_string()),
                ("source_group_id", &source.to_string()),
                ("item_ids[0]", &item.to_string()),
                ("item_ids[2]", &item.to_string()),
            ],
        )
        .await;
    assert_eq!(reply.status, axum::http::StatusCode::FOUND);
    let html = client.get(&reply.location().unwrap()).await;
    assert!(html.body.contains("Wish"));
    assert!(html.body.contains("Wishes added to your list."));
    client.headers.clear();
    assert_eq!(list(&mut client, target).await.items.len(), 1);
}
