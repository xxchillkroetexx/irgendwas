//! Helpers for the account tests: the `AccountError` view of a server fn reply, one wrapper
//! per account server fn, and a few mail/DB shortcuts. Builds on `common::auth`.
#![allow(dead_code)]

use std::time::Duration;

use app::account_api::AccountError;
use app::i18n::{Locale, translate, translate_with};
use app::test_support::create_alice;
use app::test_support::mailpit::{Mailpit, unique_email};
use axum::Router;
use serde::de::DeserializeOwned;
use sqlx::MySqlPool;

use super::auth::{Client, Opts, Reply, router, token_in};

/// Path prefix of the verification link in the mail.
pub const VERIFY: &str = "/user/verify-email/";
/// What `APP_URL` is in the tests.
pub const APP_URL: &str = "http://localhost:3000";

/// A 43-character token made of `c` (well-formed, base64url).
pub fn tok(c: char) -> String {
    c.to_string().repeat(43)
}

/// Waits long enough for a (wrongly) sent mail to show up in Mailpit.
pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(700)).await;
}

pub fn en(key: &str) -> String {
    translate(Locale::En, key)
}

pub fn de(key: &str) -> String {
    translate(Locale::De, key)
}

pub fn en_with(key: &str, params: &[(&str, &str)]) -> String {
    translate_with(Locale::En, key, params)
}

/// `AccountError` view of a server fn reply (2xx: the JSON value, else the JSON error).
pub trait AccountReply {
    fn account_result<T: DeserializeOwned>(&self) -> Result<T, AccountError>;
    fn account_ok(&self) -> &Self;
    fn account_err(&self) -> AccountError;
}

impl AccountReply for Reply {
    fn account_result<T: DeserializeOwned>(&self) -> Result<T, AccountError> {
        if self.status.is_success() {
            Ok(serde_json::from_str(&self.body)
                .unwrap_or_else(|e| panic!("bad success body {:?}: {e}", self.body)))
        } else {
            Err(serde_json::from_str(&self.body).unwrap_or_else(|e| {
                panic!(
                    "bad error body {:?} (status {}): {e}",
                    self.body, self.status
                )
            }))
        }
    }

    fn account_ok(&self) -> &Self {
        if let Err(err) = self.account_result::<serde_json::Value>() {
            panic!("expected success, got {err:?}");
        }
        self
    }

    fn account_err(&self) -> AccountError {
        match self.account_result::<serde_json::Value>() {
            Ok(v) => panic!("expected an AccountError, got success {v}"),
            Err(e) => e,
        }
    }
}

pub async fn get_profile(c: &mut Client) -> Reply {
    c.call("get_profile", &[]).await
}

pub async fn update_name(c: &mut Client, name: &str) -> Reply {
    c.call("update_name", &[("name", name)]).await
}

pub async fn change_password(c: &mut Client, current: &str, new: &str, confirm: &str) -> Reply {
    c.call(
        "change_password",
        &[
            ("current_password", current),
            ("new_password", new),
            ("new_password_confirm", confirm),
        ],
    )
    .await
}

pub async fn request_email_change(c: &mut Client, new_email: &str, password: &str) -> Reply {
    c.call(
        "request_email_change",
        &[("new_email", new_email), ("current_password", password)],
    )
    .await
}

pub async fn cancel_email_change(c: &mut Client) -> Reply {
    c.call("cancel_email_change", &[]).await
}

pub async fn confirm_email_change(c: &mut Client, token: &str) -> Reply {
    c.call("confirm_email_change", &[("token", token)]).await
}

/// Alice (unique address, password `ALICE_PASSWORD`) logged in on a router whose mailer
/// talks to Mailpit.
pub struct World {
    pub app: Router,
    pub c: Client,
    pub id: i32,
    pub email: String,
}

pub async fn alice_world(pool: &MySqlPool) -> World {
    alice_world_with(pool, &[], Opts::mail()).await
}

pub async fn alice_world_with(pool: &MySqlPool, vars: &[(&str, &str)], opts: Opts) -> World {
    let email = unique_email("alice");
    let id = create_alice(pool, &email).await;
    let app = router(pool, vars, opts);
    let mut c = Client::new(&app);
    c.login(&email, app::test_support::ALICE_PASSWORD)
        .await
        .ok();
    World { app, c, id, email }
}

/// The token of the verification mail that arrived at `to`.
pub async fn token_from_mail(mailpit: &Mailpit, to: &str) -> String {
    let mail = mailpit.wait_for_latest(to).await;
    let link = mail
        .first_url_with_path(VERIFY)
        .expect("verification link in the mail");
    token_in(&link, VERIFY)
}

/// Number of mails to `address` after giving a wrongly sent one time to arrive.
pub async fn mails_after_settle(mailpit: &Mailpit, address: &str) -> usize {
    settle().await;
    mailpit.messages_for(address).await.len()
}

/// `SHA2(s, 256)` as the database computes it.
pub async fn sha256_hex(pool: &MySqlPool, s: &str) -> String {
    sqlx::query_scalar("SELECT SHA2(?, 256)")
        .bind(s)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A TCP port nobody listens on (mail transport that fails fast).
pub fn closed_port() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port.to_string()
}
