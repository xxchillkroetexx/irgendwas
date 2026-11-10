//! auth slice, review round 1: email collation (accent variants must be different
//! addresses), the login limiter under concurrency, and oversized login emails.

use std::time::Duration;

use crate::common::auth::*;
use app::auth_api::AuthError;
use app::server::password;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, create_alice, insert_pending_registration, set_reset_token,
};
use sqlx::MySqlPool;

#[tokio::test]
async fn password_admission_limits_waiters_and_recovers_after_cancellation() {
    use app::server::password::{PasswordError, TestPasswordLimiter};
    use std::sync::mpsc;

    let limiter = TestPasswordLimiter::new(1, 2);
    let (release, blocked) = mpsc::channel::<()>();
    let (started, mut running) = tokio::sync::mpsc::unbounded_channel();
    let first_limiter = limiter.clone();
    let first = tokio::spawn(async move {
        first_limiter
            .run(move || {
                started.send(()).unwrap();
                let _ = blocked.recv();
            })
            .await
    });
    running.recv().await.unwrap();
    let waiting_limiter = limiter.clone();
    let waiting = tokio::spawn(async move { waiting_limiter.run(|| {}).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while limiter.available_admissions() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(limiter.run(|| {}).await, Err(PasswordError::Busy)));
    waiting.abort();
    let _ = waiting.await;
    assert_eq!(limiter.available_admissions(), 1);
    drop(release);
    first.await.unwrap().unwrap();
    limiter.run(|| {}).await.unwrap();
}

#[tokio::test]
async fn duplicate_token_work_is_rejected_until_cancelled_blocking_work_finishes() {
    use app::server::password::{PasswordError, test_token_work};
    use std::sync::mpsc;

    let token = format!("test-admission-{}", rand::random::<u64>());
    let (release, blocked) = mpsc::channel::<()>();
    let (release_second, blocked_second) = mpsc::channel::<()>();
    let (started, mut running) = tokio::sync::mpsc::unbounded_channel();
    let first_token = token.clone();
    let started_second = started.clone();
    let first = tokio::spawn(async move {
        test_token_work(first_token, move || {
            started.send(()).unwrap();
            let _ = blocked.recv();
        })
        .await
    });
    running.recv().await.unwrap();
    let second_token = token.clone();
    let second = tokio::spawn(async move {
        test_token_work(second_token, move || {
            started_second.send(()).unwrap();
            let _ = blocked_second.recv();
        })
        .await
    });
    running.recv().await.unwrap();
    first.abort();
    let _ = first.await;
    assert!(matches!(
        test_token_work(token.clone(), || {}).await,
        Err(PasswordError::Busy)
    ));
    drop(release);
    drop(release_second);
    second.await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match test_token_work(token.clone(), || {}).await {
                Ok(()) => break,
                Err(PasswordError::Busy) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected error: {error}"),
            }
        }
    })
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../migrations")]
async fn busy_reset_token_is_not_consumed_and_can_be_retried(pool: MySqlPool) {
    use std::sync::mpsc;

    let alice = create_alice(&pool, "busy-reset@example.test").await;
    let token = app::server::token::generate().unwrap();
    set_reset_token(&pool, alice, &token, 3600).await;
    let options = pool.connect_options();
    let key = format!(
        "{}:{}:{}:{}",
        options.get_host(),
        options.get_port(),
        options.get_database().unwrap_or_default(),
        app::server::token::hash(&token)
    );
    let (release_a, blocked_a) = mpsc::channel::<()>();
    let (release_b, blocked_b) = mpsc::channel::<()>();
    let (started, mut running) = tokio::sync::mpsc::unbounded_channel();
    let started_a = started.clone();
    let key_a = key.clone();
    let first = tokio::spawn(async move {
        password::test_token_work(key_a, move || {
            started_a.send(()).unwrap();
            let _ = blocked_a.recv();
        })
        .await
    });
    let second = tokio::spawn(async move {
        password::test_token_work(key, move || {
            started.send(()).unwrap();
            let _ = blocked_b.recv();
        })
        .await
    });
    running.recv().await.unwrap();
    running.recv().await.unwrap();
    let app = router(&pool, &[], Opts::default());
    let mut client = Client::new(&app);
    assert_eq!(
        client
            .reset(&token, "new-password-123", "new-password-123")
            .await
            .err(),
        AuthError::TooManyRequests
    );
    let stored: Option<String> = sqlx::query_scalar("SELECT reset_token FROM users WHERE id = ?")
        .bind(alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        stored.is_some(),
        "capacity rejection must leave the token valid"
    );
    drop(release_a);
    drop(release_b);
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    client
        .reset(&token, "new-password-123", "new-password-123")
        .await
        .ok();
}

#[tokio::test]
async fn password_work_is_bounded_even_when_a_request_is_cancelled() {
    use std::sync::{Arc, Condvar, Mutex};

    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut tasks = Vec::new();
    for index in 0..5 {
        let gate = gate.clone();
        let started_tx = started_tx.clone();
        tasks.push(tokio::spawn(async move {
            password::test_password_work(move || {
                started_tx.send(index).unwrap();
                let (lock, ready) = &*gate;
                let mut released = lock.lock().unwrap();
                while !*released {
                    released = ready.wait(released).unwrap();
                }
            })
            .await
        }));
    }
    let mut started = Vec::new();
    for _ in 0..4 {
        started.push(
            tokio::time::timeout(Duration::from_secs(30), started_rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    tasks[started[0]].abort();
    // Cancelling a request cannot free its slot while its blocking job is still running.
    let fifth_started_early = tokio::time::timeout(Duration::from_millis(200), started_rx.recv())
        .await
        .is_ok();
    {
        let (lock, ready) = &*gate;
        *lock.lock().unwrap() = true;
        ready.notify_all();
    }
    assert!(
        !fifth_started_early,
        "a fifth blocking job entered before a permit was released"
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(30), started_rx.recv())
            .await
            .unwrap()
            .is_some()
    );
    for task in tasks {
        if let Ok(result) = task.await {
            result.unwrap();
        }
    }
}

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

/// The test holds the user's row lock. Wait until the request has read the old
/// snapshot and is blocked on its UPDATE, then commit the competing change. The
/// database user must be able to inspect its own connections in PROCESSLIST.
async fn wait_for_blocked_update(pool: &MySqlPool, pattern: &str) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.PROCESSLIST \
                 WHERE DB = DATABASE() AND INFO LIKE ?",
            )
            .bind(pattern)
            .fetch_one(pool)
            .await
            .unwrap();
            if count > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("request never reached the locked update");
}

#[sqlx::test(migrations = "../migrations")]
async fn password_rehash_does_not_undo_a_concurrent_password_change(pool: MySqlPool) {
    use argon2::password_hash::PasswordHasher;
    use argon2::{Algorithm, Argon2, Params, Version};

    let email = "alice@example.com";
    let alice = create_alice(&pool, email).await;
    let old_hash = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(8192, 1, 1, None).unwrap(),
    )
    .hash_password(ALICE_PASSWORD.as_bytes())
    .unwrap()
    .to_string();
    assert!(password::needs_rehash(&old_hash));
    sqlx::query("UPDATE users SET password = ? WHERE id = ?")
        .bind(&old_hash)
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    let new_hash = password::hash("new-password-123").unwrap();
    let app = router(&pool, &[], Opts::default());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE id = ? FOR UPDATE")
        .bind(alice)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let login = tokio::spawn(async move {
        let mut c = Client::new(&app);
        let result = c.login(email, ALICE_PASSWORD).await.err();
        (result, c)
    });
    wait_for_blocked_update(&pool, "UPDATE users SET password = %").await;
    // Both password-change and password-reset flows replace the hash and increment
    // the epoch. Commit this while the already-verified login is waiting.
    sqlx::query("UPDATE users SET password = ?, session_epoch = session_epoch + 1 WHERE id = ?")
        .bind(&new_hash)
        .bind(alice)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (result, mut c) = login.await.unwrap();
    assert_eq!(result, AuthError::InvalidCredentials);
    assert_eq!(c.whoami().await, None);
    let stored: String = sqlx::query_scalar("SELECT password FROM users WHERE id = ?")
        .bind(alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, new_hash);
    c.login(email, "new-password-123").await.ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn login_verified_before_password_reset_does_not_report_success(pool: MySqlPool) {
    let email = "alice@example.com";
    let alice = create_alice(&pool, email).await;
    let app = router(&pool, &[], Opts::default());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE id = ? FOR UPDATE")
        .bind(alice)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let login = tokio::spawn(async move {
        let mut client = Client::new(&app);
        let reply = client.login(email, ALICE_PASSWORD).await;
        (reply, client)
    });
    // The normal current hash needs no rehash: this is the last-login update.
    wait_for_blocked_update(&pool, "UPDATE users SET last_login = %").await;
    let new_hash = password::hash("new-password-123").unwrap();
    sqlx::query("UPDATE users SET password = ?, session_epoch = session_epoch + 1 WHERE id = ?")
        .bind(&new_hash)
        .bind(alice)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (reply, mut client) = login.await.unwrap();
    assert_eq!(reply.err(), AuthError::InvalidCredentials);
    assert_eq!(client.whoami().await, None);
    let last_login: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_login FROM users WHERE id = ?")
            .bind(alice)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(last_login.is_none());
    client.login(email, "new-password-123").await.ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn reset_request_does_not_mail_former_address_after_email_change(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("reset-old-address");
    let new_email = unique_email("reset-new-address");
    let alice = create_alice(&pool, &email).await;
    let app = router(&pool, &[], Opts::mail());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE id = ? FOR UPDATE")
        .bind(alice)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let old_email = email.clone();
    let reset = tokio::spawn(async move { Client::new(&app).forgot(&old_email).await });
    wait_for_blocked_update(&pool, "UPDATE users SET reset_token = %").await;
    sqlx::query(
        "UPDATE users SET email = ?, reset_token = NULL, reset_token_expires = NULL WHERE id = ?",
    )
    .bind(&new_email)
    .bind(alice)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    reset.await.unwrap().ok();
    let token: Option<String> = sqlx::query_scalar("SELECT reset_token FROM users WHERE id = ?")
        .bind(alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(token.is_none());
    settle().await;
    assert!(mailpit.messages_for(&email).await.is_empty());
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
