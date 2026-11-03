//! Top-level pages.

pub mod auth;
mod dashboard;
mod error;
mod home;
mod install;
mod profile;
mod verify_email;

pub use dashboard::DashboardPage;
pub use error::{ErrorPage, NotFound};
pub use home::HomePage;
pub use install::InstallPage;
pub use profile::ProfilePage;
pub use verify_email::VerifyEmailPage;

pub mod wishlist;
