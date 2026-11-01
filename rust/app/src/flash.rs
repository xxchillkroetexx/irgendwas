//! Flash messages: one-shot notices shown on the next rendered page.
//!
//! Server fns call `server::flash::push(...)` (stored in the session) and then usually
//! `leptos_axum::redirect(...)`. The layout fetches and clears them with
//! [`crate::auth::get_session_info`] on every navigation (also after client-side
//! redirects). Messages carry an i18n key, so they render in the reader's locale.
//!
//! Client-only feedback (no round trip) goes through [`Toasts::push`].

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlashMessage {
    pub kind: FlashKind,
    /// i18n key.
    pub key: String,
    /// `{name}` placeholder values.
    #[serde(default)]
    pub params: Vec<(String, String)>,
}

impl FlashMessage {
    pub fn new(kind: FlashKind, key: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
            params: Vec::new(),
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
        self.params.push((name.into(), value.into()));
        self
    }
}

/// Client-side notices (e.g. after an action that does not redirect). Empty during SSR,
/// so it never causes hydration mismatches.
#[derive(Debug, Clone, Copy)]
pub struct Toasts(pub RwSignal<Vec<FlashMessage>>);

impl Toasts {
    pub fn push(&self, message: FlashMessage) {
        self.0.update(|list| list.push(message));
    }
}

pub fn use_toasts() -> Toasts {
    expect_context::<Toasts>()
}
