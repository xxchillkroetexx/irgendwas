//! Flash messages: one-shot notices shown on the next rendered page.
//!
//! Server fns call `server::flash::push(...)` (stored in the session) and then usually
//! `leptos_axum::redirect(...)`. The layout fetches and clears them with
//! [`crate::auth::get_session_info`] on every navigation (also after client-side
//! redirects). Messages carry an i18n key, so they render in the reader's locale.
//!
//! Client-only feedback (no round trip) goes through [`Toasts::push`].

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::i18n::I18n;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlashKind {
    Success,
    Info,
    Warning,
    Error,
}

impl FlashKind {
    /// Bootstrap alert modifier.
    pub fn alert_class(self) -> &'static str {
        match self {
            FlashKind::Success => "alert-success",
            FlashKind::Info => "alert-info",
            FlashKind::Warning => "alert-warning",
            FlashKind::Error => "alert-danger",
        }
    }

    /// Success and info notices dismiss themselves; warnings and errors stay.
    pub fn auto_dismiss(self) -> bool {
        matches!(self, FlashKind::Success | FlashKind::Info)
    }
}

/// A typed `{name}` value of a flash message; rendered with the reader's `I18n`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlashArg {
    Text(String),
    Number(i64),
    Date(DateTime<Utc>),
    DateTime(DateTime<Utc>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlashMessage {
    pub kind: FlashKind,
    /// i18n key.
    pub key: String,
    /// `{name}` placeholder values.
    #[serde(default)]
    pub params: Vec<(String, FlashArg)>,
    /// Selects `<key>.one` / `<key>.other` by this count.
    #[serde(default)]
    pub plural: Option<i64>,
}

impl FlashMessage {
    pub fn new(kind: FlashKind, key: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
            params: Vec::new(),
            plural: None,
        }
    }

    pub fn success(key: impl Into<String>) -> Self {
        Self::new(FlashKind::Success, key)
    }

    pub fn info(key: impl Into<String>) -> Self {
        Self::new(FlashKind::Info, key)
    }

    pub fn warning(key: impl Into<String>) -> Self {
        Self::new(FlashKind::Warning, key)
    }

    pub fn error(key: impl Into<String>) -> Self {
        Self::new(FlashKind::Error, key)
    }

    pub fn with_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.params
            .push((name.into(), FlashArg::Text(value.into())));
        self
    }

    /// A number argument, grouped by the reader's locale.
    pub fn with_number(mut self, name: impl Into<String>, n: i64) -> Self {
        self.params.push((name.into(), FlashArg::Number(n)));
        self
    }

    /// A date argument, formatted with the reader's `I18n::date`.
    pub fn with_date(mut self, name: impl Into<String>, dt: DateTime<Utc>) -> Self {
        self.params.push((name.into(), FlashArg::Date(dt)));
        self
    }

    /// A date-time argument, formatted with the reader's `I18n::datetime`.
    pub fn with_datetime(mut self, name: impl Into<String>, dt: DateTime<Utc>) -> Self {
        self.params.push((name.into(), FlashArg::DateTime(dt)));
        self
    }

    /// Treats `key` as a plural base: `<key>.one` if `n == 1`, else `<key>.other`.
    /// `n` only selects the form; add `with_number("count", n)` to show it.
    pub fn with_plural(mut self, n: i64) -> Self {
        self.plural = Some(n);
        self
    }

    /// The message text in the locale (and time zone) of `i18n`.
    pub fn text(&self, i18n: &I18n) -> String {
        let params: Vec<(&str, String)> = self
            .params
            .iter()
            .map(|(name, arg)| {
                let value = match arg {
                    FlashArg::Text(text) => text.clone(),
                    FlashArg::Number(n) => i18n.number(i128::from(*n)),
                    FlashArg::Date(dt) => i18n.date(*dt),
                    FlashArg::DateTime(dt) => i18n.datetime(*dt),
                };
                (name.as_str(), value)
            })
            .collect();
        match self.plural {
            Some(n) => i18n.t_plural(&self.key, n, &params),
            None => i18n.t_with(&self.key, &params),
        }
    }
}

/// Client-side notices belong to a pathname, so old feedback does not follow navigation.
/// Empty during SSR, so it never causes hydration mismatches.
#[derive(Debug, Clone, Copy)]
pub struct Toasts {
    list: RwSignal<Vec<(String, FlashMessage)>>,
    pathname: Memo<String>,
}

impl Toasts {
    pub fn new(pathname: Memo<String>) -> Self {
        Self {
            list: RwSignal::new(Vec::new()),
            pathname,
        }
    }

    pub fn push(&self, message: FlashMessage) {
        let path = self.pathname.get_untracked();
        self.list.update(|list| list.push((path, message)));
    }

    /// Capture the page before an asynchronous operation starts.
    pub fn current_path(&self) -> Option<String> {
        self.pathname.try_get_untracked()
    }

    /// Discard feedback if the operation finished after leaving its source page.
    pub fn push_if_current(&self, path: &str, message: FlashMessage) {
        if self.pathname.try_get_untracked().as_deref() == Some(path) {
            self.push_for(path, message);
        }
    }

    /// Use the destination when a toast precedes a client-side redirect.
    pub fn push_for(&self, to: &str, message: FlashMessage) {
        let path = to.split(['?', '#']).next().unwrap_or_default().to_string();
        self.list.update(|list| list.push((path, message)));
    }

    pub fn current(&self) -> Vec<FlashMessage> {
        let path = self.pathname.get();
        self.list.with(|list| {
            list.iter()
                .filter(|(p, _)| *p == path)
                .map(|(_, message)| message.clone())
                .collect()
        })
    }

    pub fn keep_only(&self, path: &str) {
        self.list.update(|list| list.retain(|(p, _)| p == path));
    }
}

pub fn use_toasts() -> Toasts {
    expect_context::<Toasts>()
}
