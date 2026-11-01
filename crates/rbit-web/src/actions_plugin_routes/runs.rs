//! Repository runs and generation-bound native run controls.
use crate::{
    routes::{actions::runs, repo::RepoPage},
    state::WeakAppState,
};
use axum::extract::{Path, State};
use rbit_core::actions::{JobId, ReviewDecision};
use rbit_services::{
    ActionsRouteBundle, ServiceError,
    actions::runs::Rerun,
    plugin_pages::{
        FormField, FormFieldKind, FormReference, FormSchema, MutationAudience, MutationDescriptor, MutationOutcome, PageAudience,
        PageDescriptor, PageScope, PluginRedirect, compiled_mutation, compiled_request_page,
    },
};
use std::time::Duration;

const CONTROLS: &[&str] = &["cancel", "rerun", "rerun-failed", "approve", "review", "rerun-job"];

pub(super) fn bundle(weak: WeakAppState) -> Result<ActionsRouteBundle, ServiceError> {
    let mut bundle = ActionsRouteBundle::default();
    workflow_pages(&weak, &mut bundle)?;
    let forms = CONTROLS.iter().map(|name| FormReference::new(name, name)).collect::<Result<Vec<_>, _>>().map_err(super::declaration)?;
    let descriptor = PageDescriptor::new("run", PageScope::Repository, "run", PageAudience::RepositoryReader, Duration::from_secs(1))
        .and_then(|d| d.with_query(&["run", "jobs_page", "artifacts_page"]))
        .and_then(|d| d.with_forms(forms))
        .map_err(super::declaration)?
        .with_document();
    let state = weak.clone();
    bundle.pages.push(compiled_request_page(descriptor, move |context, forms, query| {
        let weak = state.clone();
        async move {
            let (state, viewer, mut page, owner, repo) = super::request(&weak, &context, forms).await?;
            page.path = super::repo_url(&owner, &repo, "run");
            page.plugin_query = query.clone();
            for cursor in ["jobs_page", "artifacts_page"] {
                if query.get(cursor).is_some_and(|value| value.parse::<u16>().is_err()) {
                    return Err(rbit_services::plugin_pages::PageError::Forbidden);
                }
            }

            let run = query.get("run").ok_or(rbit_services::plugin_pages::PageError::Forbidden)?.to_owned();
            let view = runs::show(State(state), viewer, page, Path((owner, repo, run))).await.map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    }));
    for path in CONTROLS {
        bundle.mutations.push(control(weak.clone(), path)?);
    }
    let schema = FormSchema::large_text(vec![text("workflow", 255, true), text("ref", 1024, true)])
        .and_then(|s| s.with_prefix("inputs[", "]", 25, 4096))
        .map_err(super::declaration)?;
    let descriptor = MutationDescriptor::new(
        "dispatch",
        PageScope::Repository,
        "dispatch",
        MutationAudience::RepositoryWriter,
        Duration::from_secs(1),
        schema,
    )
    .map_err(super::declaration)?
    .with_document();
    let state = weak.clone();
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = state.clone();
        async move {
            let (state, user, viewer, page, owner, repo) = super::mutation(&weak, &context).await?;
            let workflow = values.text("workflow").ok_or(rbit_services::plugin_pages::MutationError::InvalidForm)?;
            let result = runs::dispatch_native(&state, &user, &viewer, page, &owner, &repo, workflow, values.to_pairs())
                .await
                .map_err(super::mutation_error)?;
            match result {
                runs::DispatchResult::Run(number) => Ok(MutationOutcome::Redirect(run_redirect(number)?)),
                runs::DispatchResult::Invalid(view) => Ok(MutationOutcome::Invalid(super::document(*view, context.locale)?)),
            }
        }
    }));
    let schema = FormSchema::new(vec![
        text("workflow", 255, true),
        FormField { name: "disabled".into(), kind: FormFieldKind::Boolean, required: true },
    ])
    .map_err(super::declaration)?;
    let descriptor = MutationDescriptor::new(
        "switch",
        PageScope::Repository,
        "switch",
        MutationAudience::RepositoryWriter,
        Duration::from_secs(1),
        schema,
    )
    .map_err(super::declaration)?
    .with_document();
    bundle.mutations.push(compiled_mutation(descriptor, move |context, values| {
        let weak = weak.clone();
        async move {
            let (state, user, viewer, _, owner, repo) = super::mutation(&weak, &context).await?;
            let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await.map_err(super::mutation_error)?;
            let workflow = values.text("workflow").ok_or(rbit_services::plugin_pages::MutationError::InvalidForm)?;
            let switched = runs::actions(&state)
                .map_err(super::mutation_error)?
                .set_workflow_disabled(&user, &loaded.ctx, workflow, values.boolean("disabled").unwrap_or(false))
                .await
                .map_err(crate::error::WebError::from)
                .map_err(super::mutation_error)?;
            Ok(MutationOutcome::<super::Localized<crate::views::actions::RunsPage>>::Redirect(
                PluginRedirect::new("workflow")?.with_query(&[("workflow", crate::views::actions::workflow_file(&switched.path))])?,
            ))
        }
    }));
    Ok(bundle)
}
fn workflow_pages(weak: &WeakAppState, bundle: &mut ActionsRouteBundle) -> Result<(), ServiceError> {
    for path in ["runs", "workflow"] {
        let descriptor = PageDescriptor::new(path, PageScope::Repository, path, PageAudience::RepositoryReader, Duration::from_secs(1))
            .and_then(|d| d.with_query(&["workflow", "branch", "actor", "event", "status", "before", "ref"]))
            .and_then(|d| d.with_forms(vec![FormReference::new("dispatch", "dispatch")?, FormReference::new("switch", "switch")?]))
            .map_err(super::declaration)?
            .with_document();
        let weak = (*weak).clone();
        bundle.pages.push(compiled_request_page(descriptor, move |context, forms, query| {
            let weak = weak.clone();
            async move {
                let (state, viewer, mut page, owner, repo) = super::request(&weak, &context, forms).await?;
                page.path = super::repo_url(&owner, &repo, path);
                let list = runs::ListQuery {
                    branch: query.get("branch").unwrap_or_default().into(),
                    actor: query.get("actor").unwrap_or_default().into(),
                    event: query.get("event").unwrap_or_default().into(),
                    status: query.get("status").unwrap_or_default().into(),
                    before: query
                        .get("before")
                        .map(str::parse)
                        .transpose()
                        .map_err(|_| rbit_services::plugin_pages::PageError::Forbidden)?,
                    reference: query.get("ref").unwrap_or_default().into(),
                };
                let workflow = if path == "workflow" {
                    Some(query.get("workflow").ok_or(rbit_services::plugin_pages::PageError::Forbidden)?)
                } else {
                    None
                };
                let view = runs::runs_page(&state, &viewer, page, &owner, &repo, workflow, &list, None).await.map_err(super::page_error)?;
                super::document(view, context.locale)
            }
        }));
    }
    Ok(())
}

fn text(name: &str, max_bytes: usize, required: bool) -> FormField {
    FormField { name: name.into(), kind: FormFieldKind::Text { max_bytes, multiline: false }, required }
}
fn run_redirect(number: i64) -> Result<PluginRedirect, rbit_services::plugin_pages::MutationError> {
    PluginRedirect::new("run")?.with_query(&[("run", &number.to_string())])
}
fn control(weak: WeakAppState, path: &'static str) -> Result<rbit_services::plugin_pages::MutationRegistration, ServiceError> {
    let mut fields = vec![FormField { name: "run".into(), kind: FormFieldKind::Integer { min: 1, max: i64::MAX }, required: true }];
    match path {
        "rerun-job" => {
            fields.push(FormField { name: "job".into(), kind: FormFieldKind::Integer { min: 1, max: i64::MAX }, required: true });
        }
        "approve" => fields.push(FormField { name: "trust_author".into(), kind: FormFieldKind::Boolean, required: false }),
        "review" => fields.extend([
            text("environment", 255, true),
            text("decision", 16, true),
            FormField { name: "comment".into(), kind: FormFieldKind::Text { max_bytes: 4096, multiline: true }, required: false },
        ]),
        _ => {}
    }
    let audience = if path == "review" { MutationAudience::RepositorySignedIn } else { MutationAudience::RepositoryWriter };
    let descriptor = MutationDescriptor::new(
        path,
        PageScope::Repository,
        path,
        audience,
        Duration::from_secs(1),
        FormSchema::new(fields).map_err(super::declaration)?,
    )
    .map_err(super::declaration)?
    .with_document();
    Ok(compiled_mutation(descriptor, move |context, values| {
        let weak = weak.clone();
        async move {
            let (state, user, viewer, _, owner, repo) = super::mutation(&weak, &context).await?;
            let number = values.integer("run").ok_or(rbit_services::plugin_pages::MutationError::InvalidForm)?;
            let (loaded, id, _) =
                runs::load_run(&state, &viewer, &owner, &repo, &number.to_string()).await.map_err(super::mutation_error)?;
            let actions = runs::actions(&state).map_err(super::mutation_error)?;
            match path {
                "cancel" => {
                    actions.cancel(&user, &loaded.ctx, id).await.map_err(service)?;
                }
                "rerun" | "rerun-failed" | "rerun-job" => {
                    let which = match path {
                        "rerun" => Rerun::All,
                        "rerun-failed" => Rerun::Failed,
                        _ => Rerun::Job(JobId(values.integer("job").ok_or(rbit_services::plugin_pages::MutationError::InvalidForm)?)),
                    };
                    actions.rerun(&user, &loaded.ctx, id, which).await.map_err(service)?;
                }
                "approve" => {
                    actions.approve(&user, &loaded.ctx, id, values.boolean("trust_author").unwrap_or(false)).await.map_err(service)?;
                }
                "review" => {
                    let detail = actions.run_detail(&loaded.ctx, number).await.map_err(service)?;
                    let decision = match values.text("decision") {
                        Some("approve") => ReviewDecision::Approved,
                        Some("reject") => ReviewDecision::Rejected,
                        _ => return Err(rbit_services::plugin_pages::MutationError::InvalidForm),
                    };
                    let environments =
                        vec![values.text("environment").ok_or(rbit_services::plugin_pages::MutationError::InvalidForm)?.to_owned()];
                    actions
                        .review_deployments(
                            &user,
                            &loaded.ctx,
                            &detail.run,
                            &environments,
                            decision,
                            values.text("comment").unwrap_or_default(),
                        )
                        .await
                        .map_err(service)?;
                }
                _ => unreachable!(),
            }
            Ok(MutationOutcome::<super::Localized<crate::views::actions::RunPage>>::Redirect(run_redirect(number)?))
        }
    }))
}

fn service(error: ServiceError) -> rbit_services::plugin_pages::MutationError {
    super::mutation_error(error.into())
}
