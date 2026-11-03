//! Invitation confirmation and the authorized group destination.
use crate::{
    AppError,
    auth::get_current_user,
    components::{ClientRedirect, RequireUser},
    groups_api::{JoinGroup, invitation_group, preview_invitation},
    i18n::use_i18n,
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

#[component]
pub fn InvitationPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    crate::server::respond::token_page_headers();
    let params = use_params_map();
    let code = Memo::new(move |_| params.with(|p| p.get("code").unwrap_or_default()));
    let preview = Resource::new_blocking(
        move || code.get(),
        |code| async move {
            if !matches!(get_current_user().await, Ok(Some(_))) {
                return Err(AppError::Unauthorized);
            }
            let result = preview_invitation(code).await;
            #[cfg(feature = "ssr")]
            if let Ok(preview) = &result
                && preview.already_member
            {
                crate::server::respond::redirect_page(&format!("/groups/{}", preview.group_id));
            }
            result
        },
    );
    let action = ServerAction::<JoinGroup>::new();
    let i18n = use_i18n();
    view! {
        <RequireUser>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    match preview.await {
                        Ok(preview) if preview.already_member => view! { <ClientRedirect path=format!("/groups/{}", preview.group_id)/> }.into_any(),
                        Ok(preview) => view! {
                            <h1>{i18n.t("group.invitation.title")}</h1>
                            <h2 class="h4">{preview.name}</h2>
                            {if preview.closed {
                                view! { <p role="status">{i18n.t("group.invitation.closed")}</p> }.into_any()
                            } else {
                                view! {
                                    <ActionForm action=action>
                                        <input type="hidden" name="code" value=move || code.get()/>
                                        <button class="btn btn-primary" type="submit" disabled=move || action.pending().get()>{i18n.t("group.invitation.join")}</button>
                                    </ActionForm>
                                    {move || action.value().get().and_then(Result::err).map(|e| view! { <p role="alert">{i18n.t(e.message_key())}</p> })}
                                }.into_any()
                            }}
                        }.into_any(),
                        Err(error) => view! { <p role="alert">{i18n.t(error.message_key())}</p> }.into_any(),
                    }
                })}
            </Suspense>
        </RequireUser>
    }
}

#[component]
pub fn GroupPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    let params = use_params_map();
    let group = Resource::new_blocking(
        move || params.with(|p| p.get("id")),
        |id| async move {
            if !matches!(get_current_user().await, Ok(Some(_))) {
                return Err(AppError::Unauthorized);
            }
            let id = id
                .and_then(|id| id.parse::<i32>().ok())
                .filter(|id| *id > 0)
                .ok_or(AppError::NotFound)?;
            invitation_group(id).await
        },
    );
    view! {
        <RequireUser>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    group.await.map(|group| view! { <h1>{group.name}</h1><p>{group.description}</p> })
                })}
            </Suspense>
        </RequireUser>
    }
}
