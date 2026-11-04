//! Helpers for the exclusions tests: the `ExclusionError` view of a server fn reply, one
//! wrapper per exclusions server fn, DB readers and the spec's fixture worlds.
#![allow(dead_code)]

use app::exclusions_api::{ExclusionError, ExclusionsPage};
use app::test_support::mailpit::unique_email;
use app::test_support::{self as ts, EVE_PASSWORD};
use serde::de::DeserializeOwned;
use sqlx::MySqlPool;

use super::auth::{Client, Reply};
use super::groups::{W, world};

pub trait ExReply {
    fn ex_result<T: DeserializeOwned>(&self) -> Result<T, ExclusionError>;
    fn ex_value<T: DeserializeOwned>(&self) -> T;
    fn ex_ok(&self) -> &Self;
    fn ex_err(&self) -> ExclusionError;
}

impl ExReply for Reply {
    fn ex_result<T: DeserializeOwned>(&self) -> Result<T, ExclusionError> {
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

    fn ex_value<T: DeserializeOwned>(&self) -> T {
        self.ex_result::<T>()
            .unwrap_or_else(|e| panic!("expected success, got {e:?}"))
    }

    fn ex_ok(&self) -> &Self {
        if let Err(err) = self.ex_result::<serde_json::Value>() {
            panic!("expected success, got {err:?}");
        }
        self
    }

    fn ex_err(&self) -> ExclusionError {
        match self.ex_result::<serde_json::Value>() {
            Ok(v) => panic!("expected an ExclusionError, got success {v}"),
            Err(e) => e,
        }
    }
}

pub async fn get_ex(c: &mut Client, group: i32) -> Reply {
    c.call("get_exclusions", &[("group_id", &group.to_string())])
        .await
}

pub async fn add_ex(c: &mut Client, group: i32, target: i32) -> Reply {
    c.call(
        "add_exclusion",
        &[
            ("group_id", &group.to_string()),
            ("excluded_user_id", &target.to_string()),
        ],
    )
    .await
}

pub async fn remove_ex(c: &mut Client, group: i32, target: i32) -> Reply {
    c.call(
        "remove_exclusion",
        &[
            ("group_id", &group.to_string()),
            ("excluded_user_id", &target.to_string()),
        ],
    )
    .await
}

/// The caller's page data.
pub async fn page(c: &mut Client, group: i32) -> ExclusionsPage {
    get_ex(c, group).await.ex_value()
}

/// `/groups/{g}/exclusions`
pub fn ex_path(g: i32) -> String {
    format!("/groups/{g}/exclusions")
}

/// All rules of the group as `(user_id, excluded_user_id)`, ordered.
pub async fn rules(pool: &MySqlPool, g: i32) -> Vec<(i32, i32)> {
    sqlx::query_as::<_, (i32, i32)>(
        "SELECT user_id, excluded_user_id FROM exclusion_rules WHERE group_id = ? \
         ORDER BY user_id, excluded_user_id",
    )
    .bind(g)
    .fetch_all(pool)
    .await
    .expect("read rules")
}

/// Every rule of every group, for "nothing changed" assertions.
pub async fn all_rules(pool: &MySqlPool) -> Vec<(i32, i32, i32)> {
    sqlx::query_as::<_, (i32, i32, i32)>(
        "SELECT group_id, user_id, excluded_user_id FROM exclusion_rules \
         ORDER BY group_id, user_id, excluded_user_id",
    )
    .fetch_all(pool)
    .await
    .expect("read all rules")
}

pub async fn unset_drawn(pool: &MySqlPool, g: i32) {
    sqlx::query("UPDATE `groups` SET is_drawn = 0 WHERE id = ?")
        .bind(g)
        .execute(pool)
        .await
        .expect("unset drawn");
}

/// The spec's world: users Alice, Bob, Carol, Dave, Eve. `x.w.g` is **G** (admin Alice,
/// members Alice, Bob, Carol, Dave, not drawn). Eve is in no group.
pub struct X {
    pub w: W,
    pub eve: i32,
    pub eve_email: String,
}

impl X {
    pub fn g(&self) -> i32 {
        self.w.g
    }
    pub fn alice(&self) -> i32 {
        self.w.alice
    }
    pub fn bob(&self) -> i32 {
        self.w.bob
    }
    pub fn carol(&self) -> i32 {
        self.w.carol
    }
    pub fn dave(&self) -> i32 {
        self.w.dave
    }

    pub async fn eve_client(&self) -> Client {
        let mut c = Client::new(&self.w.app);
        c.login(&self.eve_email, EVE_PASSWORD).await.ok();
        c
    }

    /// A new group with admin Alice and the given members besides her.
    pub async fn group_of(&self, name: &str, others: &[i32], pool: &MySqlPool) -> i32 {
        let g = ts::create_group(pool, self.w.alice, name).await;
        for user in others {
            ts::add_member(pool, g, *user).await;
        }
        g
    }

    /// **G3**: Alice (admin), Bob, Carol.
    pub async fn g3(&self, pool: &MySqlPool) -> i32 {
        self.group_of("Trio", &[self.w.bob, self.w.carol], pool)
            .await
    }

    /// **G2**: Alice (admin), Bob.
    pub async fn g2(&self, pool: &MySqlPool) -> i32 {
        self.group_of("Duo", &[self.w.bob], pool).await
    }

    /// **G8**: Alice (admin), Bob and six more members named "M1".."M6"; returns the group
    /// and the six extra user ids.
    pub async fn g8(&self, pool: &MySqlPool) -> (i32, Vec<i32>) {
        let g = self.group_of("Octet", &[self.w.bob], pool).await;
        let mut extra = vec![];
        for i in 1..=6 {
            let id = ts::create_user(pool, &format!("M{i}"), &unique_email("m")).await;
            ts::add_member(pool, g, id).await;
            extra.push(id);
        }
        (g, extra)
    }
}

/// G (Alice admin; Alice, Bob, Carol, Dave), not drawn, no rules.
pub async fn x_world(pool: &MySqlPool) -> X {
    let w = world(pool).await;
    ts::add_member(pool, w.g, w.carol).await;
    ts::add_member(pool, w.g, w.dave).await;
    let eve_email = unique_email("eve");
    let eve = ts::create_eve(pool, &eve_email).await;
    X { w, eve, eve_email }
}

/// The `<option>`s of the first `<select>` of the page as `(value, label)`, without the
/// placeholder (empty value).
pub fn select_options(html: &str) -> Vec<(String, String)> {
    let Some(start) = html.find("<select") else {
        return vec![];
    };
    let rest = &html[start..];
    let rest = &rest[..rest.find("</select>").unwrap_or(rest.len())];
    let mut out = vec![];
    let mut cursor = rest;
    while let Some(at) = cursor.find("<option") {
        cursor = &cursor[at..];
        let tag_end = cursor.find('>').expect("option tag end");
        let tag = &cursor[..tag_end];
        let body_end = cursor.find("</option>").unwrap_or(cursor.len());
        let label = cursor[tag_end + 1..body_end].trim().to_string();
        let value = tag
            .split("value=\"")
            .nth(1)
            .and_then(|v| v.split('"').next())
            .unwrap_or("")
            .to_string();
        if !value.is_empty() {
            out.push((value, label));
        }
        cursor = &cursor[body_end.min(cursor.len())..];
        if cursor.starts_with("</option>") {
            cursor = &cursor["</option>".len()..];
        }
    }
    out
}
