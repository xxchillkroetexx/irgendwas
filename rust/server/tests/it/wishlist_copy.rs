//! #57: copy own wishes without altering source rows or weakening wishlist authorization.
use crate::common::auth::*;
use crate::common::wishlists::*;
use app::wishlists_api::{NamedRef, WishlistError, WishlistView};
use chrono::{TimeZone, Utc};
use sqlx::MySqlPool;

async fn copy(c: &mut Client, group: i32, source: i32, ids: &[i32]) -> Reply {
    let mut values = vec![
        ("group_id".to_string(), group.to_string()),
        ("source_group_id".to_string(), source.to_string()),
    ];
    values.extend(
        ids.iter()
            .enumerate()
            .map(|(n, id)| (format!("item_ids[{n}]"), id.to_string())),
    );
    let refs: Vec<_> = values
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    c.call("copy_wishlist_items", &refs).await
}

#[sqlx::test(migrations = "../migrations")]
async fn copies_multiple_own_items_in_source_order_preserving_details_and_source(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    set_updated_at(
        &pool,
        f.bob_list,
        Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
    )
    .await;
    let before = updated_at(&pool, f.w.g, f.w.bob).await;
    let original = get_my_wishlist(&mut bob, f.w.g)
        .await
        .wl_value::<WishlistView>();
    copy(&mut bob, f.h, f.w.g, &[f.c, f.a, f.a]).await.wl_ok();
    let target = get_my_wishlist(&mut bob, f.h)
        .await
        .wl_value::<WishlistView>();
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X, ITEM_A, ITEM_C]);
    assert_eq!(target.items[1].description.as_deref(), Some("Desc A"));
    assert_eq!(target.items[1].link.as_deref(), Some("https://a.example/a"));
    assert_ne!(target.items[1].id, f.a);
    assert_eq!(
        get_my_wishlist(&mut bob, f.w.g)
            .await
            .wl_value::<WishlistView>(),
        original
    );
    assert_eq!(updated_at(&pool, f.w.g, f.w.bob).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn rejects_foreign_items_groups_departed_members_and_empty_selection_atomically(
    pool: MySqlPool,
) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let before = dump(&pool).await;
    assert_eq!(
        copy(&mut bob, f.h, f.w.g, &[f.a, f.alice_item])
            .await
            .wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(dump(&pool).await, before);
    assert!(matches!(
        copy(&mut bob, f.h, f.w.g, &[]).await.wl_err(),
        WishlistError::Validation(_)
    ));
    assert!(matches!(
        copy(&mut bob, f.w.g, f.w.g, &[f.a]).await.wl_err(),
        WishlistError::Validation(_)
    ));
    let mut alice = f.w.alice().await;
    assert_eq!(
        copy(&mut alice, f.h, f.w.g, &[f.alice_item]).await.wl_err(),
        WishlistError::NotFound
    );
    sqlx::query("DELETE FROM group_members WHERE group_id=? AND user_id=?")
        .bind(f.w.g)
        .bind(f.w.bob)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        copy(&mut bob, f.h, f.w.g, &[f.a]).await.wl_err(),
        WishlistError::NotFound
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn cap_applies_to_whole_selection_without_partial_copies(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    for n in 0..48 {
        add_item(&mut bob, f.h, &format!("Extra {n}"), "", "")
            .await
            .wl_ok();
    }
    let before = dump(&pool).await;
    assert!(matches!(
        copy(&mut bob, f.h, f.w.g, &[f.a, f.b]).await.wl_err(),
        WishlistError::Validation(_)
    ));
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn sources_are_own_nonempty_current_lists_including_historical_drawn_groups(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    sqlx::query("UPDATE `groups` SET is_drawn=TRUE WHERE id=?")
        .bind(f.w.g)
        .execute(&pool)
        .await
        .unwrap();
    let sources: Vec<NamedRef> = bob
        .call("get_wishlist_sources", &[("group_id", &f.h.to_string())])
        .await
        .wl_value();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].id, f.w.g);
    let page = bob
        .get(&format!("/groups/{}/wishlist?source={}", f.h, f.w.g))
        .await;
    assert_eq!(page.status, 200);
    assert!(page.body.contains("copy-source"));
    assert!(page.body.contains(&format!("value=\"{}\"", f.a)));
    assert_no_raw_keys(&page.body, "copy editor");
    copy(&mut bob, f.h, f.w.g, &[f.a]).await.wl_ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn plain_form_posts_copy_sparse_checkbox_selection_and_redirect_with_flash(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    bob.headers.push(("accept".into(), "text/html".into()));
    let response = bob
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &f.h.to_string()),
                ("source_group_id", &f.w.g.to_string()),
                ("item_ids[0]", &f.a.to_string()),
                ("item_ids[2]", &f.c.to_string()),
            ],
        )
        .await;
    assert_eq!(response.status, 302, "{}", response.body);
    assert_eq!(response.location(), Some(edit_path(f.h)));
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X, ITEM_A, ITEM_C]);
    let response = copy(&mut bob, f.h, f.w.g, &[]).await;
    assert_eq!(response.status, 302);
    let page = bob.get(&edit_path(f.h)).await;
    assert!(page.body.contains("Select one or more wishes"));
}

#[sqlx::test(migrations = "../migrations")]
async fn opposite_copies_serialize_without_deadlock_or_source_reordering(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut first = f.w.bob().await;
    let mut second = f.w.bob().await;
    let first_ids = [f.a];
    let second_ids = [f.x];
    let (a, b) = tokio::join!(
        copy(&mut first, f.h, f.w.g, &first_ids),
        copy(&mut second, f.w.g, f.h, &second_ids)
    );
    a.wl_ok();
    b.wl_ok();
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_B, ITEM_C, ITEM_X]
    );
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X, ITEM_A]);
}
