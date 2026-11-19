//! `/groups/archive`: the caller's archived groups, newest archive first (spec:
//! group-archive, UI notes). Read-only cards with a link to each group; no assignment data.
//!
//! Test markers: one `archived-group` per card, `archive-empty` for the empty text.

use leptos::prelude::*;

use super::{GroupBadges, GroupErrorView, group_path, when_logged_in};
use crate::components::RequireUser;
use crate::format::LocalDate;
use crate::groups_api::{ArchivedGroupSummary, GroupError, GroupStatus, list_archived_groups};
use crate::i18n::use_i18n;
use crate::paths::GROUPS_PATH;
use crate::{t, t_plural};

type ArchiveResource = Resource<Result<Vec<ArchivedGroupSummary>, GroupError>>;

#[component]
pub fn GroupArchivePage() -> impl IntoView {
    // The caller's memberships: no cache keeps the page.
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    let groups: ArchiveResource = Resource::new(|| (), |_| when_logged_in(list_archived_groups()));
    view! {
        <RequireUser>
            <ArchiveList groups/>
        </RequireUser>
    }
}

#[component]
fn ArchiveList(groups: ArchiveResource) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="d-flex flex-wrap justify-content-between align-items-center gap-2 mb-3">
            <h1 class="h2 mb-0">{i18n.t("group.archive.title")}</h1>
            <a href=GROUPS_PATH class="btn btn-outline-primary">{i18n.t("nav.groups")}</a>
        </div>
        <p class="text-muted mb-4">{i18n.t("group.archive.intro")}</p>
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                match groups.await {
                    Ok(list) if list.is_empty() => {
                        view! {
                            <div class="card">
                                <div class="card-body text-center py-5">
                                    <p class="lead mb-0" data-testid="archive-empty">
                                        {i18n.t("group.archive.empty")}
                                    </p>
                                </div>
                            </div>
                        }
                            .into_any()
                    }
                    Ok(list) => {
                        view! {
                            <div class="row">
                                {list
                                    .into_iter()
                                    .map(|group| view! { <ArchivedCard group/> })
                                    .collect_view()}
                            </div>
                        }
                            .into_any()
                    }
                    Err(error) => view! { <GroupErrorView error/> }.into_any(),
                }
            })}
        </Transition>
    }
}

#[component]
fn ArchivedCard(group: ArchivedGroupSummary) -> impl IntoView {
    let i18n = use_i18n();
    let count = group.member_count;
    view! {
        <div class="col-md-6 col-lg-4 mb-4">
            <div class="card h-100" data-testid="archived-group">
                <div class="card-body">
                    <div class="d-flex justify-content-between align-items-start gap-2 mb-2">
                        <h2 class="h5 card-title mb-0">{group.name}</h2>
                        <GroupBadges is_admin=group.is_admin status=GroupStatus::Archived/>
                    </div>
                    <p class="card-text small mb-1">
                        <span>{t_plural!(i18n, "group.memberCount", i64::from(count), count = count)}</span>
                    </p>
                    <p class="card-text small text-muted mb-1">
                        {i18n.t("group.adminLabel")} ": " <span>{group.admin_name}</span>
                    </p>
                    {group
                        .drawn_at
                        .map(|at| {
                            view! {
                                <p class="card-text small text-muted mb-1">
                                    {t!(i18n, "group.draw.drawnOn", date = LocalDate(at))}
                                </p>
                            }
                        })}
                    <p class="card-text small text-muted">
                        {t!(i18n, "group.archive.archivedOn", date = LocalDate(group.archived_at))}
                    </p>
                </div>
                <div class="card-footer bg-transparent">
                    <a href=group_path(group.id) class="btn btn-sm btn-outline-primary">
                        {i18n.t("group.viewDetails")}
                    </a>
                </div>
            </div>
        </div>
    }
}
