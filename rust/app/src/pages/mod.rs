//! Top-level pages.

pub mod auth;
mod dashboard;
pub mod draw;
mod error;
pub mod exclusions;
pub mod groups;
mod home;
mod install;
mod profile;
mod share;
mod verify_email;
pub mod wishlists;

pub use dashboard::DashboardPage;
pub use error::{ErrorPage, Forbidden, NotFound, ServerError};
pub use home::HomePage;
pub use install::InstallPage;
pub use profile::ProfilePage;
pub use share::SharedWishPage;
pub use verify_email::VerifyEmailPage;

pub mod wishlist;
