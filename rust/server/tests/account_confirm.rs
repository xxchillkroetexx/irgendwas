//! account slice: opening and confirming a verification link (AC29-AC36). The pending
//! change is seeded with a known token (stored as SHA-256 like the app does); the mail side
//! of the request is covered in account_email.rs.

mod common;

use app::account_api::{AccountError, ConfirmResult, ProfileView};
use app::auth_api::AuthError;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{
    ALICE_PASSWORD, BOB_PASSWORD, create_bob, set_pending_email, set_reset_token, user_row,
    user_rows,
};
use common::account::*;
use common::auth::*;
use sqlx::MySqlPool;

/// Alice with a live pending change to a fresh address, verification token `tok('v')`.
async fn pending_alice(pool: &MySqlPool) -> (World, String) {
    let w = alice_world(pool).await;
    let new = unique_email("alice-new");
    set_pending_email(pool, w.id, &new, &tok('v'), 3600).await;
    (w, new)
}

fn verify_path(token: &str) -> String {
    format!("{VERIFY}{token}")
}

// -------------------------------------------------------------------------- AC29

#[sqlx::test(migrations = "../migrations")]
async fn ac29_opening_the_link_shows_a_confirm_button_and_changes_nothing(pool: MySqlPool) {
    let (mut w, _new) = pending_alice(&pool).await;
    let before = user_rows(&pool).await;
    let path = verify_path(&tok('v'));

    let mut anon = Client::new(&w.app);
    for (who, client) in [("anonymous", &mut anon), ("logged in", &mut w.c)] {
        for _ in 0..2 {
            let page = client.get(&path).await;
            assert_eq!(page.status, 200, "{who}");
            for key in [
                "user.verifyEmail.title",
                "user.verifyEmail.description",
                "user.verifyEmail.submit",
            ] {
                assert!(html_contains(&page.body, &en(key)), "{who}: {key}");
            }
        }
    }
    assert_eq!(user_rows(&pool).await, before, "GET is read-only");
}

// -------------------------------------------------------------------------- AC30

#[sqlx::test(migrations = "../migrations")]
async fn ac30_confirming_without_a_session_applies_the_change(pool: MySqlPool) {
    let (w, new) = pending_alice(&pool).await;
    set_reset_token(&pool, w.id, &tok('r'), 1800).await;
    let before = user_row(&pool, w.id).await;
    let mut anon = Client::new(&w.app);

    let result = confirm_email_change(&mut anon, &tok('v'))
        .await
        .account_result::<ConfirmResult>()
        .expect("confirmed");
    assert!(
        !result.logged_in,
        "no session: the page offers the login link"
    );

    let after = user_row(&pool, w.id).await;
    assert_eq!(after.email, new);
    assert_eq!(after.pending_email, None);
    assert_eq!(after.email_verification_token, None);
    assert_eq!(after.email_verification_expires, None);
    assert_eq!(after.reset_token, None);
    assert_eq!(after.reset_token_expires, None);
    assert_eq!(after.password, before.password);
    assert_eq!(after.name, before.name);
    assert_eq!(after.session_epoch, before.session_epoch);

    // Logging in with the new address works, with the old one it fails.
    let mut login = Client::new(&w.app);
    assert_eq!(
        login.login(&w.email, ALICE_PASSWORD).await.err(),
        AuthError::InvalidCredentials
    );
    login.login(&new, ALICE_PASSWORD).await.ok();
    assert_eq!(login.whoami().await.as_deref(), Some(new.as_str()));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac30_the_notice_goes_to_the_old_address_and_holds_no_token(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let (w, new) = pending_alice(&pool).await;
    let mut anon = Client::new(&w.app);

    confirm_email_change(&mut anon, &tok('v'))
        .await
        .account_ok();

    let notices = mailpit.wait_for_count(&w.email, 1).await;
    assert_eq!(
        mails_after_settle(&mailpit, &w.email).await,
        1,
        "exactly one notice"
    );
    assert_eq!(
        mailpit.messages_for(&new).await.len(),
        0,
        "nothing to the new address"
    );
    let notice = &notices[0];
    assert_eq!(notice.subject, en("email.emailChanged.subject"));
    assert!(
        notice.html.contains(&new) && notice.text.contains(&new),
        "names the new address"
    );
    for body in [&notice.html, &notice.text] {
        assert!(!body.contains(&tok('v')), "no token");
        assert!(!body.contains(VERIFY), "no verification link");
        assert!(!body.contains("reset-password"), "no reset link");
    }
    assert!(!notice.text.trim().is_empty(), "plain-text part");
}

// -------------------------------------------------------------------------- AC31

#[sqlx::test(migrations = "../migrations")]
async fn ac31_a_link_works_only_once(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let (w, new) = pending_alice(&pool).await;
    let mut anon = Client::new(&w.app);

    confirm_email_change(&mut anon, &tok('v'))
        .await
        .account_ok();
    let done = user_row(&pool, w.id).await;
    assert_eq!(done.email, new);

    assert_eq!(
        confirm_email_change(&mut anon, &tok('v'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(user_row(&pool, w.id).await, done);
    assert_eq!(
        mails_after_settle(&mailpit, &w.email).await,
        1,
        "no second notice"
    );
}

// -------------------------------------------------------------------------- AC32

#[sqlx::test(migrations = "../migrations")]
async fn ac32_an_expired_link_is_rejected_and_the_row_is_left_alone(pool: MySqlPool) {
    let w = alice_world(&pool).await;
    let new = unique_email("alice-new");
    set_pending_email(&pool, w.id, &new, &tok('v'), -1).await;
    let before = user_rows(&pool).await;
    let mut anon = Client::new(&w.app);

    assert_eq!(
        confirm_email_change(&mut anon, &tok('v'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(
        user_rows(&pool).await,
        before,
        "email unchanged, expired row kept"
    );
    assert_eq!(user_row(&pool, w.id).await.email, w.email);
}

// -------------------------------------------------------------------------- AC33

#[sqlx::test(migrations = "../migrations")]
async fn ac33_malformed_and_unknown_tokens_are_invalid(pool: MySqlPool) {
    let (w, _new) = pending_alice(&pool).await;
    let before = user_rows(&pool).await;
    let mut anon = Client::new(&w.app);

    let cases = [
        ("unknown, well-formed", tok('u')),
        ("too short", "abc".to_string()),
        ("empty", String::new()),
        ("43 chars with +", format!("{}+", "a".repeat(42))),
        ("43 chars with /", format!("/{}", "a".repeat(42))),
        ("43 chars with =", format!("{}=", "a".repeat(42))),
        ("43 chars with a space", format!(" {}", "a".repeat(42))),
        ("43 chars, one multi-byte", format!("é{}", "a".repeat(42))),
        ("44 chars", "v".repeat(44)),
        ("42 chars", "v".repeat(42)),
        ("the right token in the wrong case", tok('V')),
        ("the right token with a newline", format!("{}\n", tok('v'))),
    ];
    for (what, token) in cases {
        assert_eq!(
            confirm_email_change(&mut anon, &token).await.account_err(),
            AccountError::InvalidOrExpiredToken,
            "{what}"
        );
    }
    assert_eq!(user_rows(&pool).await, before, "nothing changed");
    // The real link still works afterwards.
    confirm_email_change(&mut anon, &tok('v'))
        .await
        .account_ok();
}

#[sqlx::test(migrations = "../migrations")]
async fn ac33_a_token_hash_without_a_pending_address_is_invalid(pool: MySqlPool) {
    let w = alice_world(&pool).await;
    sqlx::query(
        "UPDATE users SET email_verification_token = SHA2(?, 256), \
         email_verification_expires = DATE_ADD(CURRENT_TIMESTAMP, INTERVAL 1 HOUR) WHERE id = ?",
    )
    .bind(tok('n'))
    .bind(w.id)
    .execute(&pool)
    .await
    .unwrap();
    let before = user_rows(&pool).await;
    let mut anon = Client::new(&w.app);

    assert_eq!(
        confirm_email_change(&mut anon, &tok('n'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(user_rows(&pool).await, before);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac33_the_stored_hash_itself_is_not_a_token(pool: MySqlPool) {
    let (w, _new) = pending_alice(&pool).await;
    let hash = sha256_hex(&pool, &tok('v')).await;
    let mut anon = Client::new(&w.app);
    // 64 hex characters: not a valid token shape. A leaked hash must not be usable.
    assert_eq!(
        confirm_email_change(&mut anon, &hash).await.account_err(),
        AccountError::InvalidOrExpiredToken
    );
    assert_eq!(user_row(&pool, w.id).await.email, w.email);
}

// -------------------------------------------------------------------------- AC34

#[sqlx::test(migrations = "../migrations")]
async fn ac34_an_address_taken_meanwhile_yields_email_taken(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let w = alice_world(&pool).await;
    let bob_email = unique_email("bob");
    let bob_id = create_bob(&pool, &bob_email).await;
    // Alice asked for Bob's address (allowed: requests do not check), Bob owns it now.
    set_pending_email(&pool, w.id, &bob_email, &tok('v'), 3600).await;
    let bob_before = user_row(&pool, bob_id).await;
    let mut anon = Client::new(&w.app);

    assert_eq!(
        confirm_email_change(&mut anon, &tok('v'))
            .await
            .account_err(),
        AccountError::EmailTaken
    );

    let alice = user_row(&pool, w.id).await;
    assert_eq!(alice.email, w.email, "Alice's address is unchanged");
    assert_eq!(alice.pending_email, None);
    assert_eq!(alice.email_verification_token, None);
    assert_eq!(alice.email_verification_expires, None);
    assert_eq!(
        user_row(&pool, bob_id).await,
        bob_before,
        "Bob's row is unchanged"
    );
    assert_eq!(
        mails_after_settle(&mailpit, &w.email).await,
        0,
        "no notice for a failed change"
    );

    // The link is spent.
    assert_eq!(
        confirm_email_change(&mut anon, &tok('v'))
            .await
            .account_err(),
        AccountError::InvalidOrExpiredToken
    );
    // Bob can still log in with his address and password.
    let mut bob = Client::new(&w.app);
    bob.login(&bob_email, BOB_PASSWORD).await.ok();
}

// -------------------------------------------------------------------------- AC35

#[sqlx::test(migrations = "../migrations")]
async fn ac35_confirming_in_another_users_browser_changes_the_token_owner(pool: MySqlPool) {
    let (w, new) = pending_alice(&pool).await;
    let bob_email = unique_email("bob");
    let bob_id = create_bob(&pool, &bob_email).await;
    let bob_before = user_row(&pool, bob_id).await;
    let mut bob = Client::new(&w.app);
    bob.login(&bob_email, BOB_PASSWORD).await.ok();

    let result = confirm_email_change(&mut bob, &tok('v'))
        .await
        .account_result::<ConfirmResult>()
        .expect("confirmed");
    assert!(
        result.logged_in,
        "Bob has a session: the page offers the profile link"
    );

    assert_eq!(
        user_row(&pool, w.id).await.email,
        new,
        "Alice's email changed"
    );
    assert_eq!(
        user_row(&pool, bob_id).await,
        bob_before,
        "Bob's row is unchanged"
    );
    assert_eq!(
        bob.whoami().await.as_deref(),
        Some(bob_email.as_str()),
        "Bob's session is valid"
    );
    let profile = get_profile(&mut bob)
        .await
        .account_result::<ProfileView>()
        .unwrap();
    assert_eq!(profile.email, bob_email);
}

// -------------------------------------------------------------------------- AC36

#[sqlx::test(migrations = "../migrations")]
async fn ac36_confirming_keeps_every_session_of_the_owner_valid(pool: MySqlPool) {
    let (w, new) = pending_alice(&pool).await;
    let epoch_before = user_row(&pool, w.id).await.session_epoch;
    let mut a = w.c;
    let mut b = Client::new(&w.app);
    b.login(&w.email, ALICE_PASSWORD).await.ok();

    // Confirmed from inside session A.
    let result = confirm_email_change(&mut a, &tok('v'))
        .await
        .account_result::<ConfirmResult>()
        .expect("confirmed");
    assert!(result.logged_in);

    for (name, client) in [("A", &mut a), ("B", &mut b)] {
        assert_eq!(
            client.whoami().await.as_deref(),
            Some(new.as_str()),
            "{name}"
        );
        let profile = get_profile(client)
            .await
            .account_result::<ProfileView>()
            .unwrap();
        assert_eq!(profile.email, new, "{name}");
        assert_eq!(client.get("/user/profile").await.status, 200, "{name}");
    }
    assert_eq!(user_row(&pool, w.id).await.session_epoch, epoch_before);
}
