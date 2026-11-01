//! The three auth mails (spec: auth, "Mail sending"): registration link, existing-account
//! notice, password reset. HTML with a plain-text alternative; every interpolated value is
//! HTML-escaped; links are built from `APP_URL`. Sending is off the request path; failures
//! are logged with the mail kind and user id only (never address, token or link).

use crate::i18n::I18n;
use crate::server::mail::{Message, escape_html};
use crate::server::{AppState, Config};
use crate::t;

/// Absolute URL of `path` under `APP_URL`.
pub fn app_link(config: &Config, path: &str) -> String {
    format!("{}{path}", config.app_url.trim_end_matches('/'))
}

/// One block of a mail body.
enum Block {
    Heading(String),
    Paragraph(String),
    /// A button-like link: (label, url).
    Button(String, String),
    /// "If the button does not work ..." followed by the raw link.
    Fallback(String, String),
}

/// Renders `blocks` as (html, text).
fn render(i18n: &I18n, subject: &str, blocks: &[Block]) -> (String, String) {
    let mut html = format!(
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head><meta charset=\"utf-8\"><title>{}</title></head>\n\
         <body style=\"font-family: Arial, sans-serif; line-height: 1.5; color: #212529; max-width: 600px; margin: 0 auto; padding: 16px;\">\n",
        i18n.locale().code(),
        escape_html(subject)
    );
    let mut text = String::new();
    for block in blocks {
        match block {
            Block::Heading(heading) => {
                html.push_str(&format!(
                    "<h1 style=\"font-size: 22px; color: #c0392b;\">{}</h1>\n",
                    escape_html(heading)
                ));
                text.push_str(&format!("{heading}\n\n"));
            }
            Block::Paragraph(paragraph) => {
                html.push_str(&format!("<p>{}</p>\n", escape_html(paragraph)));
                text.push_str(&format!("{paragraph}\n\n"));
            }
            Block::Button(label, url) => {
                html.push_str(&format!(
                    "<p><a href=\"{url}\" style=\"display: inline-block; padding: 10px 18px; background: #c0392b; color: #ffffff; text-decoration: none; border-radius: 4px;\">{label}</a></p>\n",
                    url = escape_html(url),
                    label = escape_html(label)
                ));
                text.push_str(&format!("{label}: {url}\n\n"));
            }
            Block::Fallback(note, url) => {
                html.push_str(&format!(
                    "<p style=\"font-size: 13px; color: #6c757d;\">{note}<br><a href=\"{url}\">{url}</a></p>\n",
                    note = escape_html(note),
                    url = escape_html(url)
                ));
            }
        }
    }
    html.push_str("</body>\n</html>\n");
    (html, text)
}

fn common_footer(i18n: &I18n) -> [Block; 2] {
    [
        Block::Paragraph(i18n.t("email.common.ignore")),
        Block::Paragraph(i18n.t("email.common.signature")),
    ]
}

/// The registration mail (new address), in the request locale.
pub fn registration(i18n: &I18n, to: &str, link: &str) -> Message {
    let subject = i18n.t("email.registration.subject");
    let mut blocks = vec![
        Block::Heading(i18n.t("email.registration.heading")),
        Block::Paragraph(i18n.t("email.registration.body")),
        Block::Button(i18n.t("email.registration.button"), link.to_string()),
        Block::Fallback(i18n.t("email.common.linkFallback"), link.to_string()),
        Block::Paragraph(i18n.t("email.registration.expires")),
    ];
    blocks.extend(common_footer(i18n));
    let (html, text) = render(i18n, &subject, &blocks);
    Message {
        to: to.to_string(),
        subject,
        html,
        text,
    }
}

/// The notice for a register call with an already registered address, in the account
/// locale.
pub fn existing_account(i18n: &I18n, config: &Config, to: &str) -> Message {
    let subject = i18n.t("email.existingAccount.subject");
    let mut blocks = vec![
        Block::Heading(i18n.t("email.existingAccount.heading")),
        Block::Paragraph(i18n.t("email.existingAccount.body")),
        Block::Button(
            i18n.t("email.existingAccount.loginButton"),
            app_link(config, "/auth/login"),
        ),
        Block::Button(
            i18n.t("email.existingAccount.resetButton"),
            app_link(config, "/auth/forgot-password"),
        ),
    ];
    blocks.extend(common_footer(i18n));
    let (html, text) = render(i18n, &subject, &blocks);
    Message {
        to: to.to_string(),
        subject,
        html,
        text,
    }
}

/// The password-reset mail, in the account locale.
pub fn password_reset(i18n: &I18n, to: &str, name: &str, link: &str) -> Message {
    let subject = i18n.t("email.passwordReset.subject");
    let mut blocks = vec![
        Block::Paragraph(t!(i18n, "email.passwordReset.greeting", name = name)),
        Block::Paragraph(i18n.t("email.passwordReset.body")),
        Block::Button(i18n.t("email.passwordReset.button"), link.to_string()),
        Block::Fallback(i18n.t("email.common.linkFallback"), link.to_string()),
        Block::Paragraph(i18n.t("email.passwordReset.expires")),
    ];
    blocks.extend(common_footer(i18n));
    let (html, text) = render(i18n, &subject, &blocks);
    Message {
        to: to.to_string(),
        subject,
        html,
        text,
    }
}

/// Queues `message` and logs a failure with `kind` and `user_id` only.
pub fn queue(state: &AppState, kind: &'static str, user_id: Option<i32>, message: Message) {
    let handle = state.mailer.send(message);
    let mail_id = handle.id();
    tokio::spawn(async move {
        if let Err(err) = handle.outcome().await {
            tracing::warn!(kind, ?user_id, mail_id, error = %err, "auth mail not delivered");
        }
    });
}
