//! Personal issue and pull request dashboards across readable repositories.

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
use rbit_services::issues::DashboardRelation;
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
    let (base, title) = if kind == IssueKind::Pull { ("/pulls", "Pull requests") } else { ("/issues", "Issues") };
    let relation = match params.view.as_deref().unwrap_or("created") {
        "created" => DashboardRelation::Created,
        "assigned" => DashboardRelation::Assigned,
        "mentioned" => DashboardRelation::Mentioned,
        "review-requested" if kind == IssueKind::Pull => DashboardRelation::ReviewRequested,
        _ => return Err(crate::error::WebError::BadRequest("unknown dashboard view".to_owned())),
    };
    let typed = params.q.as_deref().map(str::trim).filter(|q| !q.is_empty()).unwrap_or("is:open").to_owned();
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
    for (view, label) in [
        (DashboardRelation::Created, "Created by me"),
        (DashboardRelation::Assigned, "Assigned"),
        (DashboardRelation::Mentioned, "Mentioned"),
        (DashboardRelation::ReviewRequested, "Review requested"),
    ] {
        if kind == IssueKind::Pull || view != DashboardRelation::ReviewRequested {
            tabs.push(DashboardTab { label, url: url(view, &typed, 1), active: view == relation });
        }
    }
    let mut rows = Vec::new();
    // At most one page (25 threads), never a cross-repository metadata scan.
    for issue in &results.issues {
        if let Some(repo) = state.services.issues.dashboard_repository(user, issue.repo_id).await? {
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
        page: page.titled(title),
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
