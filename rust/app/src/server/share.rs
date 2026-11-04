//! Pending shares live in the existing session, never in auth return URLs.
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use super::{auth, respond};
use crate::AppError;
use crate::share_api::{ShareDraft, ShareInput, prepare_share};
use crate::wishlists_api::{WishlistError, WishlistField, WishlistFieldError};

const DRAFTS_KEY: &str = "shared_link_drafts";
const MAX_PENDING: usize = 5;
pub const MAX_SHARE_BYTES: usize = 16_384;

#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    id: String,
    draft: ShareDraft,
}

pub fn review_path(id: &str) -> String {
    format!("/wishes/shared?id={}", crate::paths::encode_query_value(id))
}

async fn drafts(session: &Session) -> Result<Vec<Pending>, AppError> {
    Ok(session.get(DRAFTS_KEY).await?.unwrap_or_default())
}

async fn store(session: &Session, id: String, draft: ShareDraft) -> Result<(), AppError> {
    let mut pending = drafts(session).await?;
    pending.retain(|p| p.id != id);
    pending.push(Pending { id, draft });
    if pending.len() > MAX_PENDING {
        pending.drain(..pending.len() - MAX_PENDING);
    }
    session.insert(DRAFTS_KEY, pending).await?;
    Ok(())
}

/// A share navigation stages text for review. It does not need a logged-in user yet.
pub async fn receive(session: &Session, input: ShareInput) -> Result<String, AppError> {
    if input.title.len() + input.text.len() + input.url.len() > MAX_SHARE_BYTES {
        return Err(AppError::Validation("share.too_large".into()));
    }
    let id = super::token::generate()?;
    store(session, id.clone(), prepare_share(input)).await?;
    Ok(review_path(&id))
}

pub async fn get_draft(id: String) -> Result<ShareDraft, AppError> {
    auth::require_user().await?;
    let session = auth::session().await?;
    drafts(&session)
        .await?
        .into_iter()
        .find(|p| p.id == id)
        .map(|p| p.draft)
        .ok_or(AppError::NotFound)
}

fn mutation_error(error: AppError) -> WishlistError {
    match error {
        AppError::Unauthorized => WishlistError::Unauthorized,
        AppError::NotFound => WishlistError::NotFound,
        _ => WishlistError::Internal,
    }
}

pub async fn add_wish(
    id: String,
    group_id: i32,
    title: String,
    description: String,
    link: String,
) -> Result<(), WishlistError> {
    let user = auth::current_user().await.map_err(mutation_error)?;
    let session = auth::session().await.map_err(mutation_error)?;
    if user.is_some()
        && !id.is_empty()
        && !drafts(&session)
            .await
            .map_err(mutation_error)?
            .iter()
            .any(|p| p.id == id)
    {
        return Err(WishlistError::NotFound);
    }
    if title.len() + description.len() + link.len() > MAX_SHARE_BYTES {
        return Err(WishlistError::Validation(vec![(
            WishlistField::Description,
            WishlistFieldError::DescriptionTooLong,
        )]));
    }
    let id = if id.is_empty() || user.is_none() {
        super::token::generate().map_err(mutation_error)?
    } else {
        id
    };
    // Keep corrections across login and a no-JS validation/authorization failure.
    store(
        &session,
        id.clone(),
        ShareDraft {
            title: title.clone(),
            description: description.clone(),
            link: link.clone(),
        },
    )
    .await
    .map_err(mutation_error)?;
    if user.is_none() {
        respond::redirect_document(&crate::paths::login_url(&review_path(&id)));
        return if respond::is_plain_form_post() {
            Ok(())
        } else {
            Err(WishlistError::Unauthorized)
        };
    }
    let result = super::wishlists_flow::add_item(group_id, title, description, link).await;
    match result {
        Ok(_) => {
            let mut pending = drafts(&session).await.map_err(mutation_error)?;
            pending.retain(|p| p.id != id);
            session
                .insert(DRAFTS_KEY, pending)
                .await
                .map_err(AppError::from)
                .map_err(mutation_error)?;
            let target = format!("/groups/{group_id}/wishlist");
            if respond::is_plain_form_post() {
                super::flash::push(
                    &session,
                    crate::flash::FlashMessage::success("flash.success.item_added"),
                )
                .await
                .map_err(mutation_error)?;
                respond::redirect_document(&target);
            } else {
                leptos_axum::redirect(&target);
            }
            Ok(())
        }
        Err(error) => {
            if respond::is_plain_form_post() {
                super::flash::push(&session, error.flash())
                    .await
                    .map_err(mutation_error)?;
                respond::redirect_document(&review_path(&id));
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}
