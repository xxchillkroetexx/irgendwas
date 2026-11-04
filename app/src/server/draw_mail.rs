//! The two draw mails (spec: draw, "Emails"): the draw result to each giver and the reset
//! notice to each former giver. Each is in its recipient's account language (D12), HTML
//! with a plain-text alternative, every value HTML-escaped, links from `APP_URL`.
//!
//! A draw-result mail names only its own giver's receiver; the reset mail names nobody's
//! receiver (D7).

use crate::i18n::I18n;
use crate::paths::group_path;
use crate::server::Config;
use crate::server::auth_mail::{Block, app_link, render};
use crate::server::mail::Message;
use crate::t;

/// The draw result for `giver_name` at `to` (the stored address): who they give to.
pub fn draw_result(
    i18n: &I18n,
    config: &Config,
    to: &str,
    giver_name: &str,
    receiver_name: &str,
    group_id: i32,
    group_name: &str,
) -> Message {
    let subject = t!(i18n, "email.drawResult.subject", group = group_name);
    let blocks = [
        Block::Paragraph(t!(i18n, "email.drawResult.greeting", name = giver_name)),
        Block::Paragraph(t!(
            i18n,
            "email.drawResult.body",
            group = group_name,
            receiver = receiver_name
        )),
        Block::Button(
            i18n.t("email.drawResult.button"),
            app_link(config, &group_path(group_id)),
        ),
        Block::Paragraph(i18n.t("email.drawResult.secret")),
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

/// The notice that `admin_name` reset the draw of `group_name`; no receiver names.
pub fn draw_reset(
    i18n: &I18n,
    config: &Config,
    to: &str,
    name: &str,
    admin_name: &str,
    group_id: i32,
    group_name: &str,
) -> Message {
    let subject = t!(i18n, "email.drawReset.subject", group = group_name);
    let blocks = [
        Block::Paragraph(t!(i18n, "email.drawReset.greeting", name = name)),
        Block::Paragraph(t!(
            i18n,
            "email.drawReset.body",
            group = group_name,
            admin = admin_name
        )),
        Block::Paragraph(i18n.t("email.drawReset.next")),
        // The link to the group, labelled with its name.
        Block::Button(
            group_name.to_string(),
            app_link(config, &group_path(group_id)),
        ),
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
