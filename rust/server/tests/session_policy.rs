//! #60: persistent long-lived sessions, sliding renewal and explicit per-device logout.

mod common;

use app::paths::DASHBOARD_PATH;
use app::test_support::{ALICE_PASSWORD, create_alice};
use chrono::{DateTime, Utc};
use common::auth::{Client, Opts, router};
use sqlx::MySqlPool;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::SessionStore;

const ALICE: &str = "alice@example.com";
const NINETY_DAYS: i64 = 7_776_000;

async fn expiry(pool: &MySqlPool, id: &str) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT expiry_date FROM sessions WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Move both the SQL expiry and serialized record, as if most of the idle period elapsed.
async fn set_expiry(pool: &MySqlPool, id: &str, seconds_ahead: i64) {
    let store = server::MySqlSessionStore::new(pool.clone());
    let mut record: Record = store
        .load(&id.parse::<Id>().unwrap())
        .await
        .unwrap()
        .unwrap();
    record.expiry_date = time::OffsetDateTime::now_utc() + time::Duration::seconds(seconds_ahead);
    store.save(&record).await.unwrap();
}

#[sqlx::test(migrations = "../migrations")]
async fn default_login_survives_restart_and_renews_on_reads(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut first = Client::new(&app);
    let reply = first.login(ALICE, ALICE_PASSWORD).await;
    reply.ok();
    let raw = reply.set_cookie(server::SESSION_COOKIE).unwrap();
    assert!(raw.contains("Max-Age=7776000"), "{raw}");
    let id = first.session_cookie().unwrap().to_owned();
    let remaining = (expiry(&pool, &id).await - Utc::now()).num_seconds();
    assert!((NINETY_DAYS - 10..=NINETY_DAYS).contains(&remaining));

    // Simulate day 89, then a new server process/router and a browser restoring its cookie.
    set_expiry(&pool, &id, 86_400).await;
    let restarted = router(&pool, &[], Opts::default());
    let mut restored = Client::new(&restarted).with_cookie(server::SESSION_COOKIE, &id);
    let res = restored.get(DASHBOARD_PATH).await;
    assert_eq!(res.status, 200);
    assert!(res.body.contains("Alice"));
    let raw = res.set_cookie(server::SESSION_COOKIE).unwrap();
    assert!(raw.contains("Max-Age=7776000"), "{raw}");
    let remaining = (expiry(&pool, &id).await - Utc::now()).num_seconds();
    assert!((NINETY_DAYS - 10..=NINETY_DAYS).contains(&remaining));

    // /health only reads the session locale in middleware, without touching flash or
    // modifying session data. This must renew too (with_always_save is intentional).
    set_expiry(&pool, &id, 60).await;
    let read = restored.get("/health").await;
    assert_eq!(read.status, 200);
    assert!(
        read.set_cookie(server::SESSION_COOKIE)
            .unwrap()
            .contains("Max-Age=7776000")
    );
    let remaining = (expiry(&pool, &id).await - Utc::now()).num_seconds();
    assert!((NINETY_DAYS - 10..=NINETY_DAYS).contains(&remaining));
    assert_eq!(restored.whoami().await.as_deref(), Some(ALICE));

    // Expiry is still enforced rather than resurrected by a subsequent request.
    set_expiry(&pool, &id, -60).await;
    let res = restored.get(DASHBOARD_PATH).await;
    assert!(res.status.is_redirection());
    assert!(res.location().unwrap().starts_with("/auth/login?next="));
}

#[sqlx::test(migrations = "../migrations")]
async fn logout_revokes_only_that_persistent_login(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = router(&pool, &[], Opts::default());
    let mut phone = Client::new(&app);
    let mut desktop = Client::new(&app);
    phone.login(ALICE, ALICE_PASSWORD).await.ok();
    desktop.login(ALICE, ALICE_PASSWORD).await.ok();
    let old_phone = phone.session_cookie().unwrap().to_owned();
    phone.call("logout", &[]).await.ok();
    assert_ne!(phone.session_cookie(), Some(old_phone.as_str()));

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id = ?")
        .bind(&old_phone)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0, "logout deletes the server-side session");

    // An old id can acquire anonymous locale/flash data again; it must never regain
    // the revoked user identity. Check deletion before making those requests.
    let mut replay = Client::new(&app).with_cookie(server::SESSION_COOKIE, &old_phone);
    assert_eq!(replay.whoami().await, None);
    assert_eq!(phone.whoami().await, None);
    assert_eq!(desktop.whoami().await.as_deref(), Some(ALICE));
}
