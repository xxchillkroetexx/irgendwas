//! Reusable UI components.

mod flash;
pub mod guard;
mod layout;

pub use flash::{FlashAlert, FlashMessages};
pub use guard::{ClientRedirect, RedirectIfLoggedIn, RequireUser};
pub use layout::{Footer, Layout, NavBar};

/// Reads an attribute of the `<html>` element (client only; `None` on the server).
pub fn document_attribute(name: &str) -> Option<String> {
    #[cfg(feature = "hydrate")]
    {
        leptos::prelude::document()
            .document_element()
            .and_then(|el| el.get_attribute(name))
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = name;
        None
    }
}
