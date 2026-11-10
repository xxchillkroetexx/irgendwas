//! Session storage for [`FlashMessage`]s.

use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::error::AppError;
use crate::flash::FlashMessage;

pub const FLASH_KEY: &str = "flash";

/// IDs distinguish otherwise identical messages across concurrent take/push operations.
/// Older sessions have no ID and remain readable.
#[derive(Serialize, Deserialize)]
struct StoredFlash {
    #[serde(flatten)]
    message: FlashMessage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

/// Queues a message for the next rendered page.
pub async fn push(session: &Session, message: FlashMessage) -> Result<(), AppError> {
    let mut messages: Vec<StoredFlash> = session.get(FLASH_KEY).await?.unwrap_or_default();
    messages.push(StoredFlash {
        message,
        id: Some(super::token::generate()?),
    });
    session.insert(FLASH_KEY, messages).await?;
    Ok(())
}

/// Queues a message from inside a server fn.
pub async fn flash(message: FlashMessage) -> Result<(), AppError> {
    push(&super::auth::session().await?, message).await
}

/// Removes and returns all pending messages.
pub async fn take(session: &Session) -> Result<Vec<FlashMessage>, AppError> {
    let messages: Vec<StoredFlash> = session.remove(FLASH_KEY).await?.unwrap_or_default();
    Ok(messages.into_iter().map(|entry| entry.message).collect())
}
