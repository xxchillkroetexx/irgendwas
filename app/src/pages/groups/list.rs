//! `/groups`: the caller's groups, newest first (spec: groups, UI notes "`/groups` list").

use leptos::prelude::*;

use super::{GroupErrorView, group_path, when_logged_in};
use crate::components::RequireUser;
use crate::groups_api::{GroupError, GroupSummary, list_my_groups};
use crate::i18n::use_i18n;
use crate::t_plural;

type ListResource = Resource<Result<Vec<GroupSummary>, GroupError>>;

#[component]
pub fn GroupsPage() -> impl IntoView {
    let groups: ListResource = Resource::new(|| (), |_| when_logged_in(list_my_groups()));
    view! {
        <RequireUser>
            <GroupList groups/>
        </RequireUser>
    }
}

#[component]
fn GroupList(groups: ListResource) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="d-flex flex-wrap justify-content-between align-items-center gap-2 mb-4">
            <h1 class="h2 mb-0">{i18n.t("group.title")}</h1>
            <div class="d-flex flex-wrap gap-2">
                <a href="/groups/new" class="btn btn-primary">{i18n.t("group.createButton")}</a>
                <a href="/join" class="btn btn-outline-primary">{i18n.t("group.joinButton")}</a>
            </div>
        </div>
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                match groups.await {
                    Ok(list) if list.is_empty() => {
                        view! {
                            <div class="card">
                                <div class="card-body text-center py-5">
                                    <p class="lead">{i18n.t("group.emptyMessage")}</p>
                                    <p class="text-muted mb-0">{i18n.t("group.emptyMessageAction")}</p>
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
                                    .map(|group| view! { <GroupCard group/> })
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
fn GroupCard(group: GroupSummary) -> impl IntoView {
    let i18n = use_i18n();
    let count = group.member_count;
    let description = match group.description {
        Some(text) => {
            view! { <p class="card-text" style="white-space: pre-line">{text}</p> }.into_any()
        }
        None => {
            view! { <p class="card-text text-muted fst-italic">{i18n.t("group.noDescription")}</p> }
                .into_any()
        }
    };
    view! {
        <div class="col-md-6 col-lg-4 mb-4">
            <div class="card h-100">
                <div class="card-body">
                    <div class="d-flex justify-content-between align-items-start gap-2 mb-2">
                        <h2 class="h5 card-title mb-0">{group.name}</h2>
                        <div class="d-flex flex-wrap gap-1">
                            {group
                                .is_admin
                                .then(|| view! { <span class="badge bg-primary">{i18n.t("group.admin")}</span> })}
                            <span class=group.status.badge_class()>{i18n.t(group.status.label_key())}</span>
                        </div>
                    </div>
                    {description}
                    <p class="card-text small mb-1">
                        <span>{t_plural!(i18n, "group.memberCount", i64::from(count), count = count)}</span>
                    </p>
                    <p class="card-text small text-muted">
                        {i18n.t("group.adminLabel")} ": " <span>{group.admin_name}</span>
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
