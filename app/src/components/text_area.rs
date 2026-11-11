//! A `<textarea>` with an initial value. Use it for every textarea.
//!
//! Leptos (tachys) renders the children of `<textarea>` without escaping (raw text, like
//! `<script>`), so a stored value such as `</textarea><script>…` would end the element and
//! inject markup. During SSR the value is therefore HTML-escaped here; the browser decodes
//! the entities (textarea content is escapable raw text), so the DOM text equals the stored
//! value. On the client (hydration and client-side rendering) the raw value is used, which
//! matches that DOM text. Without JavaScript the form is still pre-filled.

use leptos::prelude::*;

/// The initial content as it must be written into the element.
fn initial_content(value: String) -> String {
    #[cfg(feature = "ssr")]
    {
        // The parser drops one newline right after `<textarea>`; keep a leading one.
        let mut out = String::with_capacity(value.len() + 8);
        if value.starts_with('\n') {
            out.push('\n');
        }
        out.push_str(&crate::server::mail::escape_html(&value));
        out
    }
    #[cfg(not(feature = "ssr"))]
    {
        value
    }
}

#[component]
pub fn TextArea(
    #[prop(into)] id: String,
    name: &'static str,
    rows: &'static str,
    #[prop(into)] class: Signal<&'static str>,
    #[prop(optional, into)] maxlength: Option<String>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional, into)] aria_describedby: Option<String>,
    /// The initial text (shown as text, never as markup).
    #[prop(into)]
    value: String,
) -> impl IntoView {
    let content = initial_content(value);
    view! {
        <textarea
            id=id
            name=name
            rows=rows
            class=class
            maxlength=maxlength
            placeholder=placeholder
            aria-describedby=aria_describedby
        >
            {content}
        </textarea>
    }
}
