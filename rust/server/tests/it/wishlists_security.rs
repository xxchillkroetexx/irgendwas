//! wishlists slice: authorization, item-id handling, link safety, rendering safety and
//! protocol plumbing (AC17, AC19, AC27-AC29, W2, W3, W9, W10).

use crate::common::auth::*;
use crate::common::wishlists::*;
use crate::common::{TEST_ORIGIN, post_from, send};
use app::wishlists_api::{WishlistError, WishlistField as Fld, WishlistFieldError as Err_};
use axum::http::StatusCode;
use sqlx::MySqlPool;

// ------------------------------------------------------------------------ AC17

/// All item fns on `item`, as `c`. Names are only for the failure message.
async fn item_calls(c: &mut Client, item: i32) -> Vec<(&'static str, Reply)> {
    vec![
        (
            "update_item",
            update_item(c, item, "Hacked", "x", "https://evil.example/").await,
        ),
        ("delete_item", delete_item(c, item).await),
        ("move_item up", up(c, item).await),
        ("move_item down", down(c, item).await),
    ]
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_nobody_but_the_owner_can_change_an_item_and_nobody_can_tell_why(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let order = titles(&pool, f.w.g, f.w.bob).await;
    // Carol: a member of G. Alice: the admin of G. Dave: not in G (but in H, where Bob's
    // other list lives). All act on Bob's item B.
    let mut absent_ref = f.w.carol().await;
    let absent = item_calls(&mut absent_ref, 999_999).await;
    for (who, mut c) in [
        ("carol", f.w.carol().await),
        ("alice (admin)", f.w.alice().await),
        ("dave", f.w.dave().await),
    ] {
        for (target, id) in [("B", f.b), ("X in H", f.x), ("nonexistent", 999_999)] {
            for ((name, reply), (_, reference)) in
                item_calls(&mut c, id).await.iter().zip(absent.iter())
            {
                assert_eq!(reply.status, 404, "{who} {name} {target}");
                assert_eq!(
                    reply.wl_err(),
                    WishlistError::NotFound,
                    "{who} {name} {target}"
                );
                assert_eq!(
                    reply.body, reference.body,
                    "{who} {name} {target}: same body"
                );
            }
        }
    }
    assert_eq!(dump(&pool).await, before);
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await, order);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_ids_that_are_not_positive_integers_are_not_found(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let mut bob = f.w.bob().await;
    for id in ["0", "-1", "abc", "1.5", "99999999999999999999"] {
        for name in ["delete_item", "move_item", "update_item"] {
            let r = bob
                .call(
                    name,
                    &[("item_id", id), ("title", "x"), ("direction", "Up")],
                )
                .await;
            assert!(!r.status.is_success(), "{name} {id}");
            assert_ne!(
                r.status,
                StatusCode::INTERNAL_SERVER_ERROR,
                "{name} {id}: {}",
                r.body
            );
        }
    }
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn w2_the_owner_loses_access_to_an_item_when_no_longer_a_member(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    up(&mut bob, f.b).await.wl_ok(); // control: Bob may change his item now
    // Bob's membership vanishes but his rows stay (as a half-finished leave would leave them).
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(f.w.g)
        .bind(f.w.bob)
        .execute(&pool)
        .await
        .unwrap();
    let before = dump(&pool).await;
    for (name, reply) in item_calls(&mut bob, f.a).await {
        assert_eq!(reply.wl_err(), WishlistError::NotFound, "{name}");
    }
    assert_eq!(
        add_item(&mut bob, f.w.g, "x", "", "").await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(
        clear_wishlist(&mut bob, f.w.g, true).await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(dump(&pool).await, before);
    // His list in H is still his to edit.
    update_item(&mut bob, f.x, "X2", "", "").await.wl_ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ids_in_the_request_never_choose_whose_list_is_written(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let (g, bob, alice) = (
        f.w.g.to_string(),
        f.w.bob.to_string(),
        f.w.alice.to_string(),
    );
    // add_item: extra user_id / wishlist_id fields are ignored; the item lands on Carol's list.
    let r = carol
        .call(
            "add_item",
            &[
                ("group_id", g.as_str()),
                ("title", "Mine"),
                ("description", ""),
                ("link", ""),
                ("user_id", bob.as_str()),
                ("owner_id", alice.as_str()),
                ("wishlist_id", &f.bob_list.to_string()),
            ],
        )
        .await;
    assert!(r.status.is_success(), "{}", r.body);
    assert_eq!(titles(&pool, f.w.g, f.w.carol).await, ["Mine"]);
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_B, ITEM_C]
    );
    assert_eq!(titles(&pool, f.w.g, f.w.alice).await, [ALICE_ITEM]);
    // clear_wishlist: only the caller's list, whatever else is posted.
    carol
        .call(
            "clear_wishlist",
            &[
                ("group_id", g.as_str()),
                ("confirm", "true"),
                ("user_id", bob.as_str()),
            ],
        )
        .await;
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await.len(), 3);
    // A group the caller is not in cannot be written through a G item id or vice versa.
    assert_eq!(
        add_item(&mut carol, f.h, "Into H", "", "").await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X]);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_non_member_cannot_tell_a_foreign_group_from_a_missing_one(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let mut dave = f.w.dave().await;
    for (name, group) in [("foreign", f.w.g), ("missing", 999_999)] {
        let replies = [
            get_my_wishlist(&mut dave, group).await,
            get_wishlist(&mut dave, group, f.w.bob).await,
            get_wishlist_panel(&mut dave, group).await,
            add_item(&mut dave, group, "x", "", "").await,
            clear_wishlist(&mut dave, group, true).await,
        ];
        for r in replies {
            assert_eq!(
                (r.status.as_u16(), r.wl_err()),
                (404, WishlistError::NotFound),
                "{name}"
            );
            assert_eq!(r.body, replies_body(), "{name}");
        }
    }
    assert_eq!(dump(&pool).await, before);
}

/// The one body every NotFound has.
fn replies_body() -> String {
    serde_json::to_string(&WishlistError::NotFound).unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn the_acting_user_always_comes_from_the_session(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let mut dave = f.w.dave().await;
    let reply = dave
        .call(
            "get_my_wishlist",
            &[
                ("group_id", &f.w.g.to_string()),
                ("user_id", &f.w.bob.to_string()),
            ],
        )
        .await;
    assert_eq!(reply.wl_err(), WishlistError::NotFound);
    assert_eq!(dump(&pool).await, before);
}

// ------------------------------------------------------------- no email in payloads

#[sqlx::test(migrations = "../migrations")]
async fn no_other_users_email_appears_in_wishlist_pages_or_payloads(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let mut pages = vec![];
    for path in [
        edit_path(f.w.g),
        view_path(f.w.g, f.w.bob),
        view_path(f.w.g, f.w.alice),
        format!("{}?edit={}", edit_path(f.w.g), f.a),
        format!("/groups/{}", f.w.g),
    ] {
        pages.push((path.clone(), carol.get(&path).await));
    }
    pages.push((
        "get_wishlist".into(),
        get_wishlist(&mut carol, f.w.g, f.w.bob).await,
    ));
    pages.push((
        "get_my_wishlist".into(),
        get_my_wishlist(&mut carol, f.w.g).await,
    ));
    pages.push(("panel".into(), get_wishlist_panel(&mut carol, f.w.g).await));
    for (what, page) in pages {
        assert_eq!(page.status, 200, "{what}");
        for (i, email) in f.w.emails.iter().enumerate() {
            if i != 2 {
                assert!(!page.body.contains(email.as_str()), "{what} leaks {email}");
            }
        }
    }
}

// ------------------------------------------------------------------ links (W9)

#[sqlx::test(migrations = "../migrations")]
async fn w9_script_like_and_odd_schemes_are_always_refused(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    for link in [
        "JavaScript:alert(1)",
        "JAVASCRIPT://%0aalert(1)",
        " javascript:alert(1)",
        "\tjavascript:alert(1)",
        "java\nscript:alert(1)",
        "java\tscript:alert(1)",
        "javascript:alert(1)//http://x",
        "javascript://example.com/%0aalert(1)",
        "vbscript:msgbox(1)",
        "VBScript://x",
        "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
        "DATA://text/html,x",
        "file:///etc/passwd",
        "blob:https://example.com/x",
        "mailto:a@b.c",
        "tel:+491234",
        "about:blank",
        "ftp://example.com/x",
        "ws://example.com/",
        "gopher://example.com/",
        "https://user@example.com/",
        ":pw@example.com",
        "http://",
        "https:///",
        "https://exa mple.com",
        "https://ex\u{0}ample.com",
        "http://[",
        "https://example.com:99999/",
        "http://:80/",
        "https://\u{1}",
    ] {
        let r = add_item(&mut carol, f.w.g, "T", "", link).await;
        assert_eq!(r.status, 422, "{link:?}: {}", r.body);
        assert_eq!(r.problems(), [(Fld::Link, Err_::LinkInvalid)], "{link:?}");
    }
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn w9_whatever_is_accepted_is_an_absolute_http_or_https_url_without_markup(pool: MySqlPool) {
    // Relative, protocol-relative and tricky inputs may be refused or repaired to an
    // https URL (the spec prefixes `https://`); they may never be stored as given.
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let mut accepted = 0;
    for link in [
        "//evil.example/x",
        "/relative/path",
        "relative",
        "?x=1",
        "#frag",
        "\\\\evil.example\\x",
        "http:\\\\evil.example",
        "https://example.com/\"><script>alert(1)</script>",
        "https://example.com/?q=\"onmouseover=\"alert(1)",
        "https://example.com/a b",
        "https://example.com/\u{202e}x",
        "https://exa\nmple.com/",
        "https://example.com/\tx",
        "\u{a0}https://example.com/nbsp",
        "https://example.com/'x'",
        "https://xn--e1afmkfd.example/\u{43f}",
        "https://\u{ff45}xample.com/",
        "HTTP://EXAMPLE.COM/UP",
    ] {
        let r = add_item(&mut carol, f.w.g, "T", "", link).await;
        if !r.status.is_success() {
            assert_eq!(r.problems(), [(Fld::Link, Err_::LinkInvalid)], "{link:?}");
            continue;
        }
        accepted += 1;
        let id: i32 = r.wl_value();
        let stored = item_row(&pool, id)
            .await
            .unwrap()
            .link
            .expect("link stored");
        assert!(
            stored.starts_with("https://") || stored.starts_with("http://"),
            "{link:?} stored as {stored:?}"
        );
        assert!(
            !stored.chars().any(|c| c.is_control() || c.is_whitespace()),
            "{link:?} stored with whitespace/control: {stored:?}"
        );
        for bad in ['<', '>', '"', '\\'] {
            assert!(
                !stored.contains(bad),
                "{link:?} stored with {bad:?}: {stored:?}"
            );
        }
        assert!(
            !stored.contains("@") || stored.split('/').nth(2).is_some_and(|h| !h.contains('@'))
        );
    }
    assert!(accepted >= 1, "ordinary upper-case input is accepted");
}

#[sqlx::test(migrations = "../migrations")]
async fn w9_update_item_applies_the_same_link_rules(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    for link in [
        "javascript:alert(1)",
        "data:text/html,x",
        "https://u:p@example.com/",
        "ftp://x.example/",
    ] {
        let r = update_item(&mut bob, f.a, "A", "", link).await;
        assert_eq!(r.problems(), [(Fld::Link, Err_::LinkInvalid)], "{link:?}");
    }
    assert_eq!(
        item_row(&pool, f.a).await.unwrap().link.as_deref(),
        Some("https://a.example/a")
    );
    update_item(&mut bob, f.a, "A", "", "HTTP://Shop.Example/Z")
        .await
        .wl_ok();
    assert_eq!(
        item_row(&pool, f.a).await.unwrap().link.as_deref(),
        Some("http://shop.example/Z")
    );
}

// ------------------------------------------------------------- rendering (W10)

/// The opening tags `<a ...>` of a page.
fn anchors(html: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut rest = html;
    while let Some(start) = rest.find("<a ") {
        let after = &rest[start..];
        let end = after.find('>').expect("closed tag");
        out.push(&after[..=end]);
        rest = &after[end..];
    }
    out
}

#[sqlx::test(migrations = "../migrations")]
async fn ac27_links_open_in_a_new_tab_without_referrer_and_show_their_host(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    add_item(&mut bob, f.w.g, "Shop thing", "", "https://shop.example/x")
        .await
        .wl_ok();
    let mut carol = f.w.carol().await;
    for (who, path) in [
        ("view", view_path(f.w.g, f.w.bob)),
        ("edit", edit_path(f.w.g)),
    ] {
        let c = if who == "view" { &mut carol } else { &mut bob };
        let page = c.get(&path).await;
        assert_eq!(page.status, 200, "{who}");
        let tags: Vec<&str> = anchors(&page.body)
            .into_iter()
            .filter(|t| t.contains("href=\"https://shop.example/x\""))
            .collect();
        assert!(!tags.is_empty(), "{who}: the item link is rendered");
        for tag in &tags {
            assert!(tag.contains("target=\"_blank\""), "{who}: {tag}");
            assert!(
                tag.contains("rel=\"noopener noreferrer nofollow\""),
                "{who}: {tag}"
            );
        }
        let after_tag = &page.body[pos(&page.body, "href=\"https://shop.example/x\"")..];
        assert!(
            after_tag
                .chars()
                .take(600)
                .collect::<String>()
                .contains("shop.example"),
            "{who}: the host is shown next to the link"
        );
        // Every link that leaves the site or opens a tab carries the full rel.
        for tag in anchors(&page.body) {
            if tag.contains("target=\"_blank\"") || tag.contains("href=\"http") {
                assert!(
                    tag.contains("rel=\"noopener noreferrer nofollow\""),
                    "{who}: {tag}"
                );
            }
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac28_user_text_is_rendered_as_text_everywhere(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let title = "<script>alert(1)</script>";
    let desc = "<img src=x onerror=alert(1)>\nline 2";
    let id: i32 = add_item(&mut carol, f.w.g, title, desc, "")
        .await
        .wl_value();
    let attr_id: i32 = add_item(
        &mut carol,
        f.w.g,
        "\"><script>alert(2)</script>",
        "</textarea><script>alert(3)</script>",
        "",
    )
    .await
    .wl_value();

    let mut bob = f.w.bob().await;
    for (who, path) in [
        ("owner edit", edit_path(f.w.g)),
        ("owner edit form", format!("{}?edit={id}", edit_path(f.w.g))),
        (
            "owner attr form",
            format!("{}?edit={attr_id}", edit_path(f.w.g)),
        ),
        ("viewer", view_path(f.w.g, f.w.carol)),
    ] {
        let c = if who == "viewer" {
            &mut bob
        } else {
            &mut carol
        };
        let page = c.get(&path).await;
        assert_eq!(page.status, 200, "{who}");
        let body = &page.body;
        for raw in [
            "<script>alert",
            "<img src=x",
            "\"><script>alert(2)",
            "</textarea><script>alert(3)",
            "alert(1)</script>",
            "alert(2)</script>",
            "alert(3)</script>",
        ] {
            // Inside a <textarea> (RCDATA) a literal `<img ...>` is inert text; only a
            // `</textarea>` breakout would matter, and that is checked by the other markers.
            if who == "owner edit form" && raw == "<img src=x" {
                continue;
            }
            assert!(!body.contains(raw), "{who}: unescaped {raw:?}");
        }
        assert!(
            who == "owner edit form" || body.contains("&lt;"),
            "{who}: markup is escaped"
        );
        assert!(
            !body.to_lowercase().contains("</textarea><script"),
            "{who}: textarea breakout"
        );
    }
    let page = bob.get(&view_path(f.w.g, f.w.carol)).await;
    assert!(page.body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(page.body.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(
        page.body.contains("&gt;\nline 2"),
        "the line break is kept in the text"
    );
    assert!(
        page.body.contains("pre-line"),
        "and shown through white-space: pre-line"
    );
    // The API hands the text back untouched; escaping is the renderer's job.
    let view: app::wishlists_api::WishlistView =
        get_wishlist(&mut bob, f.w.g, f.w.carol).await.wl_value();
    assert_eq!(view.items[0].title, title);
    assert_eq!(view.items[0].description.as_deref(), Some(desc));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac28_markup_in_names_is_not_a_way_around_it_on_the_panel(pool: MySqlPool) {
    let f = fixture(&pool).await;
    sqlx::query("UPDATE users SET name = '<b>Mallory</b><script>alert(9)</script>' WHERE id = ?")
        .bind(f.w.carol)
        .execute(&pool)
        .await
        .unwrap();
    let mut bob = f.w.bob().await;
    for path in [format!("/groups/{}", f.w.g), view_path(f.w.g, f.w.carol)] {
        let page = bob.get(&path).await;
        assert_eq!(page.status, 200);
        assert!(!page.body.contains("<script>alert(9)"), "{path}");
        assert!(!page.body.contains("<b>Mallory"), "{path}");
    }
}

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
fn w10_no_wishlist_source_uses_inner_html() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/src");
    let mut files = vec![];
    rust_files(&root, &mut files);
    let ours: Vec<_> = files
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy().to_lowercase();
            s.contains("wishlist") && !s.ends_with("test_support.rs")
        })
        .collect();
    assert!(ours.iter().any(|p| p.ends_with("wishlists_api.rs")));
    for file in ours {
        let source = std::fs::read_to_string(&file).unwrap();
        assert!(
            !source.contains("inner_html"),
            "{} uses inner_html",
            file.display()
        );
    }
}

// ------------------------------------------------------------------------ AC29

#[sqlx::test(migrations = "../migrations")]
async fn ac29_a_plain_get_of_a_mutating_endpoint_changes_nothing(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let order = titles(&pool, f.w.g, f.w.bob).await;
    let mut bob = f.w.bob().await;
    let (g, b) = (f.w.g, f.b);
    for uri in [
        format!("/api/add_item?group_id={g}&title=Hacked&description=&link="),
        format!("/api/delete_item?item_id={b}"),
        format!("/api/move_item?item_id={b}&direction=Up"),
        format!("/api/update_item?item_id={b}&title=Hacked&description=&link="),
        format!("/api/clear_wishlist?group_id={g}&confirm=true"),
    ] {
        let reply = bob.get(&uri).await;
        assert!(!reply.status.is_success(), "{uri}: {}", reply.status);
        assert!(!reply.status.is_server_error(), "{uri}: {}", reply.status);
    }
    assert_eq!(dump(&pool).await, before);
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await, order);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac29_a_cross_origin_or_origin_less_post_is_refused_and_changes_nothing(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let order = titles(&pool, f.w.g, f.w.bob).await;
    let bob = f.w.bob().await;
    let cookie = format!(
        "{}={}",
        server::SESSION_COOKIE,
        bob.session_cookie().unwrap()
    );
    let (g, b) = (f.w.g, f.b);
    let calls = [
        (
            "add_item",
            format!("group_id={g}&title=Hacked&description=&link="),
        ),
        ("delete_item", format!("item_id={b}")),
        ("move_item", format!("item_id={b}&direction=Up")),
        (
            "update_item",
            format!("item_id={b}&title=Hacked&description=&link="),
        ),
        ("clear_wishlist", format!("group_id={g}&confirm=true")),
    ];
    // Control: the same-origin POST works (on a scratch copy of the call: move is a no-op
    // on a list we then check again).
    for (name, body) in &calls {
        for origin in [Some("https://evil.example"), Some("null"), None] {
            let req = post_from(origin, &format!("/api/{name}"), Some(&cookie), body);
            let res = send(&f.w.app, req).await;
            assert_eq!(
                res.status(),
                StatusCode::FORBIDDEN,
                "{name} origin {origin:?}"
            );
        }
    }
    assert_eq!(dump(&pool).await, before);
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await, order);
    // And the same request from the right origin is honoured (the tests above are not
    // refused for another reason).
    let req = post_from(
        Some(TEST_ORIGIN),
        "/api/move_item",
        Some(&cookie),
        &format!("item_id={b}&direction=Down"),
    );
    assert!(send(&f.w.app, req).await.status().is_success());
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_C, ITEM_B]
    );
}
