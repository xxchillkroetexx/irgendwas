//! auth slice, review round 1: email collation (accent variants must be different
//! addresses), the login limiter under concurrency, and oversized login emails.

use std::time::Duration;

use crate::common::auth::*;
use app::auth_api::AuthError;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, create_alice, insert_pending_registration, set_reset_token,
};
use sqlx::MySqlPool;

fn tok(c: char) -> String {
    c.to_string().repeat(43)
}

/// The same address with an accent in the domain. Under `utf8mb4_unicode_ci` it equals the
/// original (`a` = `ä`); as a byte string it is a different address.
fn accent_variant(email: &str) -> String {
    let (local, domain) = email.split_once('@').unwrap();
    let domain = domain.replacen('a', "ä", 1);
    assert_ne!(
        domain,
        email.split_once('@').unwrap().1,
        "domain has an `a`"
    );
    format!("{local}@{domain}")
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(800)).await;
}

async fn pending_rows_binary(pool: &MySqlPool, email: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM pending_registrations WHERE BINARY email = ?")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn users_binary(pool: &MySqlPool, email: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE BINARY email = ?")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ------------------------------------------------------------- collation

#[sqlx::test(migrations = "../migrations")]
async fn collation_email_columns_compare_bytes(pool: MySqlPool) {
    for table in ["users", "pending_registrations"] {
        let collation: String = sqlx::query_scalar(
            "SELECT CAST(COLLATION_NAME AS CHAR) FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND COLUMN_NAME = 'email'",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            collation.ends_with("_bin"),
            "{table}.email has collation {collation}: accent/case variants would collide"
        );
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn collation_reset_for_an_accent_variant_does_not_reach_alice(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("alice");
    let variant = accent_variant(&email);
    let alice = create_alice(&pool, &email).await;
    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    let before: Option<String> = sqlx::query_scalar("SELECT reset_token FROM users WHERE id = ?")
        .bind(alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    let app = router(&pool, &[], Opts::mail());

    Client::new(&app).forgot(&variant).await.ok();
    settle().await;

    let after: Option<String> = sqlx::query_scalar("SELECT reset_token FROM users WHERE id = ?")
        .bind(alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before, "Alice's reset token must not be replaced");
    assert!(
        mailpit.messages_for(&variant).await.is_empty(),
        "no reset link to the attacker's address"
    );
    assert!(
        mailpit.messages_for(&email).await.is_empty(),
        "and none to Alice either: the variant is not her address"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn collation_register_for_an_accent_variant_leaves_the_pending_row_alone(pool: MySqlPool) {
    let base = unique_email("alice");
    let variant = accent_variant(&base);
    insert_pending_registration(&pool, &base, &tok('p'), None, 3600).await;
    let hash_before: String =
        sqlx::query_scalar("SELECT token_hash FROM pending_registrations WHERE BINARY email = ?")
            .bind(&base)
            .fetch_one(&pool)
            .await
            .unwrap();
    let app = router(&pool, &[], Opts::default());

    Client::new(&app).register(&variant, None).await.ok();

    let hash_after: String =
        sqlx::query_scalar("SELECT token_hash FROM pending_registrations WHERE BINARY email = ?")
            .bind(&base)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        hash_after, hash_before,
        "the original pending token is untouched"
    );
    assert_eq!(
        pending_rows_binary(&pool, &variant).await,
        1,
        "the variant has its own row"
    );
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_registrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 2);
}

#[sqlx::test(migrations = "../migrations")]
async fn collation_register_for_a_variant_of_an_existing_user_is_a_new_registration(
    pool: MySqlPool,
) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("alice");
    let variant = accent_variant(&email);
    create_alice(&pool, &email).await;
    let app = router(&pool, &[], Opts::mail());

    Client::new(&app).register(&variant, None).await.ok();
    settle().await;

    assert_eq!(
        pending_rows_binary(&pool, &variant).await,
        1,
        "pending row for the variant"
    );
    assert!(
        mailpit.messages_for(&email).await.is_empty(),
        "no existing-account notice: the variant is not Alice's address"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn collation_login_with_an_accent_variant_fails(pool: MySqlPool) {
    create_alice(&pool, "alice@gmail.com").await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    let reply = c.login("alice@gmäil.com", ALICE_PASSWORD).await;
    assert_eq!(reply.err(), AuthError::InvalidCredentials);
    assert_eq!(c.whoami().await, None);
    // Control: the real address works.
    c.login("alice@gmail.com", ALICE_PASSWORD).await.ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn collation_completing_for_a_variant_of_an_existing_user_creates_a_second_user(
    pool: MySqlPool,
) {
    create_alice(&pool, "alice@gmail.com").await;
    insert_pending_registration(&pool, "alice@gmäil.com", &tok('v'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    c.complete(&tok('v'), "Mallory", "mallorypass1", "mallorypass1")
        .await
        .ok();
    assert_eq!(users_binary(&pool, "alice@gmäil.com").await, 1);
    assert_eq!(users_binary(&pool, "alice@gmail.com").await, 1);
    assert_eq!(c.whoami().await.as_deref(), Some("alice@gmäil.com"));
}

#[sqlx::test(migrations = "../migrations")]
async fn mails_go_to_the_stored_address_exactly(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("alice");
    create_alice(&pool, &email).await;
    let stored: String = sqlx::query_scalar("SELECT email FROM users WHERE BINARY email = ?")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);

    // Existing-account notice, requested with sloppy input.
    c.register(&format!("  {}  ", email.to_uppercase()), None)
        .await
        .ok();
    let mails = mailpit.wait_for_count(&email, 1).await;
    assert_eq!(mails[0].to.len(), 1);
    assert_eq!(
        mails[0].to[0].address, stored,
        "existing-account notice recipient"
    );

    // Password reset, likewise.
    c.forgot(&format!("  {}  ", email.to_uppercase()))
        .await
        .ok();
    let mails = mailpit.wait_for_count(&email, 2).await;
    for mail in &mails {
        assert_eq!(mail.to.len(), 1);
        assert_eq!(mail.to[0].address, stored);
    }
}

// ------------------------------------------------------- limiter concurrency

#[sqlx::test(migrations = "../migrations")]
async fn login_limiter_holds_under_concurrent_wrong_passwords(pool: MySqlPool) {
    create_alice(&pool, "alice@example.com").await;
    let app = router(&pool, &[], Opts::default());

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..30 {
        let mut c = Client::new(&app);
        set.spawn(async move { c.login("alice@example.com", "wrong password").await.err() });
    }
    let (mut verified, mut limited) = (0, 0);
    while let Some(result) = set.join_next().await {
        match result.expect("task") {
            AuthError::InvalidCredentials => verified += 1,
            AuthError::TooManyAttempts { .. } => limited += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(verified + limited, 30);
    assert!(
        verified <= 10,
        "{verified} concurrent attempts got through password verification (limit 10)"
    );
}

// ------------------------------------------------------------ oversized email

#[sqlx::test(migrations = "../migrations")]
async fn login_with_an_oversized_email_is_a_normal_failure(pool: MySqlPool) {
    create_alice(&pool, "alice@example.com").await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    for len in [256, 300, 10_000] {
        let email = format!("{}@example.com", "a".repeat(len));
        let reply = c.login(&email, "whatever password").await;
        assert!(
            !reply.status.is_server_error(),
            "{len} chars: status {}",
            reply.status
        );
        let err = reply.err();
        assert!(
            matches!(
                err,
                AuthError::InvalidCredentials | AuthError::Validation(_)
            ),
            "{len} chars: {err:?}"
        );
    }
    // The ordinary login is unaffected.
    c.login("alice@example.com", ALICE_PASSWORD).await.ok();
}
