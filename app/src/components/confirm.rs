//! Confirmation before consequential actions (#71): a native disclosure until the app has
//! hydrated (and without JavaScript), the browser's confirm dialog afterwards.

use leptos::html::Details;
use leptos::prelude::*;

use crate::i18n::use_i18n;
use crate::t;

/// `true` once the app has hydrated: `false` during SSR and during the hydration render,
/// `true` from the first effect run after it. Provided once at the root by
/// [`provide_hydrated`], so a component mounted later (refetch, client-side navigation)
/// starts in the hydrated state instead of repeating the server markup first.
#[derive(Clone, Copy)]
pub struct Hydrated(ReadSignal<bool>);

/// Provides [`Hydrated`]; called once in `App`. Effects never run on the server, and on
/// the client the first one runs after the synchronous hydration of the whole page, so
/// SSR and the hydration render both see `false`.
pub fn provide_hydrated() {
    let hydrated = RwSignal::new(false);
    provide_context(Hydrated(hydrated.read_only()));
    Effect::new(move |_| hydrated.set(true));
}

fn use_hydrated() -> Signal<bool> {
    use_context::<Hydrated>().map_or_else(|| Signal::stored(false), |Hydrated(h)| h.into())
}

/// Asks the browser's confirm dialog; `true` without JavaScript (never runs on the server).
fn confirmed(message: &str) -> bool {
    #[cfg(feature = "hydrate")]
    {
        window().confirm_with_message(message).unwrap_or(false)
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = message;
        true
    }
}

/// Whether the focused element is `details` or inside it.
#[cfg(feature = "hydrate")]
fn focus_within(details: &leptos::web_sys::HtmlDetailsElement) -> bool {
    document()
        .active_element()
        .is_some_and(|active| details.contains(Some(&active)))
}

/// Whether the server-rendered disclosure is open or focused: someone is using it, so
/// hydration must not swap it for the button (it would close it and drop the focus).
fn disclosure_in_use(disclosure: NodeRef<Details>) -> bool {
    #[cfg(feature = "hydrate")]
    {
        disclosure
            .get_untracked()
            .is_some_and(|details| details.open() || focus_within(&details))
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = disclosure;
        false
    }
}

/// A submit button for a consequential action that asks `confirm` first.
///
/// Server-rendered (before hydration, and without JavaScript) it is a `<details>`
/// disclosure: the summary shows `label`, opening it shows `confirm` and the actual submit
/// button. Once the app has hydrated it is a plain submit button behind the browser's
/// confirm dialog, so interactive users confirm once. A disclosure that is open or focused
/// when hydration finishes stays until it is closed or loses the focus. `aria_label` names
/// the controls for assistive technology when the visible label alone is ambiguous (one per
/// list row); the disclosure's submit button is named "Confirm: <that name>".
#[component]
pub fn SafeConfirmSubmit(
    #[prop(into)] label: String,
    #[prop(into)] confirm: String,
    class: &'static str,
    #[prop(optional, into)] aria_label: Option<String>,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] testid: Option<&'static str>,
) -> impl IntoView {
    let i18n = use_i18n();
    let hydrated = use_hydrated();
    let disclosure = NodeRef::<Details>::new();
    // Set when a disclosure kept at hydration is closed (focus elsewhere) or loses focus.
    let released = RwSignal::new(false);
    // Read only after hydration (the DOM check never runs on the server or while the
    // hydration render must match it); for a component mounted later the disclosure ref
    // is empty, so it renders the button straight away.
    let show_button =
        move || disabled || (hydrated.get() && (released.get() || !disclosure_in_use(disclosure)));

    let on_toggle = move |_: leptos::ev::Event| {
        #[cfg(feature = "hydrate")]
        {
            if let Some(details) = disclosure.get_untracked()
                && !details.open()
                && !focus_within(&details)
            {
                released.set(true);
            }
        }
    };
    let on_focusout = move |ev: leptos::ev::FocusEvent| {
        #[cfg(feature = "hydrate")]
        {
            use leptos::wasm_bindgen::JsCast;
            let next = ev.related_target();
            let stays_inside = disclosure.get_untracked().is_some_and(|details| {
                next.as_ref()
                    .and_then(|target| target.dyn_ref::<leptos::web_sys::Node>())
                    .is_some_and(|node| details.contains(Some(node)))
            });
            if !stays_inside {
                released.set(true);
            }
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = ev;
    };

    let name = aria_label.clone().unwrap_or_else(|| label.clone());
    let confirm_name = t!(i18n, "common.confirmAction", action = name);
    let dialog_text = confirm.clone();
    let on_click = move |ev: leptos::ev::MouseEvent| {
        if !confirmed(&dialog_text) {
            ev.prevent_default();
        }
    };
    let summary_label = label.clone();
    let summary_aria = aria_label.clone();
    view! {
        <Show
            when=show_button
            fallback=move || {
                view! {
                    <details node_ref=disclosure on:toggle=on_toggle on:focusout=on_focusout>
                        <summary class=class aria-label=summary_aria.clone() data-testid=testid>
                            {summary_label.clone()}
                        </summary>
                        <p class="mt-2 mb-2" style="white-space: pre-line">{confirm.clone()}</p>
                        <button type="submit" class=class aria-label=confirm_name.clone()>
                            {i18n.t("common.confirm")}
                        </button>
                    </details>
                }
            }
        >
            <button
                type="submit"
                class=class
                aria-label=aria_label.clone()
                data-testid=testid
                disabled=disabled
                on:click=on_click.clone()
            >
                {label.clone()}
            </button>
        </Show>
    }
}
