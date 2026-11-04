//! The two account mails (spec: account, "Emails"): the verification link for a new address
//! and the "email changed" notice to the old one. Both are in the account language of the
//! user they concern, HTML with a plain-text alternative, every value HTML-escaped.

use crate::i18n::I18n;
use crate::server::auth_mail::{Block, render};
use crate::server::mail::Message;
use crate::t;

/// The verification mail, sent to the stored pending address `to`.
pub fn verify_email(i18n: &I18n, to: &str, name: &str, link: &str) -> Message {
    let subject = i18n.t("email.verifyEmail.subject");
    let blocks = [
        Block::Paragraph(t!(i18n, "email.verifyEmail.greeting", name = name)),
        Block::Paragraph(t!(i18n, "email.verifyEmail.body", email = to)),
        Block::Button(i18n.t("email.verifyEmail.button"), link.to_string()),
        Block::Fallback(i18n.t("email.verifyEmail.linkHint"), link.to_string()),
        Block::Paragraph(i18n.t("email.verifyEmail.expiry")),
        Block::Paragraph(i18n.t("email.verifyEmail.ignore")),
        Block::Paragraph(i18n.t("email.common.signature")),
    ];
    let (html, text) = render(i18n, &subject, &blocks);
    Message {
        to: to.to_string(),
        subject,
        html,
        text,
    }
}

/// The notice to the old address `to` that the account now uses `new_email`. No links.
pub fn email_changed(i18n: &I18n, to: &str, new_email: &str) -> Message {
    let subject = i18n.t("email.emailChanged.subject");
    let blocks = [
        Block::Paragraph(t!(i18n, "email.emailChanged.body", email = new_email)),
        Block::Paragraph(i18n.t("email.common.signature")),
    ];
    let (html, text) = render(i18n, &subject, &blocks);
    Message {
        to: to.to_string(),
        subject,
        html,
        text,
    }
}
