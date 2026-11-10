//! Exclusion server functions, their error type and view types (spec: exclusions).
//!
//! The endpoints are `POST /api/<fn name>`, form-urlencoded, with the arguments flat
//! (`group_id`, `excluded_user_id`), so a plain HTML form can post them. Errors are the
//! JSON-encoded [`ExclusionError`] with the HTTP status of the variant.
//!
//! Rules are self-service and one-way (D1, D2): the giver of a rule is always the session
//! user, never an argument. Nothing here returns another member's rules or any assignment
//! (D4); the only aggregate is the one-bit [`FeasibilityNotice`]. The bodies live in
//! `server::exclusions`.

use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

use crate::flash::FlashMessage;

/// Most exclusion rules one member may have per group (spec D6).
pub const MAX_EXCLUSIONS_PER_MEMBER: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ExclusionError {
    #[error("authentication required")]
    Unauthorized,
    /// Missing group, not a member, or an id that is no integer: all the same.
    #[error("not found")]
    NotFound,
    #[error("draw already performed")]
    AlreadyDrawn,
    #[error("cannot exclude yourself")]
    CannotExcludeSelf,
    #[error("target is not a member")]
    TargetNotMember,
    #[error("too many exclusions")]
    TooManyExclusions { max: u8 },
    #[error("would make the draw impossible")]
    WouldMakeDrawImpossible,
    #[error("group exceeds draw size limit")]
    GroupTooLarge { max: u32 },
    #[error("internal server error")]
    Internal,
}

impl ExclusionError {
    /// The i18n key of the message (spec, "Error → key mapping").
    pub fn message_key(&self) -> &'static str {
        match self {
            ExclusionError::Unauthorized => "auth.login.required",
            ExclusionError::NotFound => "error.notFound.message",
            ExclusionError::AlreadyDrawn => "flash.error.exclusions_locked",
            ExclusionError::CannotExcludeSelf => "flash.error.exclusion_self",
            ExclusionError::TargetNotMember => "flash.error.user_not_in_group",
            ExclusionError::TooManyExclusions { .. } => "flash.error.exclusion_limit_reached",
            ExclusionError::WouldMakeDrawImpossible => "flash.error.exclusion_would_block_draw",
            ExclusionError::GroupTooLarge { .. } => "exclusion.warning.groupTooLarge",
            ExclusionError::Internal => "error.generic.message",
        }
    }

    /// The message as a flash (plain form posts) or a toast (the server fn client).
    pub fn flash(&self) -> FlashMessage {
        let message = FlashMessage::error(self.message_key());
        match self {
            ExclusionError::TooManyExclusions { max } => message.with_number("max", (*max).into()),
            ExclusionError::GroupTooLarge { max } => message.with_number("max", (*max).into()),
            _ => message,
        }
    }

    /// HTTP status of the server fn response carrying this error.
    pub fn status_code(&self) -> u16 {
        match self {
            ExclusionError::Unauthorized => 401,
            ExclusionError::NotFound => 404,
            ExclusionError::AlreadyDrawn
            | ExclusionError::WouldMakeDrawImpossible
            | ExclusionError::GroupTooLarge { .. } => 409,
            ExclusionError::CannotExcludeSelf
            | ExclusionError::TargetNotMember
            | ExclusionError::TooManyExclusions { .. } => 422,
            ExclusionError::Internal => 500,
        }
    }
}

impl crate::error::HttpStatus for ExclusionError {
    fn status_code(&self) -> u16 {
        ExclusionError::status_code(self)
    }
}

impl FromServerFnError for ExclusionError {
    type Encoder = JsonEncoding;

    /// Arguments that do not deserialise (an id that is no integer or out of range, a
    /// missing field) address nothing that exists: `NotFound` (404), never a 500.
    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        match value {
            ServerFnErrorErr::Args(_) | ServerFnErrorErr::MissingArg(_) => {
                #[cfg(feature = "ssr")]
                let _ = crate::server::respond::with_error_status::<(), _>(Err(
                    ExclusionError::NotFound,
                ));
                ExclusionError::NotFound
            }
            _ => ExclusionError::Internal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberRef {
    pub user_id: i32,
    pub name: String,
}

/// The only aggregate a member may see (spec D7): one bit, no names, no rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeasibilityNotice {
    GroupTooLarge {
        max: u32,
    },
    DrawImpossible {
        /// The caller's own rules leave them nobody to draw. Serialised as `me`: the page
        /// embeds this data for hydration, and no internal wording belongs in the HTML.
        #[serde(rename = "me")]
        no_receiver_for_me: bool,
    },
}

/// The caller's own rules only; never another member's rules or any assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExclusionsPage {
    pub group_id: i32,
    pub group_name: String,
    pub is_drawn: bool,
    /// Only `user_id = me` rules, ordered by `name, user_id`.
    pub my_rules: Vec<MemberRef>,
    /// Current members except me and except `my_rules`, ordered by `name, user_id`. Empty
    /// when drawn.
    pub candidates: Vec<MemberRef>,
    pub max_rules: u8,
    /// `!is_drawn && my_rules.len() < max_rules && !candidates.is_empty()`.
    pub can_add: bool,
    /// `None` when drawn, feasible, or with fewer than 2 members.
    pub notice: Option<FeasibilityNotice>,
}

/// The caller's exclusions page. `NotFound` for non-members and missing groups.
#[server(prefix = "/api", endpoint = "get_exclusions")]
pub async fn get_exclusions(group_id: i32) -> Result<ExclusionsPage, ExclusionError> {
    crate::server::respond::with_error_status(
        crate::server::exclusions::get_exclusions(group_id).await,
    )
}

/// Adds the rule "I must not draw `excluded_user_id`" in the group. Idempotent.
#[server(prefix = "/api", endpoint = "add_exclusion")]
pub async fn add_exclusion(group_id: i32, excluded_user_id: i32) -> Result<(), ExclusionError> {
    use crate::server::exclusions as flow;
    let result = flow::add_exclusion(group_id, excluded_user_id).await;
    crate::server::respond::with_error_status(
        flow::outcome(group_id, result, "flash.success.exclusion_added").await,
    )
}

/// Removes my rule for `excluded_user_id` in the group. Idempotent.
#[server(prefix = "/api", endpoint = "remove_exclusion")]
pub async fn remove_exclusion(group_id: i32, excluded_user_id: i32) -> Result<(), ExclusionError> {
    use crate::server::exclusions as flow;
    let result = flow::remove_exclusion(group_id, excluded_user_id).await;
    crate::server::respond::with_error_status(
        flow::outcome(group_id, result, "flash.success.exclusion_removed").await,
    )
}
