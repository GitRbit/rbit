//! Compiled job and step-log documents; raw log transport stays host-owned.
use crate::{routes::actions::jobs, state::WeakAppState};
use axum::extract::{Path, State};
use rbit_services::{
    ActionsRouteBundle, ServiceError,
    plugin_pages::{PageAudience, PageDescriptor, PageError, PageScope, compiled_request_page},
};
use std::time::Duration;

pub(super) fn bundle(weak: WeakAppState) -> Result<ActionsRouteBundle, ServiceError> {
    let mut bundle = ActionsRouteBundle::default();
    let descriptor = PageDescriptor::new("job", PageScope::Repository, "job", PageAudience::RepositoryReader, Duration::from_secs(1))
        .and_then(|d| d.with_query(&["run", "job"]))
        .and_then(|d| d.with_forms(vec![rbit_services::plugin_pages::FormReference::new("rerun-job", "rerun-job")?]))
        .map_err(super::declaration)?
        .with_document();
    let state = weak.clone();
    bundle.pages.push(compiled_request_page(descriptor, move |context, forms, query| {
        let weak = state.clone();
        async move {
            let (state, viewer, mut page, owner, repo) = super::request(&weak, &context, forms).await?;
            page.path = format!("/{owner}/{repo}/-/plugins/rbit.actions/job");
            let run = positive(query.get("run"))?;
            let job = positive(query.get("job"))?;
            let view = jobs::show(State(state), viewer, page, Path((owner, repo, run, job))).await.map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    }));
    let descriptor = PageDescriptor::new("logs", PageScope::Repository, "logs", PageAudience::RepositoryReader, Duration::from_secs(1))
        .and_then(|d| d.with_query(&["run", "job", "step"]))
        .map_err(super::declaration)?
        .with_document();
    bundle.pages.push(compiled_request_page(descriptor, move |context, forms, query| {
        let weak = weak.clone();
        async move {
            let (state, viewer, mut page, owner, repo) = super::request(&weak, &context, forms).await?;
            page.path = format!("/{owner}/{repo}/-/plugins/rbit.actions/logs");
            let run = positive(query.get("run"))?;
            let job = positive(query.get("job"))?;
            let step =
                query.get("step").and_then(|value| value.parse::<i64>().ok()).filter(|number| *number >= 0).ok_or(PageError::Forbidden)?;
            let view = jobs::step_page(&state, &viewer, page, &owner, &repo, &run, &job, step).await.map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    }));
    Ok(bundle)
}
fn positive(value: Option<&str>) -> Result<String, PageError> {
    let value = value.ok_or(PageError::Forbidden)?;
    value.parse::<i64>().ok().filter(|id| *id > 0).map(|id| id.to_string()).ok_or(PageError::Forbidden)
}
