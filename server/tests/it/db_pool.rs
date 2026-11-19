//! #99: a pooled connection must never be handed out with a transaction still open.
//!
//! A request dropped while its `BEGIN` was in flight left the server inside a transaction
//! that sqlx never recorded. The next request on that connection failed with 1568 at its
//! own `SET TRANSACTION` (seen as a `create_group` 500 in e2e). The pool from
//! `db::pool_options` rolls such a transaction back when the connection is released.
//!
//! Every pool here has a single connection, so the second acquire gets the same one
//! (checked with `CONNECTION_ID()`).

use std::time::Duration;

use app::server::db::{self, IsolationLevel};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::pool::PoolConnection;
use sqlx::{Connection, MySql, MySqlPool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const SESSION_ID: &str = "leftovertransaction001";

async fn single_connection_pool(options: MySqlConnectOptions) -> MySqlPool {
    db::pool_options()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

async fn connection_id(conn: &mut PoolConnection<MySql>) -> u64 {
    sqlx::query_scalar("SELECT CONNECTION_ID()")
        .fetch_one(&mut **conn)
        .await
        .unwrap()
}

async fn in_transaction(conn: &mut PoolConnection<MySql>) -> i64 {
    sqlx::query_scalar("SELECT CAST(@@in_transaction AS SIGNED)")
        .fetch_one(&mut **conn)
        .await
        .unwrap()
}

/// The state a dropped `BEGIN` leaves behind: the server is in a transaction, sqlx
/// doesn't know (a raw statement, not `Connection::begin`).
#[sqlx::test(migrations = "../migrations")]
async fn untracked_transaction_is_rolled_back_on_release(
    _pool_options: MySqlPoolOptions,
    connect_options: MySqlConnectOptions,
) {
    let pool = single_connection_pool(connect_options).await;

    let mut conn = pool.acquire().await.unwrap();
    let id = connection_id(&mut conn).await;
    sqlx::raw_sql("START TRANSACTION")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, data, expiry_date) \
         VALUES (?, ?, UTC_TIMESTAMP() + INTERVAL 1 DAY)",
    )
    .bind(SESSION_ID)
    .bind(b"uncommitted".as_slice())
    .execute(&mut *conn)
    .await
    .unwrap();
    assert_eq!(in_transaction(&mut conn).await, 1);
    drop(conn);

    let mut conn = pool.acquire().await.unwrap();
    assert_eq!(
        connection_id(&mut conn).await,
        id,
        "expected the same connection"
    );
    assert_eq!(in_transaction(&mut conn).await, 0);

    // The uncommitted write is gone, and a transaction with an explicit level starts
    // (it failed with 1568 while the leftover transaction was open).
    let mut tx = db::begin_with(&mut conn, IsolationLevel::ReadCommitted)
        .await
        .unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id = ?")
        .bind(SESSION_ID)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        rows, 0,
        "the leftover transaction's write must be rolled back"
    );
    tx.commit().await.unwrap();
}

/// The real failure: `begin_with` is dropped after its statements reached the server and
/// before the answer came back. A proxy between pool and server holds the server's
/// replies back, so the cut-off happens at exactly that point.
#[sqlx::test(migrations = "../migrations")]
async fn dropped_begin_does_not_leak_into_the_next_acquire(
    _pool_options: MySqlPoolOptions,
    connect_options: MySqlConnectOptions,
) {
    let (port, hold_replies) = reply_holding_proxy(&connect_options).await;
    let pool = single_connection_pool(connect_options.host("127.0.0.1").port(port)).await;

    let mut conn = pool.acquire().await.unwrap();
    let id = connection_id(&mut conn).await;
    hold_replies.send_replace(true);
    let cut_off = tokio::time::timeout(
        Duration::from_millis(500),
        db::begin_with(&mut conn, IsolationLevel::ReadCommitted),
    )
    .await
    .is_err();
    assert!(cut_off, "the begin should have been cut off");
    hold_replies.send_replace(false);
    drop(conn);

    let mut conn = pool.acquire().await.unwrap();
    assert_eq!(
        connection_id(&mut conn).await,
        id,
        "expected the same connection"
    );
    assert_eq!(in_transaction(&mut conn).await, 0);
    let tx = db::begin_with(&mut conn, IsolationLevel::RepeatableRead)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

/// A TCP proxy to the database in `options`. While the returned flag is `true`, it keeps
/// the server's replies (and forwards them once it is `false` again); the client's
/// packets always go through. Returns the proxy's port on 127.0.0.1.
async fn reply_holding_proxy(options: &MySqlConnectOptions) -> (u16, watch::Sender<bool>) {
    let upstream = format!("{}:{}", options.get_host(), options.get_port());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (hold, holding) = watch::channel(false);
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let Ok(server) = TcpStream::connect(&upstream).await else {
                return;
            };
            let (mut from_client, mut to_client) = client.into_split();
            let (mut from_server, mut to_server) = server.into_split();
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut from_client, &mut to_server).await;
            });
            let mut holding = holding.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16 * 1024];
                loop {
                    let n = match from_server.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    if holding.wait_for(|held| !*held).await.is_err()
                        || to_client.write_all(&buf[..n]).await.is_err()
                    {
                        return;
                    }
                }
            });
        }
    });
    (port, hold)
}

/// A transaction sqlx tracks and the caller commits leaves nothing to clean up, and the
/// level set by `begin_with` applies to that transaction only.
#[sqlx::test(migrations = "../migrations")]
async fn begin_with_sets_the_level_for_one_transaction(
    _pool_options: MySqlPoolOptions,
    connect_options: MySqlConnectOptions,
) {
    let pool = single_connection_pool(connect_options).await;
    let mut conn = pool.acquire().await.unwrap();

    let session_level: String = sqlx::query_scalar("SELECT @@session.transaction_isolation")
        .fetch_one(&mut *conn)
        .await
        .unwrap();

    let mut tx = db::begin_with(&mut conn, IsolationLevel::ReadCommitted)
        .await
        .unwrap();
    // `SET TRANSACTION` doesn't change the session value; the transaction's level shows
    // in how it reads: a READ COMMITTED transaction sees a row committed after its first
    // read, a REPEATABLE READ one would not.
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let mut other = second_connection(&pool).await;
    sqlx::query(
        "INSERT INTO sessions (id, data, expiry_date) \
         VALUES (?, ?, UTC_TIMESTAMP() + INTERVAL 1 DAY)",
    )
    .bind(SESSION_ID)
    .bind(b"committed".as_slice())
    .execute(&mut other)
    .await
    .unwrap();
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(after, before + 1, "READ COMMITTED sees the new row");
    tx.commit().await.unwrap();

    assert_eq!(in_transaction(&mut conn).await, 0);
    let still: String = sqlx::query_scalar("SELECT @@session.transaction_isolation")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(still, session_level);
}

/// A second, unpooled connection to the same test database.
async fn second_connection(pool: &MySqlPool) -> sqlx::MySqlConnection {
    sqlx::MySqlConnection::connect_with(&pool.connect_options())
        .await
        .unwrap()
}
