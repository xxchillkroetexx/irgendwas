//! Top-level pages.

pub mod auth;
mod dashboard;
mod error;
mod home;
pub mod groups;
mod profile;
mod verify_email;

pub use dashboard::DashboardPage;
pub use error::{ErrorPage, NotFound};
pub use home::HomePage;
pub use profile::ProfilePage;
pub use verify_email::VerifyEmailPage;
