//! One complete current-task step summary, with bounded keyset navigation.
use crate::state::WeakAppState;
use rbit_core::actions::JobId;
use rbit_services::{
    ActionsRouteBundle, ServiceError,
    plugin_pages::{PageAudience, PageDescriptor, PageError, PageScope, compiled_request_page},
};
use std::time::Duration;

pub(super) fn bundle(weak: WeakAppState) -> Result<ActionsRouteBundle, ServiceError> {
    let descriptor =
        PageDescriptor::new("summary", PageScope::Repository, "summary", PageAudience::RepositoryReader, Duration::from_secs(1))
            .and_then(|d| d.with_query(&["run", "job", "step"]))
            .and_then(|d| d.with_document().with_body_limit(2 * 1024 * 1024))
            .map_err(super::declaration)?;
    let page = compiled_request_page(descriptor, move |context, forms, query| {
        let weak = weak.clone();
        async move {
            let (state, viewer, mut page, owner, repo) = super::request(&weak, &context, forms).await?;
            let positive = |name: &str| query.get(name).and_then(|v| v.parse::<i64>().ok()).filter(|n| *n > 0).ok_or(PageError::Forbidden);
            let run = positive("run")?;
            let job = JobId(positive("job")?);
            let step =
                query.get("step").map(|value| value.parse::<i64>().ok().filter(|n| *n >= 0).ok_or(PageError::Forbidden)).transpose()?;
            page.path = super::repo_url(&owner, &repo, "summary");
            let view = crate::routes::actions::runs::summary_page(&state, &viewer, page, &owner, &repo, (run, job, step))
                .await
                .map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    });
    Ok(ActionsRouteBundle { pages: vec![page], mutations: Vec::new() })
}
