//! exclusions slice: adding and removing rules (AC3, AC6-AC14, AC17, AC18) and the plain
//! (no-JS) form posts. The spec's G is `x.g()` (Alice admin; Alice, Bob, Carol, Dave).

use crate::common::auth::*;
use crate::common::exclusions::*;
use crate::common::groups::*;
use app::exclusions_api::{ExclusionError, MAX_EXCLUSIONS_PER_MEMBER};
use app::test_support as ts;
use sqlx::MySqlPool;

fn accept_html(c: &mut Client) {
    c.headers.push(("accept".into(), "text/html".into()));
}

// ------------------------------------------------------------------------ AC3

#[sqlx::test(migrations = "../migrations")]
async fn ac3_adding_a_rule_creates_it_and_flashes_and_updates_the_lists(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    bob.session_info().await; // drop the login flash

    add_ex(&mut bob, x.g(), x.carol()).await.ex_ok();

    assert_eq!(rules(&pool, x.g()).await, [(x.bob(), x.carol())]);
    assert!(has_flash_key(
        &bob.session_info().await,
        "flash.success.exclusion_added"
    ));

    let p = page(&mut bob, x.g()).await;
    assert_eq!(p.my_rules.len(), 1);
    assert_eq!(
        (p.my_rules[0].user_id, p.my_rules[0].name.as_str()),
        (x.carol(), "Carol")
    );
    let candidates: Vec<i32> = p.candidates.iter().map(|m| m.user_id).collect();
    assert_eq!(candidates, [x.alice(), x.dave()]);

    let html = bob.get(&ex_path(x.g())).await;
    assert_eq!(html.status, 200);
    let options = select_options(&html.body);
    assert_eq!(
        options.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(),
        ["Alice", "Dave"]
    );
    assert!(html.body.contains("Carol"), "Carol is in the Current list");
    assert!(!html_contains(&html.body, &en("exclusion.current.empty")));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac3_the_admin_adds_rules_like_any_member(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut alice = x.w.alice().await;
    add_ex(&mut alice, x.g(), x.dave()).await.ex_ok();
    assert_eq!(rules(&pool, x.g()).await, [(x.alice(), x.dave())]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac3_a_plain_form_post_ends_on_the_page_with_the_flash(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    accept_html(&mut bob);
    let reply = add_ex(&mut bob, x.g(), x.carol()).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(ex_path(x.g())));
    assert_eq!(rules(&pool, x.g()).await, [(x.bob(), x.carol())]);
    let html = bob.get(&ex_path(x.g())).await;
    assert_eq!(html.status, 200);
    assert!(html_contains(
        &html.body,
        &en("flash.success.exclusion_added")
    ));
    assert!(html.body.contains("Carol"));
}

// ------------------------------------------------------------------------ AC6

#[sqlx::test(migrations = "../migrations")]
async fn ac6_excluding_yourself_is_refused(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    assert_eq!(
        add_ex(&mut bob, x.g(), x.bob()).await.ex_err(),
        ExclusionError::CannotExcludeSelf
    );
    assert!(all_rules(&pool).await.is_empty());
}

// ------------------------------------------------------------------------ AC7

#[sqlx::test(migrations = "../migrations")]
async fn ac7_targets_that_are_no_member_are_refused(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut bob = x.w.bob().await;
    for target in [x.eve, 0, -1, 999_999] {
        assert_eq!(
            add_ex(&mut bob, x.g(), target).await.ex_err(),
            ExclusionError::TargetNotMember,
            "target {target}"
        );
    }
    assert!(all_rules(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac7_a_member_of_another_group_only_is_no_target(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let other = x.group_of("Other", &[x.eve], &pool).await;
    ts::add_member(&pool, other, x.bob()).await;
    let mut bob = x.w.bob().await;
    assert_eq!(
        add_ex(&mut bob, x.g(), x.eve).await.ex_err(),
        ExclusionError::TargetNotMember
    );
    // In the other group Eve is a valid target.
    add_ex(&mut bob, other, x.eve).await.ex_ok();
    assert_eq!(rules(&pool, other).await, [(x.bob(), x.eve)]);
    assert!(rules(&pool, x.g()).await.is_empty());
}

// ------------------------------------------------------------------------ AC8

#[sqlx::test(migrations = "../migrations")]
async fn ac8_adding_an_existing_rule_again_is_ok_and_creates_no_second_row(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let mut bob = x.w.bob().await;
    add_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
    add_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
    assert_eq!(rules(&pool, x.g()).await, [(x.bob(), x.carol())]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac8_a_repeated_add_does_not_count_towards_the_cap(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let (g, extra) = x.g8(&pool).await;
    for target in &extra[..4] {
        ts::add_exclusion(&pool, g, x.bob(), *target).await;
    }
    let mut bob = x.w.bob().await;
    // The 5th rule through the fn, then the same call twice more: still one row, no error.
    add_ex(&mut bob, g, extra[4]).await.ex_ok();
    add_ex(&mut bob, g, extra[4]).await.ex_ok();
    add_ex(&mut bob, g, extra[0]).await.ex_ok();
    assert_eq!(
        rules(&pool, g)
            .await
            .iter()
            .filter(|r| r.0 == x.bob())
            .count(),
        5
    );
}

// ------------------------------------------------------------------------ AC9

#[sqlx::test(migrations = "../migrations")]
async fn ac9_the_sixth_rule_is_refused_and_removing_one_makes_room(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let (g, extra) = x.g8(&pool).await;
    for target in &extra[..5] {
        ts::add_exclusion(&pool, g, x.bob(), *target).await;
    }
    let mut bob = x.w.bob().await;
    assert_eq!(MAX_EXCLUSIONS_PER_MEMBER, 5);

    let p = page(&mut bob, g).await;
    assert_eq!((p.my_rules.len(), p.max_rules), (5, 5));
    assert!(!p.can_add, "at the cap");
    assert!(p.notice.is_none(), "the group is feasible");

    assert_eq!(
        add_ex(&mut bob, g, extra[5]).await.ex_err(),
        ExclusionError::TooManyExclusions { max: 5 }
    );
    assert_eq!(
        rules(&pool, g)
            .await
            .iter()
            .filter(|r| r.0 == x.bob())
            .count(),
        5
    );

    let html = bob.get(&ex_path(g)).await;
    assert_eq!(html.status, 200);
    assert!(html_contains(
        &html.body,
        &en_with("exclusion.addNew.limitReached", &[("max", "5")])
    ));
    assert!(select_options(&html.body).is_empty(), "no form at the cap");
    assert!(!html_contains(&html.body, &en("exclusion.addNew.submit")));

    remove_ex(&mut bob, g, extra[0]).await.ex_ok();
    let p = page(&mut bob, g).await;
    assert!(p.can_add);
    add_ex(&mut bob, g, extra[5]).await.ex_ok();
    assert_eq!(
        rules(&pool, g)
            .await
            .iter()
            .filter(|r| r.0 == x.bob())
            .count(),
        5
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_the_cap_is_per_member_and_per_group(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let (g, extra) = x.g8(&pool).await;
    for target in &extra[..5] {
        ts::add_exclusion(&pool, g, x.bob(), *target).await;
    }
    // Alice has no rules in the same group; Bob has room in G.
    let mut alice = x.w.alice().await;
    add_ex(&mut alice, g, extra[0]).await.ex_ok();
    let mut bob = x.w.bob().await;
    add_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac9_checks_run_in_the_specified_order(pool: MySqlPool) {
    // Specified order: group lock, member, drawn, self, target, duplicate, cap, feasibility.
    let x = x_world(&pool).await;
    let (g, extra) = x.g8(&pool).await;
    // Bob has 5 rules; M6 may only draw Alice and M1..M5 may not draw Alice, so Bob adding
    // M6 would force Bob and M6 onto Alice.
    for target in &extra[..5] {
        ts::add_exclusion(&pool, g, x.bob(), *target).await;
        ts::add_exclusion(&pool, g, *target, x.alice()).await;
    }
    for target in std::iter::once(x.bob()).chain(extra[..5].iter().copied()) {
        ts::add_exclusion(&pool, g, extra[5], target).await;
    }
    let mut bob = x.w.bob().await;
    // Cap before feasibility.
    assert_eq!(
        add_ex(&mut bob, g, extra[5]).await.ex_err(),
        ExclusionError::TooManyExclusions { max: 5 }
    );
    // Self and target before cap; duplicate before cap.
    assert_eq!(
        add_ex(&mut bob, g, x.bob()).await.ex_err(),
        ExclusionError::CannotExcludeSelf
    );
    assert_eq!(
        add_ex(&mut bob, g, x.eve).await.ex_err(),
        ExclusionError::TargetNotMember
    );
    add_ex(&mut bob, g, extra[0]).await.ex_ok();
    // Drawn before everything else.
    ts::set_drawn(&pool, g).await;
    for target in [x.bob(), x.eve, extra[0], extra[5]] {
        assert_eq!(
            add_ex(&mut bob, g, target).await.ex_err(),
            ExclusionError::AlreadyDrawn,
            "target {target}"
        );
    }
}

// ------------------------------------------------------------------- AC10-AC12

#[sqlx::test(migrations = "../migrations")]
async fn ac10_a_rule_that_leaves_a_member_without_a_giver_is_refused(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.dave()).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.dave()).await;
    let mut carol = x.w.carol().await;
    assert_eq!(
        add_ex(&mut carol, x.g(), x.dave()).await.ex_err(),
        ExclusionError::WouldMakeDrawImpossible
    );
    assert_eq!(
        rules(&pool, x.g()).await,
        [(x.alice(), x.dave()), (x.bob(), x.dave())]
    );
    add_ex(&mut carol, x.g(), x.bob()).await.ex_ok();
    assert!(rules(&pool, x.g()).await.contains(&(x.carol(), x.bob())));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac11_a_rule_that_leaves_a_member_nobody_to_draw_is_refused(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let g3 = x.g3(&pool).await;
    ts::add_exclusion(&pool, g3, x.bob(), x.alice()).await;
    let mut bob = x.w.bob().await;
    assert_eq!(
        add_ex(&mut bob, g3, x.carol()).await.ex_err(),
        ExclusionError::WouldMakeDrawImpossible
    );
    assert_eq!(rules(&pool, g3).await, [(x.bob(), x.alice())]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac12_in_a_two_member_group_no_rule_is_possible(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let g2 = x.g2(&pool).await;
    let mut alice = x.w.alice().await;
    let p = page(&mut alice, g2).await;
    assert!(p.notice.is_none(), "reciprocal pairs are allowed: feasible");
    assert_eq!(
        add_ex(&mut alice, g2, x.bob()).await.ex_err(),
        ExclusionError::WouldMakeDrawImpossible
    );
    let mut bob = x.w.bob().await;
    assert_eq!(
        add_ex(&mut bob, g2, x.alice()).await.ex_err(),
        ExclusionError::WouldMakeDrawImpossible
    );
    assert!(all_rules(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac10_a_refused_rule_is_not_a_hidden_success_on_a_plain_post(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.dave()).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.dave()).await;
    let mut carol = x.w.carol().await;
    accept_html(&mut carol);
    let reply = add_ex(&mut carol, x.g(), x.dave()).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(ex_path(x.g())));
    let html = carol.get(&ex_path(x.g())).await;
    assert!(html_contains(
        &html.body,
        &en("flash.error.exclusion_would_block_draw")
    ));
    assert!(!html_contains(
        &html.body,
        &en("flash.success.exclusion_added")
    ));
    assert_eq!(rules(&pool, x.g()).await.len(), 2);
}

// ------------------------------------------------------------------------ AC13

#[sqlx::test(migrations = "../migrations")]
async fn ac13_removing_a_rule_deletes_it_flashes_and_frees_the_candidate(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let mut bob = x.w.bob().await;
    bob.session_info().await;
    remove_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
    assert!(rules(&pool, x.g()).await.is_empty());
    assert!(has_flash_key(
        &bob.session_info().await,
        "flash.success.exclusion_removed"
    ));
    let p = page(&mut bob, x.g()).await;
    assert!(p.my_rules.is_empty());
    assert!(p.candidates.iter().any(|m| m.user_id == x.carol()));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac13_a_plain_form_post_of_remove_ends_on_the_page(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let mut bob = x.w.bob().await;
    accept_html(&mut bob);
    let reply = remove_ex(&mut bob, x.g(), x.carol()).await;
    assert_eq!(reply.status, 302, "{}", reply.body);
    assert_eq!(reply.location(), Some(ex_path(x.g())));
    assert!(rules(&pool, x.g()).await.is_empty());
    let html = bob.get(&ex_path(x.g())).await;
    assert!(html_contains(
        &html.body,
        &en("flash.success.exclusion_removed")
    ));
    assert!(html_contains(&html.body, &en("exclusion.current.empty")));
}

// ------------------------------------------------------------------------ AC14

#[sqlx::test(migrations = "../migrations")]
async fn ac14_remove_only_ever_touches_the_callers_own_rule(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.carol()).await;
    let mut bob = x.w.bob().await;
    remove_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
    remove_ex(&mut bob, x.g(), x.eve).await.ex_ok();
    remove_ex(&mut bob, x.g(), 0).await.ex_ok();
    remove_ex(&mut bob, x.g(), 999_999).await.ex_ok();
    assert_eq!(rules(&pool, x.g()).await, [(x.alice(), x.carol())]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_removing_twice_is_idempotent_and_removing_a_former_member_works(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    let mut bob = x.w.bob().await;
    remove_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
    remove_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
    assert!(rules(&pool, x.g()).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn ac14_posted_giver_ids_are_ignored(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.alice(), x.carol()).await;
    let mut bob = x.w.bob().await;
    let (g, a, c) = (
        x.g().to_string(),
        x.alice().to_string(),
        x.carol().to_string(),
    );
    for field in ["user_id", "giver_id", "giver", "me"] {
        // Remove "as Alice": Alice's rule stays.
        bob.call(
            "remove_exclusion",
            &[("group_id", &g), ("excluded_user_id", &c), (field, &a)],
        )
        .await;
        assert_eq!(
            rules(&pool, x.g()).await,
            [(x.alice(), x.carol())],
            "{field}"
        );
        // Add "as Alice": the rule is Bob's own.
        bob.call(
            "add_exclusion",
            &[("group_id", &g), ("excluded_user_id", &c), (field, &a)],
        )
        .await;
        assert_eq!(
            rules(&pool, x.g()).await,
            [(x.alice(), x.carol()), (x.bob(), x.carol())],
            "{field}"
        );
        remove_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
    }
}

// ------------------------------------------------------------------------ AC17

#[sqlx::test(migrations = "../migrations")]
async fn ac17_after_the_draw_neither_add_nor_remove_works(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::set_drawn(&pool, x.g()).await;
    let mut bob = x.w.bob().await;
    assert_eq!(
        add_ex(&mut bob, x.g(), x.dave()).await.ex_err(),
        ExclusionError::AlreadyDrawn
    );
    assert_eq!(
        remove_ex(&mut bob, x.g(), x.carol()).await.ex_err(),
        ExclusionError::AlreadyDrawn
    );
    assert_eq!(rules(&pool, x.g()).await, [(x.bob(), x.carol())]);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac17_drawn_is_reported_by_a_plain_post_as_a_locked_flash(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::set_drawn(&pool, x.g()).await;
    let mut bob = x.w.bob().await;
    accept_html(&mut bob);
    let reply = remove_ex(&mut bob, x.g(), x.carol()).await;
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location(), Some(ex_path(x.g())));
    let html = bob.get(&ex_path(x.g())).await;
    assert!(html_contains(
        &html.body,
        &en("flash.error.exclusions_locked")
    ));
    assert_eq!(rules(&pool, x.g()).await, [(x.bob(), x.carol())]);
}

// ------------------------------------------------------------------------ AC18

#[sqlx::test(migrations = "../migrations")]
async fn ac18_rules_survive_a_reset_and_are_editable_again(pool: MySqlPool) {
    let x = x_world(&pool).await;
    ts::add_exclusion(&pool, x.g(), x.bob(), x.carol()).await;
    ts::set_drawn(&pool, x.g()).await;
    let mut bob = x.w.bob().await;
    assert_eq!(
        remove_ex(&mut bob, x.g(), x.carol()).await.ex_err(),
        ExclusionError::AlreadyDrawn
    );
    unset_drawn(&pool, x.g()).await;
    assert_eq!(rules(&pool, x.g()).await, [(x.bob(), x.carol())]);
    let p = page(&mut bob, x.g()).await;
    assert_eq!(p.my_rules.len(), 1);
    assert!(!p.is_drawn);
    remove_ex(&mut bob, x.g(), x.carol()).await.ex_ok();
    assert!(rules(&pool, x.g()).await.is_empty());
    add_ex(&mut bob, x.g(), x.dave()).await.ex_ok();
}

// ------------------------------------------------------- plain-post error flashes

#[sqlx::test(migrations = "../migrations")]
async fn plain_post_errors_come_back_as_flashes_on_the_page(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let (g8, extra) = x.g8(&pool).await;
    for target in &extra[..5] {
        ts::add_exclusion(&pool, g8, x.bob(), *target).await;
    }
    let mut bob = x.w.bob().await;
    accept_html(&mut bob);

    let cases: Vec<(i32, i32, String)> = vec![
        (x.g(), x.bob(), en("flash.error.exclusion_self")),
        (x.g(), x.eve, en("flash.error.user_not_in_group")),
        (
            g8,
            extra[5],
            en_with("flash.error.exclusion_limit_reached", &[("max", "5")]),
        ),
    ];
    for (group, target, expected) in cases {
        let reply = add_ex(&mut bob, group, target).await;
        assert_eq!(reply.status, 302, "{expected}");
        assert_eq!(reply.location(), Some(ex_path(group)), "{expected}");
        let html = bob.get(&ex_path(group)).await;
        assert!(html_contains(&html.body, &expected), "{expected}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn plain_post_of_a_non_member_goes_to_the_group_list_without_leaking(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut eve = x.eve_client().await;
    accept_html(&mut eve);
    for group in [x.g(), 999_999] {
        let reply = add_ex(&mut eve, group, x.bob()).await;
        assert_eq!(reply.status, 302, "group {group}");
        assert_eq!(
            reply.location().as_deref(),
            Some("/groups"),
            "group {group}"
        );
    }
    assert!(all_rules(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../migrations")]
async fn plain_post_without_a_session_goes_to_login(pool: MySqlPool) {
    let x = x_world(&pool).await;
    let mut anon = x.w.anon();
    accept_html(&mut anon);
    let reply = add_ex(&mut anon, x.g(), x.carol()).await;
    assert!(reply.status.is_redirection(), "{}", reply.status);
    assert!(
        reply.location().unwrap().starts_with("/auth/login?next="),
        "{:?}",
        reply.location()
    );
    assert!(all_rules(&pool).await.is_empty());
}
