//! Native runs/workflow GET declarations behind a trusted request-scoped host port.
use askama::Template;
use rbit_plugin_web::{
    CompiledPage, FormActions, FormReference, PageAudience, PageContext, PageDescriptor, PageError, PageQuery, PageRegistration, PageScope,
    compiled_request_page,
};
use std::{future::Future, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunsPageKind {
    Runs,
    Workflow,
}
impl RunsPageKind {
    pub const fn path(self) -> &'static str {
        match self {
            Self::Runs => "runs",
            Self::Workflow => "workflow",
        }
    }
}
/// Same bounded input projection as the existing native callback. Domain filter
/// parsing, repository authority and workflow lookup remain in the host.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunsQuery {
    pub branch: String,
    pub actor: String,
    pub event: String,
    pub status: String,
    pub before: Option<i64>,
    pub reference: String,
}

/// Long-lived blueprints must retain a weak host, never its composition root.
pub trait RunsHost: Clone + Send + Sync + 'static {
    type Request: RunsRequest;
    /// Upgrade the host once, reload the real actor, and construct its page with
    /// the captured locale/form appearance before parsing the route query.
    /// Route audience/context metadata never substitutes for current authority.
    fn connect(
        &self,
        kind: RunsPageKind,
        context: PageContext,
        forms: FormActions,
    ) -> impl Future<Output = Result<Self::Request, PageError>> + Send;
}
/// One owned request. The host retains SQL/service calls, read permission, the
/// single repository/header load, workflow/dispatch policy and full document.
/// The borrowed workflow and owned filter strings introduce no new buffer copy.
/// Futures are concrete; this port adds no async trait-object allocation.
pub trait RunsRequest: Send + 'static {
    type Document: Template + Send + 'static;
    fn render(
        self,
        query: RunsQuery,
        workflow: Option<&str>,
    ) -> impl Future<Output = Result<CompiledPage<Self::Document>, PageError>> + Send;
}

/// Exactly the two existing compiled GET factories; no intermediate route Vec.
pub fn pages<H: RunsHost>(host: H) -> Result<[PageRegistration; 2], PageError> {
    Ok([page(host.clone(), RunsPageKind::Runs)?, page(host, RunsPageKind::Workflow)?])
}
fn page<H: RunsHost>(host: H, kind: RunsPageKind) -> Result<PageRegistration, PageError> {
    let descriptor =
        PageDescriptor::new(kind.path(), PageScope::Repository, kind.path(), PageAudience::RepositoryReader, Duration::from_secs(1))?
            .with_query(&["workflow", "branch", "actor", "event", "status", "before", "ref"])?
            .with_forms(vec![FormReference::new("dispatch", "dispatch")?, FormReference::new("switch", "switch")?])?
            .with_document();
    Ok(compiled_request_page(descriptor, move |context, forms, query| {
        let host = host.clone();
        async move {
            let request = host.connect(kind, context, forms).await?;
            let (filters, workflow) = parse(&query, kind)?;
            request.render(filters, workflow).await
        }
    }))
}
fn parse(query: &PageQuery, kind: RunsPageKind) -> Result<(RunsQuery, Option<&str>), PageError> {
    let filters = RunsQuery {
        branch: query.get("branch").unwrap_or_default().into(),
        actor: query.get("actor").unwrap_or_default().into(),
        event: query.get("event").unwrap_or_default().into(),
        status: query.get("status").unwrap_or_default().into(),
        before: query.get("before").map(str::parse).transpose().map_err(|_| PageError::Forbidden)?,
        reference: query.get("ref").unwrap_or_default().into(),
    };
    let workflow = match kind {
        RunsPageKind::Runs => None,
        RunsPageKind::Workflow => Some(query.get("workflow").ok_or(PageError::Forbidden)?),
    };
    Ok((filters, workflow))
}

#[cfg(test)]
mod tests;
