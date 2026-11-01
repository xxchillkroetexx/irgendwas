//! Types shared between server and client for the logged-in user, plus the server fn the
//! layout uses to learn who is logged in and which flash messages are pending.
//!
//! Server-side helpers (`current_user`, `require_user`, `login`, `logout`) live in
//! [`crate::server::auth`].

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::flash::FlashMessage;

/// The logged-in user, as resolved from the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentUser {
    pub id: i32,
    pub email: String,
    pub name: String,
}

/// What the layout needs on every page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub user: Option<CurrentUser>,
    /// Pending flash messages; reading them clears them from the session.
    pub flash: Vec<FlashMessage>,
}

/// Returns the current user (if any) and takes the pending flash messages.
#[server(prefix = "/api", endpoint = "session_info")]
pub async fn get_session_info() -> Result<SessionInfo, AppError> {
    use crate::server::{auth, flash};

    let session = auth::session().await?;
    let user = auth::current_user().await?;
    let flash = flash::take(&session).await?;
    Ok(SessionInfo { user, flash })
}

/// The layout's session resource, available to every page through context.
#[derive(Clone, Copy)]
pub struct SessionResource(pub Resource<Result<SessionInfo, AppError>>);

pub fn use_session_resource() -> SessionResource {
    expect_context::<SessionResource>()
}
