//! #59: invitation links resolve membership before displaying or attempting a join.
mod common;
use app::test_support::{TEST_PASSWORD, add_member, create_group, create_user};
use common::auth::{Client, Opts, router};
use sqlx::MySqlPool;

async fn fixture(pool: &MySqlPool) -> (i32, i32) {
    let owner = create_user(pool, "Owner", "owner@example.com").await;
    let member = create_user(pool, "Member", "member@example.com").await;
    let id = create_group(pool, owner, "Gift exchange").await;
    add_member(pool, id, member).await;
    sqlx::query("UPDATE `groups` SET invitation_code = 'ABCDEFGH' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    (id, member)
}

async fn count(pool: &MySqlPool, id: i32) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn existing_member_redirects_without_join_even_after_registration_closed(pool: MySqlPool) {
    let (id, _) = fixture(&pool).await;
    sqlx::query(
        "UPDATE `groups` SET is_drawn = TRUE, registration_deadline = '2020-01-01' WHERE id = ?",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login("member@example.com", TEST_PASSWORD).await.ok();
    let before = count(&pool, id).await;
    let response = c.get("/join/ABCD-EFGH").await;
    assert!(response.status.is_redirection());
    assert_eq!(response.location(), Some(format!("/groups/{id}")));
    assert!(!response.body.contains("name=\"code\""));
    assert_eq!(count(&pool, id).await, before);
    assert_eq!(c.get(&format!("/groups/{id}")).await.status, 200);
    // A stale form or repeated POST is idempotent too.
    c.call("join_group", &[("code", "ABCDEFGH")]).await.ok();
    assert_eq!(count(&pool, id).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn anonymous_link_survives_login_then_checks_membership(pool: MySqlPool) {
    let (id, _) = fixture(&pool).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    let response = c.get("/join/ABCDEFGH").await;
    assert_eq!(
        response.location().as_deref(),
        Some("/auth/login?next=%2Fjoin%2FABCDEFGH")
    );
    let login = c
        .login_next("member@example.com", TEST_PASSWORD, "/join/ABCDEFGH")
        .await;
    assert_eq!(login.location().as_deref(), Some("/join/ABCDEFGH"));
    assert_eq!(
        c.get("/join/ABCDEFGH").await.location(),
        Some(format!("/groups/{id}"))
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn nonmember_must_confirm_and_closed_groups_reject_join(pool: MySqlPool) {
    let (id, _) = fixture(&pool).await;
    create_user(&pool, "Visitor", "visitor@example.com").await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    c.login("visitor@example.com", TEST_PASSWORD).await.ok();
    assert_eq!(c.get(&format!("/groups/{id}")).await.status, 404);
    let page = c.get("/join/ABCDEFGH").await;
    assert_eq!(page.status, 200);
    assert!(page.body.contains("name=\"code\""));
    assert_eq!(count(&pool, id).await, 2, "GET must never join");
    sqlx::query("UPDATE `groups` SET is_drawn = TRUE WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        c.call("join_group", &[("code", "ABCDEFGH")]).await.status,
        422
    );
    assert_eq!(count(&pool, id).await, 2);
    sqlx::query("UPDATE `groups` SET is_drawn = FALSE WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let joined = c.call("join_group", &[("code", "abcd-efgh")]).await;
    joined.ok();
    assert_eq!(joined.location(), Some(format!("/groups/{id}")));
    assert_eq!(count(&pool, id).await, 3);
}

#[sqlx::test(migrations = "../migrations")]
async fn invitation_codes_are_exact_and_rate_limited_and_anonymous_mutations_rejected(
    pool: MySqlPool,
) {
    fixture(&pool).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    assert_eq!(
        c.call("join_group", &[("code", "ABCDEFGH")]).await.status,
        401
    );
    c.login("member@example.com", TEST_PASSWORD).await.ok();
    for _ in 0..10 {
        assert_eq!(
            c.call("preview_invitation", &[("code", "ÀBCDEFGH")])
                .await
                .status,
            422
        );
    }
    let response = c.call("preview_invitation", &[("code", "ABCDEFGH")]).await;
    assert_eq!(response.status, 422);
    assert!(response.body.contains("tooManyAttempts"));
}
