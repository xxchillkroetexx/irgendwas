//! Client for the Mailpit HTTP API (dev compose: SMTP `127.0.0.1:1026`, API
//! `http://127.0.0.1:8026`), for tests that read tokens out of mails.
//!
//! Tests run in parallel against one Mailpit, so **send to a unique address**
//! ([`unique_email`]) and query by that address; [`Mailpit::delete_all`] is only safe in a
//! test binary that has a single mail test.
//!
//! ```ignore
//! let mailpit = Mailpit::from_env();
//! let to = unique_email("reset");
//! // ... trigger the mail ...
//! let mail = mailpit.wait_for_latest(&to).await;
//! let link = mail.first_url_with_path("/auth/reset/").expect("reset link");
//! ```
//!
//! Environment (all optional): `MAILPIT_URL` (API base, default `http://127.0.0.1:8026`),
//! `MAILPIT_SMTP_HOST` (default `127.0.0.1`), `MAILPIT_SMTP_PORT` (default `1026`).

use std::time::Duration;

use serde::Deserialize;

const DEFAULT_API: &str = "http://127.0.0.1:8026";
const DEFAULT_SMTP_HOST: &str = "127.0.0.1";
const DEFAULT_SMTP_PORT: &str = "1026";
/// How long [`Mailpit::wait_for_latest`] polls before panicking.
pub const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// An address nobody else in this test run uses (`{prefix}-{pid}-{n}@mail.test`).
pub fn unique_email(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}@mail.test",
        std::process::id(),
        super::next()
    )
}

/// `MAIL_*` variables that point the app's [`Mailer`](crate::server::mail::Mailer) at
/// Mailpit, for `Config::from_lookup` / the test `config_with` helper.
pub fn smtp_vars() -> Vec<(&'static str, String)> {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
    vec![
        ("MAIL_HOST", env("MAILPIT_SMTP_HOST", DEFAULT_SMTP_HOST)),
        ("MAIL_PORT", env("MAILPIT_SMTP_PORT", DEFAULT_SMTP_PORT)),
        ("MAIL_ENCRYPTION", "none".into()),
        ("MAIL_FROM_ADDRESS", "santa@mail.test".into()),
        ("MAIL_FROM_NAME", "Secret Santa".into()),
    ]
}

/// One entry of a message list.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageSummary {
    #[serde(rename = "ID")]
    pub id: String,
    pub subject: String,
    #[serde(default)]
    pub to: Vec<Address>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Address {
    #[serde(default)]
    pub name: String,
    pub address: String,
}

/// A full message with its bodies.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Mail {
    #[serde(rename = "ID")]
    pub id: String,
    pub subject: String,
    #[serde(default)]
    pub from: Option<Address>,
    #[serde(default)]
    pub to: Vec<Address>,
    #[serde(default)]
    pub text: String,
    #[serde(rename = "HTML", default)]
    pub html: String,
}

impl Mail {
    /// The first absolute URL whose path starts with `path_prefix`, from the text body,
    /// else from the HTML body. See [`first_url_with_path`].
    pub fn first_url_with_path(&self, path_prefix: &str) -> Option<String> {
        first_url_with_path(&self.text, path_prefix)
            .or_else(|| first_url_with_path(&self.html, path_prefix))
    }
}

#[derive(Deserialize)]
struct MessageList {
    messages: Vec<MessageSummary>,
}

pub struct Mailpit {
    base: String,
    http: reqwest::Client,
}

impl Mailpit {
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Uses `MAILPIT_URL`, default `http://127.0.0.1:8026`.
    pub fn from_env() -> Self {
        Self::new(std::env::var("MAILPIT_URL").unwrap_or_else(|_| DEFAULT_API.into()))
    }

    /// Messages sent to exactly `address` (case-insensitive), newest first.
    pub async fn messages_for(&self, address: &str) -> Vec<MessageSummary> {
        let list: MessageList = self
            .http
            .get(format!("{}/api/v1/search", self.base))
            .query(&[
                ("query", format!("to:\"{address}\"")),
                ("limit", "100".into()),
            ])
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .expect("mailpit search")
            .json()
            .await
            .expect("mailpit search json");
        list.messages
            .into_iter()
            .filter(|m| m.to.iter().any(|a| a.address.eq_ignore_ascii_case(address)))
            .collect()
    }

    /// One message with text and HTML bodies.
    pub async fn message(&self, id: &str) -> Mail {
        self.http
            .get(format!("{}/api/v1/message/{id}", self.base))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .expect("mailpit message")
            .json()
            .await
            .expect("mailpit message json")
    }

    /// The newest message to `address`, if any.
    pub async fn latest_for(&self, address: &str) -> Option<Mail> {
        let first = self.messages_for(address).await.into_iter().next()?;
        Some(self.message(&first.id).await)
    }

    /// Polls until a message to `address` arrives (mail is sent off the request path).
    /// Panics after [`WAIT_TIMEOUT`].
    pub async fn wait_for_latest(&self, address: &str) -> Mail {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            if let Some(mail) = self.latest_for(address).await {
                return mail;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "no mail to {address} within {WAIT_TIMEOUT:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Deletes every message to `address`.
    pub async fn delete_for(&self, address: &str) {
        let ids: Vec<String> = self
            .messages_for(address)
            .await
            .into_iter()
            .map(|m| m.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        self.http
            .delete(format!("{}/api/v1/messages", self.base))
            .json(&serde_json::json!({ "IDs": ids }))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .expect("mailpit delete");
    }

    /// Deletes **all** messages in Mailpit (affects parallel tests; see module docs).
    pub async fn delete_all(&self) {
        self.http
            .delete(format!("{}/api/v1/messages", self.base))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .expect("mailpit delete all");
    }
}

/// The first `http(s)://` URL in `body` whose path starts with `path_prefix`. Handles
/// plain text and HTML (`&amp;` is decoded; quotes and `<>` end a URL; trailing `.,;:!?)`
/// are dropped).
pub fn first_url_with_path(body: &str, path_prefix: &str) -> Option<String> {
    let mut rest = body;
    while let Some(start) = find_scheme(rest) {
        let candidate = &rest[start..];
        let end = candidate
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>'))
            .unwrap_or(candidate.len());
        let url = candidate[..end]
            .trim_end_matches(['.', ',', ';', ':', '!', '?', ')'])
            .replace("&amp;", "&");
        if url_path(&url).is_some_and(|p| p.starts_with(path_prefix)) {
            return Some(url);
        }
        rest = &candidate[end.max(1)..];
    }
    None
}

fn find_scheme(s: &str) -> Option<usize> {
    match (s.find("http://"), s.find("https://")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn url_path(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://")?.1;
    let slash = after_scheme.find('/')?;
    Some(&after_scheme[slash..])
}
