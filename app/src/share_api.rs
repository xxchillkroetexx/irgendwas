//! Receiving a shared link only stages a draft; saving always requires confirmation.
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppError;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareInput {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareDraft {
    pub title: String,
    pub description: String,
    pub link: String,
}

/// Apps often put the product URL in `text` on Android instead of `url`.
/// Never interpret received content as HTML, script or a request to fetch a product.
#[cfg(feature = "ssr")]
pub fn prepare_share(input: ShareInput) -> ShareDraft {
    let safe_url = |candidate: &str| {
        let parsed = url::Url::parse(candidate).ok()?;
        (matches!(parsed.scheme(), "http" | "https")
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none())
        .then(|| parsed.to_string())
    };
    // A dedicated URL field is already structured data. Keep valid trailing URL
    // characters (for example a product path ending in `)`), unlike prose wrappers.
    let link = safe_url(input.url.trim())
        .or_else(|| {
            [&input.url, &input.text, &input.title]
                .into_iter()
                .flat_map(|field| field.split_whitespace())
                .find_map(|candidate| {
                    safe_url(candidate.trim_matches(['<', '>', '"', '\'', ',', ')', ']']))
                })
        })
        .unwrap_or_default();
    ShareDraft {
        title: input.title.trim().to_string(),
        description: input.text.trim().to_string(),
        link,
    }
}

#[server(prefix = "/api", endpoint = "get_shared_draft")]
pub async fn get_shared_draft(id: String) -> Result<ShareDraft, AppError> {
    crate::server::respond::with_error_status(crate::server::share::get_draft(id).await)
}

#[server(prefix = "/api", endpoint = "add_shared_wish")]
pub async fn add_shared_wish(
    id: String,
    group_id: i32,
    title: String,
    description: String,
    link: String,
) -> Result<(), crate::wishlists_api::WishlistError> {
    crate::server::respond::with_error_status(
        crate::server::share::add_wish(id, group_id, title, description, link).await,
    )
}
