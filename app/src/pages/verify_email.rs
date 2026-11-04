//! `/user/verify-email/:token` (spec: account D5): the GET only renders a confirmation
//! page and changes nothing (mail scanners may prefetch links); the button calls
//! `confirm_email_change`. Open to everyone: the token proves control of the new mailbox.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::account_api::{AccountError, ConfirmEmailChange};
use crate::auth::use_session_resource;
use crate::i18n::use_i18n;
use crate::paths::PROFILE_PATH;

const LOGIN_PATH: &str = "/auth/login";

#[component]
pub fn VerifyEmailPage() -> impl IntoView {
    // The URL carries a secret: never sent to another origin as a referrer, never cached.
    #[cfg(feature = "ssr")]
    crate::server::respond::token_page_headers();
    let i18n = use_i18n();
    let params = use_params_map();
    let token = Memo::new(move |_| params.read().get("token").unwrap_or_default());
    let action = ServerAction::<ConfirmEmailChange>::new();
    let result = action.value();

    let form = move || {
        view! {
            <ActionForm action>
                <input type="hidden" name="token" value=token.get_untracked()/>
                <button type="submit" class="btn btn-primary">{i18n.t("user.verifyEmail.submit")}</button>
            </ActionForm>
        }
    };

    view! {
        <div class="row justify-content-center">
            <div class="col-md-8 col-lg-6">
                <div class="card">
                    <div class="card-body">
                        <h1 class="h3 card-title mb-3">{i18n.t("user.verifyEmail.title")}</h1>
                        {move || match result.get() {
                            None => {
                                view! {
                                    <p>{i18n.t("user.verifyEmail.description")}</p>
                                    {form()}
                                }
                                    .into_any()
                            }
                            Some(Ok(confirmed)) => {
                                view! {
                                    <div class="alert alert-success" role="status">
                                        {i18n.t("flash.success.email_verified")}
                                    </div>
                                    <NextLink logged_in=Some(confirmed.logged_in)/>
                                }
                                    .into_any()
                            }
                            Some(Err(err)) => {
                                // A spent or taken link stays that way; anything else may
                                // work on a second try.
                                let retry = !matches!(
                                    err,
                                    AccountError::InvalidOrExpiredToken | AccountError::EmailTaken
                                );
                                view! {
                                    <div class="alert alert-danger" role="alert">{err.text(&i18n)}</div>
                                    {retry.then(form)}
                                    <NextLink logged_in=None/>
                                }
                                    .into_any()
                            }
                        }}
                    </div>
                </div>
            </div>
        </div>
    }
}

/// "Go to profile" for a caller with a session, else "Go to login". `None`: look at the
/// layout's session resource.
#[component]
fn NextLink(logged_in: Option<bool>) -> impl IntoView {
    let i18n = use_i18n();
    let session = use_session_resource().0;
    let link = move |logged_in: bool| {
        if logged_in {
            view! { <a href=PROFILE_PATH>{i18n.t("user.verifyEmail.goToProfile")}</a> }.into_any()
        } else {
            view! { <a href=LOGIN_PATH>{i18n.t("user.verifyEmail.goToLogin")}</a> }.into_any()
        }
    };
    view! {
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                let logged_in = match logged_in {
                    Some(known) => known,
                    None => session.await.ok().and_then(|info| info.user).is_some(),
                };
                link(logged_in)
            })}
        </Transition>
    }
}
