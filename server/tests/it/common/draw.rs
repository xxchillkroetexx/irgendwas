//! Helpers for the draw tests: the `DrawApiError` view of a server fn reply, one wrapper per
//! draw server fn, a mail transport the tests can make fail or block, DB readers, mail and
//! HTML helpers, and the spec's fixture world.
//!
//! UI contract the tests rely on (the implementer must render these `data-testid`s; they
//! are the only way to tell "an assignment element" from other page text):
//! - `draw-panel`: the whole `<DrawPanel>` (group page).
//! - `draw-pending`: the non-admin "not drawn yet" text.
//! - `draw-perform`: the admin's "Perform draw" button (`disabled` attribute when blocked).
//!   It is the only draw entry point: exactly one such element, and no other element
//!   linking to a draw.
//! - `draw-blocker`: the blocker text below the button (only when blocked).
//! - `my-assignment`: the assignment card (exactly one per page). `assignment-receiver`:
//!   the receiver's name element inside it. `assignment-wishlist`: the `<a>` to the
//!   receiver's wishlist inside it. `assignment-drawn-on`: the "Drawn on {date}" element.
//! - `draw-notifications`: the admin's notifications section. `notifications-summary`,
//!   `notifications-pending`, `notifications-failed`: its text lines (pending/failed only
//!   when shown). `draw-resend`: the resend button (only when failed > 0). `draw-reset`:
//!   the reset button.
//! - `draw-ready`, `draw-small-hint`: the "{count} members will take part" line and the
//!   small-group hint.
//! - Dashboard: `dashboard-assignments` (the card), one `dashboard-assignment` per row
//!   (containing a link to `/groups/{id}` with the group name, the receiver's name, and an
//!   `<a>` to `/groups/{id}/wishlist/{receiver}`), `dashboard-assignments-empty` (the empty
//!   texts).
//! - The no-JS forms are `<form method="post" action="/api/perform_draw">` etc. with a
//!   hidden `group_id`; a plain post answers 302 back to `/groups/{id}` with a flash.
#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use app::draw_api::{DrawAdminStatus, DrawApiError, MyAssignment, MyAssignmentSummary};
use app::server::draw_worker::{DrawWorker, MailTransport, SendFuture};
use app::server::mail::{MailError, Mailer, Message};
use app::test_support::mailpit::{Mail, Mailpit, smtp_vars, unique_email};
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, CAROL_PASSWORD, DAVE_PASSWORD, add_member, create_alice,
    create_bob, create_carol, create_dave, create_group_with_code,
};
use axum::Router;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use sqlx::MySqlPool;
use tokio::sync::Notify;

use super::auth::{Client, Reply};
use super::groups::{CODE, de_with, en_with};
use super::{config_with, state};
use app::i18n::{I18n, Locale};

// ------------------------------------------------------------------ error view of replies

pub trait DrawReply {
    fn dr_result<T: DeserializeOwned>(&self) -> Result<T, DrawApiError>;
    fn dr_value<T: DeserializeOwned>(&self) -> T;
    fn dr_ok(&self) -> &Self;
    fn dr_err(&self) -> DrawApiError;
}

impl DrawReply for Reply {
    fn dr_result<T: DeserializeOwned>(&self) -> Result<T, DrawApiError> {
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

    fn dr_value<T: DeserializeOwned>(&self) -> T {
        self.dr_result::<T>()
            .unwrap_or_else(|e| panic!("expected success, got {e:?}"))
    }

    fn dr_ok(&self) -> &Self {
        if let Err(err) = self.dr_result::<serde_json::Value>() {
            panic!("expected success, got {err:?}");
        }
        self
    }

    fn dr_err(&self) -> DrawApiError {
        match self.dr_result::<serde_json::Value>() {
            Ok(v) => panic!("expected a DrawApiError, got success {v}"),
            Err(e) => e,
        }
    }
}

pub async fn perform(c: &mut Client, group: i32) -> Reply {
    c.call("perform_draw", &[("group_id", &group.to_string())])
        .await
}

pub async fn reset(c: &mut Client, group: i32) -> Reply {
    c.call("reset_draw", &[("group_id", &group.to_string())])
        .await
}

pub async fn resend(c: &mut Client, group: i32) -> Reply {
    c.call(
        "resend_draw_notifications",
        &[("group_id", &group.to_string())],
    )
    .await
}

pub async fn status_reply(c: &mut Client, group: i32) -> Reply {
    c.call("get_draw_admin_status", &[("group_id", &group.to_string())])
        .await
}

pub async fn status(c: &mut Client, group: i32) -> DrawAdminStatus {
    status_reply(c, group).await.dr_value()
}

pub async fn my_assignment_reply(c: &mut Client, group: i32) -> Reply {
    c.call("get_my_assignment", &[("group_id", &group.to_string())])
        .await
}

pub async fn my_assignment(c: &mut Client, group: i32) -> Option<MyAssignment> {
    my_assignment_reply(c, group).await.dr_value()
}

pub async fn list_mine_reply(c: &mut Client) -> Reply {
    c.call("list_my_assignments", &[]).await
}

pub async fn list_mine(c: &mut Client) -> Vec<MyAssignmentSummary> {
    list_mine_reply(c).await.dr_value()
}

// ------------------------------------------------------------------ the mail transport

/// Sends through the real (Mailpit-backed) [`Mailer`], unless told to fail for an address or
/// to block. Records every attempt.
pub struct TestTransport {
    mailer: Mailer,
    failing: Mutex<HashSet<String>>,
    blocked: Mutex<bool>,
    gate: Notify,
    attempts: Mutex<Vec<String>>,
}

impl TestTransport {
    pub fn new(mailer: Mailer) -> Arc<Self> {
        Arc::new(TestTransport {
            mailer,
            failing: Mutex::new(HashSet::new()),
            blocked: Mutex::new(false),
            gate: Notify::new(),
            attempts: Mutex::new(Vec::new()),
        })
    }

    /// Every send to `address` fails until [`heal`](Self::heal).
    pub fn fail(&self, address: &str) {
        self.failing.lock().unwrap().insert(address.to_lowercase());
    }

    pub fn heal(&self, address: &str) {
        self.failing.lock().unwrap().remove(&address.to_lowercase());
    }

    /// Sends wait (before touching SMTP) until [`unblock`](Self::unblock).
    pub fn block(&self) {
        *self.blocked.lock().unwrap() = true;
    }

    pub fn unblock(&self) {
        *self.blocked.lock().unwrap() = false;
        self.gate.notify_waiters();
    }

    /// Number of sends started to `address` (failed ones included).
    pub fn attempts_to(&self, address: &str) -> usize {
        let address = address.to_lowercase();
        self.attempts
            .lock()
            .unwrap()
            .iter()
            .filter(|a| **a == address)
            .count()
    }

    /// Number of sends started in total.
    pub fn attempts_total(&self) -> usize {
        self.attempts.lock().unwrap().len()
    }
}

impl MailTransport for TestTransport {
    fn send(&self, message: Message) -> SendFuture<'_> {
        Box::pin(async move {
            let to = message.to.to_lowercase();
            self.attempts.lock().unwrap().push(to.clone());
            loop {
                // Created before the check, so an `unblock` in between is not missed.
                let notified = self.gate.notified();
                let blocked = *self.blocked.lock().unwrap();
                if !blocked {
                    break;
                }
                notified.await;
            }
            let fails = self.failing.lock().unwrap().contains(&to);
            if fails {
                return Err(MailError::Smtp("injected failure".to_string()));
            }
            self.mailer.send(message).outcome().await
        })
    }
}

// ------------------------------------------------------------------ the world

/// Users Alice (admin), Bob, Carol, Dave (outside) with unique addresses, group G =
/// {Alice, Bob, Carol} (code [`CODE`]), not drawn, no rules, no dates. The app runs with a
/// worker that sends through [`TestTransport`] with zero retry delays.
pub struct D {
    pub app: Router,
    pub pool: MySqlPool,
    pub transport: Arc<TestTransport>,
    pub worker: DrawWorker,
    pub mailpit: Mailpit,
    pub alice: i32,
    pub bob: i32,
    pub carol: i32,
    pub dave: i32,
    pub emails: [String; 4],
    pub g: i32,
}

pub const GROUP_NAME: &str = "Family";

pub async fn draw_world(pool: &MySqlPool) -> D {
    draw_world_with(pool, &[]).await
}

pub async fn draw_world_with(pool: &MySqlPool, vars: &[(&str, &str)]) -> D {
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
    let g = create_group_with_code(pool, alice, GROUP_NAME, CODE).await;
    add_member(pool, g, bob).await;
    add_member(pool, g, carol).await;

    let mut all: Vec<(String, String)> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    all.extend(smtp_vars().into_iter().map(|(k, v)| (k.to_string(), v)));
    let borrowed: Vec<(&str, &str)> = all.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut st = state(pool.clone(), config_with(&borrowed));
    let transport = TestTransport::new(Mailer::new(&st.config.mail).expect("mailer"));
    let worker = DrawWorker::new(
        pool.clone(),
        st.config.clone(),
        transport.clone(),
        vec![Duration::ZERO, Duration::ZERO],
    );
    st.draw_worker = worker.clone();
    D {
        app: server::router_with(st, Router::new()),
        pool: pool.clone(),
        transport,
        worker,
        mailpit: Mailpit::from_env(),
        alice,
        bob,
        carol,
        dave,
        emails,
        g,
    }
}

impl D {
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

    /// A logged-in browser in German (cookie `locale=de`).
    pub async fn german(&self, user: &str) -> Client {
        let (email, password) = match user {
            "alice" => (&self.emails[0], ALICE_PASSWORD),
            "bob" => (&self.emails[1], BOB_PASSWORD),
            "carol" => (&self.emails[2], CAROL_PASSWORD),
            _ => (&self.emails[3], DAVE_PASSWORD),
        };
        let mut c = Client::new(&self.app).with_cookie("locale", "de");
        c.login(email, password).await.ok();
        c
    }

    pub fn anon(&self) -> Client {
        Client::new(&self.app)
    }

    pub fn email_of(&self, user: i32) -> &str {
        if user == self.alice {
            &self.emails[0]
        } else if user == self.bob {
            &self.emails[1]
        } else if user == self.carol {
            &self.emails[2]
        } else if user == self.dave {
            &self.emails[3]
        } else {
            panic!("no fixture user {user}")
        }
    }

    pub fn name_of(&self, user: i32) -> &'static str {
        if user == self.alice {
            "Alice"
        } else if user == self.bob {
            "Bob"
        } else if user == self.carol {
            "Carol"
        } else {
            "Dave"
        }
    }

    /// Waits until the worker has nothing left to do.
    pub async fn idle(&self) {
        self.worker.wait_idle().await;
    }

    /// Alice performs the draw of `g` and the worker finishes delivering.
    pub async fn draw_and_deliver(&self, g: i32) {
        let mut alice = self.alice().await;
        perform(&mut alice, g).await.dr_ok();
        self.idle().await;
    }

    /// Every mail to the user's address, newest first, with bodies.
    pub async fn mails(&self, user: i32) -> Vec<Mail> {
        self.mailpit.wait_for_count(self.email_of(user), 0).await
    }

    /// Mails to `user` that are draw-result mails of a group called `group`.
    pub async fn draw_mails(&self, user: i32, group: &str) -> Vec<Mail> {
        self.mails(user)
            .await
            .into_iter()
            .filter(|m| kind(m, group) == MailKind::Draw)
            .collect()
    }

    pub async fn reset_mails(&self, user: i32, group: &str) -> Vec<Mail> {
        self.mails(user)
            .await
            .into_iter()
            .filter(|m| kind(m, group) == MailKind::Reset)
            .collect()
    }

    pub async fn mail_count(&self, user: i32) -> usize {
        self.mails(user).await.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailKind {
    Draw,
    Reset,
    Other,
}

/// Classifies a mail by its subject (en or de) for a group called `group`.
pub fn kind(mail: &Mail, group: &str) -> MailKind {
    let p = [("group", group)];
    let draw = [
        en_with("email.drawResult.subject", &p),
        de_with("email.drawResult.subject", &p),
    ];
    let reset = [
        en_with("email.drawReset.subject", &p),
        de_with("email.drawReset.subject", &p),
    ];
    if draw.contains(&mail.subject) {
        MailKind::Draw
    } else if reset.contains(&mail.subject) {
        MailKind::Reset
    } else {
        MailKind::Other
    }
}

// ------------------------------------------------------------------ DB readers

/// `(giver, receiver)` rows of the group, ordered by giver.
pub async fn assignments(pool: &MySqlPool, g: i32) -> Vec<(i32, i32)> {
    sqlx::query_as::<_, (i32, i32)>(
        "SELECT giver_id, receiver_id FROM gift_assignments WHERE group_id = ? ORDER BY giver_id",
    )
    .bind(g)
    .fetch_all(pool)
    .await
    .expect("read assignments")
}

/// `(giver, notification_sent, notification_attempts)` of the group, ordered by giver.
pub async fn delivery(pool: &MySqlPool, g: i32) -> Vec<(i32, bool, u8)> {
    sqlx::query_as::<_, (i32, bool, u8)>(
        "SELECT giver_id, notification_sent, notification_attempts FROM gift_assignments \
         WHERE group_id = ? ORDER BY giver_id",
    )
    .bind(g)
    .fetch_all(pool)
    .await
    .expect("read delivery state")
}

pub async fn is_drawn(pool: &MySqlPool, g: i32) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT is_drawn FROM `groups` WHERE id = ?")
        .bind(g)
        .fetch_one(pool)
        .await
        .expect("read is_drawn")
}

pub async fn drawn_at(pool: &MySqlPool, g: i32) -> Option<DateTime<Utc>> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>("SELECT drawn_at FROM `groups` WHERE id = ?")
        .bind(g)
        .fetch_one(pool)
        .await
        .expect("read drawn_at")
}

pub async fn set_drawn_at(pool: &MySqlPool, g: i32, at: &str) {
    sqlx::query("UPDATE `groups` SET drawn_at = ? WHERE id = ?")
        .bind(at)
        .bind(g)
        .execute(pool)
        .await
        .expect("set drawn_at");
}

pub async fn rename(pool: &MySqlPool, user: i32, name: &str) {
    sqlx::query("UPDATE users SET name = ? WHERE id = ?")
        .bind(name)
        .bind(user)
        .execute(pool)
        .await
        .expect("rename user");
}

pub async fn rename_group(pool: &MySqlPool, g: i32, name: &str) {
    sqlx::query("UPDATE `groups` SET name = ? WHERE id = ?")
        .bind(name)
        .bind(g)
        .execute(pool)
        .await
        .expect("rename group");
}

/// Sets the invariants of G1: a permutation of `members` without self-assignment that
/// honours every rule. Panics with a description otherwise.
pub fn assert_valid(pairs: &[(i32, i32)], members: &[i32], rules: &[(i32, i32)]) {
    let mut givers: Vec<i32> = pairs.iter().map(|p| p.0).collect();
    let mut receivers: Vec<i32> = pairs.iter().map(|p| p.1).collect();
    givers.sort_unstable();
    receivers.sort_unstable();
    let mut expected = members.to_vec();
    expected.sort_unstable();
    expected.dedup();
    assert_eq!(givers, expected, "every member gives exactly once");
    assert_eq!(receivers, expected, "every member receives exactly once");
    for (giver, receiver) in pairs {
        assert_ne!(giver, receiver, "self assignment");
        assert!(
            !rules.contains(&(*giver, *receiver)),
            "rule ({giver}, {receiver}) violated"
        );
    }
}

/// Unordered pairs {a, b} with a -> b and b -> a.
pub fn reciprocal_pairs(pairs: &[(i32, i32)]) -> usize {
    pairs
        .iter()
        .filter(|(a, b)| a < b && pairs.contains(&(*b, *a)))
        .count()
}

// ------------------------------------------------------------------ HTML helpers

pub fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// Number of elements carrying `data-testid="id"`.
pub fn testid_count(html: &str, id: &str) -> usize {
    count(html, &format!("data-testid=\"{id}\""))
}

/// The opening tag of the first element with `data-testid="id"`.
pub fn open_tag<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let at = html.find(&format!("data-testid=\"{id}\""))?;
    let start = html[..at].rfind('<')?;
    let end = at + html[at..].find('>')? + 1;
    Some(&html[start..end])
}

/// The whole first element (opening tag to matching closing tag) with `data-testid="id"`.
pub fn block<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let at = html.find(&format!("data-testid=\"{id}\""))?;
    let start = html[..at].rfind('<')?;
    let name: String = html[start + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let boundary = |rest: &str, at: usize| {
        rest[at..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/')
    };
    let mut depth = 1usize;
    let mut i = start + open.len();
    loop {
        let rest = &html[i..];
        let next_open = rest.find(&open);
        let next_close = rest.find(&close)?;
        match next_open {
            Some(o) if o < next_close => {
                if boundary(rest, o + open.len()) {
                    depth += 1;
                }
                i += o + open.len();
            }
            _ => {
                depth -= 1;
                i += next_close + close.len();
                if depth == 0 {
                    return Some(&html[start..i]);
                }
            }
        }
    }
}

/// The visible text of an HTML fragment: tags removed, whitespace collapsed.
pub fn text_of(fragment: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in fragment.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A mail's visible content (subject, plain text, HTML) for "does it mention X" checks.
pub fn everything(mail: &Mail) -> String {
    format!("{}\n{}\n{}", mail.subject, mail.text, mail.html)
}

/// The value of `attr` in an opening tag.
pub fn attr<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let needle = format!("{attr}=\"");
    let at = tag.find(&needle)? + needle.len();
    Some(&tag[at..at + tag[at..].find('"')?])
}

/// Whether an opening tag has the boolean attribute (`disabled`).
pub fn has_flag(tag: &str, flag: &str) -> bool {
    tag.split(|c: char| c.is_whitespace() || c == '>' || c == '<')
        .any(|w| w == flag || w.starts_with(&format!("{flag}=")))
}

/// The plural form of `base` for `n` in `locale` (UTC time zone); panics on a missing key.
pub fn plural(locale: Locale, base: &str, n: i64, params: &[(&str, &str)]) -> String {
    let text = I18n::new(locale, chrono_tz::UTC).t_plural(base, n, params);
    assert!(
        !text.starts_with(base),
        "plural key {base}.one/.other is missing from the {locale:?} catalog"
    );
    text
}

pub fn en_plural(base: &str, n: i64, params: &[(&str, &str)]) -> String {
    plural(Locale::En, base, n, params)
}

pub fn de_plural(base: &str, n: i64, params: &[(&str, &str)]) -> String {
    plural(Locale::De, base, n, params)
}

/// The page (HTML + hydration payload) carries at most the caller's own assignment: no
/// `receiver_id` / `receiver_name` other than one, and that one is `own_receiver`.
pub fn assert_only_own_assignment(html: &str, own_receiver: i32, what: &str) {
    assert!(
        count(html, "receiver_id") <= 1,
        "{what}: several receiver_id"
    );
    assert!(
        count(html, "receiver_name") <= 1,
        "{what}: several receiver_name"
    );
    assert!(!html.contains("giver_id"), "{what}: giver_id");
    assert!(
        !html.contains("\"giver\"") && !html.contains("\\\"giver\\\""),
        "{what}: giver key"
    );
    // The caller has an assignment: exactly one card and one serialized assignment.
    assert_eq!(
        testid_count(html, "my-assignment"),
        1,
        "{what}: assignment card"
    );
    assert_eq!(
        count(html, "receiver_id"),
        1,
        "{what}: serialized assignment"
    );
    if let Some(at) = html.find("receiver_id") {
        let digits: String = html[at + "receiver_id".len()..]
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        assert_eq!(
            digits,
            own_receiver.to_string(),
            "{what}: not the caller's receiver"
        );
    }
}
