//! wishlists slice: concurrency and membership interplay (AC14 under load, AC30, AC31,
//! W11). The group row is the lock: wishlist writes take it in shared mode, memberships
//! changes (leave, remove, join, draw) take it `FOR UPDATE`; the wishlist row serialises
//! the owner's own writes.

use std::time::Duration;

use crate::common::auth::*;
use crate::common::groups::*;
use crate::common::wishlists::*;
use app::server::membership::remove_member_data;
use app::test_support as ts;
use app::wishlists_api::{WishlistError, WishlistField as Fld, WishlistFieldError as Err_};
use sqlx::{MySql, MySqlPool, Transaction};
use tokio::task::JoinHandle;

async fn hold_lock(pool: &MySqlPool, group: i32) -> Transaction<'static, MySql> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM `groups` WHERE id = ? FOR UPDATE")
        .bind(group)
        .fetch_one(&mut *tx)
        .await
        .expect("lock the group row");
    tx
}

async fn assert_waiting<T>(handle: &JoinHandle<T>, what: &str) {
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !handle.is_finished(),
        "{what} must wait for the group row lock"
    );
}

/// What `remove_member` does under the lock.
async fn remove_under_lock(tx: &mut Transaction<'static, MySql>, group: i32, user: i32) {
    remove_member_data(tx, group, user)
        .await
        .expect("remove_member_data");
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(group)
        .bind(user)
        .execute(&mut **tx)
        .await
        .unwrap();
}

// ------------------------------------------------------------------------ AC31

#[sqlx::test(migrations = "../migrations")]
async fn ac31_add_item_waits_for_a_removal_and_then_finds_the_member_gone(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let mut tx = hold_lock(&pool, f.w.g).await;

    let g = f.w.g;
    let add = tokio::spawn(async move { add_item(&mut carol, g, "Late", "", "").await });
    assert_waiting(&add, "add_item").await;

    remove_under_lock(&mut tx, f.w.g, f.w.carol).await;
    tx.commit().await.unwrap();

    let reply = add.await.unwrap();
    assert_eq!(reply.wl_err(), WishlistError::NotFound);
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 0);
    assert_eq!(ts::member_ids(&pool, f.w.g).await, vec![f.w.alice, f.w.bob]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_add_item_that_waits_for_an_unrelated_lock_succeeds_afterwards(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut carol = f.w.carol().await;
    let tx = hold_lock(&pool, f.w.g).await;
    let g = f.w.g;
    let add = tokio::spawn(async move { add_item(&mut carol, g, "Patient", "", "").await });
    assert_waiting(&add, "add_item").await;
    tx.commit().await.unwrap();
    add.await.unwrap().wl_ok();
    assert_eq!(titles(&pool, f.w.g, f.w.carol).await, ["Patient"]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_every_wishlist_write_waits_and_then_sees_the_removal(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let before_alice = titles(&pool, f.w.g, f.w.alice).await;
    let mut tx = hold_lock(&pool, f.w.g).await;
    let (g, a, b, c) = (f.w.g, f.a, f.b, f.c);
    let mut handles: Vec<(&str, JoinHandle<Reply>)> = vec![];
    let mut bob = f.w.bob().await;
    handles.push((
        "add_item",
        tokio::spawn(async move { add_item(&mut bob, g, "Late", "", "").await }),
    ));
    let mut bob = f.w.bob().await;
    handles.push((
        "update_item",
        tokio::spawn(async move { update_item(&mut bob, a, "Late", "", "").await }),
    ));
    let mut bob = f.w.bob().await;
    handles.push((
        "delete_item",
        tokio::spawn(async move { delete_item(&mut bob, b).await }),
    ));
    let mut bob = f.w.bob().await;
    handles.push((
        "move_item",
        tokio::spawn(async move { up(&mut bob, c).await }),
    ));
    let mut bob = f.w.bob().await;
    handles.push((
        "clear_wishlist",
        tokio::spawn(async move { clear_wishlist(&mut bob, g, true).await }),
    ));
    for (name, handle) in &handles {
        assert_waiting(handle, name).await;
    }

    remove_under_lock(&mut tx, f.w.g, f.w.bob).await;
    tx.commit().await.unwrap();

    for (name, handle) in handles {
        let reply = handle.await.unwrap();
        assert_eq!(reply.wl_err(), WishlistError::NotFound, "{name}");
    }
    assert_eq!(
        wishlist_rows(&pool, f.w.g, f.w.bob).await,
        0,
        "no list left behind for a non-member"
    );
    assert_eq!(
        total_items(&pool).await,
        2,
        "only Alice's item and Bob's item in H"
    );
    assert_eq!(titles(&pool, f.w.g, f.w.alice).await, before_alice);
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac31_a_write_that_races_with_a_group_delete_fails_cleanly(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let mut tx = hold_lock(&pool, f.w.g).await;
    let g = f.w.g;
    let add = tokio::spawn(async move { add_item(&mut bob, g, "Late", "", "").await });
    assert_waiting(&add, "add_item").await;
    sqlx::query("DELETE FROM `groups` WHERE id = ?")
        .bind(f.w.g)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let reply = add.await.unwrap();
    assert_ne!(reply.status, 500, "{}", reply.body);
    assert_eq!(reply.wl_err(), WishlistError::NotFound);
    assert_eq!(total_items(&pool).await, 1, "only the item in H");
}

// ------------------------------------------------------------ shared lock mode

#[sqlx::test(migrations = "../migrations")]
async fn w11_wishlist_writes_take_the_group_row_in_shared_mode_and_do_not_queue_each_other(
    pool: MySqlPool,
) {
    let f = fixture(&pool).await;
    // Another shared holder (e.g. a second wishlist write in flight) must not block us ...
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM `groups` WHERE id = ? LOCK IN SHARE MODE")
        .bind(f.w.g)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let mut carol = f.w.carol().await;
    let mut bob = f.w.bob().await;
    let g = f.w.g;
    let done = tokio::time::timeout(Duration::from_secs(10), async {
        add_item(&mut carol, g, "Parallel", "", "").await.wl_ok();
        up(&mut bob, f.c).await.wl_ok();
    })
    .await;
    assert!(
        done.is_ok(),
        "wishlist writes must not wait for a shared holder"
    );
    // ... but a FOR UPDATE holder (a draw or a leave) is excluded by a running write: the
    // exclusive lock request waits while the shared one is held.
    let pool2 = pool.clone();
    let exclusive = tokio::spawn(async move {
        let mut t = pool2.begin().await.unwrap();
        sqlx::query("SELECT id FROM `groups` WHERE id = ? FOR UPDATE")
            .bind(g)
            .fetch_one(&mut *t)
            .await
            .unwrap();
        t.commit().await.unwrap();
    });
    assert_waiting(&exclusive, "a FOR UPDATE on the group").await;
    tx.commit().await.unwrap();
    exclusive.await.unwrap();
}

// -------------------------------------------------------- the item limit under load

#[sqlx::test(migrations = "../migrations")]
async fn ac14_concurrent_adds_cannot_exceed_50_items(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    for i in 4..=48 {
        add_item(&mut bob, f.w.g, &format!("Item {i}"), "", "")
            .await
            .wl_ok();
    }
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await.len(), 48);

    let g = f.w.g;
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let mut tab = bob.fork();
            tokio::spawn(async move { add_item(&mut tab, g, &format!("Race {i}"), "", "").await })
        })
        .collect();
    let mut ok = 0;
    let mut refused = 0;
    for handle in handles {
        let reply = handle.await.unwrap();
        if reply.status.is_success() {
            ok += 1;
        } else {
            assert_eq!(
                reply.wl_err(),
                WishlistError::Validation(vec![(Fld::Items, Err_::TooManyItems)])
            );
            refused += 1;
        }
    }
    assert_eq!((ok, refused), (2, 4), "exactly the two free slots are used");
    assert_eq!(titles(&pool, f.w.g, f.w.bob).await.len(), 50);
    let pos = positions(&pool, f.w.g, f.w.bob).await;
    assert!(
        pos.windows(2).all(|w| w[0] < w[1]),
        "unique positions: {pos:?}"
    );
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.bob).await, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn first_adds_in_two_tabs_create_one_list_and_distinct_positions(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let carol = f.w.carol().await;
    let g = f.w.g;
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let mut tab = carol.fork();
            tokio::spawn(async move { add_item(&mut tab, g, &format!("T{i}"), "", "").await })
        })
        .collect();
    for h in handles {
        h.await.unwrap().wl_ok();
    }
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.carol).await, 1);
    let pos = positions(&pool, f.w.g, f.w.carol).await;
    assert_eq!(pos.len(), 5);
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "{pos:?}");
}

// ------------------------------------------------------- reorder under concurrency

#[sqlx::test(migrations = "../migrations")]
async fn concurrent_moves_deletes_and_adds_leave_a_consistent_list(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let bob = f.w.bob().await;
    let mut tab = bob.fork();
    let mut ids = vec![f.a, f.b, f.c];
    for i in 4..=8 {
        ids.push(
            add_item(&mut tab, f.w.g, &format!("Item {i}"), "", "")
                .await
                .wl_value::<i32>(),
        );
    }
    let g = f.w.g;
    let mut handles: Vec<JoinHandle<Reply>> = vec![];
    for (n, id) in ids.iter().copied().enumerate() {
        let mut t = bob.fork();
        handles.push(tokio::spawn(async move {
            match n % 4 {
                0 => up(&mut t, id).await,
                1 => down(&mut t, id).await,
                2 => up(&mut t, id).await,
                _ => down(&mut t, id).await,
            }
        }));
    }
    let deleted = ids[2];
    let mut t = bob.fork();
    handles.push(tokio::spawn(
        async move { delete_item(&mut t, deleted).await },
    ));
    let mut t = bob.fork();
    handles.push(tokio::spawn(async move {
        add_item(&mut t, g, "Concurrent add", "", "").await
    }));
    for (index, h) in handles.into_iter().enumerate() {
        let reply = tokio::time::timeout(Duration::from_secs(30), h)
            .await
            .expect("no deadlock")
            .unwrap();
        if index == 2 && reply.status == 404 {
            // The move of the item that the concurrent delete removed first.
            assert_eq!(reply.wl_err(), WishlistError::NotFound);
            continue;
        }
        assert!(
            reply.status.is_success(),
            "{}: {}",
            reply.status,
            reply.body
        );
    }
    let mut got = titles(&pool, f.w.g, f.w.bob).await;
    got.sort();
    let mut expected: Vec<String> = [
        ITEM_A,
        ITEM_B,
        "Item 4",
        "Item 5",
        "Item 6",
        "Item 7",
        "Item 8",
        "Concurrent add",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    expected.sort();
    assert_eq!(got, expected, "no item lost or duplicated");
    let pos = positions(&pool, f.w.g, f.w.bob).await;
    assert!(
        pos.windows(2).all(|w| w[0] < w[1]),
        "positions unique: {pos:?}"
    );
}

// ----------------------------------------------------- AC30 and lock-order (no deadlock)

#[sqlx::test(migrations = "../migrations")]
async fn ac30_leaving_deletes_the_list_and_the_page_becomes_404_for_others(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut bob = f.w.bob().await;
    let mut alice = f.w.alice().await;
    assert_eq!(
        alice.get(&view_path(f.w.g, f.w.bob)).await.status,
        200,
        "control"
    );
    leave_group(&mut bob, f.w.g).await.group_ok();

    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.bob).await, 0);
    assert!(titles(&pool, f.w.g, f.w.bob).await.is_empty());
    for item in [f.a, f.b, f.c] {
        assert!(item_row(&pool, item).await.is_none());
    }
    assert_eq!(titles(&pool, f.h, f.w.bob).await, [ITEM_X], "H is intact");
    assert_eq!(alice.get(&view_path(f.w.g, f.w.bob)).await.status, 404);
    assert_eq!(
        get_wishlist(&mut alice, f.w.g, f.w.bob).await.wl_err(),
        WishlistError::NotFound
    );
    // Bob himself is out: the pages are 404 for him too.
    assert_eq!(bob.get(&edit_path(f.w.g)).await.status, 404);
    // Alice's list is untouched.
    assert_eq!(titles(&pool, f.w.g, f.w.alice).await, [ALICE_ITEM]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac30_removing_a_member_deletes_the_list_too(pool: MySqlPool) {
    let f = fixture(&pool).await;
    let mut alice = f.w.alice().await;
    assert_eq!(
        alice.get(&view_path(f.w.g, f.w.bob)).await.status,
        200,
        "control"
    );
    remove_member(&mut alice, f.w.g, f.w.bob).await.group_ok();
    assert_eq!(wishlist_rows(&pool, f.w.g, f.w.bob).await, 0);
    assert_eq!(total_items(&pool).await, 2);
    assert_eq!(alice.get(&view_path(f.w.g, f.w.bob)).await.status, 404);
}

#[sqlx::test(migrations = "../migrations")]
async fn wishlist_writes_racing_with_leave_and_remove_neither_deadlock_nor_leave_orphans(
    pool: MySqlPool,
) {
    for round in 0..3 {
        let f = fixture(&pool).await;
        let carol = f.w.carol().await;
        let mut handles: Vec<JoinHandle<Reply>> = vec![];
        let g = f.w.g;
        for i in 0..4 {
            let mut t = carol.fork();
            handles.push(tokio::spawn(async move {
                add_item(&mut t, g, &format!("r{round}-{i}"), "", "").await
            }));
        }
        let mut t = carol.fork();
        handles.push(tokio::spawn(async move {
            clear_wishlist(&mut t, g, true).await
        }));
        let mut t = carol.fork();
        handles.push(tokio::spawn(async move { leave_group(&mut t, g).await }));
        let mut alice = f.w.alice().await;
        let carol_id = f.w.carol;
        handles.push(tokio::spawn(async move {
            remove_member(&mut alice, g, carol_id).await
        }));

        for h in handles {
            let reply = tokio::time::timeout(Duration::from_secs(30), h)
                .await
                .expect("deadlock between wishlist writes and membership changes")
                .unwrap();
            assert_ne!(reply.status, 500, "round {round}: {}", reply.body);
        }
        assert!(!ts::member_ids(&pool, g).await.contains(&f.w.carol));
        assert_eq!(
            wishlist_rows(&pool, g, f.w.carol).await,
            0,
            "round {round}: orphaned list"
        );
        let orphans: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM wishlists w WHERE NOT EXISTS \
             (SELECT 1 FROM group_members m WHERE m.group_id = w.group_id AND m.user_id = w.user_id)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(orphans, 0, "round {round}");
        // Next round gets a clean slate (the fixture uses fixed invitation codes).
        for group in [f.w.g, f.h] {
            sqlx::query("DELETE FROM `groups` WHERE id = ?")
                .bind(group)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
}

// ------------------------------------------- many members' first adds at the same moment

/// Creates `n` extra members in `group` and returns their ids and emails.
async fn extra_members(pool: &MySqlPool, group: i32, n: usize) -> Vec<(i32, String)> {
    let mut out = vec![];
    for i in 0..n {
        let email = app::test_support::mailpit::unique_email(&format!("burst{i}"));
        let id = ts::create_user(pool, &format!("Burst {i}"), &email).await;
        ts::add_member(pool, group, id).await;
        out.push((id, email));
    }
    out
}

async fn burst(pool: &MySqlPool, w: &W, groups: &[i32], members: &[Vec<(i32, String)>]) {
    let mut clients = vec![];
    for (group, list) in groups.iter().zip(members) {
        for (user, email) in list {
            let mut c = w.anon();
            c.login(email, ts::TEST_PASSWORD).await.ok();
            clients.push((*group, *user, c));
        }
    }
    // Everyone fires without any await in between.
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(clients.len()));
    let mut set = tokio::task::JoinSet::new();
    for (group, user, mut c) in clients {
        let barrier = barrier.clone();
        set.spawn(async move {
            barrier.wait().await;
            let reply = add_item(&mut c, group, "First", "", "").await;
            (group, user, reply)
        });
    }
    while let Some(done) = set.join_next().await {
        let (group, user, reply) = done.unwrap();
        assert!(
            reply.status.is_success(),
            "group {group} user {user}: {} {}",
            reply.status,
            reply.body
        );
    }
    for (group, list) in groups.iter().zip(members) {
        for (user, _) in list {
            assert_eq!(wishlist_rows(pool, *group, *user).await, 1, "one list");
            assert_eq!(titles(pool, *group, *user).await, ["First"]);
            assert_eq!(positions(pool, *group, *user).await, [1]);
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn simultaneous_first_adds_of_many_members_all_succeed(pool: MySqlPool) {
    let f = fixture(&pool).await;
    for round in 0..5 {
        let g =
            ts::create_group_with_code(&pool, f.w.alice, "Burst", &format!("BRST{round}234")).await;
        let members = extra_members(&pool, g, 8).await;
        burst(&pool, &f.w, &[g], &[members]).await;
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn simultaneous_first_adds_in_several_groups_at_once_all_succeed(pool: MySqlPool) {
    // Fresh DB for the lists: the new rows are the highest wishlist ids, next to each other.
    let f = fixture(&pool).await;
    let mut groups = vec![];
    let mut members = vec![];
    for i in 0..5 {
        let g = ts::create_group_with_code(&pool, f.w.alice, "Multi", &format!("MULT{i}234")).await;
        members.push(extra_members(&pool, g, 8).await);
        groups.push(g);
    }
    burst(&pool, &f.w, &groups, &members).await;
}
