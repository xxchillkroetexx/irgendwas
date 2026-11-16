//! #57 and #76: copy own wishes without altering source rows or weakening wishlist
//! authorization; the copy section of the edit page (docs/specs/copy-wishes.md, AC1-AC24).
use crate::common::auth::*;
use crate::common::groups::{comparable, de, de_with, en, en_with};
use crate::common::wishlists::*;
use crate::common::{TEST_ORIGIN, post_from, send};
use app::flash::FlashMessage;
use app::i18n::Locale;
use app::paths::{self, login_url};
use app::test_support::{self as ts, BOB_PASSWORD};
use app::wishlists_api::{
    CopyItem, CopySource, CopySources, WishlistError, WishlistField, WishlistFieldError,
    WishlistView,
};
use axum::http::StatusCode;
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

// ------------------------------------------------------------------ copy-wishes (#76) helpers

fn accept_html(c: &mut Client) {
    c.headers.push(("accept".into(), "text/html".into()));
}

fn copy_open(group: i32) -> String {
    format!("/groups/{group}/wishlist?copy=1")
}

fn tags<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let needle = format!("<{name}");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = html[from..].find(&needle) {
        let start = from + at;
        let end = start + html[start..].find('>').expect("end of tag") + 1;
        out.push(&html[start..end]);
        from = end;
    }
    out
}

/// True when the opening `tag` carries the attribute `name` (with or without a value).
fn has_attr(tag: &str, name: &str) -> bool {
    tag.split(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .any(|part| part == name || part.starts_with(&format!("{name}=")))
}

/// The copy section: from `<details id="copy"` to the "Your items" heading. `None` without it.
fn section(html: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(at) = html[from..].find("<details") {
        let start = from + at;
        let end = start + html[start..].find('>').expect("end of tag") + 1;
        if html[start..end].contains("id=\"copy\"") {
            let heading = [
                en("wishlist.edit.itemsHeading"),
                de("wishlist.edit.itemsHeading"),
            ]
            .iter()
            .filter_map(|h| html[end..].find(h.as_str()))
            .min()
            .map_or(html.len(), |h| end + h);
            return Some(&html[start..heading]);
        }
        from = end;
    }
    None
}

fn section_of(html: &str) -> &str {
    section(html).unwrap_or_else(|| panic!("no <details id=\"copy\"> in the page"))
}

/// The opening tag of the copy section.
fn copy_tag(html: &str) -> &str {
    let s = section_of(html);
    &s[..s.find('>').unwrap() + 1]
}

fn is_open(tag: &str) -> bool {
    has_attr(tag, "open")
}

/// The opening tags of the nested per-source `<details>` (in page order).
fn source_tags(html: &str) -> Vec<&str> {
    tags(section_of(html), "details").split_off(1)
}

/// The checkbox of item `id` inside the copy section.
fn item_box(html: &str, id: i32) -> &str {
    let value = format!("value=\"{id}\"");
    tags(section_of(html), "input")
        .into_iter()
        .find(|t| t.contains("item_ids[") && t.contains(&value))
        .unwrap_or_else(|| panic!("no checkbox for item {id} in the copy section"))
}

fn has_item_box(html: &str, id: i32) -> bool {
    let value = format!("value=\"{id}\"");
    tags(section_of(html), "input")
        .into_iter()
        .any(|t| t.contains("item_ids[") && t.contains(&value))
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// Everything of Bob's S1 list that a copy must not touch.
#[derive(Debug, PartialEq)]
struct S1State {
    titles: Vec<String>,
    positions: Vec<i32>,
    updated_at: Option<chrono::DateTime<Utc>>,
    items: Vec<ItemRow>,
}

async fn s1_state(pool: &MySqlPool, f: &CopyWorld) -> S1State {
    let items = dump(pool)
        .await
        .0
        .into_iter()
        .filter(|i| i.wishlist_id == f.s1_list)
        .collect();
    S1State {
        titles: titles(pool, f.s1, f.w.bob).await,
        positions: positions(pool, f.s1, f.w.bob).await,
        updated_at: updated_at(pool, f.s1, f.w.bob).await,
        items,
    }
}

async fn sources(c: &mut Client, group: i32) -> CopySources {
    get_copy_sources(c, group).await.wl_value::<CopySources>()
}

fn titles_of(source: &CopySource) -> Vec<&str> {
    source.items.iter().map(|i| i.title.as_str()).collect()
}

fn item<'a>(source: &'a CopySource, title: &str) -> &'a CopyItem {
    source
        .items
        .iter()
        .find(|i| i.title == title)
        .unwrap_or_else(|| panic!("no item {title:?}"))
}

/// A logged-in client whose pending login flash has been taken.
async fn ready(mut c: Client) -> Client {
    c.session_info().await;
    c
}

// ------------------------------------------------------------------ AC1 - AC3: the section

#[sqlx::test(migrations = "../migrations")]
async fn ac1_the_section_sits_between_the_add_form_and_the_items_and_is_closed(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = ready(f.w.bob().await).await;
    let page = bob.get(&edit_path(f.t)).await;
    assert_eq!(page.status, 200);
    let tag = copy_tag(&page.body);
    assert!(!is_open(tag), "his T list has an item: closed ({tag})");
    let at = page.body.find(tag).unwrap();
    assert!(
        pos(&page.body, "id=\"add-title\"") < at,
        "after the add form"
    );
    assert!(
        at < pos(&page.body, &en("wishlist.edit.itemsHeading")),
        "before the items list"
    );
    let summary = &page.body[at..];
    assert!(
        pos(summary, "<summary") < pos(summary, &en("wishlist.copy.title")),
        "the title is the summary"
    );
    assert_eq!(
        count(&page.body, &en("wishlist.copy.title")),
        1,
        "no second copy card"
    );
    assert!(
        pos(&page.body, &en("wishlist.copy.title")) < pos(&page.body, "clear-confirm"),
        "nothing after Remove all items"
    );
    // The old chooser is gone.
    assert!(!page.body.contains("name=\"source\""));
    assert!(!page.body.contains("copy-source"));
    assert!(
        !html_contains(&page.body, "Show wishes"),
        "the removed wishlist.copy.load"
    );
    assert_no_raw_keys(&page.body, "copy section");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac2_without_another_nonempty_list_there_is_no_section(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    // Carol has no list anywhere.
    let mut carol = ready(f.w.carol().await).await;
    let page = carol.get(&edit_path(f.t)).await;
    assert_eq!(page.status, 200);
    assert!(section(&page.body).is_none());
    assert!(!html_contains(&page.body, &en("wishlist.copy.title")));
    // Bob's only other list is an emptied one: still none.
    let g2 = ts::create_group(&pool, f.w.alice, "Other").await;
    ts::add_member(&pool, g2, f.w.bob).await;
    ts::create_wishlist(&pool, g2, f.w.bob, &[]).await;
    sqlx::query("DELETE FROM wishlist_items WHERE wishlist_id IN (?, ?)")
        .bind(f.s1_list)
        .bind(f.s2_list)
        .execute(&pool)
        .await
        .unwrap();
    let mut bob = ready(f.w.bob().await).await;
    let page = bob.get(&edit_path(f.t)).await;
    assert_eq!(page.status, 200);
    assert!(section(&page.body).is_none());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac3_the_section_is_open_for_an_empty_list_and_for_the_copy_parameter(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = ready(f.w.bob().await).await;
    let empty = bob.get(&edit_path(f.e)).await;
    assert_eq!(empty.status, 200);
    assert!(is_open(copy_tag(&empty.body)), "his list in E is empty");
    for path in [
        copy_open(f.t),
        format!("{}?copy=0", edit_path(f.t)),
        format!("{}?copy=", edit_path(f.t)),
    ] {
        let page = bob.get(&path).await;
        assert_eq!(page.status, 200, "{path}");
        assert!(
            is_open(copy_tag(&page.body)),
            "{path}: any value of copy opens it"
        );
    }
    assert_eq!(paths::wishlist_copy_open_path(f.t), copy_open(f.t));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac3_the_old_source_parameter_is_ignored(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = ready(f.w.bob().await).await;
    let plain_path = edit_path(f.t);
    let plain = bob.get(&plain_path).await;
    for path in [
        format!("{plain_path}?source={}", f.s1),
        format!("{plain_path}?source={}", f.t),
        format!("{plain_path}?source={}", f.w.g + 999),
        format!("{plain_path}?source=abc"),
    ] {
        let page = bob.get(&path).await;
        assert_eq!(page.status, 200, "{path}");
        assert_eq!(
            comparable(&page.body, &path),
            comparable(&plain.body, &plain_path),
            "{path} renders like the plain page"
        );
    }
}

// ------------------------------------------------------------------ AC4: get_copy_sources access

#[sqlx::test(migrations = "../migrations")]
async fn ac4_get_copy_sources_checks_session_and_membership_and_writes_nothing(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let before = dump(&pool).await;
    let rows = total_wishlists(&pool).await;
    let mut dave = f.w.dave().await;
    assert_eq!(
        get_copy_sources(&mut dave, f.t).await.wl_err(),
        WishlistError::NotFound
    );
    let mut bob = f.w.bob().await;
    assert_eq!(
        get_copy_sources(&mut bob, 999_999).await.wl_err(),
        WishlistError::NotFound
    );
    let mut anon = f.w.anon();
    assert_eq!(
        get_copy_sources(&mut anon, f.t).await.wl_err(),
        WishlistError::Unauthorized
    );
    // Bob in E (no list there): fine, and still no row.
    let for_e = sources(&mut bob, f.e).await;
    assert_eq!(for_e.free, 50);
    assert_eq!(wishlist_rows(&pool, f.e, f.w.bob).await, 0);
    sources(&mut bob, f.t).await;
    assert_eq!(dump(&pool).await, before);
    assert_eq!(total_wishlists(&pool).await, rows);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac4_the_old_get_wishlist_sources_is_gone(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    let reply = bob
        .call("get_wishlist_sources", &[("group_id", &f.t.to_string())])
        .await;
    assert!(!reply.status.is_success(), "removed: {}", reply.body);
}

// ------------------------------------------------------------------ AC5: content

#[sqlx::test(migrations = "../migrations")]
async fn ac5_sources_are_own_nonempty_current_lists_newest_first_including_drawn(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    let got = sources(&mut bob, f.t).await;
    assert_eq!(got.free, 49);
    let ids: Vec<i32> = got.sources.iter().map(|s| s.group.id).collect();
    assert_eq!(
        ids,
        [f.s1, f.s2],
        "S1 (drawn) before S2; not T, E, nor G (no list)"
    );
    let s1 = &got.sources[0];
    assert_eq!(s1.group.name, "Family 2025");
    assert_eq!(
        s1.updated_at,
        Some(Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap())
    );
    assert_eq!(titles_of(s1), ["Book", "socks", "Tea"]);
    let book = item(s1, "Book");
    assert_eq!(book.id, f.book);
    assert_eq!(book.description.as_deref(), Some("Paperback"));
    assert_eq!(book.link_host.as_deref(), Some("shop.example"));
    assert_eq!(item(s1, "Tea").description, None);
    assert_eq!(item(s1, "Tea").link_host, None);
    assert_eq!(titles_of(&got.sources[1]), ["Mug"]);
    // Nobody else's data, no emails.
    let reply = get_copy_sources(&mut bob, f.t).await;
    for email in &f.w.emails {
        assert!(
            !reply.body.contains(email.as_str()),
            "no email in the reply"
        );
    }
    assert!(!reply.body.contains("Scarf"));
    // From E: T joins the sources, oldest last.
    let from_e: Vec<i32> = sources(&mut bob, f.e)
        .await
        .sources
        .iter()
        .map(|s| s.group.id)
        .collect();
    assert_eq!(from_e, [f.s1, f.s2, f.t]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_sources_with_equal_timestamps_are_ordered_by_group_id_descending(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let s3 = ts::create_group(&pool, f.w.alice, "Family 2024").await;
    ts::add_member(&pool, s3, f.w.bob).await;
    let list = ts::create_wishlist(&pool, s3, f.w.bob, &["Old gift"]).await;
    set_updated_at(
        &pool,
        list,
        Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap(),
    )
    .await;
    let mut bob = f.w.bob().await;
    let ids: Vec<i32> = sources(&mut bob, f.t)
        .await
        .sources
        .iter()
        .map(|s| s.group.id)
        .collect();
    assert_eq!(ids, [s3, f.s1, f.s2]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_groups_the_caller_left_or_lists_of_others_are_no_sources(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut carol = f.w.carol().await;
    assert!(sources(&mut carol, f.t).await.sources.is_empty());
    // Carol joins S2 with a list of her own: only hers shows, never Bob's.
    ts::create_wishlist(&pool, f.s2, f.w.carol, &["Carol thing"]).await;
    let got = sources(&mut carol, f.t).await;
    assert_eq!(got.sources.len(), 1);
    assert_eq!(titles_of(&got.sources[0]), ["Carol thing"]);
    // Bob loses his S2 membership.
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(f.s2)
        .bind(f.w.bob)
        .execute(&pool)
        .await
        .unwrap();
    let mut bob = f.w.bob().await;
    let ids: Vec<i32> = sources(&mut bob, f.t)
        .await
        .sources
        .iter()
        .map(|s| s.group.id)
        .collect();
    assert_eq!(ids, [f.s1]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_the_page_lists_both_sources_with_the_first_open_and_nothing_ticked(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = ready(f.w.bob().await).await;
    let page = bob.get(&edit_path(f.t)).await;
    assert_eq!(page.status, 200);
    let sec = section_of(&page.body);
    assert!(html_contains(sec, &en("wishlist.copy.instructions")));
    let nested = source_tags(&page.body);
    assert_eq!(nested.len(), 2);
    assert!(is_open(nested[0]), "S1 open");
    assert!(!is_open(nested[1]), "S2 closed");
    // Order: S1, its items, S2.
    let order = [
        "Family 2025",
        "Book",
        "Paperback",
        "shop.example",
        "socks",
        "Tea",
        "Office",
        "Mug",
    ];
    let mut last = 0;
    for needle in order {
        let at = sec[last..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing or out of order"))
            + last;
        last = at + needle.len();
    }
    let three = en_with("wishlist.panel.itemCount.other", &[("count", "3")]);
    let one = en_with("wishlist.panel.itemCount.one", &[("count", "1")]);
    assert!(sec.contains(&three) && sec.contains(&one));
    let date = app::format::date(
        Locale::En,
        chrono_tz::Europe::Berlin,
        Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap(),
    );
    assert!(html_contains(
        sec,
        &en_with("wishlist.view.lastUpdated", &[("date", &date)])
    ));
    for input in tags(sec, "input") {
        assert!(!has_attr(input, "checked"), "nothing ticked: {input}");
    }
    // Nothing of T, E, Alice or Carol in the section; no other user's email anywhere.
    assert!(!sec.contains("Family 2026") && !sec.contains("Empty Group"));
    assert!(!sec.contains("Scarf"));
    for email in [&f.w.emails[0], &f.w.emails[2], &f.w.emails[3]] {
        assert!(!page.body.contains(email.as_str()), "no foreign email");
    }
    // Every item has a checkbox with a sparse-friendly name and its id as value.
    for id in [f.book, f.socks, f.tea, f.mug] {
        assert!(item_box(&page.body, id).contains("type=\"checkbox\""));
    }
    assert!(!has_item_box(&page.body, f.scarf));
    assert!(!has_item_box(&page.body, f.socks_t));
    assert_no_raw_keys(&page.body, "copy section");
}

// ------------------------------------------------------------------ AC6: already on the list

#[sqlx::test(migrations = "../migrations")]
async fn ac6_an_item_with_the_title_of_one_on_the_list_is_greyed_out(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = ready(f.w.bob().await).await;
    let got = sources(&mut bob, f.t).await;
    let s1 = &got.sources[0];
    assert!(item(s1, "socks").on_target, "'socks' equals 'Socks'");
    assert!(!item(s1, "Book").on_target);
    assert!(!item(s1, "Tea").on_target);
    assert!(!got.sources[1].items[0].on_target);
    assert!(s1.items.iter().all(|i| i.copyable));
    let page = bob.get(&edit_path(f.t)).await;
    let hint = en("wishlist.copy.alreadyOnList");
    assert_eq!(count(section_of(&page.body), &hint), 1);
    assert!(has_attr(item_box(&page.body, f.socks), "disabled"));
    assert!(!has_attr(item_box(&page.body, f.book), "disabled"));
    assert!(!has_attr(item_box(&page.body, f.tea), "disabled"));
    assert!(!has_attr(item_box(&page.body, f.mug), "disabled"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_case_differences_count_but_other_titles_do_not(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    add_item(&mut bob, f.t, "BOOK", "", "").await.wl_ok();
    add_item(&mut bob, f.t, "Teas", "", "").await.wl_ok();
    let got = sources(&mut bob, f.t).await;
    assert!(item(&got.sources[0], "Book").on_target, "BOOK equals Book");
    assert!(
        !item(&got.sources[0], "Tea").on_target,
        "Teas is another title"
    );
    let page = bob.get(&edit_path(f.t)).await;
    assert_eq!(
        count(section_of(&page.body), &en("wishlist.copy.alreadyOnList")),
        2
    );
    assert!(has_attr(item_box(&page.body, f.book), "disabled"));
    assert!(!has_attr(item_box(&page.body, f.tea), "disabled"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_the_title_key_trims_unicode_whitespace_and_lowercases_without_folding(
    pool: MySqlPool,
) {
    let f = copy_world(&pool).await;
    for title in ["Café", "ÄPFEL", "Straße", "Red Book", "Pen!"] {
        insert_item(&pool, f.s1_list, title, None, None).await;
    }
    for title in [
        "cafe",
        "\u{2003}äpfel\u{3000}",
        "STRASSE",
        "Red  Book",
        "Pen",
    ] {
        insert_item(&pool, f.t_list, title, None, None).await;
    }
    insert_item(&pool, f.t_list, "\u{a0}Tea\u{2003}", None, None).await;
    // Another member's item with the title does not count.
    ts::create_wishlist(&pool, f.t, f.w.alice, &["Book"]).await;
    let mut bob = f.w.bob().await;
    let got = sources(&mut bob, f.t).await;
    let s1 = &got.sources[0];
    let on: Vec<(&str, bool)> = s1
        .items
        .iter()
        .map(|i| (i.title.as_str(), i.on_target))
        .collect();
    assert!(item(s1, "Tea").on_target, "trimmed: {on:?}");
    assert!(
        item(s1, "ÄPFEL").on_target,
        "trimmed and lowercased: {on:?}"
    );
    assert!(!item(s1, "Café").on_target, "no accent folding: {on:?}");
    assert!(!item(s1, "Straße").on_target, "no ß folding: {on:?}");
    assert!(
        !item(s1, "Red Book").on_target,
        "inner whitespace counts: {on:?}"
    );
    assert!(!item(s1, "Pen!").on_target, "punctuation counts: {on:?}");
    assert!(
        !item(s1, "Book").on_target,
        "only my own list counts: {on:?}"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac6_the_description_and_the_link_do_not_matter_for_a_duplicate(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    insert_item(
        &pool,
        f.t_list,
        "Tea",
        Some("Green"),
        Some("https://other.example/"),
    )
    .await;
    let mut bob = f.w.bob().await;
    let got = sources(&mut bob, f.t).await;
    assert!(item(&got.sources[0], "Tea").on_target);
}

// ------------------------------------------------------------------ AC7: not copyable

#[sqlx::test(migrations = "../migrations")]
async fn ac7_an_item_failing_todays_rules_is_greyed_out_with_a_link_to_fix_it(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let old = insert_item(&pool, f.s1_list, "Old", None, Some("ftp://example.com/x")).await;
    let long = insert_item(&pool, f.s1_list, "Long", Some(&"x".repeat(2001)), None).await;
    let mut bob = ready(f.w.bob().await).await;
    let got = sources(&mut bob, f.t).await;
    let s1 = &got.sources[0];
    assert!(!item(s1, "Old").copyable);
    assert!(!item(s1, "Long").copyable);
    assert!(item(s1, "Book").copyable && item(s1, "Tea").copyable);
    let page = bob.get(&edit_path(f.t)).await;
    assert_eq!(page.status, 200);
    assert!(has_attr(item_box(&page.body, old), "disabled"));
    assert!(has_attr(item_box(&page.body, long), "disabled"));
    let sec = section_of(&page.body);
    assert_eq!(count(sec, &en("wishlist.copy.notCopyable")), 2);
    assert!(sec.contains(&en("wishlist.copy.fixInSource")));
    let href = format!("/groups/{}/wishlist?edit={old}#item-{old}", f.s1);
    assert!(sec.contains(&href), "link to fix it: {href}");
    // An item that is both greyed out for the title and not copyable shows only the former.
    insert_item(&pool, f.t_list, "OLD", None, None).await;
    let page = bob.get(&edit_path(f.t)).await;
    let sec = section_of(&page.body);
    assert_eq!(count(sec, &en("wishlist.copy.notCopyable")), 1);
    assert_eq!(count(sec, &en("wishlist.copy.alreadyOnList")), 2);
    let got = sources(&mut bob, f.t).await;
    assert!(item(&got.sources[0], "Old").on_target);
    assert!(!item(&got.sources[0], "Old").copyable);
}

// ------------------------------------------------------------------ AC8: full list

#[sqlx::test(migrations = "../migrations")]
async fn ac8_a_full_list_shows_only_the_limit_message(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    add_fillers(&pool, f.t_list, 49).await;
    let mut bob = ready(f.w.bob().await).await;
    assert_eq!(sources(&mut bob, f.t).await.free, 0);
    let page = bob.get(&copy_open(f.t)).await;
    assert_eq!(page.status, 200);
    assert!(is_open(copy_tag(&page.body)));
    let sec = section_of(&page.body);
    assert!(html_contains(
        sec,
        &en_with("validation.wishlist.too_many_items", &[("max", "50")])
    ));
    assert!(tags(sec, "details").len() == 1, "no source lists");
    assert!(!sec.contains("item_ids["));
    assert!(!sec.contains(&en("wishlist.copy.submit")));
    assert!(tags(sec, "form").is_empty() && tags(sec, "button").is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_free_counts_the_remaining_room(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    add_fillers(&pool, f.t_list, 10).await;
    let mut bob = f.w.bob().await;
    assert_eq!(sources(&mut bob, f.t).await.free, 39);
}

// ------------------------------------------------------------------ AC9, AC10, AC16: copying

#[sqlx::test(migrations = "../migrations")]
async fn ac9_copy_appends_in_source_order_keeps_details_and_leaves_the_source(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    let s1_before = s1_state(&pool, &f).await;
    let t_before = updated_at(&pool, f.t, f.w.bob).await;
    let copied = copy(&mut bob, f.t, f.s1, &[f.tea, f.book]).await;
    assert_eq!(copied.wl_value::<u32>(), 2);
    assert_eq!(titles(&pool, f.t, f.w.bob).await, ["Socks", "Book", "Tea"]);
    let list = get_my_wishlist(&mut bob, f.t)
        .await
        .wl_value::<WishlistView>();
    assert_eq!(list.items[1].description.as_deref(), Some("Paperback"));
    assert_eq!(
        list.items[1].link.as_deref(),
        Some("https://shop.example/book")
    );
    assert_ne!(list.items[1].id, f.book);
    assert!(updated_at(&pool, f.t, f.w.bob).await > t_before);
    assert_eq!(s1_state(&pool, &f).await, s1_before);
    // The flash counts the copied wishes (JSON path: kept in the session).
    let info = bob.session_info().await;
    assert_flash(
        &info,
        &FlashMessage::success("flash.success.wishes_copied")
            .with_plural(2)
            .with_number("count", 2),
    );
    assert!(!has_flash_key(&info, "flash.success.wishlist_copied"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_a_plain_form_post_ends_on_the_edit_page_with_the_count_once_and_the_section_closed(
    pool: MySqlPool,
) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    accept_html(&mut bob);
    // Sparse names, as the browser sends the ticked boxes only.
    let reply = bob
        .call(
            "copy_wishlist_items",
            &[
                ("group_id", &f.t.to_string()),
                ("source_group_id", &f.s1.to_string()),
                ("item_ids[0]", &f.book.to_string()),
                ("item_ids[2]", &f.tea.to_string()),
            ],
        )
        .await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(edit_path(f.t)), "no copy parameter");
    assert_eq!(titles(&pool, f.t, f.w.bob).await, ["Socks", "Book", "Tea"]);
    let page = bob.get(&edit_path(f.t)).await;
    assert_eq!(page.status, 200);
    let text = en_with("flash.success.wishes_copied.other", &[("count", "2")]);
    assert_eq!(text, "2 wishes were copied.");
    assert_eq!(count(&page.body, &text), 1, "shown once");
    assert!(!is_open(copy_tag(&page.body)), "closed after a copy");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_one_copied_wish_reads_singular_in_english_and_german(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    accept_html(&mut bob);
    let reply = copy(&mut bob, f.t, f.s2, &[f.mug]).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    let page = bob.get(&edit_path(f.t)).await;
    assert!(html_contains(&page.body, "1 wish was copied."));
    assert_eq!(titles(&pool, f.t, f.w.bob).await, ["Socks", "Mug"]);

    let mut german = f.w.german(&f.w.emails[1], BOB_PASSWORD).await;
    german.session_info().await;
    accept_html(&mut german);
    let tea = copy(&mut german, f.t, f.s1, &[f.tea]).await;
    assert_eq!(tea.status, 302, "{}", tea.body);
    let page = german.get(&edit_path(f.t)).await;
    assert!(html_contains(&page.body, "1 Wunsch wurde übernommen."));
    assert_eq!(
        de_with("flash.success.wishes_copied.one", &[("count", "1")]),
        "1 Wunsch wurde übernommen."
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_copy_wishlist_items_returns_the_number_copied(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    assert_eq!(
        copy(&mut bob, f.t, f.s2, &[f.mug]).await.wl_value::<u32>(),
        1
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac16_two_selected_wishes_with_the_same_title_key_are_both_copied(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let lamp = insert_item(&pool, f.s2_list, "Lamp", None, None).await;
    let small = insert_item(&pool, f.s2_list, "lamp", None, None).await;
    let mut bob = f.w.bob().await;
    let n = copy(&mut bob, f.t, f.s2, &[lamp, small])
        .await
        .wl_value::<u32>();
    assert_eq!(n, 2);
    assert_eq!(titles(&pool, f.t, f.w.bob).await, ["Socks", "Lamp", "lamp"]);
    let info = bob.session_info().await;
    assert_flash(
        &info,
        &FlashMessage::success("flash.success.wishes_copied")
            .with_plural(2)
            .with_number("count", 2),
    );
    // Afterwards both are on the list, so both are greyed out.
    let got = sources(&mut bob, f.t).await;
    let s2 = got.sources.iter().find(|s| s.group.id == f.s2).unwrap();
    assert!(item(s2, "Lamp").on_target && item(s2, "lamp").on_target);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_copying_into_a_group_without_a_list_creates_it(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    assert_eq!(wishlist_rows(&pool, f.e, f.w.bob).await, 0);
    copy(&mut bob, f.e, f.s1, &[f.book]).await.wl_ok();
    assert_eq!(titles(&pool, f.e, f.w.bob).await, ["Book"]);
}

// ------------------------------------------------------------- AC11-AC18: failures

#[sqlx::test(migrations = "../migrations")]
async fn ac11_nothing_selected_writes_nothing_and_says_so(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    let before = dump(&pool).await;
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[]).await.problems(),
        [(WishlistField::Items, WishlistFieldError::SelectionRequired)]
    );
    assert_eq!(dump(&pool).await, before);
    assert_eq!(
        en("validation.wishlist.selection_required"),
        "Select at least one wish to copy."
    );
    assert_eq!(
        de("validation.wishlist.selection_required"),
        "Wählen Sie mindestens einen Wunsch aus."
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_a_plain_post_without_ticks_lands_on_the_open_section_with_the_error(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    accept_html(&mut bob);
    let before = dump(&pool).await;
    let reply = copy(&mut bob, f.t, f.s1, &[]).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(copy_open(f.t)), "no #copy fragment");
    let page = bob.get(&copy_open(f.t)).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(
        &page.body,
        "Select at least one wish to copy."
    ));
    assert!(is_open(copy_tag(&page.body)));
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_a_selection_over_the_limit_is_refused_whole(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    add_fillers(&pool, f.t_list, 48).await; // 49 items
    let mut bob = f.w.bob().await;
    assert_eq!(sources(&mut bob, f.t).await.free, 1);
    let before = dump(&pool).await;
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book, f.tea]).await.problems(),
        [(WishlistField::Items, WishlistFieldError::TooManyItems)]
    );
    assert_eq!(dump(&pool).await, before);
    // One fits.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book]).await.wl_value::<u32>(),
        1
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_a_plain_post_over_the_limit_lands_on_the_open_section(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    add_fillers(&pool, f.t_list, 48).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    accept_html(&mut bob);
    let before = dump(&pool).await;
    let reply = copy(&mut bob, f.t, f.s1, &[f.book, f.tea]).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(copy_open(f.t)));
    let page = bob.get(&copy_open(f.t)).await;
    assert!(html_contains(
        &page.body,
        &en_with("validation.wishlist.too_many_items", &[("max", "50")])
    ));
    assert!(is_open(copy_tag(&page.body)));
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_a_vanished_source_item_gives_not_found_and_copies_nothing(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    let mut other_tab = bob.fork();
    delete_item(&mut other_tab, f.book).await.wl_ok();
    let before = dump(&pool).await;
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book, f.tea]).await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(dump(&pool).await, before);
    // The re-read data no longer lists "Book".
    let got = sources(&mut bob, f.t).await;
    assert_eq!(titles_of(&got.sources[0]), ["socks", "Tea"]);
    assert_eq!(
        en("wishlist.copy.changed"),
        "Some of the selected wishes no longer exist. Please check your selection."
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_a_plain_post_of_a_vanished_item_lands_on_the_open_section_with_the_hint(
    pool: MySqlPool,
) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    delete_item(&mut bob.fork(), f.book).await.wl_ok();
    bob.session_info().await;
    accept_html(&mut bob);
    let reply = copy(&mut bob, f.t, f.s1, &[f.book, f.tea]).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(copy_open(f.t)), "not /groups");
    let page = bob.get(&copy_open(f.t)).await;
    assert_eq!(page.status, 200);
    assert!(html_contains(&page.body, &en("wishlist.copy.changed")));
    assert!(!html_contains(&page.body, &en("error.notFound.message")));
    assert!(
        !has_item_box(&page.body, f.book),
        "the re-read section drops it"
    );
    assert!(has_item_box(&page.body, f.tea));
    assert_eq!(titles(&pool, f.t, f.w.bob).await, ["Socks"]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_an_item_that_became_a_duplicate_is_refused_and_greyed_out_afterwards(
    pool: MySqlPool,
) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    let mut other_tab = bob.fork();
    add_item(&mut other_tab, f.t, "tea", "", "").await.wl_ok();
    let before = dump(&pool).await;
    let problems = copy(&mut bob, f.t, f.s1, &[f.book, f.tea]).await.problems();
    assert_eq!(
        problems,
        [(WishlistField::Items, WishlistFieldError::AlreadyOnList)]
    );
    assert_eq!(
        WishlistFieldError::AlreadyOnList.message_key(),
        "validation.wishlist.already_on_list"
    );
    assert_eq!(dump(&pool).await, before);
    // The re-read section greys out "Tea".
    let mut page_bob = ready(bob.fork()).await;
    let page = page_bob.get(&edit_path(f.t)).await;
    assert!(has_attr(item_box(&page.body, f.tea), "disabled"));
    assert!(!has_attr(item_box(&page.body, f.book), "disabled"));
    // A direct call with the long-standing duplicate gives the same error.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.socks]).await.problems(),
        problems
    );
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_a_plain_post_of_a_duplicate_lands_on_the_open_section_with_the_text(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    accept_html(&mut bob);
    let reply = copy(&mut bob, f.t, f.s1, &[f.socks]).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(copy_open(f.t)));
    let page = bob.get(&copy_open(f.t)).await;
    assert!(html_contains(
        &page.body,
        &en("validation.wishlist.already_on_list")
    ));
    assert_eq!(
        en("validation.wishlist.already_on_list"),
        "One of the selected wishes is already on this list."
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_not_copyable_item_is_refused_with_its_own_error(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let old = insert_item(&pool, f.s1_list, "Old", None, Some("ftp://example.com/x")).await;
    let mut bob = f.w.bob().await;
    let before = dump(&pool).await;
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[old]).await.problems(),
        [(WishlistField::Items, WishlistFieldError::ItemNotCopyable)]
    );
    assert_eq!(
        WishlistFieldError::ItemNotCopyable.message_key(),
        "validation.wishlist.item_not_copyable"
    );
    assert_eq!(dump(&pool).await, before);
    // Together with good items: still the whole copy is refused.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book, old]).await.problems(),
        [(WishlistField::Items, WishlistFieldError::ItemNotCopyable)]
    );
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac15_a_plain_post_of_a_not_copyable_item_lands_on_the_open_section(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let old = insert_item(&pool, f.s1_list, "Old", None, Some("ftp://example.com/x")).await;
    let mut bob = f.w.bob().await;
    bob.session_info().await;
    accept_html(&mut bob);
    let reply = copy(&mut bob, f.t, f.s1, &[old]).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(copy_open(f.t)));
    let page = bob.get(&copy_open(f.t)).await;
    assert!(html_contains(
        &page.body,
        &en("validation.wishlist.item_not_copyable")
    ));
    assert!(!html_contains(
        &page.body,
        &en("validation.wishlist.link_invalid")
    ));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_ac15_the_first_failing_check_wins(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let old = insert_item(&pool, f.s1_list, "Old", None, Some("ftp://example.com/x")).await;
    add_fillers(&pool, f.t_list, 48).await; // 49: Book + Tea would exceed the cap
    let mut bob = f.w.bob().await;
    let before = dump(&pool).await;
    // NotFound beats everything.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.socks, old, f.scarf])
            .await
            .wl_err(),
        WishlistError::NotFound
    );
    // AlreadyOnList beats ItemNotCopyable and the cap.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[old, f.socks, f.book, f.tea])
            .await
            .problems(),
        [(WishlistField::Items, WishlistFieldError::AlreadyOnList)]
    );
    // ItemNotCopyable beats the cap.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[old, f.book, f.tea])
            .await
            .problems(),
        [(WishlistField::Items, WishlistFieldError::ItemNotCopyable)]
    );
    // The cap last.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book, f.tea]).await.problems(),
        [(WishlistField::Items, WishlistFieldError::TooManyItems)]
    );
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_the_57_rules_still_hold_and_nothing_is_written(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    let before = dump(&pool).await;
    let rows = total_wishlists(&pool).await;
    // Somebody else's item, or an item of another list than the named source.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book, f.scarf]).await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.mug]).await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.mug + 10_000]).await.wl_err(),
        WishlistError::NotFound
    );
    // Same group, empty, non-positive and oversized selections.
    let selection = (WishlistField::Items, WishlistFieldError::SelectionRequired);
    assert_eq!(
        copy(&mut bob, f.s1, f.s1, &[f.book]).await.problems(),
        [selection]
    );
    assert_eq!(copy(&mut bob, f.t, f.s1, &[]).await.problems(), [selection]);
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[0, f.book]).await.problems(),
        [selection]
    );
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[-3]).await.problems(),
        [selection]
    );
    let many: Vec<i32> = (1..=51).collect();
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &many).await.problems(),
        [selection]
    );
    // A caller who is no member of the target.
    let mut dave = f.w.dave().await;
    assert_eq!(
        copy(&mut dave, f.t, f.s1, &[f.book]).await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(dump(&pool).await, before);
    assert_eq!(total_wishlists(&pool).await, rows);
    // Duplicate ids in the selection count once.
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book, f.book])
            .await
            .wl_value::<u32>(),
        1
    );
    assert_eq!(titles(&pool, f.t, f.w.bob).await, ["Socks", "Book"]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_a_member_who_left_the_source_group_gets_not_found(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    // S1 is drawn: he cannot leave, but his row can vanish.
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(f.s1)
        .bind(f.w.bob)
        .execute(&pool)
        .await
        .unwrap();
    let before = dump(&pool).await;
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book]).await.wl_err(),
        WishlistError::NotFound
    );
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_a_get_or_a_cross_origin_post_changes_nothing(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let bob = f.w.bob().await;
    let before = dump(&pool).await;
    let cookie = format!(
        "{}={}",
        server::SESSION_COOKIE,
        bob.session_cookie().unwrap()
    );
    let body = format!(
        "group_id={}&source_group_id={}&item_ids%5B0%5D={}",
        f.t, f.s1, f.book
    );
    for origin in [Some("https://evil.example"), Some("null"), None] {
        let req = post_from(origin, "/api/copy_wishlist_items", Some(&cookie), &body);
        let res = send(&f.w.app, req).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "origin {origin:?}");
    }
    let mut client = f.w.bob().await;
    let got = client
        .get(&format!(
            "/api/copy_wishlist_items?group_id={}&source_group_id={}&item_ids%5B0%5D={}",
            f.t, f.s1, f.book
        ))
        .await;
    assert!(!got.status.is_success(), "GET: {}", got.status);
    assert_eq!(dump(&pool).await, before);
    // Control: the same body from the right origin works.
    let req = post_from(
        Some(TEST_ORIGIN),
        "/api/copy_wishlist_items",
        Some(&cookie),
        &body,
    );
    assert!(send(&f.w.app, req).await.status().is_success());
    assert_eq!(titles(&pool, f.t, f.w.bob).await, ["Socks", "Book"]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_a_plain_post_of_a_non_member_is_a_redirect_to_a_404_page(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut dave = f.w.dave().await;
    accept_html(&mut dave);
    let before = dump(&pool).await;
    let reply = copy(&mut dave, f.t, f.s1, &[f.book]).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(copy_open(f.t)));
    let page = dave.get(&copy_open(f.t)).await;
    assert_eq!(
        page.status, 404,
        "nothing is revealed beyond what a GET shows"
    );
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac18_without_a_session_a_plain_post_goes_to_login_and_back_to_the_open_section(
    pool: MySqlPool,
) {
    let f = copy_world(&pool).await;
    let mut anon = f.w.anon();
    accept_html(&mut anon);
    let before = dump(&pool).await;
    let reply = copy(&mut anon, f.t, f.s1, &[f.book]).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(login_url(&copy_open(f.t))));
    assert_eq!(dump(&pool).await, before);
    // JSON: Unauthorized.
    let mut anon = f.w.anon();
    assert_eq!(
        copy(&mut anon, f.t, f.s1, &[f.book]).await.wl_err(),
        WishlistError::Unauthorized
    );
    assert_eq!(dump(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac18_a_session_ended_in_another_tab_cannot_copy(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = f.w.bob().await;
    let mut other_tab = bob.fork();
    other_tab.call("logout", &[]).await;
    let before = dump(&pool).await;
    assert_eq!(
        copy(&mut bob, f.t, f.s1, &[f.book]).await.wl_err(),
        WishlistError::Unauthorized
    );
    assert_eq!(dump(&pool).await, before);
}

// ------------------------------------------------------------------ AC19, AC20: controls

#[sqlx::test(migrations = "../migrations")]
async fn ac19_select_all_is_rendered_hidden_without_a_name_once_per_usable_source(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let mut bob = ready(f.w.bob().await).await;
    let page = bob.get(&edit_path(f.t)).await;
    let sec = section_of(&page.body);
    assert_eq!(count(sec, &en("wishlist.copy.selectAll")), 2, "S1 and S2");
    let unnamed: Vec<&str> = tags(sec, "input")
        .into_iter()
        .filter(|t| t.contains("type=\"checkbox\"") && !t.contains("name="))
        .collect();
    assert_eq!(
        unnamed.len(),
        2,
        "one select-all checkbox per source, never submitted"
    );
    for tag in unnamed {
        assert!(has_attr(tag, "hidden"), "revealed by script only: {tag}");
    }
    // One submit button per source, labelled with the group name.
    let buttons: Vec<&str> = tags(sec, "button")
        .into_iter()
        .filter(|t| t.contains("type=\"submit\"") || !t.contains("type="))
        .collect();
    assert_eq!(buttons.len(), 2);
    assert!(buttons[0].contains("Family 2025") && buttons[1].contains("Office"));
    // Both forms post the hidden ids.
    for id in [f.s1, f.s2] {
        let hidden = format!("value=\"{id}\"");
        assert!(
            tags(sec, "input")
                .iter()
                .any(|t| t.contains("name=\"source_group_id\"") && t.contains(&hidden))
        );
    }
    assert!(
        tags(sec, "input")
            .iter()
            .any(|t| t.contains("name=\"group_id\"") && t.contains(&format!("value=\"{}\"", f.t)))
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac19_a_source_with_only_greyed_out_items_has_no_select_all_and_no_submit(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    // S2's only item is on the list; S1 keeps an enabled item.
    insert_item(&pool, f.t_list, "mug", None, None).await;
    let mut bob = ready(f.w.bob().await).await;
    let page = bob.get(&edit_path(f.t)).await;
    let sec = section_of(&page.body);
    assert_eq!(count(sec, &en("wishlist.copy.selectAll")), 1);
    assert_eq!(tags(sec, "button").len(), 1);
    assert!(has_item_box(&page.body, f.mug), "its rows are still shown");
    assert!(has_attr(item_box(&page.body, f.mug), "disabled"));
    // Everything greyed out: neither anywhere.
    for title in ["Book", "Tea"] {
        insert_item(&pool, f.t_list, title, None, None).await;
    }
    let page = bob.get(&edit_path(f.t)).await;
    let sec = section_of(&page.body);
    assert_eq!(count(sec, &en("wishlist.copy.selectAll")), 0);
    assert!(tags(sec, "button").is_empty());
    assert!(!sec.contains(&en("wishlist.copy.submit")));
    assert_eq!(count(sec, &en("wishlist.copy.alreadyOnList")), 4);
}

// ------------------------------------------------------------------ AC22, AC23: layout, safety

#[sqlx::test(migrations = "../migrations")]
async fn ac22_a_long_title_is_rendered_as_text_in_a_wrapping_element(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    let long = "W".repeat(255);
    let id = insert_item(&pool, f.s1_list, &long, Some("About it"), None).await;
    let mut bob = ready(f.w.bob().await).await;
    let page = bob.get(&copy_open(f.t)).await;
    assert_eq!(page.status, 200);
    let sec = section_of(&page.body);
    let at = pos(sec, &long);
    assert!(
        sec[at.saturating_sub(300)..at].contains("text-break"),
        "the title wraps (text-break)"
    );
    assert!(!has_attr(item_box(&page.body, id), "disabled"));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_user_text_in_the_section_is_escaped(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    insert_item(
        &pool,
        f.s1_list,
        "<script>alert(1)</script>",
        Some("<img src=x onerror=alert(1)>"),
        None,
    )
    .await;
    sqlx::query("UPDATE `groups` SET name = ? WHERE id = ?")
        .bind("<i>Office</i>")
        .bind(f.s2)
        .execute(&pool)
        .await
        .unwrap();
    let mut bob = ready(f.w.bob().await).await;
    for path in [edit_path(f.t), copy_open(f.t)] {
        let page = bob.get(&path).await;
        assert_eq!(page.status, 200);
        let sec = section_of(&page.body);
        assert!(!sec.contains("<script>alert(1)"), "no raw script in {path}");
        assert!(!sec.contains("<img src=x"), "no raw tag in {path}");
        assert!(!sec.contains("<i>Office</i>"));
        assert!(html_contains(sec, "<script>alert(1)</script>"));
        assert!(html_contains(sec, "<img src=x onerror=alert(1)>"));
        assert!(html_contains(sec, "<i>Office</i>"));
    }
}

// ------------------------------------------------------------------ AC24: i18n

fn catalog_of(locale: &str) -> std::collections::HashMap<String, String> {
    let path = format!("{}/../locales/{locale}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("catalog")).expect("catalog json")
}

#[test]
fn ac24_the_catalogs_have_the_new_and_reworded_texts_and_not_the_removed_keys() {
    let (en_cat, de_cat) = (catalog_of("en"), catalog_of("de"));
    let texts = [
        ("wishlist.copy.selectAll", "Select all", "Alle auswählen"),
        (
            "wishlist.copy.alreadyOnList",
            "Already on this list",
            "Schon auf dieser Liste",
        ),
        (
            "wishlist.copy.notCopyable",
            "Can't be copied as it is.",
            "Kann so nicht übernommen werden.",
        ),
        (
            "wishlist.copy.fixInSource",
            "Edit it in that list",
            "In dieser Liste bearbeiten",
        ),
        (
            "wishlist.copy.changed",
            "Some of the selected wishes no longer exist. Please check your selection.",
            "Einige der ausgewählten Wünsche gibt es nicht mehr. Bitte prüfen Sie Ihre Auswahl.",
        ),
        (
            "flash.success.wishes_copied.one",
            "{count} wish was copied.",
            "{count} Wunsch wurde übernommen.",
        ),
        (
            "flash.success.wishes_copied.other",
            "{count} wishes were copied.",
            "{count} Wünsche wurden übernommen.",
        ),
        (
            "validation.wishlist.already_on_list",
            "One of the selected wishes is already on this list.",
            "Einer der ausgewählten Wünsche steht schon auf dieser Liste.",
        ),
        (
            "validation.wishlist.item_not_copyable",
            "One of the selected wishes can't be copied as it is. Edit it in its list first.",
            "Einer der ausgewählten Wünsche kann so nicht übernommen werden. Bearbeiten Sie ihn zuerst in seiner Liste.",
        ),
        (
            "wishlist.copy.instructions",
            "Choose wishes from your other wishlists. They are added to the end of this list; the other lists stay unchanged.",
            "Wählen Sie Wünsche aus Ihren anderen Wunschlisten. Sie werden am Ende dieser Liste angefügt; die anderen Listen bleiben unverändert.",
        ),
        (
            "validation.wishlist.selection_required",
            "Select at least one wish to copy.",
            "Wählen Sie mindestens einen Wunsch aus.",
        ),
    ];
    for (key, en_text, de_text) in texts {
        assert_eq!(
            en_cat.get(key).map(String::as_str),
            Some(en_text),
            "en {key}"
        );
        assert_eq!(
            de_cat.get(key).map(String::as_str),
            Some(de_text),
            "de {key}"
        );
    }
    for key in [
        "wishlist.copy.title",
        "wishlist.copy.submit",
        "wishlist.edit.itemAction",
        "wishlist.panel.itemCount.one",
        "wishlist.panel.itemCount.other",
        "wishlist.view.lastUpdated",
        "validation.wishlist.too_many_items",
    ] {
        assert!(
            en_cat.contains_key(key) && de_cat.contains_key(key),
            "reused {key}"
        );
    }
    for key in [
        "wishlist.copy.source",
        "wishlist.copy.choose",
        "wishlist.copy.load",
        "flash.success.wishlist_copied",
    ] {
        assert!(
            !en_cat.contains_key(key) && !de_cat.contains_key(key),
            "removed {key}"
        );
    }
}

fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn ac24_nothing_references_the_removed_keys_or_the_removed_server_fn() {
    let mut files = Vec::new();
    for dir in ["app/src", "server/src", "frontend/src"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(dir);
        if path.exists() {
            rust_files(&path, &mut files);
        }
    }
    assert!(!files.is_empty());
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap();
        for needle in [
            "\"wishlist.copy.source\"",
            "\"wishlist.copy.choose\"",
            "\"wishlist.copy.load\"",
            "\"flash.success.wishlist_copied\"",
            "get_wishlist_sources",
        ] {
            assert!(
                !source.contains(needle),
                "{} still has {needle}",
                file.display()
            );
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac24_the_open_section_renders_without_raw_keys_in_both_locales(pool: MySqlPool) {
    let f = copy_world(&pool).await;
    insert_item(&pool, f.s1_list, "Old", None, Some("ftp://example.com/x")).await;
    let mut english = ready(f.w.bob().await).await;
    let mut german = ready(f.w.german(&f.w.emails[1], BOB_PASSWORD).await).await;
    for (client, lang) in [(&mut english, "en"), (&mut german, "de")] {
        let page = client.get(&copy_open(f.t)).await;
        assert_eq!(page.status, 200);
        assert_eq!(lang_of(&page.body), lang);
        assert_no_raw_keys(&page.body, "copy section");
        for prefix in [
            "wishlist.copy.",
            "flash.success.wishes_copied",
            "validation.wishlist.",
        ] {
            assert!(!page.body.contains(prefix), "{lang}: raw key {prefix}*");
        }
        let (title, hint, bad) = if lang == "en" {
            (
                en("wishlist.copy.title"),
                en("wishlist.copy.alreadyOnList"),
                en("wishlist.copy.notCopyable"),
            )
        } else {
            (
                de("wishlist.copy.title"),
                de("wishlist.copy.alreadyOnList"),
                de("wishlist.copy.notCopyable"),
            )
        };
        let sec = section_of(&page.body);
        assert!(html_contains(sec, &title) && html_contains(sec, &hint));
        assert!(html_contains(sec, &bad));
    }
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
