//! Renders flash messages from the session plus client-side toasts.

use std::time::Duration;

use leptos::prelude::*;

use crate::auth::SessionResource;
use crate::flash::{FlashMessage, Toasts};
use crate::i18n::use_i18n;

const AUTO_DISMISS_AFTER: Duration = Duration::from_secs(5);

#[component]
pub fn FlashMessages() -> impl IntoView {
    let SessionResource(session) = expect_context::<SessionResource>();
    let toasts = expect_context::<Toasts>();

    view! {
        <div class="flash-messages" aria-live="polite">
            <Transition fallback=|| ()>
                {move || Suspend::new(async move {
                    let messages = session.await.map(|info| info.flash).unwrap_or_default();
                    messages
                        .into_iter()
                        .map(|message| view! { <FlashAlert message/> })
                        .collect_view()
                })}
            </Transition>
            <For
                each=move || toasts.current().into_iter().enumerate()
                key=|(index, message)| (*index, message.clone())
                children=|(_, message)| view! { <FlashAlert message/> }
            />
        </div>
    }
}

#[component]
pub fn FlashAlert(message: FlashMessage) -> impl IntoView {
    let i18n = use_i18n();
    let visible = RwSignal::new(true);
    let text = message.text(&i18n);
    let class = format!("alert {} alert-dismissible", message.kind.alert_class());
    let close_label = i18n.t("common.close");

    if message.kind.auto_dismiss() {
        // Effects only run in the browser, after hydration.
        Effect::new(move |_| {
            set_timeout(
                move || {
                    visible.try_set(false);
                },
                AUTO_DISMISS_AFTER,
            );
        });
    }

    view! {
        <Show when=move || visible.get()>
            <div class=class.clone() role="alert">
                {text.clone()}
                <button
                    type="button"
                    class="btn-close"
                    aria-label=close_label.clone()
                    on:click=move |_| visible.set(false)
                ></button>
            </div>
        </Show>
    }
}
