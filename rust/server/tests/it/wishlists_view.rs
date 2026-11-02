//! wishlists slice: viewing (AC1-AC8, AC32, AC33). Interactions are in wishlists.spec.ts.

use crate::common::auth::*;
use crate::common::groups::*;
use crate::common::wishlists::*;
use app::test_support::{self as ts, ALICE_PASSWORD, BOB_PASSWORD, CAROL_PASSWORD};
use app::wishlists_api::{WishlistError, WishlistPanel, WishlistView};
use chrono::{TimeZone, Utc};
use sqlx::MySqlPool;

// -------------------------------------------------------------------------- AC1

#[sqlx::test(migrations = "../migrations")]
async fn ac1_carol_sees_bobs_items_in_order_with_hint_date_and_no_edit_controls(pool: MySqlPool) {
    let f = fixture(&pool).await;
    set_updated_at(
        &pool,
        f.bob_list,
        Utc.with_ymd_and_hms(2026, 12, 10, 12, 0, 0).unwrap(),
    )
    .await;
    let mut carol = f.w.carol().await;

    let page = carol.get(&view_path(f.w.g, f.w.bob)).await;
    assert_eq!(page.status, 200);
    let body = &page.body;
    assert!(html_contains(body, "Bob"), "the owner's name");
    assert!(
        body.contains(&format!("/groups/{}", f.w.g)),
        "the group link"
    );
    assert!(html_contains(body, "Family"));
    assert!(pos(body, ITEM_A) < pos(body, ITEM_B));
    assert!(pos(body, ITEM_B) < pos(body, ITEM_C));
    assert!(html_contains(body, "Desc A"));
    assert!(body.contains("https://a.example/a"));
    assert!(html_contains(body, &en("wishlist.view.orderHint")));
    assert!(html_contains(
        body,
        &en_with("wishlist.view.lastUpdated", &[("date", "10 Dec 2026")])
    ));
    assert!(html_contains(body, &en("wishlist.view.viewItem")));
    for edit_control in [
        en("wishlist.edit.moveUp"),
        en("wishlist.edit.moveDown"),
        en("wishlist.edit.removeItem"),
        en("wishlist.edit.addItem"),
    ] {
        assert!(!html_contains(body, &edit_control), "{edit_control}");
    }
    assert!(!body.contains("?edit="));
    assert_no_raw_keys(body, "view page");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac1_get_wishlist_returns_the_items_in_order_with_the_date(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let view: WishlistView = get_wishlist(&mut carol, f.w.g, f.w.bob).await.wl_value();
    assert_eq!(view.group.id, f.w.g);
    assert_eq!(view.group.name, "Family");
    assert_eq!(view.owner.id, f.w.bob);
    assert!(!view.is_own);
    let got: Vec<&str> = view.items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(got, [ITEM_A, ITEM_B, ITEM_C]);
    assert_eq!(view.items[0].description.as_deref(), Some("Desc A"));
    assert_eq!(view.items[0].link.as_deref(), Some("https://a.example/a"));
    assert_eq!(view.items[0].link_host.as_deref(), Some("a.example"));
    assert_eq!(view.items[1].link, None);
    assert_eq!(view.items[1].link_host, None);
    assert_eq!(view.updated_at, updated_at(&pool, f.w.g, f.w.bob).await);
    assert!(view.updated_at.is_some());
}

// -------------------------------------------------------------------------- AC2

#[sqlx::test(migrations = "../migrations")]
async fn ac2_an_empty_list_reads_as_empty_and_creates_no_row(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = total_wishlists(&pool).await;
    let mut bob = f.w.bob().await;

    let page = bob.get(&view_path(f.w.g, f.w.carol)).await;
    assert_eq!(page.status, 200);
    for key in [
        "wishlist.view.empty",
        "wishlist.view.noPreference",
        "wishlist.view.suggestGiftCard",
    ] {
        assert!(html_contains(&page.body, &en(key)), "{key}");
    }
    assert!(!html_contains(&page.body, &en("wishlist.view.orderHint")));
    assert!(!page.body.contains("Last updated"));
    assert!(
        !html_contains(
            &page.body,
            &en_with("wishlist.view.lastUpdated", &[("date", "")])
        ),
        "no 'last updated' for a missing list"
    );
    assert!(!html_contains(&page.body, &en("wishlist.view.addNow")));

    let view: WishlistView = get_wishlist(&mut bob, f.w.g, f.w.carol).await.wl_value();
    assert!(view.items.is_empty());
    assert_eq!(view.updated_at, None);
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);
    assert_eq!(total_wishlists(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac2_the_owner_of_an_empty_list_is_invited_to_add_items(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let page = carol.get(&view_path(f.w.g, f.w.carol)).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("wishlist.view.empty")));
    assert!(html_contains(&page.body, &en("wishlist.edit.addSome")));
    assert!(html_contains(&page.body, &en("wishlist.view.addNow")));
    assert!(!html_contains(
        &page.body,
        &en("wishlist.view.noPreference")
    ));
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);
}

// ----------------------------------------------------------------- AC3 / AC4

#[sqlx::test(migrations = "../migrations")]
async fn ac3_a_non_member_and_a_missing_group_give_the_same_404_page_and_notfound(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut dave = f.w.dave().await;
    dave.session_info().await; // takes the "logged in" flash so both pages render alike

    // Positive control: a member does get the page, so the 404 below is not "no route".
    let mut carol = f.w.carol().await;
    assert_eq!(carol.get(&view_path(f.w.g, f.w.bob)).await.status, 200);

    for suffix in [format!("/{}", f.w.bob), String::new()] {
        let foreign = dave
            .get(&format!("/groups/{}/wishlist{suffix}", f.w.g))
            .await;
        let missing = dave.get(&format!("/groups/999999/wishlist{suffix}")).await;
        assert_eq!(foreign.status, 404, "{suffix}");
        assert_eq!(missing.status, 404, "{suffix}");
        assert_eq!(
            without_nonce(&foreign.body),
            without_nonce(&missing.body),
            "{suffix}"
        );
        for secret in ["Family", ITEM_A, "Bob", "Alice"] {
            assert!(!foreign.body.contains(secret), "{suffix} leaks {secret}");
        }
    }

    let a = get_wishlist(&mut dave, f.w.g, f.w.bob).await;
    let b = get_wishlist(&mut dave, 999_999, f.w.bob).await;
    assert_eq!((a.status.as_u16(), b.status.as_u16()), (404, 404));
    assert_eq!(a.wl_err(), WishlistError::NotFound);
    assert_eq!(a.body, b.body);
    assert_eq!(
        get_my_wishlist(&mut dave, f.w.g).await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(
        get_wishlist_panel(&mut dave, f.w.g).await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(get_wishlist_panel(&mut dave, 999_999).await.body, a.body);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac3_ids_that_are_not_positive_integers_give_the_404_page(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    assert_eq!(bob.get(&edit_path(f.w.g)).await.status, 200, "control");
    let g = f.w.g;
    for path in [
        "/groups/abc/wishlist".to_string(),
        "/groups/0/wishlist".to_string(),
        "/groups/-1/wishlist".to_string(),
        format!("/groups/{g}/wishlist/abc"),
        format!("/groups/{g}/wishlist/0"),
        format!("/groups/{g}/wishlist/-1"),
        format!("/groups/{g}/wishlist/1.5"),
        format!("/groups/{g}/wishlist/99999999999999999999"),
    ] {
        assert_eq!(bob.get(&path).await.status, 404, "{path}");
    }
    for id in ["0", "-1"] {
        let r = bob
            .call(
                "get_wishlist",
                &[("group_id", id), ("user_id", &f.w.bob.to_string())],
            )
            .await;
        assert_eq!(r.wl_err(), WishlistError::NotFound, "{id}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac4_a_target_outside_the_group_is_not_found_and_h_items_never_leak_into_g(
    pool: MySqlPool,
) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    // Dave exists and has a list in H only; 999999 does not exist.
    let dave_list = ts::create_wishlist(&pool, f.h, f.w.dave, &["Dave secret in H"]).await;
    assert!(dave_list > 0);
    for target in [f.w.dave, 999_999] {
        let r = get_wishlist(&mut bob, f.w.g, target).await;
        assert_eq!(r.wl_err(), WishlistError::NotFound, "{target}");
        assert!(!r.body.contains("Dave secret"));
        let page = bob.get(&view_path(f.w.g, target)).await;
        assert_eq!(page.status, 404, "{target}");
    }
    // Bob's H list is only reachable through H.
    let in_g: WishlistView = get_wishlist(&mut bob, f.w.g, f.w.bob).await.wl_value();
    assert!(in_g.items.iter().all(|i| i.title != ITEM_X));
    let in_h: WishlistView = get_wishlist(&mut bob, f.h, f.w.bob).await.wl_value();
    let titles_h: Vec<&str> = in_h.items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles_h, [ITEM_X]);
    // Someone who left G is not a valid target any more.
    ts::add_member(&pool, f.w.g, f.w.dave).await;
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(f.w.g)
        .bind(f.w.dave)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get_wishlist(&mut bob, f.w.g, f.w.dave).await.wl_err(),
        WishlistError::NotFound
    );
}

// -------------------------------------------------------------------------- AC5

#[sqlx::test(migrations = "../migrations")]
async fn ac5_after_the_draw_every_member_still_sees_every_list(pool: MySqlPool) {
    let f = fixture(&pool).await;
    ts::add_assignment(&pool, f.w.g, f.w.bob, f.w.carol).await;
    ts::add_assignment(&pool, f.w.g, f.w.carol, f.w.alice).await;
    ts::add_assignment(&pool, f.w.g, f.w.alice, f.w.bob).await;
    ts::set_drawn(&pool, f.w.g).await;

    for (who, mut c) in [
        ("alice", f.w.alice().await),
        ("bob", f.w.bob().await),
        ("carol", f.w.carol().await),
    ] {
        for target in [f.w.alice, f.w.bob, f.w.carol] {
            let r = get_wishlist(&mut c, f.w.g, target).await;
            r.wl_ok();
            assert!(!r.body.to_lowercase().contains("assign"), "{who}->{target}");
            assert!(!r.body.contains("giver") && !r.body.contains("receiver"));
            let page = c.get(&view_path(f.w.g, target)).await;
            assert_eq!(page.status, 200, "{who}->{target}");
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_results_do_not_depend_on_who_gives_to_whom(pool: MySqlPool) {
    // Same world, two different draws: every wishlist fn answers byte for byte the same.
    let f = fixture(&pool).await;
    ts::set_drawn(&pool, f.w.g).await;
    let mut carol = f.w.carol().await;
    let mut bob = f.w.bob().await;
    // Consume the login flash so the pages differ only by the draw.
    carol.session_info().await;
    bob.session_info().await;
    let capture = |who: &'static str| {
        let (g, a, b, c) = (f.w.g, f.w.alice, f.w.bob, f.w.carol);
        let mut client = if who == "carol" {
            carol.fork()
        } else {
            bob.fork()
        };
        async move {
            let mut out = vec![];
            for target in [a, b, c] {
                out.push(get_wishlist(&mut client, g, target).await.body);
                out.push(without_nonce(&client.get(&view_path(g, target)).await.body));
            }
            out.push(get_wishlist_panel(&mut client, g).await.body);
            out
        }
    };
    ts::add_assignment(&pool, f.w.g, f.w.bob, f.w.carol).await;
    ts::add_assignment(&pool, f.w.g, f.w.carol, f.w.alice).await;
    ts::add_assignment(&pool, f.w.g, f.w.alice, f.w.bob).await;
    let first = (capture("carol").await, capture("bob").await);
    sqlx::query("DELETE FROM gift_assignments WHERE group_id = ?")
        .bind(f.w.g)
        .execute(&pool)
        .await
        .unwrap();
    ts::add_assignment(&pool, f.w.g, f.w.bob, f.w.alice).await;
    ts::add_assignment(&pool, f.w.g, f.w.alice, f.w.carol).await;
    ts::add_assignment(&pool, f.w.g, f.w.carol, f.w.bob).await;
    let second = (capture("carol").await, capture("bob").await);
    assert_eq!(first, second);
}

// ------------------------------------------------------------------- AC5 (static)

fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn ac5_no_wishlist_source_file_mentions_gift_assignments() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/src");
    let mut files = vec![];
    rust_files(&root, &mut files);
    let wishlist_files: Vec<_> = files
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy().to_lowercase();
            s.contains("wishlist") && !s.ends_with("test_support.rs")
        })
        .collect();
    assert!(
        wishlist_files
            .iter()
            .any(|p| p.ends_with("wishlists_api.rs")),
        "the scan finds the wishlist modules"
    );
    for file in wishlist_files {
        let source = std::fs::read_to_string(&file).unwrap().to_lowercase();
        for forbidden in ["gift_assignments", "giver_id", "receiver_id"] {
            assert!(
                !source.contains(forbidden),
                "{} mentions {forbidden}",
                file.display()
            );
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_every_wishlist_fn_and_page_works_without_the_assignments_table(pool: MySqlPool) {
    let f = fixture(&pool).await;
    ts::add_assignment(&pool, f.w.g, f.w.bob, f.w.carol).await;
    ts::set_drawn(&pool, f.w.g).await;
    // Stand-in for "revoke SELECT on gift_assignments": the table is gone for everyone.
    sqlx::query("RENAME TABLE gift_assignments TO gift_assignments_hidden")
        .execute(&pool)
        .await
        .expect("rename");

    let mut bob = f.w.bob().await;
    let g = f.w.g;
    get_wishlist(&mut bob, g, f.w.alice).await.wl_ok();
    get_my_wishlist(&mut bob, g).await.wl_ok();
    get_wishlist_panel(&mut bob, g).await.wl_ok();
    let id: i32 = add_item(&mut bob, g, "New", "", "").await.wl_value();
    update_item(&mut bob, id, "New2", "", "").await.wl_ok();
    up(&mut bob, id).await.wl_ok();
    down(&mut bob, f.a).await.wl_ok();
    delete_item(&mut bob, id).await.wl_ok();
    clear_wishlist(&mut bob, g, true).await.wl_ok();
    for path in [
        edit_path(g),
        view_path(g, f.w.alice),
        format!("/groups/{g}"),
    ] {
        assert_eq!(bob.get(&path).await.status, 200, "{path}");
    }
}

// -------------------------------------------------------------------------- AC6

#[sqlx::test(migrations = "../migrations")]
async fn ac6_the_owner_sees_an_edit_button_and_others_do_not(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let url = view_path(f.w.g, f.w.bob);
    let edit_href = format!("href=\"{}\"", edit_path(f.w.g));

    let mut bob = f.w.bob().await;
    let page = bob.get(&url).await;
    assert_eq!(page.status, 200);
    assert!(
        page.body.contains(&edit_href),
        "Edit links to the edit page"
    );
    assert!(html_contains(&page.body, &en("wishlist.edit.title")));
    let own: WishlistView = get_wishlist(&mut bob, f.w.g, f.w.bob).await.wl_value();
    assert!(own.is_own);
    let mine: WishlistView = get_my_wishlist(&mut bob, f.w.g).await.wl_value();
    assert!(mine.is_own);
    assert_eq!(mine.owner.id, f.w.bob);

    for mut other in [f.w.carol().await, f.w.alice().await] {
        let page = other.get(&url).await;
        assert_eq!(page.status, 200);
        assert!(!page.body.contains(&edit_href));
        assert!(!html_contains(&page.body, &en("wishlist.edit.title")));
    }
}

// -------------------------------------------------------------------------- AC7

#[sqlx::test(migrations = "../migrations")]
async fn ac7_a_logged_out_visitor_is_sent_to_login_and_back(pool: MySqlPool) {
    let f = fixture(&pool).await;
    for path in [edit_path(f.w.g), view_path(f.w.g, f.w.bob)] {
        let mut visitor = f.w.anon();
        let res = visitor.get(&path).await;
        assert!(res.status.is_redirection(), "{path}: {}", res.status);
        assert_eq!(
            res.location().unwrap(),
            format!("/auth/login?next={}", urlencode(&path))
        );
        assert!(!res.body.contains(ITEM_A), "no data in the redirect");
        let login = visitor
            .login_next(&f.w.emails[2], CAROL_PASSWORD, &path)
            .await;
        login.ok();
        assert_eq!(login.location().as_deref(), Some(path.as_str()));
        assert_eq!(visitor.get(&path).await.status, 200);
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac7_every_server_fn_without_a_session_is_unauthorized_and_changes_nothing(
    pool: MySqlPool,
) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let mut anon = f.w.anon();
    let (g, b) = (f.w.g, f.w.bob);
    let replies = vec![
        ("get_wishlist", get_wishlist(&mut anon, g, b).await),
        ("get_my_wishlist", get_my_wishlist(&mut anon, g).await),
        ("get_wishlist_panel", get_wishlist_panel(&mut anon, g).await),
        ("add_item", add_item(&mut anon, g, "x", "", "").await),
        (
            "update_item",
            update_item(&mut anon, f.a, "x", "", "").await,
        ),
        ("delete_item", delete_item(&mut anon, f.a).await),
        ("move_item", up(&mut anon, f.b).await),
        ("clear_wishlist", clear_wishlist(&mut anon, g, true).await),
    ];
    for (name, reply) in replies {
        assert_eq!(reply.status, 401, "{name}");
        assert_eq!(reply.wl_err(), WishlistError::Unauthorized, "{name}");
    }
    assert_eq!(dump(&pool).await, before);
}

// -------------------------------------------------------------------------- AC8

#[sqlx::test(migrations = "../migrations")]
async fn ac8_the_panel_lists_counts_and_links_and_no_emails(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let panel: WishlistPanel = get_wishlist_panel(&mut bob, f.w.g).await.wl_value();
    assert_eq!(panel.my_item_count, 3);
    let rows: Vec<(i32, bool, u32)> = panel
        .members
        .iter()
        .map(|m| (m.user_id, m.is_me, m.item_count))
        .collect();
    // Ordered like the group page: joined_at, id.
    assert_eq!(
        rows,
        [
            (f.w.alice, false, 1),
            (f.w.bob, true, 3),
            (f.w.carol, false, 0)
        ]
    );
    assert!(panel.members.iter().all(|m| !m.name.is_empty()));

    let page = bob.get(&format!("/groups/{}", f.w.g)).await;
    assert_eq!(page.status, 200);
    let body = &page.body;
    assert!(html_contains(body, &en("group.show.yourWishlist")));
    assert!(html_contains(body, &en("group.show.editWishlist")));
    assert!(html_contains(body, &en("group.show.viewYourWishlist")));
    assert!(html_contains(body, &en("wishlist.panel.membersTitle")));
    assert!(html_contains(body, &en("wishlist.panel.view")));
    assert!(body.contains(&format!("href=\"{}\"", edit_path(f.w.g))));
    for user in [f.w.bob, f.w.alice, f.w.carol] {
        assert!(
            body.contains(&format!("href=\"{}\"", view_path(f.w.g, user))),
            "{user}"
        );
    }
    assert!(body.contains("3 items") && body.contains("1 item"));
    for (i, email) in f.w.emails.iter().enumerate() {
        if i != 1 {
            assert!(!body.contains(email.as_str()), "group page leaks {email}");
        }
    }
    assert!(!get_wishlist_panel(&mut bob, f.w.g).await.body.contains('@'));
    assert_no_raw_keys(body, "group page");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_the_panel_only_counts_the_list_of_this_group(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let in_h: WishlistPanel = get_wishlist_panel(&mut bob, f.h).await.wl_value();
    assert_eq!(in_h.my_item_count, 1);
    assert_eq!(in_h.members.len(), 2);
    let in_g: WishlistPanel = get_wishlist_panel(&mut bob, f.w.g).await.wl_value();
    assert_eq!(in_g.my_item_count, 3);
    // Other members' lists are never visible from the other group.
    assert!(
        in_h.members
            .iter()
            .all(|m| m.user_id != f.w.alice && m.user_id != f.w.carol)
    );
}

// ------------------------------------------------------------------ AC32 / AC33

#[sqlx::test(migrations = "../migrations")]
async fn ac32_the_edit_and_view_pages_render_without_missing_keys_in_both_locales(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut english = f.w.bob().await;
    let mut german = f.w.german(&f.w.emails[1], BOB_PASSWORD).await;
    for (c, lang) in [(&mut english, "en"), (&mut german, "de")] {
        for path in [
            edit_path(f.w.g),
            format!("{}?edit={}", edit_path(f.w.g), f.b),
            view_path(f.w.g, f.w.bob),
            view_path(f.w.g, f.w.carol),
            format!("/groups/{}", f.w.g),
        ] {
            let page = c.get(&path).await;
            assert_eq!(page.status, 200, "{lang} {path}");
            assert_eq!(lang_of(&page.body), lang);
            assert_no_raw_keys(&page.body, &format!("{lang} {path}"));
        }
    }
    // Every new text is really translated (not the English text in the German catalog).
    for key in [
        "wishlist.edit.moveUp",
        "wishlist.edit.moveDown",
        "wishlist.view.orderHint",
    ] {
        assert_ne!(en(key), de(key), "{key}");
    }
    let page = german.get(&edit_path(f.w.g)).await;
    assert!(html_contains(&page.body, &de("wishlist.edit.moveUp")));
    assert!(html_contains(
        &page.body,
        &de_with("wishlist.edit.itemCount", &[("count", "3"), ("max", "50")])
    ));
    let page = english.get(&edit_path(f.w.g)).await;
    assert!(html_contains(&page.body, "3 of 50 items"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac33_last_updated_uses_the_app_time_zone_and_the_page_locale(pool: MySqlPool) {
    let f = fixture(&pool).await;
    set_updated_at(
        &pool,
        f.bob_list,
        Utc.with_ymd_and_hms(2026, 12, 10, 22, 30, 0).unwrap(),
    )
    .await;
    let url = view_path(f.w.g, f.w.bob);
    let mut english = f.w.carol().await;
    let mut german = f.w.german(&f.w.emails[2], CAROL_PASSWORD).await;
    let en_page = english.get(&url).await;
    let de_page = german.get(&url).await;
    assert!(html_contains(
        &en_page.body,
        &en_with("wishlist.view.lastUpdated", &[("date", "10 Dec 2026")])
    ));
    assert!(html_contains(
        &de_page.body,
        &de_with("wishlist.view.lastUpdated", &[("date", "10.12.2026")])
    ));
    assert!(en_page.body.contains("10 Dec 2026"));
    assert!(de_page.body.contains("10.12.2026"));

    // 23:30 UTC is already the 11th in Europe/Berlin (the default APP_TIMEZONE).
    set_updated_at(
        &pool,
        f.bob_list,
        Utc.with_ymd_and_hms(2026, 12, 10, 23, 30, 0).unwrap(),
    )
    .await;
    let page = english.get(&url).await;
    assert!(page.body.contains("11 Dec 2026") && !page.body.contains("10 Dec 2026"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac33_the_panel_pluralises_the_item_count_in_both_locales(pool: MySqlPool) {
    let f = fixture(&pool).await;
    ts::create_wishlist(&pool, f.w.g, f.w.carol, &["One", "Two"]).await;
    // Alice has 1 item, Carol 2, Bob (the viewer, in "my" count) 3.
    let mut alice = f.w.alice().await;
    let page = alice.get(&format!("/groups/{}", f.w.g)).await;
    assert!(page.body.contains("1 item") && !page.body.contains("1 items"));
    assert!(page.body.contains("2 items"));
    let mut german = f.w.german(&f.w.emails[0], ALICE_PASSWORD).await;
    let page = german.get(&format!("/groups/{}", f.w.g)).await;
    assert!(page.body.contains("1 Artikel") && page.body.contains("2 Artikel"));
    assert!(!page.body.contains("1 Artikels"));
}
