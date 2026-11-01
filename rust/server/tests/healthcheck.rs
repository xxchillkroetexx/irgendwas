//! `server --healthcheck` probe against a real listening server.

mod common;

use std::net::SocketAddr;

use common::*;
use server::healthcheck::probe;
use sqlx::MySqlPool;

#[sqlx::test(migrations = "../migrations")]
async fn probe_reports_healthy_server_and_unreachable_port(pool: MySqlPool) {
    let app = server::router(state(pool, config_with(&[])));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let ok = tokio::task::spawn_blocking(move || probe(addr))
        .await
        .unwrap();
    assert_eq!(ok, Ok(()));

    // A port with nothing listening (bind, read the port, close).
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let err = tokio::task::spawn_blocking(move || probe(closed))
        .await
        .unwrap();
    assert!(err.is_err());
}
