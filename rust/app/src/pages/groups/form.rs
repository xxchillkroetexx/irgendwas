//! `/groups/new` and `/groups/:id/edit` (spec: groups, UI notes): the same fields, with
//! inline field errors. Dates are `<input type="date">` values in the page's time zone; an
//! empty input means "none".

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{GroupErrorView, group_path, route_group_id, when_logged_in};
use crate::components::RequireUser;
use crate::groups_api::{CreateGroup, GroupError, GroupField, UpdateGroup, get_group};
use crate::i18n::use_i18n;
use crate::tz;

/// What the form starts with (as `<input>` values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FormValues {
    name: String,
    description: String,
    registration_deadline: String,
    draw_date: String,
}

/// The error of the last submission, if it failed.
fn last_error<T>(action: ServerAction<T>) -> Signal<Option<GroupError>>
where
    T: leptos::server_fn::ServerFn<Error = GroupError> + Clone + Send + Sync + 'static,
    T::Output: Send + Sync + 'static,
{
    let value = action.value();
    Signal::derive(move || value.with(|v| v.as_ref().and_then(|r| r.as_ref().err().cloned())))
}

#[component]
pub fn GroupCreatePage() -> impl IntoView {
    view! {
        <RequireUser>
            <CreateForm/>
        </RequireUser>
    }
}

#[component]
fn CreateForm() -> impl IntoView {
    let i18n = use_i18n();
    let action = ServerAction::<CreateGroup>::new();
    let error = last_error(action);
    view! {
        <div class="row justify-content-center">
            <div class="col-lg-8">
                <h1 class="h2 mb-4">{i18n.t("group.create.title")}</h1>
                <div class="card">
                    <div class="card-body">
                        <ActionForm action>
                            <GroupFields values=FormValues::default() error/>
                            <div class="d-flex gap-2">
                                <button type="submit" class="btn btn-primary">
                                    {i18n.t("group.form.createButton")}
                                </button>
                                <a href="/groups" class="btn btn-outline-secondary">
                                    {i18n.t("group.form.cancel")}
                                </a>
                            </div>
                        </ActionForm>
                    </div>
                </div>
            </div>
        </div>
    }
}

/// The stored values the edit form starts with (admin only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EditValues {
    id: i32,
    name: String,
    description: Option<String>,
    registration_deadline: Option<DateTime<Utc>>,
    draw_date: Option<DateTime<Utc>>,
}

type EditResource = Resource<Result<EditValues, GroupError>>;

#[component]
pub fn GroupEditPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    crate::server::respond::no_store();
    let id = route_group_id();
    // Only what the form needs goes into the page; a member gets `Forbidden` (403 page).
    let group: EditResource = Resource::new(
        move || id.get(),
        |id| async move {
            let Some(id) = id else {
                return Err(GroupError::NotFound);
            };
            when_logged_in(async move {
                let detail = get_group(id).await?;
                if !detail.is_admin {
                    return Err(GroupError::Forbidden);
                }
                Ok(EditValues {
                    id: detail.id,
                    name: detail.name,
                    description: detail.description,
                    registration_deadline: detail.registration_deadline,
                    draw_date: detail.draw_date,
                })
            })
            .await
        },
    );
    view! {
        <RequireUser>
            <Transition fallback=|| ()>
                {move || Suspend::new(async move {
                    match group.await {
                        Ok(values) => view! { <EditForm values/> }.into_any(),
                        Err(error) => view! { <GroupErrorView error/> }.into_any(),
                    }
                })}
            </Transition>
        </RequireUser>
    }
}

#[component]
fn EditForm(values: EditValues) -> impl IntoView {
    let i18n = use_i18n();
    let action = ServerAction::<UpdateGroup>::new();
    let error = last_error(action);
    let zone = i18n.tz();
    let id = values.id;
    let form_values = FormValues {
        name: values.name,
        description: values.description.unwrap_or_default(),
        registration_deadline: values
            .registration_deadline
            .map(|d| tz::to_date_input_value(d, zone))
            .unwrap_or_default(),
        draw_date: values
            .draw_date
            .map(|d| tz::to_date_input_value(d, zone))
            .unwrap_or_default(),
    };
    view! {
        <div class="row justify-content-center">
            <div class="col-lg-8">
                <h1 class="h2 mb-4">{i18n.t("group.edit.title")}</h1>
                <div class="card">
                    <div class="card-body">
                        <ActionForm action>
                            <input type="hidden" name="group_id" value=id/>
                            <GroupFields values=form_values error/>
                            <div class="d-flex gap-2">
                                <button type="submit" class="btn btn-primary">
                                    {i18n.t("group.form.updateButton")}
                                </button>
                                <a href=group_path(id) class="btn btn-outline-secondary">
                                    {i18n.t("group.form.cancel")}
                                </a>
                            </div>
                        </ActionForm>
                    </div>
                </div>
            </div>
        </div>
    }
}

/// Alert for an error that belongs to no field (e.g. the group is gone).
#[component]
fn FormAlert(error: Signal<Option<GroupError>>) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        error
            .get()
            .filter(|e| !matches!(e, GroupError::Validation(_)))
            .map(|e| view! { <div class="alert alert-danger" role="alert">{e.text(&i18n)}</div> })
    }
}

/// The `invalid-feedback` of `field`.
#[component]
fn FieldFeedback(field: GroupField, error: Signal<Option<GroupError>>) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        error
            .get()
            .and_then(|e| e.field_error(field))
            .map(|problem| view! { <div class="invalid-feedback">{problem.text(&i18n)}</div> })
    }
}

fn control_class(
    error: Signal<Option<GroupError>>,
    field: GroupField,
) -> impl Fn() -> &'static str + Send + Sync + Copy + 'static {
    move || {
        if error.get().and_then(|e| e.field_error(field)).is_some() {
            "form-control is-invalid"
        } else {
            "form-control"
        }
    }
}

#[component]
fn GroupFields(values: FormValues, error: Signal<Option<GroupError>>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <FormAlert error/>
        <div class="mb-3">
            <label for="group-name" class="form-label">{i18n.t("group.form.name")}</label>
            <input
                type="text"
                id="group-name"
                name="name"
                class=control_class(error, GroupField::Name)
                aria-describedby="group-name-help"
                value=values.name
            />
            <FieldFeedback field=GroupField::Name error/>
            <div id="group-name-help" class="form-text">{i18n.t("group.form.nameHelp")}</div>
        </div>
        <div class="mb-3">
            <label for="group-description" class="form-label">{i18n.t("group.form.description")}</label>
            <textarea
                id="group-description"
                name="description"
                rows="3"
                class=control_class(error, GroupField::Description)
                aria-describedby="group-description-help"
            >
                {values.description}
            </textarea>
            <FieldFeedback field=GroupField::Description error/>
            <div id="group-description-help" class="form-text">
                {i18n.t("group.form.descriptionHelp")}
            </div>
        </div>
        <div class="row">
            <div class="col-md-6 mb-3">
                <label for="group-deadline" class="form-label">
                    {i18n.t("group.form.registrationDeadline")}
                </label>
                <input
                    type="date"
                    id="group-deadline"
                    name="registration_deadline"
                    class=control_class(error, GroupField::RegistrationDeadline)
                    aria-describedby="group-deadline-help"
                    value=values.registration_deadline
                />
                <FieldFeedback field=GroupField::RegistrationDeadline error/>
                <div id="group-deadline-help" class="form-text">
                    {i18n.t("group.form.registrationDeadlineHelp")}
                </div>
            </div>
            <div class="col-md-6 mb-3">
                <label for="group-draw-date" class="form-label">{i18n.t("group.form.drawDate")}</label>
                <input
                    type="date"
                    id="group-draw-date"
                    name="draw_date"
                    class=control_class(error, GroupField::DrawDate)
                    aria-describedby="group-draw-date-help"
                    value=values.draw_date
                />
                <FieldFeedback field=GroupField::DrawDate error/>
                <div id="group-draw-date-help" class="form-text">{i18n.t("group.form.drawDateHelp")}</div>
            </div>
        </div>
    }
}
