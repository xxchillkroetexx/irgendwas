//! Issue #72: the brand in the navbar and the pages for logged-out visitors.
//! A logged-in user's brand leads to the dashboard (SSR renders the same href as the
//! hydrated page); without JS, the landing/login/register pages still answer 302 for a
//! logged-in visitor and the full page for an anonymous one. The client-side part (no
//! flash of the landing page) is end2end/tests/navigation.spec.ts.

use crate::common::auth::*;
use crate::common::groups::en;
use app::test_support::{ALICE_PASSWORD, create_alice};
use axum::http::StatusCode;
use sqlx::MySqlPool;

const ALICE: &str = "alice@example.com";
const DASHBOARD: &str = "/user/dashboard";

fn app(pool: &MySqlPool) -> axum::Router {
    router(pool, &[("APP_LOCALE", "en")], Opts::default())
}

/// The `href` of the navbar brand.
fn brand_href(html: &str) -> &str {
    let at = html
        .find("class=\"navbar-brand\"")
        .expect("navbar brand in the page");
    let open = html[..at].rfind('<').expect("start of the brand tag");
    let tag = &html[open..at + html[at..].find('>').expect("end of the brand tag")];
    let start = tag.find("href=\"").expect("brand href") + "href=\"".len();
    let len = tag[start..].find('"').expect("end of the brand href");
    &tag[start..start + len]
}

#[sqlx::test(migrations = "../migrations")]
async fn anonymous_brand_leads_to_the_landing_page(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    for path in ["/", "/auth/login", "/auth/register"] {
        let page = c.get(path).await;
        assert_eq!(page.status, StatusCode::OK, "{path}");
        assert_eq!(brand_href(&page.body), "/", "{path}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn logged_in_brand_leads_to_the_dashboard(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    for path in [DASHBOARD, "/groups", "/user/profile"] {
        let page = c.get(path).await;
        assert_eq!(page.status, StatusCode::OK, "{path}");
        assert_eq!(brand_href(&page.body), DASHBOARD, "{path}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn anonymous_visitors_get_the_full_logged_out_pages(pool: MySqlPool) {
    let app = app(&pool);
    let mut c = Client::new(&app);
    let home = c.get("/").await.body;
    assert!(home.contains(&en("home.howItWorks.title")));
    assert!(home.contains(&en("home.features.secretAssignments")));
    let login = c.get("/auth/login").await.body;
    assert!(login.contains("id=\"login-email\""));
    let register = c.get("/auth/register").await.body;
    assert!(register.contains("id=\"register-email\""));
}

#[sqlx::test(migrations = "../migrations")]
async fn logged_in_visitors_are_redirected_from_the_logged_out_pages(pool: MySqlPool) {
    create_alice(&pool, ALICE).await;
    let app = app(&pool);
    let mut c = Client::new(&app);
    c.login(ALICE, ALICE_PASSWORD).await.ok();
    for path in ["/", "/auth/login", "/auth/register"] {
        let page = c.get(path).await;
        assert!(page.status.is_redirection(), "{path}: {}", page.status);
        assert_eq!(page.location().as_deref(), Some(DASHBOARD), "{path}");
    }
}
