//! Top-level pages.

pub mod auth;
mod dashboard;
mod error;
mod home;

pub use dashboard::DashboardPage;
pub use error::{ErrorPage, NotFound};
pub use home::HomePage;
