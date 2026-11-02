//! Groups server functions, their error type and their view types (spec: groups).
//!
//! The endpoints are `POST /api/<fn name>` with form-urlencoded arguments named like the
//! Rust parameters; errors are the JSON-encoded [`GroupError`] of a non-2xx response.
//! `create_group` / `update_group` take the form fields flat (`name`, `description`,
//! `registration_deadline`, `draw_date`; dates as `YYYY-MM-DD` strings, an empty string
//! clears) so that a plain HTML form can post them and an unparseable date can be reported
//! as [`GroupFieldError::InvalidDate`] (spec AC7). The acting user always comes from the
//! session, group ids only select a row and are checked through `server::authz`. The bodies
//! live in `server::groups_flow`.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

use crate::flash::FlashMessage;
use crate::i18n::I18n;
use crate::{t, t_plural};

/// Longest accepted group name, in characters (after trimming).
pub const GROUP_NAME_MAX: usize = 255;
/// Longest accepted description, in characters (after trimming).
pub const DESCRIPTION_MAX: usize = 2000;

/// The form field a validation error belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupField {
    Name,
    Description,
    RegistrationDeadline,
    DrawDate,
}

/// One validation problem; each maps to one `validation.group.*` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupFieldError {
    NameRequired,
    NameTooLong,
    DescriptionTooLong,
    DrawBeforeDeadline,
    InvalidDate,
}

impl GroupFieldError {
    /// The i18n key of this problem.
    pub fn message_key(self) -> &'static str {
        match self {
            GroupFieldError::NameRequired => "validation.group.name_required",
            GroupFieldError::NameTooLong => "validation.group.name_too_long",
            GroupFieldError::DescriptionTooLong => "validation.group.description_too_long",
            GroupFieldError::DrawBeforeDeadline => "validation.group.draw_before_deadline",
            GroupFieldError::InvalidDate => "validation.group.invalid_date",
        }
    }

    /// The text shown under the field (fills `{max}` of the length limits).
    pub fn text(self, i18n: &I18n) -> String {
        match self {
            GroupFieldError::NameTooLong => {
                t!(i18n, "validation.group.name_too_long", max = GROUP_NAME_MAX)
            }
            GroupFieldError::DescriptionTooLong => {
                t!(
                    i18n,
                    "validation.group.description_too_long",
                    max = DESCRIPTION_MAX
                )
            }
            other => i18n.t(other.message_key()),
        }
    }

    /// The same text as a flash message (plain form posts).
    pub fn flash(self) -> FlashMessage {
        let message = FlashMessage::error(self.message_key());
        match self {
            GroupFieldError::NameTooLong => message.with_number("max", GROUP_NAME_MAX as i64),
            GroupFieldError::DescriptionTooLong => {
                message.with_number("max", DESCRIPTION_MAX as i64)
            }
            _ => message,
        }
    }
}

/// The operation an error came from, where the same error reads differently
/// ([`GroupError::AlreadyDrawn`]: "nobody can join", "you can no longer leave", ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupOp {
    Join,
    Leave,
    Remove,
    Other,
}

/// The error of every groups server fn. Carries no internal detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum GroupError {
    #[error("authentication required")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("validation failed")]
    Validation(Vec<(GroupField, GroupFieldError)>),
    #[error("invitation code required")]
    CodeRequired,
    #[error("invalid invitation code")]
    InvalidCode,
    #[error("too many invalid codes, retry in {minutes} min")]
    TooManyAttempts { minutes: u32 },
    #[error("already a member")]
    AlreadyMember { group_id: i32 },
    #[error("registration closed")]
    RegistrationClosed,
    #[error("group already drawn")]
    AlreadyDrawn,
    #[error("the admin cannot leave")]
    AdminCannotLeave,
    #[error("cannot remove yourself")]
    CannotRemoveSelf,
    #[error("target is not a member")]
    TargetNotMember,
    #[error("target is yourself")]
    TargetIsSelf,
    #[error("internal server error")]
    Internal,
}

impl GroupError {
    /// The i18n key of the message for this error, as raised by `op`. For `Validation`,
    /// the key of the first problem.
    pub fn message_key(&self, op: GroupOp) -> &'static str {
        match self {
            GroupError::Unauthorized => "auth.login.required",
            GroupError::NotFound => "flash.error.group_not_found",
            GroupError::Forbidden => "flash.error.forbidden_admin_only",
            GroupError::Validation(list) => list
                .first()
                .map(|(_, problem)| problem.message_key())
                .unwrap_or("error.generic.message"),
            GroupError::CodeRequired => "flash.error.invitation_code_required",
            GroupError::InvalidCode => "flash.error.invalid_invitation_code",
            GroupError::TooManyAttempts { .. } => "flash.error.too_many_code_attempts",
            GroupError::AlreadyMember { .. } => "flash.info.already_member",
            GroupError::RegistrationClosed => "flash.error.registration_deadline_passed",
            GroupError::AlreadyDrawn => match op {
                GroupOp::Leave => "flash.error.cannot_leave_after_draw",
                GroupOp::Remove => "flash.error.cannot_remove_after_draw",
                GroupOp::Join | GroupOp::Other => "flash.error.join_closed_drawn",
            },
            GroupError::AdminCannotLeave => "flash.error.admin_cannot_leave",
            GroupError::CannotRemoveSelf => "flash.error.cannot_remove_self",
            GroupError::TargetNotMember => "flash.error.user_not_in_group",
            GroupError::TargetIsSelf => "flash.error.target_is_self",
            GroupError::Internal => "error.generic.message",
        }
    }

    /// The text shown to the user (`TooManyAttempts` through `t_plural!` on `minutes`).
    /// `AlreadyDrawn` reads as the join message; see [`GroupError::text_for`].
    pub fn text(&self, i18n: &I18n) -> String {
        self.text_for(GroupOp::Other, i18n)
    }

    /// [`GroupError::text`] for an error raised by `op`.
    pub fn text_for(&self, op: GroupOp, i18n: &I18n) -> String {
        match self {
            GroupError::TooManyAttempts { minutes } => t_plural!(
                i18n,
                "flash.error.too_many_code_attempts",
                i64::from(*minutes),
                minutes = *minutes
            ),
            GroupError::Validation(list) => match list.first() {
                Some((_, problem)) => problem.text(i18n),
                None => i18n.t("error.generic.message"),
            },
            other => i18n.t(other.message_key(op)),
        }
    }

    /// The message as a flash (plain HTML form posts); renders like [`GroupError::text_for`].
    pub fn flash(&self, op: GroupOp) -> FlashMessage {
        match self {
            GroupError::TooManyAttempts { minutes } => {
                FlashMessage::error("flash.error.too_many_code_attempts")
                    .with_plural(i64::from(*minutes))
                    .with_number("minutes", i64::from(*minutes))
            }
            GroupError::Validation(list) => match list.first() {
                Some((_, problem)) => problem.flash(),
                None => FlashMessage::error("error.generic.message"),
            },
            GroupError::AlreadyMember { .. } => FlashMessage::info("flash.info.already_member"),
            other => FlashMessage::error(other.message_key(op)),
        }
    }

    /// HTTP status of the server fn response carrying this error.
    pub fn status_code(&self) -> u16 {
        match self {
            GroupError::Unauthorized => 401,
            GroupError::Forbidden => 403,
            GroupError::NotFound | GroupError::InvalidCode => 404,
            GroupError::Validation(_)
            | GroupError::CodeRequired
            | GroupError::CannotRemoveSelf
            | GroupError::TargetNotMember
            | GroupError::TargetIsSelf => 422,
            GroupError::AlreadyMember { .. }
            | GroupError::RegistrationClosed
            | GroupError::AlreadyDrawn
            | GroupError::AdminCannotLeave => 409,
            GroupError::TooManyAttempts { .. } => 429,
            GroupError::Internal => 500,
        }
    }

    /// The problem reported for `field`, if this is a validation error.
    pub fn field_error(&self, field: GroupField) -> Option<GroupFieldError> {
        match self {
            GroupError::Validation(list) => list
                .iter()
                .find(|(f, _)| *f == field)
                .map(|(_, problem)| *problem),
            _ => None,
        }
    }
}

impl crate::error::HttpStatus for GroupError {
    fn status_code(&self) -> u16 {
        GroupError::status_code(self)
    }
}

impl FromServerFnError for GroupError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(_value: ServerFnErrorErr) -> Self {
        GroupError::Internal
    }
}

/// Where a group stands. Computed on the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupStatus {
    Open,
    RegistrationClosed,
    Drawn,
}

impl GroupStatus {
    /// The i18n key of the status badge.
    pub fn label_key(self) -> &'static str {
        match self {
            GroupStatus::Open => "group.status.open",
            GroupStatus::RegistrationClosed => "group.status.registrationClosed",
            GroupStatus::Drawn => "group.status.drawn",
        }
    }

    /// Bootstrap badge class.
    pub fn badge_class(self) -> &'static str {
        match self {
            GroupStatus::Open => "badge bg-success",
            GroupStatus::RegistrationClosed => "badge bg-warning text-dark",
            GroupStatus::Drawn => "badge bg-secondary",
        }
    }
}

/// One row of the "my groups" list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSummary {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub is_admin: bool,
    pub admin_name: String,
    pub member_count: u32,
    pub status: GroupStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminView {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberView {
    pub user_id: i32,
    pub name: String,
    /// `Some` only when the viewer is the admin.
    pub email: Option<String>,
    pub is_admin: bool,
    pub is_me: bool,
}

/// Shown to the admin only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation {
    pub code: String,
    /// `{APP_URL}/join/{code}`
    pub link: String,
}

/// A group as a member sees it. Never contains assignment data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupDetail {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub registration_deadline: Option<DateTime<Utc>>,
    pub draw_date: Option<DateTime<Utc>>,
    pub is_drawn: bool,
    pub status: GroupStatus,
    pub is_admin: bool,
    pub admin: AdminView,
    pub members: Vec<MemberView>,
    pub invitation: Option<Invitation>,
    pub can_leave: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClosedReason {
    Drawn,
    DeadlinePassed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitationPreview {
    pub group_id: i32,
    pub name: String,
    pub admin_name: String,
    pub member_count: u32,
    pub already_member: bool,
    pub closed: Option<ClosedReason>,
}

/// Invitation code input normalisation (spec D10): trim, remove spaces and `-`, uppercase.
/// Nothing else (no accent folding). Shared by the server and the enter-code form.
pub fn normalize_invitation_code(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|c| *c != ' ' && *c != '-')
        .collect::<String>()
        .to_uppercase()
}

/// The preview page of a (normalised) code: `/join/<percent-encoded code>`. `None` for a
/// code without any letter or digit: it can only be invalid, and a segment of dots (also
/// percent-encoded, `%2E%2E`) would be resolved by the browser as `..` (to `/`).
pub fn join_path(code: &str) -> Option<String> {
    code.chars()
        .any(char::is_alphanumeric)
        .then(|| format!("/join/{}", crate::paths::encode_query_value(code)))
}

/// The caller's groups, newest first.
#[server(prefix = "/api", endpoint = "list_my_groups")]
pub async fn list_my_groups() -> Result<Vec<GroupSummary>, GroupError> {
    crate::server::respond::with_error_status(crate::server::groups_flow::list_my_groups().await)
}

/// One group as the calling member sees it (`NotFound` for non-members and missing groups).
#[server(prefix = "/api", endpoint = "get_group")]
pub async fn get_group(group_id: i32) -> Result<GroupDetail, GroupError> {
    crate::server::respond::with_error_status(crate::server::groups_flow::get_group(group_id).await)
}

/// Creates a group with the caller as admin and only member; returns its id.
#[server(prefix = "/api", endpoint = "create_group")]
pub async fn create_group(
    name: String,
    description: String,
    registration_deadline: String,
    draw_date: String,
) -> Result<i32, GroupError> {
    use crate::server::groups_flow as flow;
    let result = flow::create_group(name, description, registration_deadline, draw_date).await;
    crate::server::respond::with_error_status(flow::create_outcome(result).await)
}

/// Admin only: name, description and dates.
#[server(prefix = "/api", endpoint = "update_group")]
pub async fn update_group(
    group_id: i32,
    name: String,
    description: String,
    registration_deadline: String,
    draw_date: String,
) -> Result<(), GroupError> {
    use crate::server::groups_flow as flow;
    let result = flow::update_group(
        group_id,
        name,
        description,
        registration_deadline,
        draw_date,
    )
    .await;
    crate::server::respond::with_error_status(flow::update_outcome(group_id, result).await)
}

/// Admin only: deletes the group and (by cascade) everything in it.
#[server(prefix = "/api", endpoint = "delete_group")]
pub async fn delete_group(group_id: i32) -> Result<(), GroupError> {
    use crate::server::groups_flow as flow;
    let result = flow::delete_group(group_id).await;
    crate::server::respond::with_error_status(flow::delete_outcome(group_id, result).await)
}

/// Admin only: a new invitation code (the old one stops working); returns it.
#[server(prefix = "/api", endpoint = "regenerate_invitation")]
pub async fn regenerate_invitation(group_id: i32) -> Result<String, GroupError> {
    use crate::server::groups_flow as flow;
    let result = flow::regenerate_invitation(group_id).await;
    crate::server::respond::with_error_status(
        flow::stay_outcome(
            group_id,
            result,
            "flash.success.invitation_generated",
            GroupOp::Other,
        )
        .await,
    )
}

/// What an invitation code leads to. Never joins.
#[server(prefix = "/api", endpoint = "preview_invitation")]
pub async fn preview_invitation(code: String) -> Result<InvitationPreview, GroupError> {
    crate::server::respond::with_error_status(
        crate::server::groups_flow::preview_invitation(code).await,
    )
}

/// The enter-code form without JavaScript: answers with a redirect to the preview page of
/// the normalised code (no lookup here; the page does it). Returns that path.
#[server(prefix = "/api", endpoint = "open_invitation")]
pub async fn open_invitation(code: String) -> Result<String, GroupError> {
    crate::server::respond::with_error_status(
        crate::server::groups_flow::open_invitation(code).await,
    )
}

/// Joins the group of an invitation code; returns its id.
#[server(prefix = "/api", endpoint = "join_group")]
pub async fn join_group(code: String) -> Result<i32, GroupError> {
    use crate::server::groups_flow as flow;
    let normalized = normalize_invitation_code(&code);
    let result = flow::join_group(code).await;
    crate::server::respond::with_error_status(flow::join_outcome(&normalized, result).await)
}

/// A member (not the admin) leaves, before the draw.
#[server(prefix = "/api", endpoint = "leave_group")]
pub async fn leave_group(group_id: i32) -> Result<(), GroupError> {
    use crate::server::groups_flow as flow;
    let result = flow::leave_group(group_id).await;
    crate::server::respond::with_error_status(flow::leave_outcome(group_id, result).await)
}

/// Admin only: removes another member, before the draw.
#[server(prefix = "/api", endpoint = "remove_member")]
pub async fn remove_member(group_id: i32, user_id: i32) -> Result<(), GroupError> {
    use crate::server::groups_flow as flow;
    let result = flow::remove_member(group_id, user_id).await;
    crate::server::respond::with_error_status(
        flow::stay_outcome(
            group_id,
            result,
            "flash.success.member_removed",
            GroupOp::Remove,
        )
        .await,
    )
}

/// Admin only: hands the admin role to another member.
#[server(prefix = "/api", endpoint = "transfer_admin")]
pub async fn transfer_admin(group_id: i32, user_id: i32) -> Result<(), GroupError> {
    use crate::server::groups_flow as flow;
    let result = flow::transfer_admin(group_id, user_id).await;
    crate::server::respond::with_error_status(
        flow::stay_outcome(
            group_id,
            result,
            "flash.success.admin_transferred",
            GroupOp::Other,
        )
        .await,
    )
}
