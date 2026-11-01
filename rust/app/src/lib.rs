//! Shared Leptos application: components, pages, server functions.
//!
//! Feature split:
//! - `ssr`: server rendering plus all server-only modules (`server`, DB, sessions, auth).
//! - `hydrate`: client-side hydration (compiled to wasm by the `frontend` crate).
//! - `test-support`: fixtures for integration tests (`test_support`), implies `ssr`.
//!
//! Code that must differ between server and client uses `#[cfg(feature = "ssr")]` and
//! `#[cfg(not(feature = "ssr"))]`, so that `--all-features` builds still compile.

#![recursion_limit = "256"]

mod app;
pub mod auth;
pub mod components;
pub mod error;
pub mod flash;
pub mod format;
pub mod i18n;
pub mod pages;
pub mod paths;
pub mod tz;

#[cfg(feature = "ssr")]
pub mod server;

#[cfg(feature = "test-support")]
pub mod test_support;

pub use app::{App, shell};
pub use error::AppError;
