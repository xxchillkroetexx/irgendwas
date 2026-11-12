//! Form pieces shared by the auth pages: labelled inputs with inline validation feedback,
//! the page-level error alert, and the "after a failed submit" password reset.

use leptos::html::Input;
use leptos::prelude::*;

use crate::auth_api::{AuthError, Field};
use crate::i18n::use_i18n;

/// The error of the last submission, if it failed.
pub fn last_error<T, V>(value: V) -> Signal<Option<AuthError>>
where
    T: Send + Sync + 'static,
    V: With<Value = Option<Result<T, AuthError>>> + Copy + Send + Sync + 'static,
{
    Signal::derive(move || value.with(|v| v.as_ref().and_then(|r| r.as_ref().err().cloned())))
}

/// Empties the given inputs after every failed submission (passwords are never kept).
pub fn clear_on_error<T, V>(value: V, inputs: Vec<NodeRef<Input>>)
where
    T: Send + Sync + 'static,
    V: With<Value = Option<Result<T, AuthError>>> + Copy + 'static,
{
    Effect::new(move |_| {
        let failed = value.with(|v| matches!(v, Some(Err(_))));
        if failed {
            for input in &inputs {
                if let Some(el) = input.get_untracked() {
                    el.set_value("");
                }
            }
        }
    });
}

/// Page-level alert for every error that does not belong to a field.
#[component]
pub fn ErrorAlert(error: Signal<Option<AuthError>>) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        error
            .get()
            .filter(|e| !matches!(e, AuthError::Validation(_)))
            .map(|e| {
                view! {
                    <div class="alert alert-danger" role="alert">
                        {e.text(&i18n)}
                    </div>
                }
            })
    }
}

/// A labelled input whose `invalid-feedback` shows the validation error of `field`.
#[component]
pub fn AuthInput(
    id: &'static str,
    name: &'static str,
    #[prop(into)] label: String,
    kind: &'static str,
    autocomplete: &'static str,
    field: Field,
    error: Signal<Option<AuthError>>,
    #[prop(optional)] node_ref: NodeRef<Input>,
    #[prop(optional, into)] help: Option<String>,
) -> impl IntoView {
    let i18n = use_i18n();
    let field_error = move || error.get().and_then(|e| e.field_error(field));
    let help_id = format!("{id}-help");
    let error_id = format!("{id}-error");
    let described_by = if help.is_some() {
        format!("{help_id} {error_id}")
    } else {
        error_id.clone()
    };
    view! {
        <div class="mb-3">
            <label for=id class="form-label">{label}</label>
            <input
                type=kind
                id=id
                name=name
                autocomplete=autocomplete
                aria-describedby=described_by
                aria-invalid=move || if field_error().is_some() { "true" } else { "false" }
                class=move || {
                    if field_error().is_some() { "form-control is-invalid" } else { "form-control" }
                }
                node_ref=node_ref
                required
            />
            <div id=error_id class="invalid-feedback" role="alert">
                {move || field_error().map(|e| e.text(&i18n))}
            </div>
            {help.map(|help| view! { <div id=help_id.clone() class="form-text">{help}</div> })}
        </div>
    }
}
