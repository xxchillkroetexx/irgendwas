//! wishlists slice: add, update, delete, clear and the edit page (AC9-AC21, AC26, W8, W9).
//! Link safety and rendering are in wishlists_security.rs.

use crate::common::auth::*;
use crate::common::groups::*;
use crate::common::wishlists::*;
use app::test_support as ts;
use app::wishlists_api::{
    WishlistError, WishlistField as Fld, WishlistFieldError as Err_, WishlistView,
};
use chrono::{Duration, TimeZone, Utc};
use sqlx::MySqlPool;

async fn mark_old(pool: &MySqlPool, list: i32) {
    set_updated_at(
        pool,
        list,
        Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
    )
    .await;
}

fn recent(at: Option<chrono::DateTime<Utc>>) -> bool {
    at.is_some_and(|t| (Utc::now() - t).abs() < Duration::minutes(5))
}

// -------------------------------------------------------------------------- AC9

#[sqlx::test(migrations = "../migrations")]
async fn ac9_the_first_add_creates_the_row_trims_and_appends(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);

    let id: i32 = add_item(
        &mut carol,
        f.w.g,
        "  Book  ",
        "Paperback",
        "https://example.com/book",
    )
    .await
    .wl_value();
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 1);
    let row = item_row(&pool, id).await.expect("stored");
    assert_eq!(row.title, "Book");
    assert_eq!(row.description.as_deref(), Some("Paperback"));
    assert_eq!(row.link.as_deref(), Some("https://example.com/book"));
    assert!(
        recent(updated_at(&pool, f.w.g, f.w.carol).await),
        "updated_at is set"
    );

    add_item(&mut carol, f.w.g, "Second", "", "").await.wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.carol).await, ["Book", "Second"]);
    assert_eq!(positions(&pool, f.w.g, f.w.carol).await, [1, 2]);
    // Appended after the existing items, nothing else touched.
    add_item(&mut carol, f.w.g, "Fourth", "", "").await.wl_ok();
    assert_eq!(
        wishlist_rows(&pool, f.w.g, f.w.carol).await,
        1,
        "one row per user and group"
    );
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_B, ITEM_C]
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_a_new_item_goes_to_the_end_of_an_existing_list_and_bumps_updated_at(pool: MySqlPool) {
    let f = fixture(&pool).await;
    mark_old(&pool, f.bob_list).await;
    let mut bob = f.w.bob().await;
    add_item(&mut bob, f.w.g, "Last", "", "").await.wl_ok();
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_B, ITEM_C, "Last"]
    );
    assert!(recent(updated_at(&pool, f.w.g, f.w.bob).await));
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_a_plain_form_post_ends_on_the_edit_page_with_the_flash(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    carol.headers.push(("accept".into(), "text/html".into()));
    let reply = add_item(&mut carol, f.w.g, "Book", "", "").await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(edit_path(f.w.g)));
    let page = carol.get(&edit_path(f.w.g)).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("flash.success.item_added")));
    assert!(html_contains(&page.body, "Book"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_a_json_call_stores_the_flash_in_the_session(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    carol.session_info().await; // drop the login flash
    add_item(&mut carol, f.w.g, "Book", "", "").await.wl_ok();
    assert!(has_flash_key(
        &carol.session_info().await,
        "flash.success.item_added"
    ));
}

// ------------------------------------------------------------------------ AC10

#[sqlx::test(migrations = "../migrations")]
async fn ac10_opening_the_edit_page_creates_no_row(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let before = total_wishlists(&pool).await;
    let page = carol.get(&edit_path(f.w.g)).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("wishlist.edit.noItems")));
    assert!(html_contains(&page.body, &en("wishlist.edit.addSome")));
    carol.get(&format!("{}?edit=1", edit_path(f.w.g))).await;
    get_my_wishlist(&mut carol, f.w.g).await.wl_ok();
    get_wishlist_panel(&mut carol, f.w.g).await.wl_ok();
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);
    assert_eq!(total_wishlists(&pool).await, before);
    // The empty state offers no "clear list" section.
    assert!(!html_contains(
        &page.body,
        &en("wishlist.edit.deleteWishlist")
    ));
}

// ------------------------------------------------------------------------ AC11

#[sqlx::test(migrations = "../migrations")]
async fn ac11_an_empty_or_blank_title_is_refused(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    for title in ["", "   ", "\t\n ", "\u{a0}\u{2003}"] {
        let reply = add_item(&mut carol, f.w.g, title, "d", "").await;
        assert_eq!(reply.status, 422, "{title:?}");
        assert_eq!(
            reply.wl_err(),
            WishlistError::Validation(vec![(Fld::Title, Err_::TitleRequired)]),
            "{title:?}"
        );
    }
    assert_eq!(
        wishlist_rows(&pool, f.w.g, f.w.carol).await,
        0,
        "nothing saved"
    );
    assert_eq!(total_items(&pool).await, 5);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_title_length_counts_scalar_values_after_trimming(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let r = add_item(&mut carol, f.w.g, &repeat('a', 256), "", "").await;
    assert_eq!(r.problems(), [(Fld::Title, Err_::TitleTooLong)]);
    add_item(&mut carol, f.w.g, &repeat('a', 255), "", "")
        .await
        .wl_ok();
    // Trimmed before counting.
    add_item(
        &mut carol,
        f.w.g,
        &format!("  {}  ", repeat('b', 255)),
        "",
        "",
    )
    .await
    .wl_ok();
    // Multi-byte: 255 scalar values are fine (more than 255 bytes), 256 are not.
    add_item(&mut carol, f.w.g, &repeat('\u{e4}', 255), "", "")
        .await
        .wl_ok();
    add_item(&mut carol, f.w.g, &repeat('\u{1F600}', 255), "", "")
        .await
        .wl_ok();
    let r = add_item(&mut carol, f.w.g, &repeat('\u{1F600}', 256), "", "").await;
    assert_eq!(r.problems(), [(Fld::Title, Err_::TitleTooLong)]);
    assert_eq!(titles(&pool, f.w.g, f.w.carol).await.len(), 4);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_the_title_zero_is_accepted(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    add_item(&mut carol, f.w.g, "0", "", "").await.wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.carol).await, ["0"]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_description_limit_and_empty_values_become_null(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let r = add_item(&mut carol, f.w.g, "T", &repeat('d', 2001), "").await;
    assert_eq!(r.problems(), [(Fld::Description, Err_::DescriptionTooLong)]);
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);

    let long: i32 = add_item(&mut carol, f.w.g, "Long", &repeat('d', 2000), "")
        .await
        .wl_value();
    assert_eq!(
        item_row(&pool, long)
            .await
            .unwrap()
            .description
            .unwrap()
            .chars()
            .count(),
        2000
    );
    let multi: i32 = add_item(&mut carol, f.w.g, "Multi", &repeat('\u{1F600}', 2000), "")
        .await
        .wl_value();
    assert_eq!(
        item_row(&pool, multi)
            .await
            .unwrap()
            .description
            .unwrap()
            .chars()
            .count(),
        2000
    );

    for desc in ["", "   ", "\n"] {
        let id: i32 = add_item(&mut carol, f.w.g, "Plain", desc, " ")
            .await
            .wl_value();
        let row = item_row(&pool, id).await.unwrap();
        assert_eq!(row.description, None, "{desc:?}");
        assert_eq!(row.link, None);
    }
    // Inner line breaks of a description survive.
    let id: i32 = add_item(&mut carol, f.w.g, "Lines", "one\ntwo", "")
        .await
        .wl_value();
    assert_eq!(
        item_row(&pool, id).await.unwrap().description.as_deref(),
        Some("one\ntwo")
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_all_field_errors_come_back_together_and_nothing_is_written(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let r = add_item(
        &mut carol,
        f.w.g,
        " ",
        &repeat('d', 2001),
        "javascript:alert(1)",
    )
    .await;
    assert_eq!(r.status, 422);
    let problems = r.problems();
    for expected in [
        (Fld::Title, Err_::TitleRequired),
        (Fld::Description, Err_::DescriptionTooLong),
        (Fld::Link, Err_::LinkInvalid),
    ] {
        assert!(problems.contains(&expected), "{expected:?} in {problems:?}");
    }
    assert_eq!(problems.len(), 3);
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);
}

// ------------------------------------------------------------------------ AC12

const BAD_LINKS: &[&str] = &[
    "javascript:alert(1)",
    "javascript://%0aalert(1)",
    "data:text/html,<b>x</b>",
    "ftp://example.com/x",
    "https://user:pw@example.com/",
    "https://",
    "http:// example.com",
];

#[sqlx::test(migrations = "../migrations")]
async fn ac12_unsafe_or_malformed_links_are_refused(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    for link in BAD_LINKS {
        let r = add_item(&mut carol, f.w.g, "T", "", link).await;
        assert_eq!(r.status, 422, "{link:?}");
        assert_eq!(r.problems(), [(Fld::Link, Err_::LinkInvalid)], "{link:?}");
    }
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);
    assert_eq!(total_items(&pool).await, 5);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_a_link_over_2048_characters_is_too_long(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let long = format!("https://example.com/{}", repeat('a', 2048));
    let r = add_item(&mut carol, f.w.g, "T", "", &long).await;
    assert_eq!(r.problems(), [(Fld::Link, Err_::LinkTooLong)]);
    // Exactly 2048 is fine.
    let ok = format!(
        "https://example.com/{}",
        repeat('a', 2048 - "https://example.com/".len())
    );
    assert_eq!(ok.chars().count(), 2048);
    add_item(&mut carol, f.w.g, "T", "", &ok).await.wl_ok();
    // Short input whose normalised form exceeds 2048 (percent-encoding grows it).
    let grows = format!("https://example.com/{}", "\u{e4}".repeat(700));
    assert!(grows.chars().count() < 2048);
    let r = add_item(&mut carol, f.w.g, "T", "", &grows).await;
    assert_eq!(
        r.problems(),
        [(Fld::Link, Err_::LinkTooLong)],
        "normalised form too long"
    );
}

// ------------------------------------------------------------------------ AC13

#[sqlx::test(migrations = "../migrations")]
async fn ac13_http_https_and_scheme_less_links_are_accepted_and_normalised(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    for (input, stored) in [
        ("http://example.com", "http://example.com/"),
        ("https://example.com/a?b=c#d", "https://example.com/a?b=c#d"),
        ("www.example.com/x", "https://www.example.com/x"),
        ("  https://example.com/pad  ", "https://example.com/pad"),
        ("HTTPS://Example.COM/Path", "https://example.com/Path"),
        ("HtTp://example.com/m", "http://example.com/m"),
    ] {
        let id: i32 = add_item(&mut carol, f.w.g, "T", "", input).await.wl_value();
        assert_eq!(
            item_row(&pool, id).await.unwrap().link.as_deref(),
            Some(stored),
            "{input:?}"
        );
    }
}

// ------------------------------------------------------------------------ AC14

#[sqlx::test(migrations = "../migrations")]
async fn ac14_the_51st_item_is_refused_and_the_other_group_is_unaffected(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    for i in 4..=50 {
        add_item(&mut bob, f.w.g, &format!("Item {i}"), "", "")
            .await
            .wl_ok();
    }
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await.len(), 50);
    let r = add_item(&mut bob, f.w.g, "One too many", "", "").await;
    assert_eq!(r.status, 422);
    assert_eq!(
        r.wl_err(),
        WishlistError::Validation(vec![(Fld::Items, Err_::TooManyItems)])
    );
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await.len(), 50);
    // Deleting one makes room again.
    delete_item(&mut bob, f.a).await.wl_ok();
    add_item(&mut bob, f.w.g, "Now it fits", "", "")
        .await
        .wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await.len(), 50);
    // H is unaffected by G's limit.
    add_item(&mut bob, f.h, "Another in H", "", "")
        .await
        .wl_ok();
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X, "Another in H"]);
    // The edit page tells the count and disables the form.
    let page = bob.get(&edit_path(f.w.g)).await;
    assert!(html_contains(
        &page.body,
        &en_with("wishlist.edit.itemCount", &[("count", "50"), ("max", "50")])
    ));
    assert!(html_contains(
        &page.body,
        &en_with("validation.wishlist.too_many_items", &[("max", "50")])
    ));
}

// ------------------------------------------------------------------------ AC15

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_non_member_cannot_add_and_nothing_is_created(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let mut dave = f.w.dave().await;
    let a = add_item(&mut dave, f.w.g, "Sneak", "", "").await;
    let b = add_item(&mut dave, 999_999, "Sneak", "", "").await;
    assert_eq!(a.status, 404);
    assert_eq!(a.wl_err(), WishlistError::NotFound);
    assert_eq!(a.body, b.body);
    // Even invalid input is NotFound (membership first, no existence leak via validation).
    let c = add_item(&mut dave, f.w.g, "", "", "javascript:x").await;
    assert_eq!(c.wl_err(), WishlistError::NotFound);
    let d = clear_wishlist(&mut dave, f.w.g, true).await;
    assert_eq!(d.wl_err(), WishlistError::NotFound);
    assert_eq!(dump(&pool).await, before);
}

// ------------------------------------------------------------------------ AC16

#[sqlx::test(migrations = "../migrations")]
async fn ac16_updating_an_item_keeps_its_position_and_bumps_updated_at(pool: MySqlPool) {
    let f = fixture(&pool).await;
    mark_old(&pool, f.bob_list).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    update_item(&mut bob, f.b, "  Better B ", "new desc", "shop.example/b")
        .await
        .wl_ok();
    let row = item_row(&pool, f.b).await.unwrap();
    assert_eq!(row.title, "Better B");
    assert_eq!(row.description.as_deref(), Some("new desc"));
    assert_eq!(row.link.as_deref(), Some("https://shop.example/b"));
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, "Better B", ITEM_C]
    );
    assert!(recent(updated_at(&pool, f.w.g, f.w.bob).await));
    assert!(has_flash_key(
        &bob.session_info().await,
        "flash.success.item_updated"
    ));
    // Clearing description and link stores NULL.
    update_item(&mut bob, f.b, "Better B", "", "").await.wl_ok();
    let row = item_row(&pool, f.b).await.unwrap();
    assert_eq!((row.description, row.link), (None, None));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac16_the_edit_page_renders_the_inline_form_for_my_item(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let page = bob.get(&format!("{}?edit={}", edit_path(f.w.g), f.a)).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("wishlist.edit.save")));
    assert!(html_contains(&page.body, &en("wishlist.edit.cancel")));
    assert!(page.body.contains("value=\"Item A\""), "pre-filled title");
    assert!(page.body.contains("https://a.example/a"));
    assert!(html_contains(&page.body, "Desc A"));
    // Without ?edit there is no save button, only the edit links.
    let plain = bob.get(&edit_path(f.w.g)).await;
    assert!(!html_contains(&plain.body, &en("wishlist.edit.save")));
    assert!(plain.body.contains(&format!("?edit={}", f.a)));
    // The links of the buttons carry the item title in an accessible name.
    assert!(plain.body.contains("aria-label"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac16_a_plain_form_post_of_the_inline_form_ends_on_the_edit_page(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    bob.headers.push(("accept".into(), "text/html".into()));
    let reply = update_item(&mut bob, f.c, "Renamed C", "", "").await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(edit_path(f.w.g)));
    let page = bob.get(&edit_path(f.w.g)).await;
    assert!(html_contains(&page.body, &en("flash.success.item_updated")));
}

// ------------------------------------------------------------------------ AC18

#[sqlx::test(migrations = "../migrations")]
async fn ac18_update_with_invalid_input_changes_nothing(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let mut bob = f.w.bob().await;
    let cases: Vec<(String, String, String, Fld, Err_)> = vec![
        (
            String::new(),
            String::new(),
            String::new(),
            Fld::Title,
            Err_::TitleRequired,
        ),
        (
            "   ".into(),
            String::new(),
            String::new(),
            Fld::Title,
            Err_::TitleRequired,
        ),
        (
            repeat('x', 256),
            String::new(),
            String::new(),
            Fld::Title,
            Err_::TitleTooLong,
        ),
        (
            "T".into(),
            repeat('d', 2001),
            String::new(),
            Fld::Description,
            Err_::DescriptionTooLong,
        ),
        (
            "T".into(),
            String::new(),
            "javascript:alert(1)".into(),
            Fld::Link,
            Err_::LinkInvalid,
        ),
        (
            "T".into(),
            String::new(),
            "ftp://example.com/x".into(),
            Fld::Link,
            Err_::LinkInvalid,
        ),
        (
            "T".into(),
            String::new(),
            format!("https://e.com/{}", repeat('a', 2048)),
            Fld::Link,
            Err_::LinkTooLong,
        ),
    ];
    for (title, desc, link, field, problem) in cases {
        let r = update_item(&mut bob, f.a, &title, &desc, &link).await;
        assert_eq!(r.status, 422);
        assert_eq!(r.problems(), [(field, problem)]);
    }
    assert_eq!(dump(&pool).await, before);
}

// ------------------------------------------------------------------------ AC19

#[sqlx::test(migrations = "../migrations")]
async fn ac19_edit_with_someone_elses_item_id_renders_no_form_and_no_foreign_item(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let control = bob.get(&format!("{}?edit={}", edit_path(f.w.g), f.a)).await;
    assert!(
        html_contains(&control.body, &en("wishlist.edit.save")),
        "control: own item"
    );
    for foreign in [f.alice_item, f.x, 999_999] {
        let page = bob
            .get(&format!("{}?edit={foreign}", edit_path(f.w.g)))
            .await;
        assert_eq!(page.status, 200, "{foreign}: the parameter is ignored");
        assert!(
            !html_contains(&page.body, &en("wishlist.edit.save")),
            "{foreign}"
        );
        assert!(!page.body.contains(ALICE_ITEM));
        assert!(!page.body.contains(ITEM_X));
        assert!(page.body.contains(ITEM_A), "own list still shown");
    }
    for junk in ["abc", "-1", "0", "1.5", ""] {
        assert_eq!(
            bob.get(&format!("{}?edit={junk}", edit_path(f.w.g)))
                .await
                .status,
            200,
            "{junk:?}"
        );
    }
}

// ------------------------------------------------------------------------ AC20

#[sqlx::test(migrations = "../migrations")]
async fn ac20_removing_an_item_leaves_the_others_in_order(pool: MySqlPool) {
    let f = fixture(&pool).await;
    mark_old(&pool, f.bob_list).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    delete_item(&mut bob, f.b).await.wl_ok();
    assert!(item_row(&pool, f.b).await.is_none());
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await, [ITEM_A, ITEM_C]);
    assert!(recent(updated_at(&pool, f.w.g, f.w.bob).await));
    assert!(has_flash_key(
        &bob.session_info().await,
        "flash.success.item_deleted"
    ));
    // The row of the list stays (only "clear" removes it).
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.bob).await, 1);
    // Deleting it again: it is gone.
    assert_eq!(
        delete_item(&mut bob, f.b).await.wl_err(),
        WishlistError::NotFound
    );
    let view: WishlistView = get_wishlist(&mut bob, f.w.g, f.w.bob).await.wl_value();
    assert_eq!(view.items.len(), 2);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac20_a_plain_form_post_of_remove_ends_on_the_edit_page(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    bob.headers.push(("accept".into(), "text/html".into()));
    let reply = delete_item(&mut bob, f.b).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(edit_path(f.w.g)));
    let page = bob.get(&edit_path(f.w.g)).await;
    assert!(html_contains(&page.body, &en("flash.success.item_deleted")));
    assert!(!page.body.contains(ITEM_B));
}

// ------------------------------------------------------------------------ AC21

#[sqlx::test(migrations = "../migrations")]
async fn ac21_clear_removes_only_my_list_in_this_group(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let alice_before = titles(&pool, f.w.g, f.w.alice).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    clear_wishlist(&mut bob, f.w.g, true).await.wl_ok();
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.bob).await, 0);
    assert!(titles(&pool, f.w.g, f.w.bob).await.is_empty());
    for item in [f.a, f.b, f.c] {
        assert!(item_row(&pool, item).await.is_none());
    }
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X]);
    assert_eq!(titles(&pool, f.w.g, f.w.alice).await, alice_before);
    assert!(has_flash_key(
        &bob.session_info().await,
        "flash.success.wishlist_deleted"
    ));
    // Idempotent.
    clear_wishlist(&mut bob, f.w.g, true).await.wl_ok();
    // The empty list reads as empty again, with no "last updated".
    let view: WishlistView = get_my_wishlist(&mut bob, f.w.g).await.wl_value();
    assert!(view.items.is_empty() && view.updated_at.is_none());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac21_clear_needs_the_confirmation(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before = dump(&pool).await;
    let mut bob = f.w.bob().await;
    let r = clear_wishlist(&mut bob, f.w.g, false).await;
    assert_eq!(r.status, 422);
    assert_eq!(
        r.wl_err(),
        WishlistError::Validation(vec![(Fld::Confirm, Err_::ConfirmRequired)])
    );
    let r = bob
        .call("clear_wishlist", &[("group_id", &f.w.g.to_string())])
        .await;
    assert_ne!(r.status, 200, "a missing confirm is no confirmation");
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac21_clear_without_a_list_succeeds_and_the_form_posts_a_ticked_checkbox(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    clear_wishlist(&mut carol, f.w.g, true).await.wl_ok();
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);
    // A browser checkbox posts `on` (no-JS form).
    let mut bob = f.w.bob().await;
    bob.headers.push(("accept".into(), "text/html".into()));
    let r = bob
        .call(
            "clear_wishlist",
            &[("group_id", &f.w.g.to_string()), ("confirm", "on")],
        )
        .await;
    assert_eq!(r.status, 302, "{}", r.body);
    assert_eq!(r.location(), Some(edit_path(f.w.g)));
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.bob).await, 0);
    let page = bob.get(&edit_path(f.w.g)).await;
    assert!(html_contains(
        &page.body,
        &en("flash.success.wishlist_deleted")
    ));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac21_the_clear_section_has_a_required_confirmation_checkbox(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let page = bob.get(&edit_path(f.w.g)).await;
    assert!(html_contains(
        &page.body,
        &en("wishlist.edit.deleteWishlist")
    ));
    assert!(html_contains(
        &page.body,
        &en("wishlist.edit.deleteWishlistConfirm")
    ));
    assert!(page.body.contains("type=\"checkbox\""));
    assert!(page.body.contains("required"));
}

// ------------------------------------------------------------------------ AC26

#[sqlx::test(migrations = "../migrations")]
async fn ac26_after_the_draw_the_owner_can_still_change_the_list(pool: MySqlPool) {
    let f = fixture(&pool).await;
    ts::add_assignment(&pool, f.w.g, f.w.bob, f.w.carol).await;
    ts::set_drawn(&pool, f.w.g).await;
    mark_old(&pool, f.bob_list).await;
    let mut bob = f.w.bob().await;
    let mut carol = f.w.carol().await;

    let new: i32 = add_item(&mut bob, f.w.g, "After draw", "d", "")
        .await
        .wl_value();
    update_item(&mut bob, f.a, "A2", "", "").await.wl_ok();
    up(&mut bob, f.b).await.wl_ok();
    delete_item(&mut bob, f.c).await.wl_ok();
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_B, "A2", "After draw"]
    );

    let view: WishlistView = get_wishlist(&mut carol, f.w.g, f.w.bob).await.wl_value();
    let got: Vec<&str> = view.items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(got, [ITEM_B, "A2", "After draw"]);
    assert!(recent(view.updated_at), "{:?}", view.updated_at);
    let page = carol.get(&view_path(f.w.g, f.w.bob)).await;
    assert!(pos(&page.body, ITEM_B) < pos(&page.body, "A2"));
    assert!(page.body.contains(&Utc::now().format("%Y").to_string()));

    clear_wishlist(&mut bob, f.w.g, true).await.wl_ok();
    assert_eq!(item_row(&pool, new).await, None);
    add_item(&mut bob, f.w.g, "Again", "", "").await.wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await, ["Again"]);
}

// ------------------------------------------------------------------ Issue #69

/// The opening tag of the element with `id="{id}"`.
fn tag_with_id<'a>(html: &'a str, id: &str) -> &'a str {
    let at = pos(html, &format!("id=\"{id}\""));
    let start = html[..at].rfind('<').expect("tag start");
    let end = at + html[at..].find('>').expect("tag end");
    &html[start..=end]
}

#[sqlx::test(migrations = "../migrations")]
async fn issue69_rows_carry_an_anchor_that_edit_and_cancel_link_to(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let path = edit_path(f.w.g);

    // Plain page: every row is `#item-{id}`, and its Edit link jumps there.
    let plain = bob.get(&path).await;
    assert_eq!(plain.status, 200);
    for id in [f.a, f.b, f.c] {
        assert!(
            tag_with_id(&plain.body, &format!("item-{id}")).starts_with("<li"),
            "row {id}"
        );
        assert!(
            plain
                .body
                .contains(&format!("href=\"{path}?edit={id}#item-{id}\"")),
            "edit link of {id}"
        );
    }

    // `?edit=`: the inline form's row keeps the anchor (a page load without JavaScript lands
    // there), and Cancel leads back to the row.
    let page = bob.get(&format!("{path}?edit={}", f.b)).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("wishlist.edit.save")));
    assert!(tag_with_id(&page.body, &format!("item-{}", f.b)).starts_with("<li"));
    let title = tag_with_id(&page.body, &format!("edit-{}-title", f.b));
    assert!(title.contains("value=\"Item B\""), "{title}");
    assert!(
        page.body.contains(&format!("href=\"{path}#item-{}\"", f.b)),
        "cancel link"
    );
    // The other rows are plain rows with their anchors.
    assert!(tag_with_id(&page.body, &format!("item-{}", f.a)).starts_with("<li"));
}

#[sqlx::test(migrations = "../migrations")]
async fn issue69_a_plain_add_ends_on_an_empty_add_form(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    bob.headers.push(("accept".into(), "text/html".into()));
    let reply = add_item(
        &mut bob,
        f.w.g,
        "Fresh wish",
        "Fresh description",
        "https://fresh.example/x",
    )
    .await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(edit_path(f.w.g)));

    let page = bob.get(&edit_path(f.w.g)).await;
    assert_eq!(page.status, 200);
    assert!(
        html_contains(&page.body, "Fresh wish"),
        "the item is listed"
    );
    for id in ["add-title", "add-link"] {
        let input = tag_with_id(&page.body, id);
        assert!(input.contains("value=\"\""), "{input}");
        assert!(
            !input.contains("Fresh") && !input.contains("fresh.example"),
            "{input}"
        );
    }
    let open = tag_with_id(&page.body, "add-description");
    let after = pos(&page.body, open) + open.len();
    let content = &page.body[after..after + page.body[after..].find("</textarea>").unwrap()];
    assert_eq!(content, "", "empty description");
}
