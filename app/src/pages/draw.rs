//! The draw UI (spec: draw, "UI notes"): the [`DrawPanel`] on the group page.
//!
//! Secrecy (S1–S4): the only receiver data on any page is the caller's own, fetched through
//! `get_my_assignment` inside the caller's own request. The admin's
//! part shows counts and the names of members whose mail failed, never a pairing.
//!
//! Perform, reset and resend are `ActionForm`s (plain form posts without JavaScript, which
//! the server answers with `302` back to the group page and a flash). With JavaScript the
//! page stays, reads the flash and re-reads its data; errors show as toasts. Perform and
//! reset ask for confirmation first.
//!
//! Test markers (`data-testid`) are the contract in `server/tests/it/common/draw.rs`.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::SafeConfirmSubmit;
use crate::draw_api::{
    DrawAdminStatus, DrawApiError, DrawBlocker, MyAssignment, PerformDraw, ResendDrawNotifications,
    ResetDraw,
};
#[cfg(not(feature = "ssr"))]
use crate::draw_api::{get_draw_admin_status, get_my_assignment};
use crate::format::LocalDate;
use crate::groups_api::{GroupDetail, GroupError};
use crate::i18n::use_i18n;
use crate::paths::wishlist_view_path;
use crate::{t, t_plural};

// --------------------------------------------------------------- group page

/// What the panel shows besides the group's own data: the caller's assignment (drawn
/// groups) and, for the admin, the draw status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawPanelData {
    pub mine: Option<MyAssignment>,
    pub admin: Option<DrawAdminStatus>,
}

pub type DrawResource = Resource<Result<DrawPanelData, DrawApiError>>;

/// The panel's data for the group in `group_id`. Waits for the group (the caller's role and
/// the draw state decide which fns run): members only ask for their own assignment, and
/// only the admin of an active group asks for the admin status (an archived group has no
/// admin section: group-archive).
pub fn draw_resource(
    group_id: Memo<Option<i32>>,
    group: Resource<Result<GroupDetail, GroupError>>,
) -> DrawResource {
    Resource::new(
        move || group_id.get(),
        move |id| async move {
            let Some(id) = id else {
                return Err(DrawApiError::NotFound);
            };
            let detail = group.await.map_err(|_| DrawApiError::NotFound)?;
            let mine = if detail.is_drawn {
                fetch_my_assignment(id).await?
            } else {
                None
            };
            let admin = if detail.is_admin && detail.archived_at.is_none() {
                Some(fetch_admin_status(id).await?)
            } else {
                None
            };
            Ok(DrawPanelData { mine, admin })
        },
    )
}

// During SSR the panel calls the fn bodies directly: a failure of the panel (e.g. the admin
// role was transferred a moment ago) leaves the panel out instead of setting the status of
// the whole group page. In the browser they are the server fns.

#[cfg(feature = "ssr")]
async fn fetch_my_assignment(group_id: i32) -> Result<Option<MyAssignment>, DrawApiError> {
    crate::server::draw_flow::get_my_assignment(group_id).await
}

#[cfg(not(feature = "ssr"))]
async fn fetch_my_assignment(group_id: i32) -> Result<Option<MyAssignment>, DrawApiError> {
    get_my_assignment(group_id).await
}

#[cfg(feature = "ssr")]
async fn fetch_admin_status(group_id: i32) -> Result<DrawAdminStatus, DrawApiError> {
    crate::server::draw_flow::get_draw_admin_status(group_id).await
}

#[cfg(not(feature = "ssr"))]
async fn fetch_admin_status(group_id: i32) -> Result<DrawAdminStatus, DrawApiError> {
    get_draw_admin_status(group_id).await
}

/// The panel's server actions and the assignment disclosure's state, created at page level
/// (a re-read of the panel rebuilds its cards and keeps these).
#[derive(Clone, Copy)]
pub struct DrawActions {
    pub perform: ServerAction<PerformDraw>,
    pub reset: ServerAction<ResetDraw>,
    pub resend: ServerAction<ResendDrawNotifications>,
    /// Whether the caller opened the assignment disclosure (dashboard spec). Starts closed,
    /// as the server renders it; perform and reset close it (another assignment).
    pub assignment_open: RwSignal<bool>,
}

impl DrawActions {
    pub fn new() -> Self {
        DrawActions {
            perform: ServerAction::new(),
            reset: ServerAction::new(),
            resend: ServerAction::new(),
            assignment_open: RwSignal::new(false),
        }
    }
}

impl Default for DrawActions {
    fn default() -> Self {
        Self::new()
    }
}

/// The draw card of the group page (hook defined by the groups spec). For an archived group
/// only the caller's own assignment card (if drawn), nothing else (group-archive).
#[component]
pub fn DrawPanel(
    group_id: i32,
    is_admin: bool,
    is_drawn: bool,
    archived: bool,
    data: Option<DrawPanelData>,
    actions: DrawActions,
) -> impl IntoView {
    let i18n = use_i18n();
    let (mine, admin) = match data {
        Some(data) => (data.mine, data.admin),
        None => (None, None),
    };
    if archived {
        return mine
            .filter(|_| is_drawn)
            .map(|mine| {
                view! {
                    <div class="card mb-4" data-testid="draw-panel">
                        <div class="card-body">
                            <h2 class="h5 card-title">{i18n.t("group.draw.title")}</h2>
                            <AssignmentCard group_id mine open=actions.assignment_open/>
                        </div>
                    </div>
                }
            })
            .into_any();
    }
    let body = if !is_drawn {
        match (is_admin, admin) {
            (true, Some(status)) => view! { <ReadySection group_id status actions/> }.into_any(),
            (true, None) => ().into_any(),
            (false, _) => view! {
                <p class="mb-0" data-testid="draw-pending">{i18n.t("group.draw.pending")}</p>
            }
            .into_any(),
        }
    } else {
        let open = actions.assignment_open;
        let card = mine.map(|mine| view! { <AssignmentCard group_id mine open/> });
        let admin_part = admin
            .filter(|_| is_admin)
            .and_then(|status| status.notifications)
            .map(|notes| view! { <DrawnAdminSection group_id notes actions/> });
        view! {
            {card}
            {admin_part}
        }
        .into_any()
    };
    view! {
        <div class="card mb-4" data-testid="draw-panel">
            <div class="card-body">
                <h2 class="h5 card-title">{i18n.t("group.draw.title")}</h2>
                {body}
            </div>
        </div>
    }
    .into_any()
}

/// The admin's undrawn view: who takes part, the small-group hint, the one "Perform draw"
/// button (disabled with the reason when blocked).
#[component]
fn ReadySection(group_id: i32, status: DrawAdminStatus, actions: DrawActions) -> impl IntoView {
    let i18n = use_i18n();
    let count = i64::from(status.member_count);
    let small = (2..=3).contains(&status.member_count);
    let blocked = status.blocker.is_some();
    let mut confirm = i18n.t("group.show.performDrawConfirm");
    if let Some(until) = status.registration_open_until {
        confirm.push_str("\n\n");
        confirm.push_str(&t!(
            i18n,
            "group.draw.confirmRegistrationOpen",
            date = LocalDate(until)
        ));
    }
    let blocker = status
        .blocker
        .map(|blocker| view! { <BlockerText blocker/> });
    view! {
        <p class="mb-1" data-testid="draw-ready">
            {t_plural!(i18n, "group.draw.ready", count, count = count)}
        </p>
        {small
            .then(|| {
                view! {
                    <p class="small text-muted" data-testid="draw-small-hint">
                        {i18n.t("group.draw.smallGroupHint")}
                    </p>
                }
            })}
        <ActionForm action=actions.perform>
            <input type="hidden" name="group_id" value=group_id/>
            <SafeConfirmSubmit
                label=i18n.t("group.show.performDraw")
                confirm=confirm
                class="btn btn-primary"
                testid="draw-perform"
                disabled=blocked
            />
        </ActionForm>
        {blocker}
    }
}

/// Why the draw cannot happen. Names no member and no rule (D10).
#[component]
fn BlockerText(blocker: DrawBlocker) -> impl IntoView {
    let i18n = use_i18n();
    let text = match blocker {
        DrawBlocker::TooFewMembers { count, min } => view! {
            <p class="mb-0">{t!(i18n, "group.draw.blocked.tooFewMembers", count = count, min = min)}</p>
        }
        .into_any(),
        DrawBlocker::TooManyMembers { max, .. } => view! {
            <p class="mb-0">{t!(i18n, "group.draw.blocked.tooManyMembers", max = max)}</p>
        }
        .into_any(),
        DrawBlocker::DrawImpossible => view! {
            <p class="mb-1">{i18n.t("exclusion.warning.drawImpossible")}</p>
            <p class="mb-0">{i18n.t("group.draw.impossible.hint")}</p>
        }
        .into_any(),
    };
    view! {
        <div class="alert alert-warning mt-3 mb-0" role="status" data-testid="draw-blocker">
            {text}
        </div>
    }
}

/// The caller's own assignment (the only one any page shows), behind a native disclosure
/// that starts closed (dashboard spec): it works without JavaScript and before hydration.
/// The summary's label says what a click does ("Reveal" / "Hide"); CSS on `details[open]`
/// picks one of the two spans, so no script is needed for that either. `open` (page level)
/// keeps the disclosure open when a re-read of the panel rebuilds the card.
#[component]
fn AssignmentCard(group_id: i32, mine: MyAssignment, open: RwSignal<bool>) -> impl IntoView {
    let i18n = use_i18n();
    let details = NodeRef::<leptos::html::Details>::new();
    // Opened before hydration (the server renders it closed): take the state over.
    Effect::new(move |_| {
        if details.get().is_some_and(|el| el.open()) {
            open.set(true);
        }
    });
    let on_toggle = move |_| {
        if let Some(el) = details.get_untracked() {
            open.set(el.open());
        }
    };
    view! {
        <div class="card border-success mb-3" data-testid="my-assignment">
            <div class="card-body">
                <h3 class="h5 card-title">{i18n.t("group.show.assignment")}</h3>
                <p class="mb-1">{i18n.t("group.show.assignmentDescription")}</p>
                <details
                    class="assignment-disclosure"
                    data-testid="assignment-disclosure"
                    node_ref=details
                    open=move || open.get()
                    on:toggle=on_toggle
                >
                    <summary class="btn btn-outline-success">
                        <span class="when-closed">{i18n.t("group.draw.revealAssignment")}</span>
                        <span class="when-open">{i18n.t("group.draw.hideAssignment")}</span>
                    </summary>
                    <p class="fs-3 fw-semibold mb-1 mt-3" data-testid="assignment-receiver">
                        {mine.receiver_name}
                    </p>
                    <p class="small text-muted" data-testid="assignment-drawn-on">
                        {t!(i18n, "group.draw.drawnOn", date = LocalDate(mine.drawn_at))}
                    </p>
                    // A document load (not a client-side route change), like the wishlist panel.
                    <a
                        href=wishlist_view_path(group_id, mine.receiver_id)
                        rel="external"
                        class="btn btn-success"
                        data-testid="assignment-wishlist"
                    >
                        {i18n.t("group.show.viewWishlist")}
                    </a>
                </details>
            </div>
        </div>
    }
}

/// The admin's drawn view: delivery counts, the resend button (only with failures) and the
/// reset button behind a confirm.
#[component]
fn DrawnAdminSection(
    group_id: i32,
    notes: crate::draw_api::NotificationSummary,
    actions: DrawActions,
) -> impl IntoView {
    let i18n = use_i18n();
    let total = i64::from(notes.total);
    let pending = i64::from(notes.pending);
    let summary = t_plural!(
        i18n,
        "group.draw.notifications.summary",
        total,
        sent = notes.sent,
        total = notes.total
    );
    let pending_line = (notes.pending > 0).then(|| {
        view! {
            <p class="mb-1" data-testid="notifications-pending">
                {t_plural!(i18n, "group.draw.notifications.pending", pending, count = pending)}
            </p>
        }
    });
    let failed = (notes.failed > 0).then(|| {
        let names = notes.failed_member_names.join(", ");
        view! {
            <p class="mb-2 text-danger" data-testid="notifications-failed">
                {t!(i18n, "group.draw.notifications.failed", names = names)}
            </p>
            <ActionForm action=actions.resend>
                <input type="hidden" name="group_id" value=group_id/>
                <button type="submit" class="btn btn-sm btn-outline-primary" data-testid="draw-resend">
                    {i18n.t("group.draw.notifications.resend")}
                </button>
            </ActionForm>
        }
    });
    view! {
        <div class="mb-3" data-testid="draw-notifications">
            <h3 class="h6">{i18n.t("group.draw.notifications.title")}</h3>
            <p class="mb-1" data-testid="notifications-summary">{summary}</p>
            {pending_line}
            {failed}
        </div>
        <ActionForm action=actions.reset>
            <input type="hidden" name="group_id" value=group_id/>
            <SafeConfirmSubmit
                label=i18n.t("group.draw.reset")
                confirm=i18n.t("group.draw.resetConfirm")
                class="btn btn-outline-danger"
                testid="draw-reset"
            />
        </ActionForm>
    }
}
