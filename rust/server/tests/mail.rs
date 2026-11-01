//! Mailer → Mailpit round trip, the Mailpit test client, and the mail helpers.
//! Needs the dev compose's Mailpit (or `MAILPIT_URL` / `MAILPIT_SMTP_HOST` / `_PORT`).

mod common;

use app::server::mail::{MailError, Mailer, Message, escape_html};
use app::test_support::mailpit::{Mailpit, first_url_with_path, smtp_vars, unique_email};
use common::*;

fn mailpit_config() -> app::server::Config {
    let vars = smtp_vars();
    let vars: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (*k, v.as_str())).collect();
    config_with(&vars)
}

#[tokio::test]
async fn sends_through_mailpit_and_reads_back() {
    let config = mailpit_config();
    let mailer = Mailer::new(&config.mail).expect("mailer");
    let mailpit = Mailpit::from_env();
    let to = unique_email("roundtrip");

    let name = "Tom & <Jerry>";
    let link = "http://localhost:3000/auth/reset/abc_DEF-123?x=1&y=2";
    let handle = mailer.send(Message {
        to: to.clone(),
        subject: "Reset your password".into(),
        html: format!(
            "<p>Hello {}</p><p><a href=\"{}\">Reset</a></p><p>See http://localhost:3000/other</p>",
            escape_html(name),
            escape_html(link)
        ),
        text: format!("Hello {name}\n\nReset: {link}\n"),
    });
    assert!(handle.id() > 0);
    handle.outcome().await.expect("delivered");

    let list = mailpit.messages_for(&to).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].subject, "Reset your password");

    let mail = mailpit.wait_for_latest(&to).await;
    assert_eq!(mail.subject, "Reset your password");
    assert_eq!(mail.to[0].address, to);
    let from = mail.from.as_ref().expect("from");
    assert_eq!(from.address, "santa@mail.test");
    assert_eq!(from.name, "Secret Santa");
    assert!(mail.text.contains("Hello Tom & <Jerry>"), "{}", mail.text);
    assert!(
        mail.html.contains("Hello Tom &amp; &lt;Jerry&gt;"),
        "{}",
        mail.html
    );
    assert_eq!(
        mail.first_url_with_path("/auth/reset/").as_deref(),
        Some(link)
    );
    assert_eq!(
        first_url_with_path(&mail.html, "/auth/reset/").as_deref(),
        Some(link),
        "HTML entities are decoded"
    );
    assert_eq!(mail.first_url_with_path("/nothing"), None);

    mailpit.delete_for(&to).await;
    assert!(mailpit.messages_for(&to).await.is_empty());
}

#[tokio::test]
async fn invalid_recipient_and_unreachable_server_fail_without_panicking() {
    let mailer = Mailer::new(&mailpit_config().mail).expect("mailer");
    let res = mailer
        .send(Message {
            to: "not an address".into(),
            subject: "x".into(),
            html: "<p>x</p>".into(),
            text: "x".into(),
        })
        .outcome()
        .await;
    assert_eq!(res, Err(MailError::Address));

    // Port 9 (discard) on localhost: nothing listens, the handle reports an SMTP error.
    let config = config_with(&[
        ("MAIL_HOST", "127.0.0.1"),
        ("MAIL_PORT", "9"),
        ("MAIL_ENCRYPTION", "none"),
    ]);
    let mailer = Mailer::new(&config.mail).expect("mailer");
    let res = mailer
        .send(Message {
            to: unique_email("unreachable"),
            subject: "x".into(),
            html: "<p>x</p>".into(),
            text: "x".into(),
        })
        .outcome()
        .await;
    assert!(matches!(res, Err(MailError::Smtp(_))), "{res:?}");
}

// Async: the pooled transport needs a Tokio runtime when it is dropped.
#[tokio::test]
async fn transports_build_for_every_encryption() {
    for (port, enc) in [("465", "ssl"), ("587", "tls"), ("1025", "none")] {
        let config = config_with(&[
            ("MAIL_HOST", "smtp.example.com"),
            ("MAIL_PORT", port),
            ("MAIL_ENCRYPTION", enc),
            ("MAIL_USERNAME", "user"),
            ("MAIL_PASSWORD", "secret"),
        ]);
        let mailer = Mailer::new(&config.mail).expect("mailer");
        let debug = format!("{mailer:?}");
        assert!(!debug.contains("secret"), "{debug}");
    }
}

#[test]
fn message_debug_hides_body() {
    let msg = Message {
        to: "alice@example.com".into(),
        subject: "s".into(),
        html: "token=SECRET".into(),
        text: "token=SECRET".into(),
    };
    let debug = format!("{msg:?}");
    assert!(
        !debug.contains("SECRET") && !debug.contains("alice"),
        "{debug}"
    );
}

#[test]
fn escape_html_escapes_markup_and_quotes() {
    assert_eq!(
        escape_html(r#"<a href="x">Tom & 'Jerry'</a>"#),
        "&lt;a href=&quot;x&quot;&gt;Tom &amp; &#39;Jerry&#39;&lt;/a&gt;"
    );
    assert_eq!(escape_html("plain äöü"), "plain äöü");
}

#[test]
fn url_extraction() {
    let text = "Go to https://x.example/auth/verify/tok123. Or (http://x.example/other).";
    assert_eq!(
        first_url_with_path(text, "/auth/verify/").as_deref(),
        Some("https://x.example/auth/verify/tok123")
    );
    assert_eq!(
        first_url_with_path(text, "/other").as_deref(),
        Some("http://x.example/other")
    );
    assert_eq!(first_url_with_path("no links", "/"), None);
    let html = r#"<a href='http://h/join/ABC?next=%2Fg&amp;x=1'>join</a>"#;
    assert_eq!(
        first_url_with_path(html, "/join/").as_deref(),
        Some("http://h/join/ABC?next=%2Fg&x=1")
    );
}
