//! Session storage for [`FlashMessage`]s.

use tower_sessions::Session;

use crate::error::AppError;
use crate::flash::FlashMessage;

const FLASH_KEY: &str = "flash";

/// Queues a message for the next rendered page.
pub async fn push(session: &Session, message: FlashMessage) -> Result<(), AppError> {
    let mut messages: Vec<FlashMessage> = session.get(FLASH_KEY).await?.unwrap_or_default();
    messages.push(message);
    session.insert(FLASH_KEY, messages).await?;
    Ok(())
}

/// Queues a message from inside a server fn.
pub async fn flash(message: FlashMessage) -> Result<(), AppError> {
    push(&super::auth::session().await?, message).await
}

/// Removes and returns all pending messages.
pub async fn take(session: &Session) -> Result<Vec<FlashMessage>, AppError> {
    Ok(session.remove(FLASH_KEY).await?.unwrap_or_default())
}
