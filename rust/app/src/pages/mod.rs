//! Top-level pages.

pub mod auth;
mod dashboard;
mod error;
pub mod groups;
mod home;
mod profile;
mod verify_email;

pub use dashboard::DashboardPage;
pub use error::{ErrorPage, Forbidden, NotFound, ServerError};
pub use home::HomePage;
pub use profile::ProfilePage;
pub use verify_email::VerifyEmailPage;
