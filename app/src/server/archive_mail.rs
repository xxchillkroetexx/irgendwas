//! The archive warning mail (spec: group-archive A7): to the group's admin, 14 days before
//! the automatic archive. In the admin's account language, HTML with a plain-text
//! alternative, every value HTML-escaped, the date a local date in `APP_TIMEZONE`, the link
//! from `APP_URL`. It names no member and no assignment.

use chrono::{DateTime, Utc};

use crate::format::LocalDate;
use crate::i18n::I18n;
use crate::paths::group_path;
use crate::server::Config;
use crate::server::auth_mail::{Block, app_link, render};
use crate::server::mail::Message;
use crate::t;

/// "`group_name` will be archived on `auto_archive_at`" for `admin_name` at `to` (the stored
/// address).
pub fn archive_warning(
    i18n: &I18n,
    config: &Config,
    to: &str,
    admin_name: &str,
    group_id: i32,
    group_name: &str,
    auto_archive_at: DateTime<Utc>,
) -> Message {
    let date = LocalDate(auto_archive_at);
    let subject = t!(
        i18n,
        "email.archiveWarning.subject",
        group = group_name,
        date = date
    );
    let blocks = [
        Block::Paragraph(t!(i18n, "email.archiveWarning.greeting", name = admin_name)),
        Block::Paragraph(t!(
            i18n,
            "email.archiveWarning.body",
            group = group_name,
            date = date
        )),
        Block::Paragraph(i18n.t("email.archiveWarning.keep")),
        Block::Paragraph(i18n.t("email.archiveWarning.nothing")),
        Block::Button(
            i18n.t("email.archiveWarning.button"),
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
