//! Invitation entry points. Opening an invitation never creates a membership.
use crate::AppError;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitationPreview {
    pub group_id: i32,
    pub name: String,
    pub already_member: bool,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSummary {
    pub name: String,
    pub description: Option<String>,
}

#[server(prefix = "/api", endpoint = "preview_invitation")]
pub async fn preview_invitation(code: String) -> Result<InvitationPreview, AppError> {
    crate::server::respond::with_error_status(crate::server::groups::preview(code).await)
}

#[server(prefix = "/api", endpoint = "join_group")]
pub async fn join_group(code: String) -> Result<(), AppError> {
    let result = crate::server::groups::join(code.clone()).await;
    // Plain form posts must retain the error across their redirect too.
    if crate::server::respond::is_plain_form_post()
        && let Err(error) = &result
    {
        if *error == AppError::Unauthorized {
            leptos_axum::redirect(&crate::paths::login_url(&format!(
                "/join/{}",
                crate::server::groups::return_code(&code)
            )));
        } else {
            let session = crate::server::auth::session().await?;
            crate::server::flash::push(
                &session,
                crate::flash::FlashMessage::error(error.message_key()),
            )
            .await?;
            leptos_axum::redirect(&format!(
                "/join/{}",
                crate::server::groups::return_code(&code)
            ));
        }
        return Ok(());
    }
    crate::server::respond::with_error_status(result)
}

#[server(prefix = "/api", endpoint = "invitation_group")]
pub async fn invitation_group(group_id: i32) -> Result<GroupSummary, AppError> {
    crate::server::respond::with_error_status(crate::server::groups::group(group_id).await)
}
