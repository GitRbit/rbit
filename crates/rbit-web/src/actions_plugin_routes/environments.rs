//! Deployment environments use bounded resource fields, never route registration per row.
use super::settings::{descriptor, entry_schema, redirect, text};
use crate::{
    routes::actions::{environments as old, settings as settings_old},
    state::WeakAppState,
    views::{
        actions_settings::{EnvironmentsPage, Notice},
        components::FormError,
    },
};
use rbit_core::actions::BranchPolicy;
use rbit_services::{
    ActionsRouteBundle, ServiceError,
    actions::{environments::EnvironmentInput, secrets::ActionsTarget},
    plugin_pages::{
        FormReference, FormSchema, MutationAudience, MutationError, MutationOutcome, PageAudience, PageDescriptor, PageScope,
        PluginRedirect, compiled_mutation, compiled_request_page,
    },
};
use std::time::Duration;
fn service(error: ServiceError) -> MutationError {
    super::mutation_error(error.into())
}
fn target(name: &str) -> Result<PluginRedirect, MutationError> {
    PluginRedirect::new("settings/environment")?.with_query(&[("environment", name)])
}

pub(super) fn bundle(weak: WeakAppState) -> Result<ActionsRouteBundle, ServiceError> {
    let mut b = ActionsRouteBundle::default();
    pages(&mut b, &weak)?;
    create(&mut b, &weak)?;
    entries(&mut b, &weak)?;
    rules(&mut b, &weak)?;
    delete(&mut b, weak)?;
    Ok(b)
}

fn pages(b: &mut ActionsRouteBundle, weak: &WeakAppState) -> Result<(), ServiceError> {
    let d = PageDescriptor::new(
        "environments",
        PageScope::Repository,
        "settings/environments",
        PageAudience::RepositoryAdmin,
        Duration::from_secs(1),
    )
    .and_then(|d| d.with_forms(vec![FormReference::new("create", "settings/environments")?]))
    .map_err(super::declaration)?
    .with_document()
    .with_body_limit(2 * 1024 * 1024)
    .map_err(super::declaration)?;
    let w = (*weak).clone();
    b.pages.push(compiled_request_page(d, move |ctx, forms, _query| {
        let weak = w.clone();
        async move {
            let (state, viewer, mut page, owner, repo) = super::request(&weak, &ctx, forms).await?;
            page.path = super::repo_url(&owner, &repo, "settings/environments");
            let loaded = settings_old::load_admin(&state, &viewer, &owner, &repo).await.map_err(super::page_error)?;
            let view = old::list_page(&state, &loaded, page, FormError::default(), String::new()).await.map_err(super::page_error)?;
            super::document(view, ctx.locale)
        }
    }));
    let forms = ["settings/environment", "settings/environment/secrets", "settings/environment/variables", "settings/environment/delete"]
        .iter()
        .enumerate()
        .map(|(i, p)| FormReference::new(&format!("form-{i}"), p))
        .collect::<Result<_, _>>()
        .map_err(super::declaration)?;
    let d = PageDescriptor::new(
        "environment",
        PageScope::Repository,
        "settings/environment",
        PageAudience::RepositoryAdmin,
        Duration::from_secs(1),
    )
    .and_then(|d| d.with_query(&["environment", "done", "edit", "after", "secret_edit", "secret_after"]))
    .and_then(|d| d.with_forms(forms))
    .map_err(super::declaration)?
    .with_document()
    .with_body_limit(2 * 1024 * 1024)
    .map_err(super::declaration)?;
    let w = (*weak).clone();
    b.pages.push(compiled_request_page(d, move |ctx, forms, query| {
        let weak = w.clone();
        async move {
            let (state, viewer, mut page, owner, repo) = super::request(&weak, &ctx, forms).await?;
            let name = query.get("environment").ok_or(rbit_services::plugin_pages::PageError::Invalid("environment"))?;
            page.path = super::repo_url(&owner, &repo, "settings/environment");
            page.plugin_query = query.clone();
            page.plugin_fields.push(("environment".into(), name.into()));
            let (loaded, env) = old::load_environment(&state, &viewer, &owner, &repo, name).await.map_err(super::page_error)?;
            let view = old::environment_page(&state, &loaded, page, &env, query.get("done").and_then(Notice::parse))
                .await
                .map_err(super::page_error)?;
            super::document(view, ctx.locale)
        }
    }));
    Ok(())
}

fn create(b: &mut ActionsRouteBundle, weak: &WeakAppState) -> Result<(), ServiceError> {
    let d = descriptor(
        "environment-create",
        "settings/environments",
        PageScope::Repository,
        MutationAudience::RepositoryAdmin,
        FormSchema::new(vec![text("name", 255, false, true)]).map_err(super::declaration)?,
    )?;
    let w = (*weak).clone();
    b.mutations.push(compiled_mutation(d, move |ctx, values| {
        let weak = w.clone();
        async move {
            let (state, user, viewer, page, owner, repo) = super::mutation(&weak, &ctx).await?;
            let loaded = settings_old::load_admin(&state, &viewer, &owner, &repo).await.map_err(super::mutation_error)?;
            let name = values.text("name").ok_or(MutationError::InvalidForm)?;
            match settings_old::actions(&state)
                .map_err(super::mutation_error)?
                .put_environment(&user, &loaded.ctx, name, &EnvironmentInput::default())
                .await
            {
                Ok(env) => Ok(MutationOutcome::Redirect(target(&env.name)?)),
                Err(ServiceError::Invalid(message)) => Ok(MutationOutcome::Invalid(super::document(
                    old::list_page(&state, &loaded, page, FormError::from_message(&message), name.into())
                        .await
                        .map_err(super::mutation_error)?,
                    ctx.locale,
                )?)),
                Err(e) => Err(service(e)),
            }
        }
    }));
    Ok(())
}

fn entries(b: &mut ActionsRouteBundle, weak: &WeakAppState) -> Result<(), ServiceError> {
    for kind in ["secret", "variable"] {
        let path = if kind == "secret" { "settings/environment/secrets" } else { "settings/environment/variables" };
        let d = descriptor(
            &format!("environment-{kind}"),
            path,
            PageScope::Repository,
            MutationAudience::RepositoryAdmin,
            entry_schema(vec![text("environment", 255, false, true)])?,
        )?;
        let w = (*weak).clone();
        b.mutations.push(compiled_mutation(d, move |ctx, values| {
            let weak = w.clone();
            async move {
                let (state, user, viewer, mut page, owner, repo) = super::mutation(&weak, &ctx).await?;
                let name = values.text("environment").ok_or(MutationError::InvalidForm)?;
                page.plugin_fields.push(("environment".into(), name.into()));
                let (loaded, env) = old::load_environment(&state, &viewer, &owner, &repo, name).await.map_err(super::mutation_error)?;
                let mut form = settings_old::EntryForm::parse(values.to_pairs());
                form.kind = kind.into();
                let actions = settings_old::actions(&state).map_err(super::mutation_error)?;
                let target = ActionsTarget::Environment(&loaded.ctx, &env);
                let result = if kind == "secret" {
                    settings_old::apply_secret(&actions, &user, target, &form).await
                } else {
                    settings_old::apply_variable(&actions, &user, target, &form).await
                };
                match result {
                    Ok(notice) => Ok(MutationOutcome::Redirect(redirect("settings/environment", notice, &[("environment", &env.name)])?)),
                    Err(ServiceError::Invalid(message)) => {
                        let mut view = old::environment_page(&state, &loaded, page, &env, None).await.map_err(super::mutation_error)?;
                        (view.secrets, view.variables) = settings_old::reject(view.secrets, view.variables, &form, &message);
                        Ok(MutationOutcome::Invalid(super::document(view, ctx.locale)?))
                    }
                    Err(e) => Err(service(e)),
                }
            }
        }));
    }
    Ok(())
}

fn rules(b: &mut ActionsRouteBundle, weak: &WeakAppState) -> Result<(), ServiceError> {
    let schema = FormSchema::new(vec![
        text("environment", 255, false, true),
        text("reviewers", 4096, true, false),
        text("prevent_self_review", 8, false, false),
        text("wait_timer", 16, false, false),
        text("policy", 16, false, true),
        text("patterns", 4096, true, false),
    ])
    .map_err(super::declaration)?;
    let d = descriptor("environment-rules", "settings/environment", PageScope::Repository, MutationAudience::RepositoryAdmin, schema)?;
    let w = (*weak).clone();
    b.mutations.push(compiled_mutation(d, move |ctx, values| {
        let weak = w.clone();
        async move {
            let (state, user, viewer, mut page, owner, repo) = super::mutation(&weak, &ctx).await?;
            let name = values.text("environment").ok_or(MutationError::InvalidForm)?;
            page.plugin_fields.push(("environment".into(), name.into()));
            let (loaded, env) = old::load_environment(&state, &viewer, &owner, &repo, name).await.map_err(super::mutation_error)?;
            let lines = |s: &str| s.lines().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect::<Vec<_>>();
            let reviewers = values.text("reviewers").unwrap_or_default();
            let patterns = values.text("patterns").unwrap_or_default();
            let timer = values.text("wait_timer").unwrap_or_default();
            let policy = values.text("policy").unwrap_or_default();
            let input = EnvironmentInput {
                reviewers: lines(&reviewers.replace(',', "\n")),
                prevent_self_review: values.text("prevent_self_review").is_some(),
                branch_policy: match policy {
                    "protected" => BranchPolicy::Protected,
                    "selected" => BranchPolicy::Selected(lines(patterns)),
                    _ => BranchPolicy::All,
                },
                wait_timer: timer.trim().parse().unwrap_or(if timer.trim().is_empty() { 0 } else { u32::MAX }),
            };
            match settings_old::actions(&state).map_err(super::mutation_error)?.put_environment(&user, &loaded.ctx, &env.name, &input).await
            {
                Ok(_) => Ok(MutationOutcome::Redirect(redirect("settings/environment", Notice::Saved, &[("environment", &env.name)])?)),
                Err(ServiceError::Invalid(message)) => {
                    let mut view = old::environment_page(&state, &loaded, page, &env, None).await.map_err(super::mutation_error)?;
                    view.error = FormError::from_message(&message);
                    view.reviewers = reviewers.into();
                    view.patterns = patterns.into();
                    view.prevent_self_review = input.prevent_self_review;
                    view.wait_timer = timer.into();
                    view.policy = policy.into();
                    Ok(MutationOutcome::Invalid(super::document(view, ctx.locale)?))
                }
                Err(e) => Err(service(e)),
            }
        }
    }));
    Ok(())
}

fn delete(b: &mut ActionsRouteBundle, weak: WeakAppState) -> Result<(), ServiceError> {
    let d = descriptor(
        "environment-delete",
        "settings/environment/delete",
        PageScope::Repository,
        MutationAudience::RepositoryAdmin,
        FormSchema::new(vec![text("environment", 255, false, true)]).map_err(super::declaration)?,
    )?;
    b.mutations.push(compiled_mutation(d, move |ctx, values| {
        let weak = weak.clone();
        async move {
            let (state, user, viewer, _, owner, repo) = super::mutation(&weak, &ctx).await?;
            let (loaded, env) =
                old::load_environment(&state, &viewer, &owner, &repo, values.text("environment").ok_or(MutationError::InvalidForm)?)
                    .await
                    .map_err(super::mutation_error)?;
            settings_old::actions(&state)
                .map_err(super::mutation_error)?
                .delete_environment(&user, &loaded.ctx, &env.name)
                .await
                .map_err(service)?;
            Ok(MutationOutcome::<super::Localized<EnvironmentsPage>>::Redirect(PluginRedirect::new("settings/environments")?))
        }
    }));
    Ok(())
}
