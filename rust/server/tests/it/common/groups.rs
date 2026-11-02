//! Helpers for the groups tests: the `GroupError` view of a server fn reply, one wrapper
//! per groups server fn, and the spec's fixture world (users Alice, Bob, Carol, Dave and
//! group G).
#![allow(dead_code)]

use app::groups_api::{GroupDetail, GroupError, GroupSummary, InvitationPreview};
use app::i18n::{Locale, translate, translate_with};
use app::test_support::mailpit::unique_email;
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, CAROL_PASSWORD, DAVE_PASSWORD, add_member, create_alice,
    create_bob, create_carol, create_dave, create_group_with_code,
};
use axum::Router;
use serde::de::DeserializeOwned;
use sqlx::MySqlPool;

use super::auth::{Client, Opts, Reply, router};

/// What `APP_URL` is in the tests.
pub const APP_URL: &str = "http://localhost:3000";
/// The invitation code of group G.
pub const CODE: &str = "ABCD2345";
/// The characters a code may consist of.
pub const ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// English catalog text of `key`; panics when the key is missing from `locales/en.json`
/// (`translate` would fall back to the key itself and make a raw-key page pass).
pub fn en(key: &str) -> String {
    let text = translate(Locale::En, key);
    assert_ne!(text, key, "key {key} is missing from locales/en.json");
    text
}

pub fn de(key: &str) -> String {
    let text = translate(Locale::De, key);
    assert_ne!(text, key, "key {key} is missing from locales/de.json");
    text
}

pub fn en_with(key: &str, params: &[(&str, &str)]) -> String {
    let text = translate_with(Locale::En, key, params);
    assert_ne!(text, key, "key {key} is missing from locales/en.json");
    text
}

pub fn de_with(key: &str, params: &[(&str, &str)]) -> String {
    let text = translate_with(Locale::De, key, params);
    assert_ne!(text, key, "key {key} is missing from locales/de.json");
    text
}

/// `GroupError` view of a server fn reply (2xx: the JSON value, else the JSON error).
pub trait GroupReply {
    fn group_result<T: DeserializeOwned>(&self) -> Result<T, GroupError>;
    /// The success value; panics with the error otherwise.
    fn value<T: DeserializeOwned>(&self) -> T;
    /// Panics unless the call succeeded.
    fn group_ok(&self) -> &Self;
    /// The error; panics when the call succeeded.
    fn group_err(&self) -> GroupError;
}

impl GroupReply for Reply {
    fn group_result<T: DeserializeOwned>(&self) -> Result<T, GroupError> {
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

    fn value<T: DeserializeOwned>(&self) -> T {
        self.group_result::<T>()
            .unwrap_or_else(|e| panic!("expected success, got {e:?}"))
    }

    fn group_ok(&self) -> &Self {
        if let Err(err) = self.group_result::<serde_json::Value>() {
            panic!("expected success, got {err:?}");
        }
        self
    }

    fn group_err(&self) -> GroupError {
        match self.group_result::<serde_json::Value>() {
            Ok(v) => panic!("expected a GroupError, got success {v}"),
            Err(e) => e,
        }
    }
}

pub async fn list_my_groups(c: &mut Client) -> Reply {
    c.call("list_my_groups", &[]).await
}

pub async fn get_group(c: &mut Client, group_id: i32) -> Reply {
    c.call("get_group", &[("group_id", &group_id.to_string())])
        .await
}

pub async fn create_group(
    c: &mut Client,
    name: &str,
    description: &str,
    deadline: &str,
    draw_date: &str,
) -> Reply {
    c.call(
        "create_group",
        &[
            ("name", name),
            ("description", description),
            ("registration_deadline", deadline),
            ("draw_date", draw_date),
        ],
    )
    .await
}

pub async fn update_group(
    c: &mut Client,
    group_id: i32,
    name: &str,
    description: &str,
    deadline: &str,
    draw_date: &str,
) -> Reply {
    c.call(
        "update_group",
        &[
            ("group_id", &group_id.to_string()),
            ("name", name),
            ("description", description),
            ("registration_deadline", deadline),
            ("draw_date", draw_date),
        ],
    )
    .await
}

pub async fn delete_group(c: &mut Client, group_id: i32) -> Reply {
    c.call("delete_group", &[("group_id", &group_id.to_string())])
        .await
}

pub async fn regenerate_invitation(c: &mut Client, group_id: i32) -> Reply {
    c.call(
        "regenerate_invitation",
        &[("group_id", &group_id.to_string())],
    )
    .await
}

pub async fn preview_invitation(c: &mut Client, code: &str) -> Reply {
    c.call("preview_invitation", &[("code", code)]).await
}

pub async fn join_group(c: &mut Client, code: &str) -> Reply {
    c.call("join_group", &[("code", code)]).await
}

pub async fn leave_group(c: &mut Client, group_id: i32) -> Reply {
    c.call("leave_group", &[("group_id", &group_id.to_string())])
        .await
}

pub async fn remove_member(c: &mut Client, group_id: i32, user_id: i32) -> Reply {
    c.call(
        "remove_member",
        &[
            ("group_id", &group_id.to_string()),
            ("user_id", &user_id.to_string()),
        ],
    )
    .await
}

pub async fn transfer_admin(c: &mut Client, group_id: i32, user_id: i32) -> Reply {
    c.call(
        "transfer_admin",
        &[
            ("group_id", &group_id.to_string()),
            ("user_id", &user_id.to_string()),
        ],
    )
    .await
}

pub async fn detail(c: &mut Client, group_id: i32) -> GroupDetail {
    get_group(c, group_id).await.value()
}

pub async fn summaries(c: &mut Client) -> Vec<GroupSummary> {
    list_my_groups(c).await.value()
}

pub async fn preview(c: &mut Client, code: &str) -> InvitationPreview {
    preview_invitation(c, code).await.value()
}

/// The spec's fixture: Alice, Bob, Carol, Dave (unique addresses) and group G with admin
/// Alice, member Bob, code [`CODE`], not drawn, no dates.
pub struct W {
    pub app: Router,
    pub alice: i32,
    pub bob: i32,
    pub carol: i32,
    pub dave: i32,
    pub emails: [String; 4],
    pub g: i32,
}

pub async fn world(pool: &MySqlPool) -> W {
    world_with(pool, &[], Opts::default()).await
}

pub async fn world_with(pool: &MySqlPool, vars: &[(&str, &str)], opts: Opts) -> W {
    let emails = [
        unique_email("alice"),
        unique_email("bob"),
        unique_email("carol"),
        unique_email("dave"),
    ];
    let alice = create_alice(pool, &emails[0]).await;
    let bob = create_bob(pool, &emails[1]).await;
    let carol = create_carol(pool, &emails[2]).await;
    let dave = create_dave(pool, &emails[3]).await;
    let g = create_group_with_code(pool, alice, "Family", CODE).await;
    add_member(pool, g, bob).await;
    W {
        app: router(pool, vars, opts),
        alice,
        bob,
        carol,
        dave,
        emails,
        g,
    }
}

impl W {
    async fn login(&self, email: &str, password: &str) -> Client {
        let mut c = Client::new(&self.app);
        c.login(email, password).await.ok();
        c
    }

    pub async fn alice(&self) -> Client {
        self.login(&self.emails[0], ALICE_PASSWORD).await
    }

    pub async fn bob(&self) -> Client {
        self.login(&self.emails[1], BOB_PASSWORD).await
    }

    pub async fn carol(&self) -> Client {
        self.login(&self.emails[2], CAROL_PASSWORD).await
    }

    pub async fn dave(&self) -> Client {
        self.login(&self.emails[3], DAVE_PASSWORD).await
    }

    pub fn anon(&self) -> Client {
        Client::new(&self.app)
    }

    /// A browser of `user` in German.
    pub async fn german(&self, email: &str, password: &str) -> Client {
        let mut c = Client::new(&self.app).with_cookie("locale", "de");
        c.login(email, password).await.ok();
        c
    }
}

/// The stored code of a freshly normalised input, for comparisons.
pub fn is_valid_code(code: &str) -> bool {
    code.chars().count() == 8 && code.chars().all(|c| ALPHABET.contains(c))
}

/// A string of `n` times `c`.
pub fn repeat(c: char, n: usize) -> String {
    c.to_string().repeat(n)
}

/// The page without the per-request CSP nonce (it also appears as `crossorigin="..."`), so
/// two renderings of the same page compare equal.
pub fn without_nonce(body: &str) -> String {
    match body.split_once("nonce=\"") {
        Some((_, rest)) => {
            let nonce = rest.split('"').next().unwrap_or_default();
            if nonce.is_empty() {
                body.to_string()
            } else {
                body.replace(nonce, "NONCE")
            }
        }
        None => body.to_string(),
    }
}
