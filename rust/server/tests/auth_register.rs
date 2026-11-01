//! auth slice: two-step registration (AC21-AC31, AC43, AC46, AC48, AC49). Tests that need a
//! mail read it from Mailpit (unique recipient addresses); the others seed the pending
//! registration directly so they stay fast.

mod common;

use std::time::Duration;

use app::auth_api::{AuthError, Field, FieldError};
use app::flash::FlashMessage;
use app::i18n::{Locale, translate, translate_with};
use app::paths::DASHBOARD_PATH;
use app::server::password;
use app::test_support::mailpit::{Mailpit, unique_email};
use app::test_support::{create_alice, insert_pending_registration, set_user_locale};
use common::auth::*;
use sqlx::MySqlPool;

const CONFIRM: &str = "/auth/register/confirm/";

/// A syntactically valid 43-character token made of one repeated character.
fn tok(c: char) -> String {
    c.to_string().repeat(43)
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(800)).await;
}

async fn count(pool: &MySqlPool, sql: &'static str, bind: &str) -> i64 {
    sqlx::query_scalar(sql)
        .bind(bind)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn users_with(pool: &MySqlPool, email: &str) -> i64 {
    count(pool, "SELECT COUNT(*) FROM users WHERE email = ?", email).await
}

async fn pending_with(pool: &MySqlPool, email: &str) -> i64 {
    count(
        pool,
        "SELECT COUNT(*) FROM pending_registrations WHERE email = ?",
        email,
    )
    .await
}

async fn pending_hash(pool: &MySqlPool, email: &str) -> String {
    sqlx::query_scalar("SELECT token_hash FROM pending_registrations WHERE email = ?")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn sha256_hex(pool: &MySqlPool, s: &str) -> String {
    sqlx::query_scalar("SELECT SHA2(?, 256)")
        .bind(s)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn validation(err: &AuthError, field: Field, error: FieldError) {
    match err {
        AuthError::Validation(list) => assert!(
            list.contains(&(field, error)),
            "expected {field:?}/{error:?} in {list:?}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}

fn assert_redirects_to(reply: &Reply, location: &str) {
    assert!(reply.status.is_redirection(), "status {}", reply.status);
    assert_eq!(reply.location().as_deref(), Some(location));
}

// --------------------------------------------------------------------- AC21

#[sqlx::test(migrations = "../migrations")]
async fn ac21_register_stores_a_hashed_token_and_mails_the_link(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);
    let carol = unique_email("carol");

    c.register(&format!(" {} ", carol.to_uppercase()), None)
        .await
        .ok();

    let (email, token_hash, next_path, ttl): (String, String, Option<String>, i64) =
        sqlx::query_as(
            "SELECT email, token_hash, next_path, TIMESTAMPDIFF(SECOND, CURRENT_TIMESTAMP, expires_at) \
             FROM pending_registrations",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(email, carol, "stored trimmed and lowercase");
    assert!(is_sha256_hex(&token_hash), "{token_hash}");
    assert_eq!(next_path, None);
    assert!(
        (86_300..=86_400).contains(&ttl),
        "expires in 24 h, got {ttl}s"
    );
    assert_eq!(users_with(&pool, &carol).await, 0, "no user yet");

    let mails = mailpit.wait_for_count(&carol, 1).await;
    assert_eq!(mails.len(), 1);
    let mail = &mails[0];
    assert_eq!(mail.to[0].address, carol);
    assert_eq!(
        mail.subject,
        translate(Locale::En, "email.registration.subject")
    );
    let link = mail
        .first_url_with_path(CONFIRM)
        .expect("confirmation link");
    assert!(
        link.starts_with(&format!("http://localhost:3000{CONFIRM}")),
        "{link}"
    );
    let token = token_in(&link, CONFIRM);
    assert!(is_token(&token), "43-char base64url token, got {token:?}");
    assert_ne!(token, token_hash);
    assert_eq!(sha256_hex(&pool, &token).await, token_hash);
}

// --------------------------------------------------------------------- AC22

#[sqlx::test(migrations = "../migrations")]
async fn ac22_registering_an_existing_address_looks_the_same_and_mails_a_notice(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let alice = unique_email("alice");
    create_alice(&pool, &alice).await;
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);

    let fresh = c.register(&unique_email("someone"), None).await;
    let existing = c.register(&alice.to_uppercase(), None).await;
    fresh.ok();
    existing.ok();
    assert_eq!(fresh.status, existing.status);
    assert_eq!(fresh.body, existing.body, "byte-identical response");

    assert_eq!(pending_with(&pool, &alice).await, 0);
    let mails = mailpit.wait_for_count(&alice, 1).await;
    settle().await;
    assert_eq!(mailpit.messages_for(&alice).await.len(), 1);
    let mail = &mails[0];
    assert_eq!(
        mail.subject,
        translate(Locale::En, "email.existingAccount.subject")
    );
    assert!(
        mail.first_url_with_path("/auth/login").is_some(),
        "{}",
        mail.html
    );
    assert!(mail.first_url_with_path("/auth/forgot-password").is_some());
    assert!(
        mail.first_url_with_path(CONFIRM).is_none(),
        "no registration link"
    );
}

// --------------------------------------------------------------------- AC23

#[sqlx::test(migrations = "../migrations")]
async fn ac23_invalid_emails_are_rejected_without_mail(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);
    let long = format!("{}@example.com", "a".repeat(250));

    let cases: Vec<(String, FieldError)> = vec![
        ("".into(), FieldError::EmailRequired),
        ("carol".into(), FieldError::EmailInvalid),
        ("carol@".into(), FieldError::EmailInvalid),
        ("@example.com".into(), FieldError::EmailInvalid),
        ("carol@example".into(), FieldError::EmailInvalid),
        ("ca rol@example.com".into(), FieldError::EmailInvalid),
        ("carol@@example.com".into(), FieldError::EmailInvalid),
        ("carol@exa@mple.com".into(), FieldError::EmailInvalid),
        ("carol@.example.com".into(), FieldError::EmailInvalid),
        ("carol@example.com.".into(), FieldError::EmailInvalid),
        ("carol@exam\tple.com".into(), FieldError::EmailInvalid),
        (long, FieldError::EmailInvalid),
    ];
    for (email, expected) in &cases {
        let err = c.register(email, None).await.err();
        validation(&err, Field::Email, *expected);
        // The password-reset form validates the same way.
        let err = c.forgot(email).await.err();
        validation(&err, Field::Email, *expected);
    }
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_registrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pending, 0);

    settle().await;
    for (email, _) in cases.iter().take(11).filter(|(e, _)| !e.is_empty()) {
        assert!(
            mailpit.messages_for(email).await.is_empty(),
            "mail to {email:?}"
        );
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn ac23_the_length_limit_is_255_characters(pool: MySqlPool) {
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    let max = format!("{}@example.com", "a".repeat(243));
    assert_eq!(max.len(), 255);
    c.register(&max, None).await.ok();
    assert_eq!(pending_with(&pool, &max).await, 1);

    let too_long = format!("{}@example.com", "a".repeat(244));
    validation(
        &c.register(&too_long, None).await.err(),
        Field::Email,
        FieldError::EmailInvalid,
    );
}

// --------------------------------------------------------------------- AC24

#[sqlx::test(migrations = "../migrations")]
async fn ac24_the_confirmation_page_shows_the_email_and_does_not_consume_the_token(
    pool: MySqlPool,
) {
    let email = "carol@example.com";
    insert_pending_registration(&pool, email, &tok('a'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    for _ in 0..2 {
        let page = c.get(&format!("{CONFIRM}{}", tok('a'))).await;
        assert_eq!(page.status, 200);
        assert!(html_contains(&page.body, email), "email shown on the page");
        assert!(html_contains(
            &page.body,
            &translate_with(
                Locale::En,
                "auth.registerConfirm.intro",
                &[("email", email)]
            )
        ));
        // Token pages must not leak the token or be cached.
        assert_eq!(
            page.header("referrer-policy").as_deref(),
            Some("no-referrer")
        );
        assert!(
            page.header("cache-control")
                .unwrap_or_default()
                .contains("no-store")
        );
    }

    // The read-only server fn behaves the same, twice.
    for _ in 0..2 {
        let reply = c
            .call("get_pending_registration", &[("token", &tok('a'))])
            .await;
        let got: app::auth_api::PendingRegistration = reply.result().unwrap();
        assert_eq!(got.email, email);
    }
    assert_eq!(pending_with(&pool, email).await, 1, "token not consumed");
}

// --------------------------------------------------------------------- AC25

#[sqlx::test(migrations = "../migrations")]
async fn ac25_completing_creates_the_user_logs_in_and_redirects(pool: MySqlPool) {
    let email = "carol@example.com";
    insert_pending_registration(&pool, email, &tok('a'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    let reply = c
        .complete(&tok('a'), "  Carol  ", "carolpass1", "carolpass1")
        .await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some(DASHBOARD_PATH));

    let (name, hash, last_login_set, epoch): (String, String, bool, u32) = sqlx::query_as(
        "SELECT name, password, last_login IS NOT NULL, session_epoch FROM users WHERE email = ?",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(name, "Carol");
    assert!(hash.starts_with("$argon2id$"), "{hash}");
    assert!(password::verify("carolpass1", &hash));
    assert!(last_login_set, "last_login is set");
    assert_eq!(epoch, 0);
    assert_eq!(pending_with(&pool, email).await, 0, "pending row gone");

    // The layout consumes the flash while rendering the dashboard.
    let page = c.get(DASHBOARD_PATH).await;
    assert_eq!(page.status, 200);
    assert!(
        html_contains(
            &page.body,
            &translate_with(
                Locale::En,
                "flash.success.registration_complete",
                &[("name", "Carol")]
            )
        ),
        "registration_complete flash with the name on the dashboard"
    );
    assert_eq!(c.whoami().await.as_deref(), Some(email));
}

// --------------------------------------------------------------------- AC26

#[sqlx::test(migrations = "../migrations")]
async fn ac26_invalid_name_or_password_is_rejected_and_the_link_survives(pool: MySqlPool) {
    let email = "carol@example.com";
    insert_pending_registration(&pool, email, &tok('a'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);
    let t = tok('a');
    let hundred_one = "n".repeat(101);
    let p257 = "p".repeat(257);

    validation(
        &c.complete(&t, "", "carolpass1", "carolpass1").await.err(),
        Field::Name,
        FieldError::NameRequired,
    );
    validation(
        &c.complete(&t, "   ", "carolpass1", "carolpass1")
            .await
            .err(),
        Field::Name,
        FieldError::NameRequired,
    );
    validation(
        &c.complete(&t, &hundred_one, "carolpass1", "carolpass1")
            .await
            .err(),
        Field::Name,
        FieldError::NameTooLong,
    );
    validation(
        &c.complete(&t, "Carol", "short12", "short12").await.err(),
        Field::Password,
        FieldError::PasswordTooShort,
    );
    validation(
        &c.complete(&t, "Carol", &p257, &p257).await.err(),
        Field::Password,
        FieldError::PasswordTooLong,
    );
    validation(
        &c.complete(&t, "Carol", "carolpass1", "carolpass2")
            .await
            .err(),
        Field::PasswordConfirm,
        FieldError::PasswordsDontMatch,
    );

    assert_eq!(users_with(&pool, email).await, 0, "no user created");
    assert_eq!(pending_with(&pool, email).await, 1);
    assert_eq!(c.whoami().await, None);
    // The link still works afterwards.
    c.complete(&t, "Carol", "carolpass1", "carolpass1")
        .await
        .ok();
    assert_eq!(users_with(&pool, email).await, 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac26_lengths_count_characters_not_bytes(pool: MySqlPool) {
    insert_pending_registration(&pool, "a@example.com", &tok('a'), None, 3600).await;
    insert_pending_registration(&pool, "b@example.com", &tok('b'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    // 7 two-byte characters are still too short; 8 are enough.
    validation(
        &c.complete(&tok('a'), "Ann", &"ä".repeat(7), &"ä".repeat(7))
            .await
            .err(),
        Field::Password,
        FieldError::PasswordTooShort,
    );
    // 100 two-byte characters in the name, 256 in the password: both at the limit.
    let name = "é".repeat(100);
    let pw = "ä".repeat(256);
    c.complete(&tok('a'), &name, &pw, &pw).await.ok();
    let stored: String = sqlx::query_scalar("SELECT name FROM users WHERE email = 'a@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, name);

    let mut d = Client::new(&app);
    d.complete(&tok('b'), "Zoë 🎅", &"ö".repeat(8), &"ö".repeat(8))
        .await
        .ok();
    assert_eq!(users_with(&pool, "b@example.com").await, 1);
}

// --------------------------------------------------------------------- AC27

#[sqlx::test(migrations = "../migrations")]
async fn ac27_expired_used_and_unknown_links_redirect_to_register(pool: MySqlPool) {
    insert_pending_registration(&pool, "old@example.com", &tok('o'), None, -60).await;
    insert_pending_registration(&pool, "used@example.com", &tok('u'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());

    // Use one link up.
    let mut first = Client::new(&app);
    first
        .complete(&tok('u'), "Used", "usedpass1", "usedpass1")
        .await
        .ok();

    for token in [tok('o'), tok('u'), tok('z'), "short".to_string()] {
        let mut c = Client::new(&app);
        let page = c.get(&format!("{CONFIRM}{token}")).await;
        assert_redirects_to(&page, "/auth/register");
        let info = c.session_info().await;
        assert_flash(
            &info,
            &FlashMessage::error("flash.error.registration_link_invalid"),
        );

        assert_eq!(
            c.complete(&token, "Name", "namepass1", "namepass1")
                .await
                .err(),
            AuthError::InvalidRegistrationToken
        );
        assert_eq!(
            c.call("get_pending_registration", &[("token", &token)])
                .await
                .err(),
            AuthError::InvalidRegistrationToken
        );
    }
    assert_eq!(users_with(&pool, "old@example.com").await, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn ac27_a_later_register_call_supersedes_the_earlier_link(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);
    let carol = unique_email("carol");

    c.register(&carol, None).await.ok();
    mailpit.wait_for_count(&carol, 1).await;
    c.register(&carol, None).await.ok();
    let mails = mailpit.wait_for_count(&carol, 2).await;

    let hash = pending_hash(&pool, &carol).await;
    assert_eq!(
        pending_with(&pool, &carol).await,
        1,
        "one pending row per address"
    );
    let mut current = None;
    let mut stale = None;
    for mail in &mails {
        let token = token_in(&mail.first_url_with_path(CONFIRM).unwrap(), CONFIRM);
        if sha256_hex(&pool, &token).await == hash {
            current = Some(token);
        } else {
            stale = Some(token);
        }
    }
    let (current, stale) = (
        current.expect("current link"),
        stale.expect("superseded link"),
    );

    let page = c.get(&format!("{CONFIRM}{stale}")).await;
    assert_redirects_to(&page, "/auth/register");
    assert_eq!(
        c.complete(&stale, "Carol", "carolpass1", "carolpass1")
            .await
            .err(),
        AuthError::InvalidRegistrationToken
    );
    c.complete(&current, "Carol", "carolpass1", "carolpass1")
        .await
        .ok();
    assert_eq!(users_with(&pool, &carol).await, 1);
}

// --------------------------------------------------------------------- AC28

#[sqlx::test(migrations = "../migrations")]
async fn ac28_an_address_taken_meanwhile_invalidates_the_registration(pool: MySqlPool) {
    let email = "carol@example.com";
    insert_pending_registration(&pool, email, &tok('a'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    // Someone else owns the address before Carol completes (e.g. through an email change).
    app::test_support::create_user(&pool, "Taken", email).await;

    assert_eq!(
        c.complete(&tok('a'), "Carol", "carolpass1", "carolpass1")
            .await
            .err(),
        AuthError::InvalidRegistrationToken
    );
    assert_eq!(pending_with(&pool, email).await, 0, "pending row deleted");
    assert_eq!(users_with(&pool, email).await, 1, "no duplicate user");
    assert_eq!(c.whoami().await, None);
}

// --------------------------------------------------------------------- AC29

#[sqlx::test(migrations = "../migrations")]
async fn ac29_next_is_carried_from_login_through_registration(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);
    let next = "/join/ABCDEFGH";
    let carol = unique_email("carol");

    // The login page links to register and carries next; so does the way back.
    let login = c
        .get(&format!("/auth/login?next={}", urlencode(next)))
        .await;
    assert_eq!(
        carried_next(&login.body, "/auth/register?next="),
        Some(next.to_string())
    );
    let register = c
        .get(&format!("/auth/register?next={}", urlencode(next)))
        .await;
    assert_eq!(register.status, 200);
    assert_eq!(
        carried_next(&register.body, "/auth/login?next="),
        Some(next.to_string())
    );

    // The register page hands next to the server fn; completing lands on it.
    c.register(&carol, Some(next)).await.ok();
    let mail = mailpit.wait_for_latest(&carol).await;
    let token = token_in(&mail.first_url_with_path(CONFIRM).unwrap(), CONFIRM);
    let reply = c
        .complete(&token, "Carol", "carolpass1", "carolpass1")
        .await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some(next));
}

/// The value of `<prefix><value>` in an href of `html`, percent-decoded.
fn carried_next(html: &str, prefix: &str) -> Option<String> {
    let start = html.find(prefix)? + prefix.len();
    let rest = &html[start..];
    let end = rest.find(['"', '\'', '<', ' '])?;
    Some(urldecode(&rest[..end].replace("&amp;", "&")))
}

// --------------------------------------------------------------------- AC30

#[sqlx::test(migrations = "../migrations")]
async fn ac30_next_path_is_stored_server_side_and_survives_another_browser(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let app = router(&pool, &[], Opts::mail());
    let dave = unique_email("dave");
    let erin = unique_email("erin");

    let mut origin_browser = Client::new(&app);
    origin_browser
        .register(&dave, Some("/join/ABCDEFGH"))
        .await
        .ok();
    origin_browser
        .register(&erin, Some("//evil.example/x"))
        .await
        .ok();

    let next_of = |email: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT next_path FROM pending_registrations WHERE email = ?",
            )
            .bind(email)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(
        next_of(dave.clone()).await.as_deref(),
        Some("/join/ABCDEFGH")
    );
    assert_eq!(next_of(erin.clone()).await, None, "unsafe next is dropped");

    // The mail link is opened in a browser without any cookie from the register request.
    let mail = mailpit.wait_for_latest(&dave).await;
    let token = token_in(&mail.first_url_with_path(CONFIRM).unwrap(), CONFIRM);
    let mut other = Client::new(&app);
    let reply = other
        .complete(&token, "Dave", "davepass12", "davepass12")
        .await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some("/join/ABCDEFGH"));

    let mail = mailpit.wait_for_latest(&erin).await;
    let token = token_in(&mail.first_url_with_path(CONFIRM).unwrap(), CONFIRM);
    let mut other = Client::new(&app);
    let reply = other
        .complete(&token, "Erin", "erinpass12", "erinpass12")
        .await;
    reply.ok();
    assert_eq!(reply.location().as_deref(), Some(DASHBOARD_PATH));
}

#[sqlx::test(migrations = "../migrations")]
async fn ac30_next_path_from_a_seeded_registration_is_applied(pool: MySqlPool) {
    insert_pending_registration(
        &pool,
        "dave@example.com",
        &tok('d'),
        Some("/join/ABCDEFGH"),
        3600,
    )
    .await;
    insert_pending_registration(&pool, "frank@example.com", &tok('f'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());

    let reply = Client::new(&app)
        .complete(&tok('d'), "Dave", "davepass12", "davepass12")
        .await;
    assert_eq!(reply.location().as_deref(), Some("/join/ABCDEFGH"));
    let reply = Client::new(&app)
        .complete(&tok('f'), "Frank", "frankpass1", "frankpass1")
        .await;
    assert_eq!(reply.location().as_deref(), Some(DASHBOARD_PATH));
}

// --------------------------------------------------------------------- AC31

#[sqlx::test(migrations = "../migrations")]
async fn ac31_a_fourth_register_call_in_an_hour_sends_no_mail(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let app = router(&pool, &[], Opts::mail());
    let mut c = Client::new(&app);
    let carol = unique_email("carol");

    let mut replies = Vec::new();
    for _ in 0..3 {
        replies.push(c.register(&carol, None).await);
    }
    let mails = mailpit.wait_for_count(&carol, 3).await;
    let hash_before = pending_hash(&pool, &carol).await;

    let fourth = c.register(&carol, None).await;
    fourth.ok();
    assert_eq!(fourth.status, replies[2].status);
    assert_eq!(fourth.body, replies[2].body, "response unchanged");
    settle().await;
    assert_eq!(
        mailpit.messages_for(&carol).await.len(),
        3,
        "no fourth mail"
    );
    assert_eq!(
        pending_hash(&pool, &carol).await,
        hash_before,
        "no row written either"
    );

    // The link from the last delivered mail (the one stored) still works.
    let mut working = 0;
    for mail in &mails {
        let token = token_in(&mail.first_url_with_path(CONFIRM).unwrap(), CONFIRM);
        if sha256_hex(&pool, &token).await == hash_before {
            working += 1;
            let got = c
                .call("get_pending_registration", &[("token", &token)])
                .await;
            got.ok();
        }
    }
    assert_eq!(working, 1, "exactly the latest link is valid");
}

// --------------------------------------------------------------------- AC43

#[sqlx::test(migrations = "../migrations")]
async fn ac43_registration_mail_follows_the_request_locale(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let app = router(&pool, &[], Opts::mail());
    let carol = unique_email("carol");
    let mut german = Client::new(&app).with_cookie("locale", "de");
    german.register(&carol, None).await.ok();

    let mail = mailpit.wait_for_latest(&carol).await;
    let de_subject = translate(Locale::De, "email.registration.subject");
    assert_ne!(
        de_subject,
        translate(Locale::En, "email.registration.subject")
    );
    assert_eq!(mail.subject, de_subject);
    assert!(mail.html.contains("lang=\"de\""), "{}", mail.html);
    assert!(html_contains(
        &mail.html,
        &translate(Locale::De, "email.registration.heading")
    ));

    let dave = unique_email("dave");
    let mut english = Client::new(&app);
    english.register(&dave, None).await.ok();
    let mail = mailpit.wait_for_latest(&dave).await;
    assert_eq!(
        mail.subject,
        translate(Locale::En, "email.registration.subject")
    );
    assert!(mail.html.contains("lang=\"en\""), "{}", mail.html);
}

// --------------------------------------------------------------------- AC46

#[sqlx::test(migrations = "../migrations")]
async fn ac46_control_characters_in_the_name_are_rejected(pool: MySqlPool) {
    let email = "carol@example.com";
    insert_pending_registration(&pool, email, &tok('a'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());
    let mut c = Client::new(&app);

    for name in ["Ca\nrol", "Ca\trol", "Ca\u{0}rol", "Ca\u{7f}rol"] {
        let err = c
            .complete(&tok('a'), name, "carolpass1", "carolpass1")
            .await
            .err();
        validation(&err, Field::Name, FieldError::NameInvalid);
    }
    assert_eq!(users_with(&pool, email).await, 0);
    assert_eq!(pending_with(&pool, email).await, 1);
}

// --------------------------------------------------------------------- AC48

#[sqlx::test(migrations = "../migrations")]
async fn ac48_registration_stores_the_request_language(pool: MySqlPool) {
    insert_pending_registration(&pool, "de@example.com", &tok('a'), None, 3600).await;
    let app = router(&pool, &[], Opts::default());

    // German page (cookie): the account is German and so is the dashboard.
    let mut german = Client::new(&app).with_cookie("locale", "de");
    german
        .complete(&tok('a'), "Dora", "dorapass12", "dorapass12")
        .await
        .ok();
    let locale: Option<String> =
        sqlx::query_scalar("SELECT locale FROM users WHERE email = 'de@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(locale.as_deref(), Some("de"));
    assert_eq!(lang_of(&german.get(DASHBOARD_PATH).await.body), "de");

    // No cookie, Accept-Language en-US, instance default de: the request locale is en.
    insert_pending_registration(&pool, "en@example.com", &tok('b'), None, 3600).await;
    let app = router(&pool, &[("APP_LOCALE", "de")], Opts::default());
    let mut english = Client::new(&app).header("accept-language", "en-US");
    english
        .complete(&tok('b'), "Emma", "emmapass12", "emmapass12")
        .await
        .ok();
    let locale: Option<String> =
        sqlx::query_scalar("SELECT locale FROM users WHERE email = 'en@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(locale.as_deref(), Some("en"));
}

// --------------------------------------------------------------------- AC49

#[sqlx::test(migrations = "../migrations")]
async fn ac49_existing_account_notice_follows_the_account_language(pool: MySqlPool) {
    let mailpit = Mailpit::from_env();
    let email = unique_email("alice");
    let alice = create_alice(&pool, &email).await;
    set_user_locale(&pool, alice, Some("de")).await;
    let app = router(&pool, &[], Opts::mail());

    // The requester's page is English.
    let mut c = Client::new(&app).with_cookie("locale", "en");
    c.register(&email, None).await.ok();

    let mail = mailpit.wait_for_latest(&email).await;
    let de_subject = translate(Locale::De, "email.existingAccount.subject");
    assert_ne!(
        de_subject,
        translate(Locale::En, "email.existingAccount.subject")
    );
    assert_eq!(mail.subject, de_subject);
    assert!(mail.html.contains("lang=\"de\""), "{}", mail.html);
}
