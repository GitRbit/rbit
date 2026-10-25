//! `/` and `/explore`.

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use rbit_core::{RepoSort, repository::ArchivedFilter};
use rbit_services::repositories::RepoSearch;
use serde::Deserialize;

use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    views::{
        components::{OrgLink, Pagination, RepoCard},
        home::{DashboardPage, ExplorePage, LandingPage, SortLink, explore_url, sort_label},
        layout::Page,
    },
};

const RECENT_ON_HOME: u32 = 8;

/// Landing page for visitors, dashboard for signed-in users. The independent reads of
/// each page run concurrently on the database pool, so the page waits for the slowest
/// one instead of their sum.
pub async fn index(State(state): State<AppState>, viewer: Viewer, page: Page) -> WebResult<Response> {
    let repositories = &state.services.repositories;
    let Some(user) = viewer.user() else {
        let recent = async { super::listing::repo_cards(&state, &repositories.recent_public(RECENT_ON_HOME).await?).await };
        let counts = async { WebResult::Ok(state.services.instance.public_counts().await?) };
        let (recent, counts) = tokio::try_join!(recent, counts)?;
        return Ok(LandingPage { page, recent, counts }.into_response());
    };
    let repos = async { WebResult::Ok(repositories.list_for_dashboard(user).await?) };
    let recent = async { WebResult::Ok(repositories.recent_public(RECENT_ON_HOME).await?) };
    let orgs = async { WebResult::Ok(state.services.orgs.memberships(user).await?.iter().map(OrgLink::from).collect::<Vec<_>>()) };
    let (repos, recent, orgs) = tokio::try_join!(repos, recent, orgs)?;
    // Both lists' star counts in one statement.
    let listed: Vec<_> = repos.iter().chain(&recent).cloned().collect();
    let star_counts = state.services.stars.counts(&listed).await?;
    let (repos, recent) = (RepoCard::list(&repos, &star_counts), RepoCard::list(&recent, &star_counts));
    let can_create_org = state.services.orgs.can_create(user);
    Ok(DashboardPage { page: page.titled("Dashboard"), repos, recent, orgs, can_create_org }.into_response())
}

/// `?q=…&sort=…&archived=…&page=…`. Everything is optional and parsed leniently: a bad
/// value falls back to its default instead of failing the request.
#[derive(Debug, Deserialize)]
pub struct ExploreQuery {
    q: Option<String>,
    sort: Option<String>,
    /// `false` hides archived repositories, `true` lists only them.
    archived: Option<String>,
    page: Option<String>,
}

pub async fn explore(State(state): State<AppState>, page: Page, Query(params): Query<ExploreQuery>) -> WebResult<ExplorePage> {
    let query = params.q.unwrap_or_default().trim().to_owned();
    let sort = params.sort.as_deref().and_then(RepoSort::parse).unwrap_or_default();
    let archived = params.archived.as_deref().and_then(ArchivedFilter::parse).unwrap_or_default();
    let requested = params.page.as_deref().and_then(|p| p.parse().ok()).unwrap_or(1);

    let search = RepoSearch { text: query.clone(), sort, archived, page: requested };
    let results = state.services.repositories.explore(&search).await?;

    let pagination = Pagination {
        page: results.page,
        page_count: results.page_count(),
        previous_url: results.has_previous().then(|| explore_url(&query, sort, archived, results.page - 1)),
        next_url: results.has_next().then(|| explore_url(&query, sort, archived, results.page + 1)),
    };
    let sorts = RepoSort::ALL
        .iter()
        .map(|&option| SortLink { label: sort_label(option), url: explore_url(&query, option, archived, 1), is_active: option == sort })
        .collect();
    let archived_filters = ArchivedFilter::ALL
        .iter()
        .map(|&option| SortLink { label: option.label(), url: explore_url(&query, sort, option, 1), is_active: option == archived })
        .collect();
    let title = if query.is_empty() { "Explore".to_owned() } else { format!("{query} · Explore") };

    Ok(ExplorePage {
        page: page.titled(title),
        repos: super::listing::repo_cards(&state, &results.repos).await?,
        query,
        sort: sort.as_str(),
        sorts,
        archived: archived.query_value(),
        archived_filters,
        total: results.total,
        pagination,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::Body,
        http::{Request, header},
    };
    use rbit_core::{Config, Visibility};
    use rbit_services::{Services, SignUp};
    use rbit_storage::{Database, KvStore, NewRepository, sqlite::orgs::NewOrganization};
    use tower::ServiceExt;

    use crate::middleware::SESSION_COOKIE;

    /// Each section comes from its own concurrent read; every one must reach the page.
    #[tokio::test]
    async fn the_dashboard_and_landing_page_show_every_section() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let router = crate::router(crate::AppState::new(config, services.clone()));

        let mut users = Vec::new();
        for name in ["ada", "bob"] {
            let signup = SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
            users.push(services.accounts.create_user(signup, Some(name == "ada")).await.unwrap());
        }
        let new_org = NewOrganization { name: "acme".parse().unwrap(), display_name: None, description: None, creator: users[0].id() };
        database.orgs().create(new_org).await.unwrap();
        let new_repo = NewRepository {
            owner_id: users[0].id(),
            name: "rocket".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        };
        database.repositories().create(new_repo).await.unwrap();

        let page = |cookie: Option<String>| {
            let router = router.clone();
            async move {
                let mut request = Request::builder().uri("/");
                if let Some(cookie) = cookie {
                    request = request.header(header::COOKIE, cookie);
                }
                let response = router.oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
                String::from_utf8(axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
            }
        };
        let mut cookies = Vec::new();
        for user in &users {
            cookies.push(format!("{SESSION_COOKIE}={}", services.accounts.start_session(user).await.unwrap().encode()));
        }

        let ada = page(Some(cookies[0].clone())).await;
        assert!(ada.contains("id=\"dashboard-repos\"") && ada.contains("href=\"/ada/rocket\""), "own repositories");
        assert!(ada.contains("href=\"/acme\""), "organisations");
        assert!(ada.contains("id=\"recent-title\""), "recently updated");
        let bob = page(Some(cookies[1].clone())).await;
        assert!(bob.contains("No repositories yet") && bob.contains("You are not a member of any organization"));
        assert!(bob.contains("href=\"/ada/rocket\""), "public repositories are recent for everyone");
        let visitor = page(None).await;
        assert!(visitor.contains("href=\"/ada/rocket\"") && !visitor.contains("id=\"dashboard-repos\""), "the landing page");
    }
}
