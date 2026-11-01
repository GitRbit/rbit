//! Runner browser documents and typed forms use the Actions activation generation.

use std::time::Duration;

use axum::extract::{Path, State};
use rbit_services::{
    ActionsRouteBundle, ServiceError,
    actions::runners::ManagedScope,
    plugin_pages::{
        FormActions, FormField, FormFieldKind, FormReference, FormSchema, FormValues, MutationAudience, MutationContext,
        MutationDescriptor, MutationError, MutationOutcome, PageAudience, PageContext, PageDescriptor, PageError, PageScope,
        PluginRedirect, compiled_mutation, compiled_request_page,
    },
};

use crate::{
    AppState,
    extract::{SignedIn, SiteAdmin, Viewer},
    routes::actions::runners::{self, RunnerForm, TokenForm},
    state::WeakAppState,
    views::layout::Page,
};

pub(super) fn bundle(weak: WeakAppState) -> Result<ActionsRouteBundle, ServiceError> {
    let mut bundle = ActionsRouteBundle::default();
    repository(&mut bundle, weak.clone())?;
    personal(&mut bundle, weak.clone())?;
    organization(&mut bundle, weak.clone())?;
    instance(&mut bundle, weak)?;
    Ok(bundle)
}

fn page_descriptor(id: &str, scope: PageScope, path: &str, audience: PageAudience, runner: &str) -> Result<PageDescriptor, ServiceError> {
    PageDescriptor::new(id, scope, path, audience, Duration::from_secs(1))
        .and_then(|descriptor| descriptor.with_forms(vec![FormReference::new("tokens", path)?, FormReference::new("runner", runner)?]))
        .map(PageDescriptor::with_document)
        .map_err(super::declaration)
}

fn mutation_descriptor(
    id: &str,
    scope: PageScope,
    path: &str,
    audience: MutationAudience,
    token: bool,
    owner: bool,
) -> Result<MutationDescriptor, ServiceError> {
    let mut fields = vec![text("action", 8, true)];
    if token {
        fields.extend([
            FormField { name: "lifetime".into(), kind: FormFieldKind::Integer { min: 1, max: 2_592_000 }, required: false },
            text("labels", 4096, false),
            FormField { name: "reusable".into(), kind: FormFieldKind::Boolean, required: false },
            FormField { name: "token".into(), kind: FormFieldKind::Integer { min: 1, max: i64::MAX }, required: false },
        ]);
    } else {
        fields.push(FormField { name: "runner".into(), kind: FormFieldKind::Integer { min: 1, max: i64::MAX }, required: true });
    }
    if owner {
        fields.push(text("owner", 64, true));
    }
    let schema = FormSchema::new(fields).map_err(super::declaration)?;
    MutationDescriptor::new(id, scope, path, audience, Duration::from_secs(1), schema)
        .map(MutationDescriptor::with_document)
        .map_err(super::declaration)
}

fn text(name: &str, max_bytes: usize, required: bool) -> FormField {
    FormField { name: name.into(), kind: FormFieldKind::Text { max_bytes, multiline: false }, required }
}

fn token_form(values: &FormValues) -> TokenForm {
    TokenForm {
        action: values.text("action").unwrap_or_default().to_owned(),
        lifetime: values.integer("lifetime").and_then(|value| u64::try_from(value).ok()),
        labels: values.text("labels").unwrap_or_default().to_owned(),
        reusable: values.boolean("reusable").filter(|value| *value).map(|_| "true".to_owned()),
        token: values.integer("token"),
    }
}

fn runner_form(values: &FormValues) -> Result<(String, RunnerForm), MutationError> {
    Ok((
        values.integer("runner").ok_or(MutationError::InvalidForm)?.to_string(),
        RunnerForm { action: values.text("action").ok_or(MutationError::InvalidForm)?.to_owned() },
    ))
}

async fn global_page(
    weak: &WeakAppState,
    context: &PageContext,
    forms: FormActions,
    path: &str,
) -> Result<(AppState, rbit_core::User, Page), PageError> {
    let state = weak.upgrade().ok_or(PageError::Unavailable)?;
    let viewer = super::principal(&state, context).await?;
    let user = viewer.user().ok_or(PageError::Forbidden)?.clone();
    let page = super::page(&state, &viewer, context.locale.clone(), forms, &super::global_url(path));
    Ok((state, user, page))
}

async fn global_mutation(
    weak: &WeakAppState,
    context: &MutationContext,
    path: &str,
) -> Result<(AppState, rbit_core::User, Page), MutationError> {
    let state = weak.upgrade().ok_or(MutationError::Unavailable)?;
    let user = super::actor(&state, context.actor.account_id).await.map_err(|_| MutationError::Forbidden)?;
    let viewer = Viewer(Some(std::sync::Arc::new(user.clone())));
    let page = super::page(&state, &viewer, context.locale.clone(), context.forms().clone(), &super::global_url(path));
    Ok((state, user, page))
}

fn repository(bundle: &mut ActionsRouteBundle, weak: WeakAppState) -> Result<(), ServiceError> {
    let descriptor =
        page_descriptor("repo-runners", PageScope::Repository, "settings/runners", PageAudience::RepositoryAdmin, "settings/runner")?;
    let page_weak = weak.clone();
    bundle.pages.push(compiled_request_page(descriptor, move |context, forms, _| {
        let weak = page_weak.clone();
        async move {
            let (state, viewer, page, owner, repo) = super::request(&weak, &context, forms).await?;
            let user = viewer.user().ok_or(PageError::Forbidden)?.clone();
            let view = runners::repository(State(state), SignedIn(std::sync::Arc::new(user)), viewer, page, Path((owner, repo)))
                .await
                .map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    }));
    let descriptor = mutation_descriptor(
        "repo-runner-tokens",
        PageScope::Repository,
        "settings/runners",
        MutationAudience::RepositoryAdmin,
        true,
        false,
    )?;
    let token_weak = weak.clone();
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = token_weak.clone();
        async move {
            let (state, user, viewer, page, owner, repo) = super::mutation(&weak, &context).await?;
            let loaded = runners::load_admin(&state, &viewer, &owner, &repo).await.map_err(super::mutation_error)?;
            let outcome = runners::apply(&state, &user, ManagedScope::Repository(&loaded.ctx), &token_form(&values))
                .await
                .map_err(super::mutation_error)?;
            let invalid = outcome.error.0.is_some();
            let view = runners::repo_page(&state, &user, &loaded, page, outcome).await.map_err(super::mutation_error)?;
            let document = super::document(view, context.locale)?;
            Ok(if invalid { MutationOutcome::Invalid(document) } else { MutationOutcome::Success(document) })
        }
    }));
    let descriptor =
        mutation_descriptor("repo-runner", PageScope::Repository, "settings/runner", MutationAudience::RepositoryAdmin, false, false)?;
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = weak.clone();
        async move {
            let (state, user, viewer, _, owner, repo) = super::mutation(&weak, &context).await?;
            let loaded = runners::load_admin(&state, &viewer, &owner, &repo).await.map_err(super::mutation_error)?;
            let (runner, form) = runner_form(&values)?;
            runners::apply_runner(&state, &user, ManagedScope::Repository(&loaded.ctx), &runner, &form)
                .await
                .map_err(super::mutation_error)?;
            Ok(MutationOutcome::<crate::views::actions_runners::RepoRunnersPage>::Redirect(PluginRedirect::new("settings/runners")?))
        }
    }));
    Ok(())
}

fn personal(bundle: &mut ActionsRouteBundle, weak: WeakAppState) -> Result<(), ServiceError> {
    let descriptor = page_descriptor("personal-runners", PageScope::Global, "settings/runners", PageAudience::SignedIn, "settings/runner")?;
    let page_weak = weak.clone();
    bundle.pages.push(compiled_request_page(descriptor, move |context, forms, _| {
        let weak = page_weak.clone();
        async move {
            let (state, user, page) = global_page(&weak, &context, forms, "settings/runners").await?;
            let view = runners::personal(State(state), SignedIn(std::sync::Arc::new(user)), page).await.map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    }));
    let descriptor =
        mutation_descriptor("personal-runner-tokens", PageScope::Global, "settings/runners", MutationAudience::SignedIn, true, false)?;
    let token_weak = weak.clone();
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = token_weak.clone();
        async move {
            let (state, user, page) = global_mutation(&weak, &context, "settings/runners").await?;
            let outcome = runners::apply(&state, &user, ManagedScope::Owner(&user.account), &token_form(&values))
                .await
                .map_err(super::mutation_error)?;
            let invalid = outcome.error.0.is_some();
            let view = runners::user_page(&state, &user, page, outcome).await.map_err(super::mutation_error)?;
            let document = super::document(view, context.locale)?;
            Ok(if invalid { MutationOutcome::Invalid(document) } else { MutationOutcome::Success(document) })
        }
    }));
    let descriptor =
        mutation_descriptor("personal-runner", PageScope::Global, "settings/runner", MutationAudience::SignedIn, false, false)?;
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = weak.clone();
        async move {
            let (state, user, _) = global_mutation(&weak, &context, "settings/runners").await?;
            let (runner, form) = runner_form(&values)?;
            runners::apply_runner(&state, &user, ManagedScope::Owner(&user.account), &runner, &form)
                .await
                .map_err(super::mutation_error)?;
            Ok(MutationOutcome::<crate::views::actions_runners::UserRunnersPage>::Redirect(PluginRedirect::new("settings/runners")?))
        }
    }));
    Ok(())
}

fn organization(bundle: &mut ActionsRouteBundle, weak: WeakAppState) -> Result<(), ServiceError> {
    let descriptor = page_descriptor("org-runners", PageScope::Global, "org/runners", PageAudience::SignedIn, "org/runner")?
        .with_query(&["owner"])
        .map_err(super::declaration)?;
    let page_weak = weak.clone();
    bundle.pages.push(compiled_request_page(descriptor, move |context, forms, query| {
        let weak = page_weak.clone();
        async move {
            let (state, user, page) = global_page(&weak, &context, forms, "org/runners").await?;
            let owner = query.get("owner").ok_or(PageError::Invalid("organization"))?.to_owned();
            let view = runners::organization(State(state), SignedIn(std::sync::Arc::new(user)), page, Path(owner))
                .await
                .map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    }));
    let descriptor = mutation_descriptor("org-runner-tokens", PageScope::Global, "org/runners", MutationAudience::SignedIn, true, true)?;
    let token_weak = weak.clone();
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = token_weak.clone();
        async move {
            let (state, user, mut page) = global_mutation(&weak, &context, "org/runners").await?;
            let owner = values.text("owner").ok_or(MutationError::InvalidForm)?;
            let org = state.services.orgs.open(Some(&user), owner).await.map_err(|error| super::mutation_error(error.into()))?;
            if !org.can_manage {
                return Err(MutationError::Forbidden);
            }
            page.plugin_fields = vec![("owner".into(), owner.to_owned())];
            let outcome = runners::apply(&state, &user, ManagedScope::Owner(&org.org.account), &token_form(&values))
                .await
                .map_err(super::mutation_error)?;
            let invalid = outcome.error.0.is_some();
            let view = runners::org_page(&state, &user, owner, page, outcome).await.map_err(super::mutation_error)?;
            let document = super::document(view, context.locale)?;
            Ok(if invalid { MutationOutcome::Invalid(document) } else { MutationOutcome::Success(document) })
        }
    }));
    let descriptor = mutation_descriptor("org-runner", PageScope::Global, "org/runner", MutationAudience::SignedIn, false, true)?;
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = weak.clone();
        async move {
            let (state, user, _) = global_mutation(&weak, &context, "org/runners").await?;
            let owner = values.text("owner").ok_or(MutationError::InvalidForm)?;
            let org = state.services.orgs.open(Some(&user), owner).await.map_err(|error| super::mutation_error(error.into()))?;
            if !org.can_manage {
                return Err(MutationError::Forbidden);
            }
            let (runner, form) = runner_form(&values)?;
            runners::apply_runner(&state, &user, ManagedScope::Owner(&org.org.account), &runner, &form)
                .await
                .map_err(super::mutation_error)?;
            Ok(MutationOutcome::<crate::views::actions_runners::OrgRunnersPage>::Redirect(
                PluginRedirect::new("org/runners")?.with_query(&[("owner", owner)])?,
            ))
        }
    }));
    Ok(())
}

fn instance(bundle: &mut ActionsRouteBundle, weak: WeakAppState) -> Result<(), ServiceError> {
    let descriptor = page_descriptor("instance-runners", PageScope::Global, "admin/runners", PageAudience::SiteAdmin, "admin/runner")?;
    let page_weak = weak.clone();
    bundle.pages.push(compiled_request_page(descriptor, move |context, forms, _| {
        let weak = page_weak.clone();
        async move {
            let (state, user, page) = global_page(&weak, &context, forms, "admin/runners").await?;
            let view = runners::instance(State(state), SiteAdmin(std::sync::Arc::new(user)), page).await.map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    }));
    let descriptor =
        mutation_descriptor("instance-runner-tokens", PageScope::Global, "admin/runners", MutationAudience::SiteAdmin, true, false)?;
    let token_weak = weak.clone();
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = token_weak.clone();
        async move {
            let (state, user, page) = global_mutation(&weak, &context, "admin/runners").await?;
            let outcome =
                runners::apply(&state, &user, ManagedScope::Instance, &token_form(&values)).await.map_err(super::mutation_error)?;
            let invalid = outcome.error.0.is_some();
            let view = runners::admin_page(&state, &user, page, outcome).await.map_err(super::mutation_error)?;
            let document = super::document(view, context.locale)?;
            Ok(if invalid { MutationOutcome::Invalid(document) } else { MutationOutcome::Success(document) })
        }
    }));
    let descriptor = mutation_descriptor("instance-runner", PageScope::Global, "admin/runner", MutationAudience::SiteAdmin, false, false)?;
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = weak.clone();
        async move {
            let (state, user, _) = global_mutation(&weak, &context, "admin/runners").await?;
            let (runner, form) = runner_form(&values)?;
            runners::apply_runner(&state, &user, ManagedScope::Instance, &runner, &form).await.map_err(super::mutation_error)?;
            Ok(MutationOutcome::<crate::views::actions_runners::AdminRunnersPage>::Redirect(PluginRedirect::new("admin/runners")?))
        }
    }));
    Ok(())
}
