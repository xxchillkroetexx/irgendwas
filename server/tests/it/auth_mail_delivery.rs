//! #102: auth mails survive a passing SMTP failure, an undelivered mail does not kill the
//! link it replaced, and register calls cannot use up an address's reset mails.
//!
//! A stub SMTP server answers the first `failures` recipients with a chosen reply and
//! accepts everything after that; it counts the attempts (`RCPT TO`) and the messages it
//! accepted.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::common::auth::*;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{create_alice, insert_pending_registration, set_reset_token};
use sqlx::MySqlPool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const ALICE: &str = "alice@example.com";
const RESET: &str = "/auth/reset-password/";
const NEW_PASSWORD: &str = "newpassword1";
const TRANSIENT: &str = "451 4.3.0 Try again later";
const PERMANENT: &str = "550 5.7.1 Rejected";

fn tok(c: char) -> String {
    c.to_string().repeat(43)
}

#[derive(Default)]
struct SmtpCounts {
    attempts: AtomicUsize,
    delivered: AtomicUsize,
}

/// A stub SMTP server on 127.0.0.1: the first `failures` `RCPT TO` get `reply`, later ones
/// are accepted and their message is counted as delivered.
async fn stub_smtp(failures: usize, reply: &'static str) -> (String, Arc<SmtpCounts>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let counts = Arc::new(SmtpCounts::default());
    let shared = counts.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let counts = shared.clone();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut lines = BufReader::new(read).lines();
                if write.write_all(b"220 stub ESMTP\r\n").await.is_err() {
                    return;
                }
                let mut in_data = false;
                while let Ok(Some(line)) = lines.next_line().await {
                    let answer = if in_data {
                        if line != "." {
                            continue;
                        }
                        in_data = false;
                        counts.delivered.fetch_add(1, Ordering::SeqCst);
                        "250 OK queued".to_string()
                    } else {
                        let upper = line.to_uppercase();
                        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                            "250 stub".to_string()
                        } else if upper.starts_with("RCPT TO") {
                            let attempt = counts.attempts.fetch_add(1, Ordering::SeqCst);
                            if attempt < failures {
                                reply.to_string()
                            } else {
                                "250 OK".to_string()
                            }
                        } else if upper.starts_with("DATA") {
                            in_data = true;
                            "354 go ahead".to_string()
                        } else if upper.starts_with("QUIT") {
                            let _ = write.write_all(b"221 bye\r\n").await;
                            return;
                        } else {
                            "250 OK".to_string()
                        }
                    };
                    let answer = format!("{answer}\r\n");
                    if write.write_all(answer.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    (port, counts)
}

/// The router with its mail going to the stub on `port`, and short waits between attempts.
fn router_to_stub(pool: &MySqlPool, port: &str, delays: &[u64]) -> axum::Router {
    router(
        pool,
        &[
            ("MAIL_HOST", "127.0.0.1"),
            ("MAIL_PORT", port),
            ("MAIL_ENCRYPTION", "none"),
        ],
        Opts {
            mail_retry_delays: Some(delays.iter().map(|ms| Duration::from_millis(*ms)).collect()),
            ..Opts::default()
        },
    )
}

/// Polls `check` until it holds, for up to 10 s.
async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for: {what}");
}

async fn sha256_hex(pool: &MySqlPool, s: &str) -> String {
    sqlx::query_scalar("SELECT SHA2(?, 256)")
        .bind(s)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn reset_hash(pool: &MySqlPool, email: &str) -> Option<String> {
    sqlx::query_scalar("SELECT reset_token FROM users WHERE email = ?")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn pending(pool: &MySqlPool, email: &str) -> Option<(String, Option<String>)> {
    sqlx::query_as("SELECT token_hash, next_path FROM pending_registrations WHERE email = ?")
        .bind(email)
        .fetch_optional(pool)
        .await
        .unwrap()
}

// ------------------------------------------------------------------ password reset

#[sqlx::test(migrations = "../migrations")]
async fn a_reset_mail_is_retried_after_a_transient_failure(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    let old = sha256_hex(&pool, &tok('r')).await;
    let (port, smtp) = stub_smtp(1, TRANSIENT).await;
    let app = router_to_stub(&pool, &port, &[50, 50]);

    Client::new(&app).forgot(ALICE).await.ok();

    eventually("the second attempt delivered", || async {
        smtp.delivered.load(Ordering::SeqCst) == 1
    })
    .await;
    assert_eq!(smtp.attempts.load(Ordering::SeqCst), 2);
    // Delivered: the new link stays, the old one is superseded as before (AC38).
    tokio::time::sleep(Duration::from_millis(200)).await;
    let stored = reset_hash(&pool, ALICE).await.expect("a reset token");
    assert_ne!(stored, old);
}

#[sqlx::test(migrations = "../migrations")]
async fn an_undelivered_reset_mail_restores_the_previous_link(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    let old = sha256_hex(&pool, &tok('r')).await;
    let (port, smtp) = stub_smtp(usize::MAX, TRANSIENT).await;
    let app = router_to_stub(&pool, &port, &[50, 50]);
    let mut c = Client::new(&app);

    let known = c.forgot(ALICE).await;
    known.ok();
    let unknown = c.forgot("ghost@example.com").await;
    assert_eq!(
        known.body, unknown.body,
        "same answer as for an unknown address"
    );

    eventually("the previous token is back", || async {
        reset_hash(&pool, ALICE).await.as_deref() == Some(old.as_str())
    })
    .await;
    assert_eq!(
        smtp.attempts.load(Ordering::SeqCst),
        3,
        "one try, two retries"
    );
    assert_eq!(smtp.delivered.load(Ordering::SeqCst), 0);

    // The link from the earlier mail works.
    c.reset(&tok('r'), NEW_PASSWORD, NEW_PASSWORD).await.ok();
    assert!(reset_hash(&pool, ALICE).await.is_none(), "used up");
}

#[sqlx::test(migrations = "../migrations")]
async fn a_permanent_failure_is_not_retried_and_clears_an_unsent_token(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let (port, smtp) = stub_smtp(usize::MAX, PERMANENT).await;
    let app = router_to_stub(&pool, &port, &[50, 50]);

    Client::new(&app).forgot(ALICE).await.ok();

    // There was no earlier token: the one nobody received is removed again.
    eventually("one attempt made", || async {
        smtp.attempts.load(Ordering::SeqCst) == 1
    })
    .await;
    eventually("the unsent token is gone", || async {
        reset_hash(&pool, ALICE).await.is_none()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        smtp.attempts.load(Ordering::SeqCst),
        1,
        "no retry after a 5xx"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn an_undelivered_mail_never_overwrites_a_newer_token(pool: MySqlPool) {
    let alice = create_alice(&pool, ALICE).await;
    set_reset_token(&pool, alice, &tok('r'), 1800).await;
    // The stub stalls nothing, so let the failing request's retries wait long enough to
    // put a newer token in place before its undo runs.
    let (port, _smtp) = stub_smtp(usize::MAX, TRANSIENT).await;
    let app = router_to_stub(&pool, &port, &[400]);

    Client::new(&app).forgot(ALICE).await.ok();
    tokio::time::sleep(Duration::from_millis(100)).await;
    set_reset_token(&pool, alice, &tok('n'), 1800).await;
    let newer = sha256_hex(&pool, &tok('n')).await;

    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(reset_hash(&pool, ALICE).await, Some(newer));
}

// ------------------------------------------------------------------ registration

#[sqlx::test(migrations = "../migrations")]
async fn an_undelivered_registration_mail_restores_the_previous_link(pool: MySqlPool) {
    let carol = "carol@example.com";
    insert_pending_registration(&pool, carol, &tok('p'), Some("/join/ABCDEFGH"), 3600).await;
    let old = sha256_hex(&pool, &tok('p')).await;
    let (port, _smtp) = stub_smtp(usize::MAX, PERMANENT).await;
    let app = router_to_stub(&pool, &port, &[50, 50]);
    let mut c = Client::new(&app);

    c.register(carol, None).await.ok();

    eventually("the previous registration is back", || async {
        pending(&pool, carol).await.map(|(hash, _)| hash).as_deref() == Some(old.as_str())
    })
    .await;
    assert_eq!(
        pending(&pool, carol)
            .await
            .and_then(|(_, next)| next)
            .as_deref(),
        Some("/join/ABCDEFGH"),
        "with its next path"
    );
    c.call("get_pending_registration", &[("token", &tok('p'))])
        .await
        .ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn an_undelivered_first_registration_mail_leaves_no_row(pool: MySqlPool) {
    let dave = "dave@example.com";
    let (port, _smtp) = stub_smtp(usize::MAX, PERMANENT).await;
    let app = router_to_stub(&pool, &port, &[50, 50]);

    Client::new(&app).register(dave, None).await.ok();

    eventually("the unsent registration is gone", || async {
        pending(&pool, dave).await.is_none()
    })
    .await;
}

// ------------------------------------------------------------------ per-address budgets

#[sqlx::test(migrations = "../migrations")]
async fn register_calls_do_not_use_up_the_reset_mails(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let alice = unique_email("alice");
    create_alice(&pool, &alice).await;
    let app = router(&pool, &[], Opts::mail());

    // Someone else registers Alice's address three times: three existing-account notices.
    let mut stranger = Client::new(&app).peer("198.51.100.20");
    for _ in 0..3 {
        stranger.register(&alice, None).await.ok();
    }
    mailpit.wait_for_count(&alice, 3).await;
    let fourth = stranger.register(&alice, None).await;
    fourth.ok();

    // Alice's own reset request still gets its mail.
    Client::new(&app).forgot(&alice).await.ok();
    let mails = mailpit.wait_for_count(&alice, 4).await;
    assert!(
        mails.iter().any(|m| m.first_url_with_path(RESET).is_some()),
        "a reset mail arrived"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn a_fourth_reset_in_an_hour_sends_no_mail_and_the_third_link_works(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let alice = unique_email("alice");
    create_alice(&pool, &alice).await;
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);

    for _ in 0..3 {
        c.forgot(&alice).await.ok();
    }
    let mails = mailpit.wait_for_count(&alice, 3).await;
    let stored = reset_hash(&pool, &alice).await;
    c.forgot(&alice).await.ok();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        mailpit.messages_for(&alice).await.len(),
        3,
        "no fourth mail"
    );
    assert_eq!(
        reset_hash(&pool, &alice).await,
        stored,
        "no new token either"
    );

    let mut working = 0;
    for mail in &mails {
        let token = token_in(&mail.first_url_with_path(RESET).unwrap(), RESET);
        if Some(sha256_hex(&pool, &token).await) == stored {
            working += 1;
        }
    }
    assert_eq!(working, 1, "the newest delivered link is the stored one");
}
