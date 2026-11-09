//! Personal issue and pull request dashboards across readable repositories.
//!
//! Which views a page offers, what an absent view or search means and the listing are
//! decided in `rbit_collaboration::issues`; this route parses, localizes and renders.

use crate::{
    AppState,
    error::WebResult,
    extract::SignedIn,
    views::{
        components::Pagination,
        dashboards::{DashboardPage, DashboardRow, DashboardTab},
        issues::IssueRow,
        layout::Page,
    },
};
use axum::{
    Router,
    extract::{Query, State},
    routing::get,
};
use rbit_core::{IssueKind, issue_query::IssueQuery};
use rbit_localization::shell::home::{HomeMessage, Involvement};
use rbit_services::issues::{DashboardRelation, dashboard_query};
use serde::Deserialize;

pub fn routes() -> Router<AppState> {
    Router::new().route("/issues", get(issues)).route("/pulls", get(pulls))
}

#[derive(Debug, Deserialize)]
struct Params {
    q: Option<String>,
    view: Option<String>,
    page: Option<u32>,
}

async fn issues(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Query(params): Query<Params>,
) -> WebResult<DashboardPage> {
    show(&state, &user, page, params, IssueKind::Issue).await
}
async fn pulls(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Query(params): Query<Params>,
) -> WebResult<DashboardPage> {
    show(&state, &user, page, params, IssueKind::Pull).await
}

async fn show(state: &AppState, user: &rbit_core::User, page: Page, params: Params, kind: IssueKind) -> WebResult<DashboardPage> {
    let (base, title) = if kind == IssueKind::Pull {
        ("/pulls", page.text(HomeMessage::PullRequests))
    } else {
        ("/issues", page.text(HomeMessage::Issues))
    };
    let Some(relation) = DashboardRelation::from_view(params.view.as_deref(), kind) else {
        return Err(crate::error::WebError::BadRequest("unknown dashboard view".to_owned()));
    };
    let typed = dashboard_query(params.q.as_deref());
    let query = IssueQuery::parse(&typed);
    let results = state.services.issues.dashboard(user, kind, relation, &query, params.page.unwrap_or(1)).await?;
    let url = |view: DashboardRelation, query: &str, n: u32| {
        format!(
            "{base}?view={}&q={}&page={n}",
            view.as_str(),
            percent_encoding::utf8_percent_encode(query, percent_encoding::NON_ALPHANUMERIC)
        )
    };
    let mut tabs = Vec::new();
    for view in DashboardRelation::views(kind) {
        let label = page.text(HomeMessage::InvolvementTab(involvement(view)));
        tabs.push(DashboardTab { label, url: url(view, &typed, 1), active: view == relation });
    }
    let mut rows = Vec::new();
    // At most one page (25 threads), never a cross-repository metadata scan.
    for issue in &results.issues {
        if let Some(repo) = results.repositories.get(&issue.repo_id) {
            rows.push(DashboardRow {
                repository: repo.full_name(),
                row: IssueRow::new(
                    issue,
                    issue.author.and_then(|id| results.authors.get(&id)),
                    results.metadata.get(&issue.id),
                    repo.owner.name.as_str(),
                    repo.repo.name.as_str(),
                    base,
                ),
            });
        }
    }
    let pagination = Pagination {
        page: results.page,
        page_count: results.page_count(),
        previous_url: (results.page > 1).then(|| url(relation, &typed, results.page - 1)),
        next_url: (results.page < results.page_count()).then(|| url(relation, &typed, results.page + 1)),
    };
    let open_query = query.with_state(rbit_core::issue_query::StateFilter::Open).to_string();
    let closed_query = query.with_state(rbit_core::issue_query::StateFilter::Closed).to_string();
    Ok(DashboardPage {
        page: page.titled(title.to_string()),
        title,
        base,
        view: relation.as_str(),
        query: typed,
        tabs,
        rows,
        hints: results.hints,
        open: results.open,
        closed: results.closed,
        open_url: url(relation, &open_query, 1),
        closed_url: url(relation, &closed_query, 1),
        pagination,
    })
}

/// The tab label of `view`.
const fn involvement(view: DashboardRelation) -> Involvement {
    match view {
        DashboardRelation::Created => Involvement::Created,
        DashboardRelation::Assigned => Involvement::Assigned,
        DashboardRelation::Mentioned => Involvement::Mentioned,
        DashboardRelation::ReviewRequested => Involvement::ReviewRequested,
    }
}
