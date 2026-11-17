//! Reusable UI components.

mod confirm;
mod field;
mod flash;
pub mod guard;
mod layout;
mod text_area;

pub use confirm::{SafeConfirmSubmit, provide_hydrated};
pub use field::{
    FieldErrors, FieldFeedback, control_class, field_invalid, invalid_attr, invalid_value,
};
pub use flash::{FlashAlert, FlashMessages};
pub use guard::{ClientRedirect, LoginRedirect, PageLoading, RedirectIfLoggedIn, RequireUser};
pub use layout::{Footer, Layout, NavBar};
pub use text_area::TextArea;

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
