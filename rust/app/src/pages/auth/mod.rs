//! The `/auth/*` pages (spec: auth, "Pages / routes"). `GET /auth/logout` is deliberately
//! not routed: logout is the `logout` server fn (a POST form in the navbar).

mod confirm;
mod forgot;
mod form;
mod login;
mod register;
mod reset;
mod token_page;

pub use confirm::RegisterConfirmPage;
pub use forgot::ForgotPasswordPage;
pub use login::LoginPage;
pub use register::RegisterPage;
pub use reset::ResetPasswordPage;
