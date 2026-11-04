//! Draw server functions, their error type and view types (spec: draw).
//!
//! The endpoints are `POST /api/<fn name>`, form-urlencoded with the arguments flat
//! (`group_id`), so a plain HTML form can post them. Errors are the JSON-encoded
//! [`DrawApiError`] with the HTTP status of the variant. The bodies live in
//! `server::draw_flow`.
//!
//! Secrecy (spec S1, S2): only [`get_my_assignment`] and [`list_my_assignments`] return
//! receiver data, always the caller's own (`giver_id = <session user>` in SQL); neither takes
//! a user id. The admin fns return nothing about the pairing.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

use crate::flash::FlashMessage;

/// Error of every draw server fn. (The pure algorithm has its own `server::draw::DrawError`.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum DrawApiError {
    #[error("authentication required")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("draw already performed")]
    AlreadyDrawn,
    #[error("draw not performed")]
    NotDrawn,
    #[error("too few members")]
    TooFewMembers { count: u32, min: u32 },
    #[error("too many members")]
    TooManyMembers { count: u32, max: u32 },
    /// Generic: never says which members or rules block the draw.
    #[error("draw impossible")]
    DrawImpossible,
    #[error("nothing to resend")]
    NothingToResend,
    #[error("internal server error")]
    Internal,
}

impl DrawApiError {
    /// The messages for this error (spec, "Error → key mapping"): one, or two for
    /// `DrawImpossible` (the reason and the hint).
    pub fn flashes(&self) -> Vec<FlashMessage> {
        let one = |key: &str| vec![FlashMessage::error(key)];
        match self {
            DrawApiError::Unauthorized => one("auth.login.required"),
            DrawApiError::NotFound => one("flash.error.group_not_found"),
            DrawApiError::Forbidden => one("flash.error.forbidden_admin_only"),
            DrawApiError::AlreadyDrawn => one("flash.error.already_drawn"),
            DrawApiError::NotDrawn => one("flash.error.not_drawn"),
            DrawApiError::NothingToResend => one("flash.error.nothing_to_resend"),
            DrawApiError::TooFewMembers { count, min } => {
                vec![
                    FlashMessage::error("group.draw.blocked.tooFewMembers")
                        .with_number("count", (*count).into())
                        .with_number("min", (*min).into()),
                ]
            }
            DrawApiError::TooManyMembers { max, .. } => {
                vec![
                    FlashMessage::error("group.draw.blocked.tooManyMembers")
                        .with_number("max", (*max).into()),
                ]
            }
            DrawApiError::DrawImpossible => vec![
                FlashMessage::error("exclusion.warning.drawImpossible"),
                FlashMessage::error("group.draw.impossible.hint"),
            ],
            DrawApiError::Internal => one("error.generic.message"),
        }
    }
}

impl crate::error::HttpStatus for DrawApiError {
    fn status_code(&self) -> u16 {
        match self {
            DrawApiError::Unauthorized => 401,
            DrawApiError::Forbidden => 403,
            DrawApiError::NotFound => 404,
            DrawApiError::AlreadyDrawn
            | DrawApiError::NotDrawn
            | DrawApiError::NothingToResend
            | DrawApiError::DrawImpossible => 409,
            DrawApiError::TooFewMembers { .. } | DrawApiError::TooManyMembers { .. } => 422,
            DrawApiError::Internal => 500,
        }
    }
}

impl FromServerFnError for DrawApiError {
    type Encoder = JsonEncoding;

    /// Arguments that do not deserialise (an id that is no integer, a missing field)
    /// address nothing that exists: `NotFound` (404), never a 500.
    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        match value {
            ServerFnErrorErr::Args(_) | ServerFnErrorErr::MissingArg(_) => {
                #[cfg(feature = "ssr")]
                let _ =
                    crate::server::respond::with_error_status::<(), _>(Err(DrawApiError::NotFound));
                DrawApiError::NotFound
            }
            _ => DrawApiError::Internal,
        }
    }
}

/// The caller's own receiver in one group (S1: filtered on `giver_id = me` in SQL).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MyAssignment {
    pub receiver_id: i32,
    pub receiver_name: String,
    pub drawn_at: DateTime<Utc>,
}

/// One row of the dashboard card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MyAssignmentSummary {
    pub group_id: i32,
    pub group_name: String,
    pub receiver_id: i32,
    pub receiver_name: String,
    pub drawn_at: DateTime<Utc>,
}

/// Why the admin cannot draw yet. Never names a member or a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawBlocker {
    TooFewMembers { count: u32, min: u32 },
    TooManyMembers { count: u32, max: u32 },
    DrawImpossible,
}

/// Delivery state of the draw mails; no receiver data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationSummary {
    pub total: u32,
    pub sent: u32,
    pub pending: u32,
    pub failed: u32,
    /// Givers whose mail failed for good, sorted.
    pub failed_member_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawAdminStatus {
    pub is_drawn: bool,
    pub drawn_at: Option<DateTime<Utc>>,
    pub member_count: u32,
    /// Only when not drawn.
    pub blocker: Option<DrawBlocker>,
    /// Some iff not drawn and the registration deadline is set and in the future.
    pub registration_open_until: Option<DateTime<Utc>>,
    /// Some iff drawn.
    pub notifications: Option<NotificationSummary>,
}

/// The caller's receiver in the group; `None` if not drawn. `NotFound` for non-members.
#[server(prefix = "/api", endpoint = "get_my_assignment")]
pub async fn get_my_assignment(group_id: i32) -> Result<Option<MyAssignment>, DrawApiError> {
    crate::server::respond::with_error_status(
        crate::server::draw_flow::get_my_assignment(group_id).await,
    )
}

/// The caller's receivers in all drawn groups they are a member of, newest draw first.
#[server(prefix = "/api", endpoint = "list_my_assignments")]
pub async fn list_my_assignments() -> Result<Vec<MyAssignmentSummary>, DrawApiError> {
    crate::server::respond::with_error_status(crate::server::draw_flow::list_my_assignments().await)
}

/// The admin's view of the draw: blockers, delivery counts. Nothing about the pairing.
#[server(prefix = "/api", endpoint = "get_draw_admin_status")]
pub async fn get_draw_admin_status(group_id: i32) -> Result<DrawAdminStatus, DrawApiError> {
    crate::server::respond::with_error_status(
        crate::server::draw_flow::get_draw_admin_status(group_id).await,
    )
}

/// Performs the draw (admin). The mails go out afterwards, off the request path.
#[server(prefix = "/api", endpoint = "perform_draw")]
pub async fn perform_draw(group_id: i32) -> Result<(), DrawApiError> {
    use crate::server::draw_flow as flow;
    let result = flow::perform_draw(group_id).await;
    crate::server::respond::with_error_status(
        flow::outcome(group_id, result, |_: &()| {
            FlashMessage::success("flash.success.draw_success")
        })
        .await,
    )
}

/// Deletes the assignments (admin); every other former giver gets a reset mail.
#[server(prefix = "/api", endpoint = "reset_draw")]
pub async fn reset_draw(group_id: i32) -> Result<(), DrawApiError> {
    use crate::server::draw_flow as flow;
    let result = flow::reset_draw(group_id).await;
    crate::server::respond::with_error_status(
        flow::outcome(group_id, result, |_: &()| {
            FlashMessage::success("flash.success.draw_reset")
        })
        .await,
    )
}

/// Re-queues failed mails; returns how many rows.
#[server(prefix = "/api", endpoint = "resend_draw_notifications")]
pub async fn resend_draw_notifications(group_id: i32) -> Result<u32, DrawApiError> {
    use crate::server::draw_flow as flow;
    let result = flow::resend_draw_notifications(group_id).await;
    crate::server::respond::with_error_status(
        flow::outcome(group_id, result, |count: &u32| {
            let n = i64::from(*count);
            FlashMessage::success("flash.success.draw_notifications_requeued")
                .with_plural(n)
                .with_number("count", n)
        })
        .await,
    )
}
