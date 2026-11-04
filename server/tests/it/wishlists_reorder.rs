//! wishlists slice: Move up / Move down (AC22, AC24, AC25, W7). The no-JS click is in
//! wishlists.spec.ts (AC23).

use crate::common::auth::*;
use crate::common::groups::*;
use crate::common::wishlists::*;
use app::wishlists_api::WishlistView;
use sqlx::MySqlPool;

async fn view_titles(c: &mut Client, g: i32, user: i32) -> Vec<String> {
    let view: WishlistView = get_wishlist(c, g, user).await.wl_value();
    view.items.into_iter().map(|i| i.title).collect()
}

// ------------------------------------------------------------------------ AC22

#[sqlx::test(migrations = "../migrations")]
async fn ac22_moving_c_up_gives_a_c_b_for_the_owner_and_for_others(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let mut carol = f.w.carol().await;
    up(&mut bob, f.c).await.wl_ok();
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_C, ITEM_B]
    );
    assert_eq!(
        view_titles(&mut bob, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_C, ITEM_B]
    );
    assert_eq!(
        view_titles(&mut carol, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_C, ITEM_B]
    );
    let page = carol.get(&view_path(f.w.g, f.w.bob)).await;
    assert!(pos(&page.body, ITEM_A) < pos(&page.body, ITEM_C));
    assert!(pos(&page.body, ITEM_C) < pos(&page.body, ITEM_B));
    assert_eq!(
        positions(&pool, f.w.g, f.w.bob).await,
        [1, 2, 3],
        "rewritten as 1..n"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_moving_the_first_up_or_the_last_down_is_a_successful_no_op(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    up(&mut bob, f.c).await.wl_ok(); // A, C, B
    let before = titles(&pool, f.w.g, f.w.bob).await;
    up(&mut bob, f.a).await.wl_ok();
    down(&mut bob, f.b).await.wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await, before);
    // A single item cannot move at all.
    let mut alice = f.w.alice().await;
    up(&mut alice, f.alice_item).await.wl_ok();
    down(&mut alice, f.alice_item).await.wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.alice).await, [ALICE_ITEM]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_down_and_up_swap_with_the_neighbour(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    down(&mut bob, f.a).await.wl_ok();
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_B, ITEM_A, ITEM_C]
    );
    up(&mut bob, f.a).await.wl_ok();
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_B, ITEM_C]
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_an_unknown_direction_is_refused_and_changes_nothing(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    for direction in ["", "up", "Sideways", "0", "Up; DROP TABLE wishlists"] {
        let r = move_item(&mut bob, f.b, direction).await;
        assert!(!r.status.is_success(), "{direction:?}");
    }
    assert_eq!(
        titles(&pool, f.w.g, f.w.bob).await,
        [ITEM_A, ITEM_B, ITEM_C]
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac22_the_first_row_has_no_move_up_and_the_last_no_move_down(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let page = bob.get(&edit_path(f.w.g)).await;
    assert_eq!(page.status, 200);
    let body = &page.body;
    let (a, b, c) = (pos(body, ITEM_A), pos(body, ITEM_B), pos(body, ITEM_C));
    assert!(a < b && b < c);
    let (move_up, move_down) = (en("wishlist.edit.moveUp"), en("wishlist.edit.moveDown"));
    let row_a = &body[a..b];
    let row_b = &body[b..c];
    let row_c = &body[c..];
    assert!(!row_a.contains(&move_up), "no Move up on the first row");
    assert!(row_a.contains(&move_down));
    assert!(row_b.contains(&move_up) && row_b.contains(&move_down));
    assert!(row_c.contains(&move_up));
    assert!(!row_c.contains(&move_down), "no Move down on the last row");
    // The buttons are plain forms (they work without JavaScript).
    assert!(row_b.contains("<form") && row_b.contains("method=\"post\""));
}

// ------------------------------------------------------------------------ AC24

#[sqlx::test(migrations = "../migrations")]
async fn ac24_reordering_in_g_does_not_touch_h_or_other_members(pool: MySqlPool) {
    let f = fixture(&pool).await;
    sqlx::query("INSERT INTO wishlist_items (wishlist_id, title) VALUES (?, 'X2'), (?, 'X3')")
        .bind(f.bob_list_h)
        .bind(f.bob_list_h)
        .execute(&pool)
        .await
        .unwrap();
    let mut bob = f.w.bob().await;
    let h_before = (
        titles(&pool, f.h, f.w.bob).await,
        positions(&pool, f.h, f.w.bob).await,
    );
    let alice_before = (
        titles(&pool, f.w.g, f.w.alice).await,
        positions(&pool, f.w.g, f.w.alice).await,
    );
    up(&mut bob, f.b).await.wl_ok();
    down(&mut bob, f.a).await.wl_ok();
    assert_eq!(
        (
            titles(&pool, f.h, f.w.bob).await,
            positions(&pool, f.h, f.w.bob).await
        ),
        h_before
    );
    assert_eq!(
        (
            titles(&pool, f.w.g, f.w.alice).await,
            positions(&pool, f.w.g, f.w.alice).await
        ),
        alice_before
    );
}

// ------------------------------------------------------------------------ AC25

/// Tiny deterministic generator (no extra dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac25_random_operation_sequences_keep_db_order_page_order_and_model_in_step(
    pool: MySqlPool,
) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let mut carol = f.w.carol().await;
    for seed in 1..=4u64 {
        // Every seed starts from Bob's empty list in G.
        clear_wishlist(&mut bob, f.w.g, true).await.wl_ok();
        let mut rng = Rng(seed * 7919);
        let mut model: Vec<(i32, String)> = vec![];
        let mut counter = 0;
        for step in 0..70 {
            match rng.next(4) {
                0 | 3 if model.len() < 3 || rng.next(3) == 0 && model.len() < 50 => {
                    counter += 1;
                    let title = format!("[s{seed}-{counter}]");
                    let id: i32 = add_item(&mut bob, f.w.g, &title, "", "").await.wl_value();
                    model.push((id, title));
                }
                1 if !model.is_empty() => {
                    let i = rng.next(model.len());
                    delete_item(&mut bob, model[i].0).await.wl_ok();
                    model.remove(i);
                }
                2 if !model.is_empty() => {
                    let i = rng.next(model.len());
                    up(&mut bob, model[i].0).await.wl_ok();
                    if i > 0 {
                        model.swap(i, i - 1);
                    }
                }
                _ if !model.is_empty() => {
                    let i = rng.next(model.len());
                    down(&mut bob, model[i].0).await.wl_ok();
                    if i + 1 < model.len() {
                        model.swap(i, i + 1);
                    }
                }
                _ => {}
            }
            let expected: Vec<&str> = model.iter().map(|(_, t)| t.as_str()).collect();
            let stored = titles(&pool, f.w.g, f.w.bob).await;
            assert_eq!(stored, expected, "seed {seed} step {step}: DB order");
            let pos = positions(&pool, f.w.g, f.w.bob).await;
            assert!(
                pos.windows(2).all(|w| w[0] < w[1]),
                "seed {seed} step {step}: positions not unique/increasing: {pos:?}"
            );
        }
        // Top up so every seed ends with a list long enough to compare orders.
        while model.len() < 3 {
            counter += 1;
            let title = format!("[s{seed}-{counter}]");
            let id: i32 = add_item(&mut bob, f.w.g, &title, "", "").await.wl_value();
            model.push((id, title));
        }
        // What the pages show is the same order.
        let expected: Vec<&str> = model.iter().map(|(_, t)| t.as_str()).collect();
        assert!(
            expected.len() >= 2,
            "seed {seed}: the sequence built a list"
        );
        assert_eq!(
            view_titles(&mut carol, f.w.g, f.w.bob).await,
            expected,
            "seed {seed}"
        );
        for (client, path) in [
            (&mut bob, edit_path(f.w.g)),
            (&mut carol, view_path(f.w.g, f.w.bob)),
        ] {
            let page = client.get(&path).await;
            let mut last = 0;
            for title in &expected {
                let at = page.body[last..].find(title).unwrap_or_else(|| {
                    panic!("seed {seed}: {title} missing/out of order on {path}")
                });
                last += at;
            }
        }
        // Other lists were never touched.
        assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X]);
        assert_eq!(titles(&pool, f.w.g, f.w.alice).await, [ALICE_ITEM]);
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac25_gaps_left_by_deletes_do_not_confuse_the_order(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    delete_item(&mut bob, f.b).await.wl_ok(); // positions 1, 3 (or renumbered)
    add_item(&mut bob, f.w.g, "D", "", "").await.wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await, [ITEM_A, ITEM_C, "D"]);
    up(&mut bob, f.c).await.wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await, [ITEM_C, ITEM_A, "D"]);
    let pos = positions(&pool, f.w.g, f.w.bob).await;
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "{pos:?}");
}
