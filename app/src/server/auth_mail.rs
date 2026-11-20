//! The three auth mails (spec: auth, "Mail sending"): registration link, existing-account
//! notice, password reset. HTML with a plain-text alternative; every interpolated value is
//! HTML-escaped; links are built from `APP_URL`. Sending is off the request path, with
//! retries, and an undelivered mail puts the link it replaced back ([`queue`], #102);
//! failures are logged with the mail kind and user id only (never address, token or link).

use chrono::{DateTime, Utc};
use sqlx::MySqlPool;

use crate::i18n::I18n;
use crate::server::mail::{MailError, Message, escape_html};
use crate::server::{AppState, Config, db};
use crate::t;

/// Absolute URL of `path` under `APP_URL`.
pub fn app_link(config: &Config, path: &str) -> String {
    format!("{}{path}", config.app_url.trim_end_matches('/'))
}

/// One block of a mail body (shared with `account_mail`).
pub(crate) enum Block {
    Heading(String),
    Paragraph(String),
    /// A button-like link: (label, url).
    Button(String, String),
    /// "If the button does not work ..." followed by the raw link.
    Fallback(String, String),
}

/// Renders `blocks` as (html, text): every interpolated value is HTML-escaped.
pub(crate) fn render(i18n: &I18n, subject: &str, blocks: &[Block]) -> (String, String) {
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

/// What an auth mail's link replaced, so it can be put back when the mail is not
/// delivered (#102). The new link exists only in the undelivered mail; without the undo
/// the link from the earlier request would be dead as well.
#[derive(Debug)]
pub enum Undo {
    /// Nothing to put back (the existing-account notice carries no token).
    Nothing,
    /// The reset token this request replaced on `users` (`None`: there was none).
    ResetToken {
        user_id: i32,
        new_hash: String,
        previous: Option<(String, Option<DateTime<Utc>>)>,
    },
    /// The pending registration this request replaced (`None`: it inserted the row).
    Registration {
        new_hash: String,
        previous: Option<PendingRegistrationRow>,
    },
}

/// A `pending_registrations` row as an [`Undo`] restores it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingRegistrationRow {
    pub token_hash: String,
    pub next_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Sends `message` off the request path. A failure that may pass (an SMTP connection
/// error, a timeout, a 4xx reply) is retried after each of `auth_retry_delays`. When the
/// mail is finally not delivered, `undo` puts the replaced link back, guarded by the new
/// token's hash so a newer request or a used link is never overwritten. Logs carry
/// `kind` and `user_id` only (never address, token or link).
///
/// The attempts live in this process: a restart during the waits ends them without the
/// undo, which leaves the state of a lost mail before #102.
pub fn queue(
    state: &AppState,
    kind: &'static str,
    user_id: Option<i32>,
    message: Message,
    undo: Undo,
) {
    let mailer = state.mailer.clone();
    let pool = state.pool.clone();
    let delays = state.config.mail.auth_retry_delays.clone();
    tokio::spawn(async move {
        let mut waits = delays.into_iter();
        let mut attempt = 1;
        let error = loop {
            let handle = mailer.send(message.clone());
            let mail_id = handle.id();
            match handle.outcome().await {
                Ok(()) => return,
                Err(err) => match waits.next() {
                    Some(wait) if may_pass(&err) => {
                        tracing::info!(kind, ?user_id, mail_id, attempt, error = %err, "auth mail not delivered, retrying");
                        tokio::time::sleep(wait).await;
                        attempt += 1;
                    }
                    _ => break err,
                },
            }
        };
        tracing::warn!(kind, ?user_id, attempt, error = %error, "auth mail not delivered");
        match apply_undo(&pool, &undo).await {
            Ok(true) => tracing::warn!(
                kind,
                ?user_id,
                "restored the link the undelivered mail replaced"
            ),
            Ok(false) => {}
            Err(err) => {
                tracing::warn!(kind, ?user_id, error = %db::redacted(&err), "could not restore the previous link")
            }
        }
    });
}

/// Whether another attempt can succeed: not for a bad address or message, a permanent
/// (5xx) SMTP reply, or a task that died.
fn may_pass(err: &MailError) -> bool {
    match err {
        MailError::Smtp(detail) => !detail.starts_with("permanent"),
        MailError::Address | MailError::Build(_) | MailError::Aborted => false,
    }
}

/// Applies `undo`; `true` when a row was put back or removed.
async fn apply_undo(pool: &MySqlPool, undo: &Undo) -> Result<bool, sqlx::Error> {
    let result = match undo {
        Undo::Nothing => return Ok(false),
        Undo::ResetToken {
            user_id,
            new_hash,
            previous,
        } => {
            let (token, expires) = match previous {
                Some((token, expires)) => (Some(token.as_str()), *expires),
                None => (None, None),
            };
            sqlx::query(
                "UPDATE users SET reset_token = ?, reset_token_expires = ? \
                 WHERE id = ? AND reset_token = ?",
            )
            .bind(token)
            .bind(expires)
            .bind(user_id)
            .bind(new_hash)
            .execute(pool)
            .await?
        }
        Undo::Registration {
            new_hash,
            previous: Some(row),
        } => {
            sqlx::query(
                "UPDATE pending_registrations \
                 SET token_hash = ?, next_path = ?, created_at = ?, expires_at = ? \
                 WHERE token_hash = ?",
            )
            .bind(&row.token_hash)
            .bind(&row.next_path)
            .bind(row.created_at)
            .bind(row.expires_at)
            .bind(new_hash)
            .execute(pool)
            .await?
        }
        Undo::Registration {
            new_hash,
            previous: None,
        } => {
            sqlx::query("DELETE FROM pending_registrations WHERE token_hash = ?")
                .bind(new_hash)
                .execute(pool)
                .await?
        }
    };
    Ok(result.rows_affected() > 0)
}
