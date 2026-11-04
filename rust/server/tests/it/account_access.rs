//! account slice: access and authorization (AC1-AC5) plus "no other user's data is
//! reachable". The user id always comes from the session, never from arguments.

use crate::common::account::*;
use crate::common::auth::*;
use crate::common::{post_from, send};
use app::account_api::{AccountError, ProfileView};
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, create_bob, set_pending_email, user_row, user_rows,
};
use sqlx::MySqlPool;

const DASHBOARD: &str = "/user/dashboard";
const PROFILE: &str = "/user/profile";

// ---------------------------------------------------------------------- AC1, AC2

async fn redirect_then_return(pool: MySqlPool, path: &str) {
    let w = alice_world_with(&pool, &[], Opts::default()).await;
    let mut anon = Client::new(&w.app);

    let res = anon.get(path).await;
    assert!(res.status.is_redirection(), "got {}", res.status);
    let location = res.location().expect("Location");
    assert!(
        location.starts_with(&format!("/auth/login?next={}", urlencode(path))),
        "{location}"
    );

    let next = urldecode(location.split_once("next=").unwrap().1);
    assert_eq!(next, path);
    let reply = anon.login_next(&w.email, ALICE_PASSWORD, &next).await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some(path));
    assert_eq!(anon.get(path).await.status, 200, "the page itself renders");
}

#[sqlx::test(migrations = "../migrations")]
async fn ac1_anonymous_dashboard_redirects_to_login_and_back(pool: MySqlPool) {
    redirect_then_return(pool, DASHBOARD).await;
}

#[sqlx::test(migrations = "../migrations")]
async fn ac2_anonymous_profile_redirects_to_login_and_back(pool: MySqlPool) {
    redirect_then_return(pool, PROFILE).await;
}

// -------------------------------------------------------------------------- AC3

#[sqlx::test(migrations = "../migrations")]
async fn ac3_anonymous_calls_are_unauthorized_and_change_nothing(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let w = alice_world(&pool).await;
    let bob = unique_email("bob");
    create_bob(&pool, &bob).await;
    // A pending change exists, so a (wrong) cancel would be visible.
    set_pending_email(&pool, w.id, "pending@mail.test", &tok('p'), 3600).await;
    let before = user_rows(&pool).await;
    let new_email = unique_email("anon-new");
    let mut anon = Client::new(&w.app);

    assert_eq!(
        get_profile(&mut anon).await.account_err(),
        AccountError::Unauthorized
    );
    assert_eq!(
        update_name(&mut anon, "Mallory").await.account_err(),
        AccountError::Unauthorized
    );
    assert_eq!(
        change_password(&mut anon, ALICE_PASSWORD, "newpassword1", "newpassword1")
            .await
            .account_err(),
        AccountError::Unauthorized
    );
    assert_eq!(
        request_email_change(&mut anon, &new_email, ALICE_PASSWORD)
            .await
            .account_err(),
        AccountError::Unauthorized
    );
    assert_eq!(
        cancel_email_change(&mut anon).await.account_err(),
        AccountError::Unauthorized
    );

    assert_eq!(user_rows(&pool).await, before, "no users row changed");
    assert_eq!(mails_after_settle(&mailpit, &new_email).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac3_a_stale_session_counts_as_anonymous(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(w.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get_profile(&mut w.c).await.account_err(),
        AccountError::Unauthorized
    );
    assert_eq!(
        update_name(&mut w.c, "Ghost").await.account_err(),
        AccountError::Unauthorized
    );
}

// -------------------------------------------------------------------------- AC4

#[sqlx::test(migrations = "../migrations")]
async fn ac4_api_user_endpoint_is_gone(pool: MySqlPool) {
    let mut w = alice_world_with(&pool, &[], Opts::default()).await;
    let bob_email = unique_email("bob");
    let bob_id = create_bob(&pool, &bob_email).await;
    let mut anon = Client::new(&w.app);
    let own = format!("/api/user/{}", w.id);
    let other = format!("/api/user/{bob_id}");

    for (who, client) in [("logged in", &mut w.c), ("anonymous", &mut anon)] {
        let res = client.get(&own).await;
        assert_eq!(res.status, 404, "{who}");
        assert!(
            !res.header("content-type")
                .unwrap_or_default()
                .contains("json"),
            "{who}: not a JSON answer"
        );
        assert!(
            html_contains(&res.body, &en("error.notFound.message")),
            "{who}: the normal 404 page"
        );
        assert!(
            !res.body.contains(&bob_email),
            "{who}: no other user's data"
        );
        // No endpoint behaviour at all: another id is the same plain 404.
        let elsewhere = client.get(&other).await;
        assert_eq!(elsewhere.status, 404, "{who}");
        assert!(!elsewhere.body.contains(&bob_email), "{who}");
    }
    // An anonymous caller learns nothing about the account (the logged-in page carries the
    // caller's own session data for the nav, which is not the endpoint's doing).
    let res = anon.get(&own).await;
    assert!(!res.body.contains(&w.email));
    assert!(!res.body.contains("\"email\""));
}

// -------------------------------------------------------------------------- AC5

#[sqlx::test(migrations = "../migrations")]
async fn ac5_foreign_origin_is_rejected_for_every_mutation(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let w = alice_world(&pool).await;
    set_pending_email(&pool, w.id, "pending@mail.test", &tok('p'), 3600).await;
    let before = user_rows(&pool).await;
    let new_email = unique_email("evil-new");
    let mut evil = w.c.fork().origin("https://evil.example");

    let replies = [
        request_email_change(&mut evil, &new_email, ALICE_PASSWORD).await,
        update_name(&mut evil, "Mallory").await,
        change_password(&mut evil, ALICE_PASSWORD, "newpassword1", "newpassword1").await,
        cancel_email_change(&mut evil).await,
        confirm_email_change(&mut evil, &tok('p')).await,
    ];
    for reply in &replies {
        assert_eq!(reply.status, 403, "{}", reply.body);
    }

    assert_eq!(user_rows(&pool).await, before, "nothing changed");
    assert_eq!(mails_after_settle(&mailpit, &new_email).await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac5_a_post_without_origin_is_rejected(pool: MySqlPool) {
    let w = alice_world_with(&pool, &[], Opts::default()).await;
    let before = user_rows(&pool).await;
    let cookie = format!(
        "{}={}",
        server::SESSION_COOKIE,
        w.c.session_cookie().unwrap()
    );
    let body = form(&[
        ("new_email", "no-origin@mail.test"),
        ("current_password", ALICE_PASSWORD),
    ]);
    let res = send(
        &w.app,
        post_from(None, "/api/request_email_change", Some(&cookie), &body),
    )
    .await;
    assert_eq!(res.status(), 403);
    assert_eq!(user_rows(&pool).await, before);
}

// ------------------------------------------------- no other user's data reachable

#[sqlx::test(migrations = "../migrations")]
async fn the_user_id_comes_from_the_session_not_from_arguments(pool: MySqlPool) {
    let mut alice = alice_world_with(&pool, &[], Opts::default()).await;
    let bob_email = unique_email("bob");
    let bob_id = create_bob(&pool, &bob_email).await;
    set_pending_email(&pool, alice.id, "alice-pending@mail.test", &tok('a'), 3600).await;
    set_pending_email(&pool, bob_id, "bob-pending@mail.test", &tok('b'), 3600).await;
    let bob_before = user_row(&pool, bob_id).await;
    let alice_id = alice.id.to_string();
    let bob_id_text = bob_id.to_string();

    // Alice smuggles Bob's id into every call.
    for key in ["id", "user_id", "userId"] {
        let profile = alice
            .c
            .call("get_profile", &[(key, &bob_id_text)])
            .await
            .account_result::<ProfileView>()
            .expect("own profile");
        assert_eq!(profile.email, alice.email);
        assert_eq!(profile.name, "Alice");
        assert_eq!(
            profile.pending_email.as_deref(),
            Some("alice-pending@mail.test")
        );
        assert!(!format!("{profile:?}").contains("bob"));
    }
    alice
        .c
        .call(
            "update_name",
            &[("name", "Alice Renamed"), ("user_id", &bob_id_text)],
        )
        .await
        .account_ok();
    alice
        .c
        .call(
            "cancel_email_change",
            &[("user_id", &bob_id_text), ("id", &bob_id_text)],
        )
        .await
        .account_ok();

    assert_eq!(
        user_row(&pool, bob_id).await,
        bob_before,
        "Bob is untouched"
    );
    let alice_row = user_row(&pool, alice.id).await;
    assert_eq!(alice_row.name, "Alice Renamed");
    assert!(
        alice_row.pending_email.is_none(),
        "Alice's own pending change was cancelled"
    );

    // And the other way round: Bob cancelling "for Alice" only cancels his own.
    set_pending_email(&pool, alice.id, "alice-pending@mail.test", &tok('a'), 3600).await;
    let mut bob = Client::new(&alice.app);
    bob.login(&bob_email, BOB_PASSWORD).await.ok();
    bob.call("cancel_email_change", &[("user_id", &alice_id)])
        .await
        .account_ok();
    assert!(user_row(&pool, bob_id).await.pending_email.is_none());
    let alice_row = user_row(&pool, alice.id).await;
    assert_eq!(alice_row.name, "Alice Renamed");
    assert_eq!(
        alice_row.pending_email.as_deref(),
        Some("alice-pending@mail.test"),
        "Alice's pending change survives Bob's cancel"
    );
}
