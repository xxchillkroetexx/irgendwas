//! What the two token pages (`/auth/register/confirm/:token`, `/auth/reset-password/:token`)
//! share: no-referrer/no-store headers, the token from the URL, and the "invalid link"
//! redirect with a flash message.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::auth::SessionResource;
use crate::flash::{FlashMessage, Toasts};

/// Sends `Referrer-Policy: no-referrer` and `Cache-Control: no-store` (SSR only).
pub fn token_page_headers() {
    #[cfg(feature = "ssr")]
    crate::server::respond::token_page_headers();
}

/// The `:token` route parameter.
pub fn route_token() -> Memo<String> {
    let params = use_params_map();
    Memo::new(move |_| params.read().get("token").unwrap_or_default())
}

/// Waits until the layout has taken this request's flash messages. A token page that
/// redirects with a flash must push it only after that, or the redirect response itself
/// would consume it.
pub async fn after_layout_flash(layout: SessionResource) {
    let _ = layout.0.await;
}

/// The link is invalid: flash `message` and redirect to `to`. During SSR this is a 302
/// with the flash stored in the session; on the client a toast plus a router navigation
/// (done by the caller rendering `ClientRedirect`).
pub async fn invalid_link(toasts: Option<Toasts>, message: FlashMessage, to: &str) {
    #[cfg(feature = "ssr")]
    {
        let _ = toasts;
        if let Err(err) = crate::server::flash::flash(message).await {
            tracing::warn!(?err, "could not store the invalid-link flash");
        }
        crate::server::respond::redirect_page(to);
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = to;
        if let Some(toasts) = toasts {
            toasts.push(message);
        }
    }
}
