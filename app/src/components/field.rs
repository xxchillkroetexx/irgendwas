//! Form field state shared by the group and wishlist forms: the Bootstrap class, the
//! `aria-invalid` attribute and the `invalid-feedback` element of a field (#71).

use leptos::prelude::*;

use crate::groups_api::{GroupError, GroupField};
use crate::i18n::{I18n, use_i18n};
use crate::wishlists_api::{WishlistError, WishlistField};

/// A form error that can name a problem with one of its fields.
pub trait FieldErrors: Clone + Send + Sync + 'static {
    type Field: Copy + Send + Sync + 'static;

    /// Whether this error reports a problem with `field`.
    fn has_field_error(&self, field: Self::Field) -> bool;

    /// The text of the problem with `field`, if any.
    fn field_error_text(&self, field: Self::Field, i18n: &I18n) -> Option<String>;
}

impl FieldErrors for GroupError {
    type Field = GroupField;

    fn has_field_error(&self, field: GroupField) -> bool {
        self.field_error(field).is_some()
    }

    fn field_error_text(&self, field: GroupField, i18n: &I18n) -> Option<String> {
        self.field_error(field).map(|problem| problem.text(i18n))
    }
}

impl FieldErrors for WishlistError {
    type Field = WishlistField;

    fn has_field_error(&self, field: WishlistField) -> bool {
        self.field_error(field).is_some()
    }

    fn field_error_text(&self, field: WishlistField, i18n: &I18n) -> Option<String> {
        self.field_error(field).map(|problem| problem.text(i18n))
    }
}

/// The value of `aria-invalid`: `"true"` for an invalid field, else no attribute at all
/// (absent means valid).
pub fn invalid_value(invalid: bool) -> Option<&'static str> {
    invalid.then_some("true")
}

/// Whether `error` reports a problem with `field`.
pub fn field_invalid<E: FieldErrors>(error: Signal<Option<E>>, field: E::Field) -> bool {
    error.with(|e| e.as_ref().is_some_and(|e| e.has_field_error(field)))
}

/// The class of the control for `field`: `form-control`, plus `is-invalid` on an error.
pub fn control_class<E: FieldErrors>(
    error: Signal<Option<E>>,
    field: E::Field,
) -> impl Fn() -> &'static str + Send + Sync + Copy + 'static {
    move || {
        if field_invalid(error, field) {
            "form-control is-invalid"
        } else {
            "form-control"
        }
    }
}

/// The `aria-invalid` attribute of the control for `field` (see [`invalid_value`]).
pub fn invalid_attr<E: FieldErrors>(
    error: Signal<Option<E>>,
    field: E::Field,
) -> impl Fn() -> Option<&'static str> + Send + Sync + Copy + 'static {
    move || invalid_value(field_invalid(error, field))
}

/// The `invalid-feedback` of `field`. Always rendered (empty while the field is valid) so
/// the control's `aria-describedby` can name its `id`.
#[component]
pub fn FieldFeedback<E: FieldErrors>(
    field: E::Field,
    error: Signal<Option<E>>,
    #[prop(into)] id: String,
) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div id=id class="invalid-feedback" role="alert">
            {move || error.with(|e| e.as_ref().and_then(|e| e.field_error_text(field, &i18n)))}
        </div>
    }
}
