//! #71 (and #69 finding 2): overlapping requests of one browser. A request that loaded the
//! session before a parallel request changed or deleted it must neither bring a deleted
//! session back (logout, cleanup) nor write its stale snapshot over the parallel change
//! (taken flash messages, other keys).
//!
//! `/test/hold` loads the session, signals `Gate::loaded` and waits for `Gate::release`
//! before it applies its own operations, so every interleaving is deterministic.

use std::collections::HashMap;
use std::sync::Arc;

use crate::common::auth::{Client, Reply, has_flash_key};
use crate::common::{config_with, state};
use app::flash::FlashMessage;
use app::server::{AppState, flash};
use app::test_support::{ALICE_PASSWORD, create_alice};
use axum::Router;
use axum::extract::Query;
use axum::routing::get as route;
use serde_json::{Value, json};
use sqlx::MySqlPool;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tower_sessions::Session;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::SessionStore;

const ALICE: &str = "alice@example.com";

#[derive(Default)]
struct Gate {
    loaded: Notify,
    release: Notify,
}

/// `set=key:value`, `remove=key`, `flash=i18n.key`, `flush=`, in query order.
type Ops = Query<Vec<(String, String)>>;

async fn apply(session: &Session, ops: Vec<(String, String)>) {
    for (op, arg) in ops {
        match op.as_str() {
            "set" => {
                let (key, value) = arg.split_once(':').expect("key:value");
                session.insert(key, value).await.unwrap();
            }
            "remove" => {
                session.remove_value(&arg).await.unwrap();
            }
            "flash" => flash::push(session, FlashMessage::success(arg))
                .await
                .unwrap(),
            "flush" => session.flush().await.unwrap(),
            other => panic!("unknown op {other}"),
        }
    }
}

fn routes(gate: Arc<Gate>) -> Router<AppState> {
    Router::new()
        .route(
            "/test/set",
            route(|session: Session, Query(ops): Ops| async move {
                apply(&session, ops).await;
                "ok"
            }),
        )
        .route(
            "/test/get",
            route(
                |session: Session, Query(q): Query<HashMap<String, String>>| async move {
                    let value = session.get_value(&q["key"]).await.unwrap();
                    serde_json::to_string(&value).unwrap()
                },
            ),
        )
        .route(
            "/test/hold",
            route(move |session: Session, Query(ops): Ops| {
                let gate = gate.clone();
                async move {
                    // Loads the record (the locale middleware has done so already).
                    session.get_value("user_id").await.unwrap();
                    gate.loaded.notify_one();
                    gate.release.notified().await;
                    apply(&session, ops).await;
                    "held"
                }
            }),
        )
}

fn app(pool: &MySqlPool) -> (Router, Arc<Gate>) {
    let gate = Arc::new(Gate::default());
    let app = server::router_with(state(pool.clone(), config_with(&[])), routes(gate.clone()));
    (app, gate)
}

/// Starts `uri` with a copy of `client`'s cookies and returns once it has loaded the session.
async fn hold(client: &Client, gate: &Gate, uri: &str) -> JoinHandle<Reply> {
    let mut late = client.fork();
    let uri = uri.to_string();
    let handle = tokio::spawn(async move { late.get(&uri).await });
    gate.loaded.notified().await;
    handle
}

async fn finish(gate: &Gate, held: JoinHandle<Reply>) {
    gate.release.notify_one();
    let reply = held.await.expect("held request");
    assert_eq!(reply.status, 200, "{}", reply.body);
}

/// The data map stored for session `id`, if the row exists.
async fn stored(pool: &MySqlPool, id: &str) -> Option<serde_json::Map<String, Value>> {
    let blob: Option<Vec<u8>> = sqlx::query_scalar("SELECT data FROM sessions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap();
    blob.map(|blob| {
        let record: Value = serde_json::from_slice(&blob).unwrap();
        record["data"].as_object().unwrap().clone()
    })
}

async fn get_key(client: &mut Client, key: &str) -> Value {
    let reply = client.get(&format!("/test/get?key={key}")).await;
    serde_json::from_str(&reply.body).unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn late_response_does_not_resurrect_a_logged_out_session(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let (app, gate) = app(&pool);
    // A late read-only request and a late request that changes the session.
    for late in [
        "/test/hold",
        "/test/hold?set=late:yes&flash=flash.success.item_added",
    ] {
        let mut alice = Client::new(&app);
        alice.login(ALICE, ALICE_PASSWORD).await.ok();
        let old = alice.session_cookie().unwrap().to_owned();
        let held = hold(&alice, &gate, late).await;

        alice.call("logout", &[]).await.ok();
        assert_ne!(alice.session_cookie(), Some(old.as_str()));
        finish(&gate, held).await;

        assert!(
            stored(&pool, &old).await.is_none(),
            "{late}: the logged-out session id was written again"
        );
        let mut replay = Client::new(&app).with_cookie(server::SESSION_COOKIE, &old);
        assert_eq!(
            replay.whoami().await,
            None,
            "{late}: old cookie authenticates"
        );
        assert_eq!(alice.whoami().await, None, "{late}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn late_response_does_not_recreate_a_cleaned_up_session(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let (app, gate) = app(&pool);
    let store = server::MySqlSessionStore::new(pool.clone());
    for late in ["/test/hold", "/test/hold?set=late:yes"] {
        let mut alice = Client::new(&app);
        alice.login(ALICE, ALICE_PASSWORD).await.ok();
        let id = alice.session_cookie().unwrap().to_owned();
        let held = hold(&alice, &gate, late).await;

        // The session expires and the cleanup task removes it meanwhile.
        sqlx::query(
            "UPDATE sessions SET expiry_date = DATE_SUB(CURRENT_TIMESTAMP(6), INTERVAL 1 MINUTE) \
             WHERE id = ?",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(store.delete_expired().await.unwrap(), 1);
        finish(&gate, held).await;

        assert!(stored(&pool, &id).await.is_none(), "{late}: row recreated");
        let mut replay = Client::new(&app).with_cookie(server::SESSION_COOKIE, &id);
        assert_eq!(replay.whoami().await, None, "{late}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn stale_read_only_request_keeps_a_parallel_change(pool: MySqlPool) {
    let (app, gate) = app(&pool);
    let mut browser = Client::new(&app);
    browser.get("/test/set?set=k1:a").await;
    let id = browser.session_cookie().unwrap().to_owned();

    let held = hold(&browser, &gate, "/test/hold").await;
    browser.get("/test/set?set=k1:b").await;
    finish(&gate, held).await;

    assert_eq!(get_key(&mut browser, "k1").await, json!("b"));
    // The store's bookkeeping never reaches the database.
    let data = stored(&pool, &id).await.unwrap();
    assert_eq!(data.keys().collect::<Vec<_>>(), ["k1"], "{data:?}");
}

#[sqlx::test(migrations = "../migrations")]
async fn parallel_changes_to_different_keys_both_survive(pool: MySqlPool) {
    let (app, gate) = app(&pool);
    let mut browser = Client::new(&app);
    browser.get("/test/set?set=k1:a&set=k3:z&set=k4:w").await;

    // The late request sets k2 and removes k3; meanwhile k1 changes and k4 is removed.
    let held = hold(&browser, &gate, "/test/hold?set=k2:x&remove=k3").await;
    browser.get("/test/set?set=k1:b&remove=k4").await;
    finish(&gate, held).await;

    assert_eq!(get_key(&mut browser, "k1").await, json!("b"));
    assert_eq!(get_key(&mut browser, "k2").await, json!("x"));
    assert_eq!(get_key(&mut browser, "k3").await, Value::Null);
    assert_eq!(get_key(&mut browser, "k4").await, Value::Null);
}

/// #69 finding 2: after an action, `session_info` takes the success flash while the page's
/// other resources run in parallel. They must not write the flash back.
#[sqlx::test(migrations = "../migrations")]
async fn taken_flash_is_not_written_back_by_a_parallel_request(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let (app, gate) = app(&pool);
    let mut alice = Client::new(&app);
    alice.login(ALICE, ALICE_PASSWORD).await.ok();
    alice.session_info().await;

    for late in ["/test/hold", "/test/hold?set=other:change"] {
        alice.get("/test/set?flash=flash.success.item_added").await;
        let held = hold(&alice, &gate, late).await;
        let info = alice.session_info().await;
        assert!(has_flash_key(&info, "flash.success.item_added"), "{late}");
        finish(&gate, held).await;

        let info = alice.session_info().await;
        assert!(
            info.flash.is_empty(),
            "{late}: flash came back: {:?}",
            info.flash
        );
        assert_eq!(info.user.map(|u| u.email).as_deref(), Some(ALICE));
    }
}

/// `Session::flush` keeps the deleted id in its record; a write in the same request goes
/// through `create`, which must not insert that id again.
#[sqlx::test(migrations = "../migrations")]
async fn a_write_after_a_flush_gets_a_new_id(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let (app, _gate) = app(&pool);

    // Generic: flush, then store something in the same request.
    let mut browser = Client::new(&app);
    browser.get("/test/set?set=k1:a").await;
    let old = browser.session_cookie().unwrap().to_owned();
    browser.get("/test/set?flush=&set=k2:b").await;
    let new = browser.session_cookie().unwrap().to_owned();
    assert_ne!(new, old);
    assert!(
        stored(&pool, &old).await.is_none(),
        "flushed id inserted again"
    );
    let data = stored(&pool, &new).await.unwrap();
    assert_eq!(data.keys().collect::<Vec<_>>(), ["k2"], "{data:?}");

    // The language switch with a login revoked by a password change elsewhere (epoch
    // bump): `load_user` flushes, then the locale is stored.
    let mut alice = Client::new(&app);
    alice.login(ALICE, ALICE_PASSWORD).await.ok();
    let old = alice.session_cookie().unwrap().to_owned();
    sqlx::query("UPDATE users SET session_epoch = session_epoch + 1 WHERE email = ?")
        .bind(ALICE)
        .execute(&pool)
        .await
        .unwrap();
    let reply = alice
        .call("set_locale", &[("locale", "de"), ("next", "/")])
        .await;
    assert!(reply.status.is_success(), "{}", reply.body);
    let new = alice.session_cookie().unwrap().to_owned();
    assert_ne!(new, old, "the response must carry a new session id");
    assert!(
        stored(&pool, &old).await.is_none(),
        "revoked id inserted again"
    );
    assert_eq!(stored(&pool, &new).await.unwrap()["locale"], json!("de"));
    let mut replay = Client::new(&app).with_cookie(server::SESSION_COOKIE, &old);
    assert_eq!(replay.whoami().await, None);
    assert_eq!(alice.whoami().await, None);
}

#[sqlx::test(migrations = "../migrations")]
async fn store_save_never_inserts_and_create_never_reuses_an_id(pool: MySqlPool) {
    let store = server::MySqlSessionStore::new(pool.clone());
    let expiry_date = time::OffsetDateTime::now_utc() + time::Duration::hours(1);
    let data = |v: &str| HashMap::from([("k".to_string(), json!(v))]);

    // `save` of an id the store does not have (deleted, never created) writes nothing.
    let mut record = Record {
        id: Id::default(),
        data: data("first"),
        expiry_date,
    };
    store.save(&record).await.unwrap();
    assert!(stored(&pool, &record.id.to_string()).await.is_none());

    // `create` draws its own id: the one handed in (here: deleted / never stored) is not
    // used.
    let given = record.id;
    store.create(&mut record).await.unwrap();
    assert_ne!(record.id, given);
    assert!(stored(&pool, &given.to_string()).await.is_none());
    let first = record.id.to_string();
    assert_eq!(stored(&pool, &first).await.unwrap()["k"], json!("first"));

    // Nor is a taken id; the existing session is untouched.
    let mut clash = Record {
        id: record.id,
        data: data("second"),
        expiry_date,
    };
    store.create(&mut clash).await.unwrap();
    assert_ne!(clash.id, record.id);
    assert_eq!(stored(&pool, &first).await.unwrap()["k"], json!("first"));
    assert_eq!(
        stored(&pool, &clash.id.to_string()).await.unwrap()["k"],
        json!("second")
    );
}
