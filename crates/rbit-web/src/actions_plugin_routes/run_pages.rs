//! Authoritative host bridge for the optional Actions runs/workflow factories.
use super::{
    AppState, CompiledPage, FormActions, LocaleSnapshot, Localized, Page, PageContext, PageError, ServiceError, Viewer, WeakAppState,
};
use crate::{routes::actions::runs, views::actions::RunsPage};
use rbit_actions_web::run_pages::{RunsHost, RunsPageKind, RunsQuery, RunsRequest};

#[derive(Clone)]
struct Host(WeakAppState);
struct Request {
    state: AppState,
    viewer: Viewer,
    page: Page,
    owner: String,
    repo: String,
    locale: LocaleSnapshot,
}
impl RunsHost for Host {
    type Request = Request;
    async fn connect(&self, kind: RunsPageKind, context: PageContext, forms: FormActions) -> Result<Request, PageError> {
        let (state, viewer, mut page, owner, repo) = super::request(&self.0, &context, forms).await?;
        page.path = super::repo_url(&owner, &repo, kind.path());
        Ok(Request { state, viewer, page, owner, repo, locale: context.locale })
    }
}
impl RunsRequest for Request {
    type Document = Localized<RunsPage>;
    async fn render(self, query: RunsQuery, workflow: Option<&str>) -> Result<CompiledPage<Self::Document>, PageError> {
        let list = runs::ListQuery {
            branch: query.branch,
            actor: query.actor,
            event: query.event,
            status: query.status,
            before: query.before,
            reference: query.reference,
        };
        let view = runs::runs_page(&self.state, &self.viewer, self.page, &self.owner, &self.repo, workflow, &list, None)
            .await
            .map_err(super::page_error)?;
        super::document(view, self.locale)
    }
}
pub(super) fn pages(weak: WeakAppState) -> Result<[rbit_services::plugin_pages::PageRegistration; 2], ServiceError> {
    rbit_actions_web::run_pages::pages(Host(weak)).map_err(super::declaration)
}
