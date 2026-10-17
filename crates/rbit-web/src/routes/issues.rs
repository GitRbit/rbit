//! Issue pages: `/{owner}/{repo}/issues`, `/issues/new`, `/issues/{n}` (a pull request
//! number answers 301 to `/pull/{n}`), plus the sidebar's forms (labels, assignees,
//! milestone, subscription, deletion).
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository pages load the repository with
//! `super::repo::RepoPage::load` and build the header with `loaded.header(&state, tab)`.
//!
//! The sidebar forms post to `/issues/{n}/…` for issues and pull requests alike (they
//! share the number) and answer `303` to the thread's canonical page. Hidden
//! repositories and unknown or malformed numbers are 404; actions the viewer may not
//! take on a thread they can see are 403.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{
    AccountId, IssueNumber, LabelId, MilestoneId, User,
    issue_query::{DEFAULT_QUERY, IssueQuery, StateFilter},
};
use rbit_services::{IssueContext, ServiceError, issues::NewIssueInput, timeline::TimelineCursor};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    urls::issue_url,
    views::{
        components::{FormError, Pagination},
        issues::{
            Choice, IssueListPage, IssuePage, IssueRow, MenuChoices, NewIssuePage, ThreadSummary, filter_menus, list_url, safe_color,
            sidebar,
        },
        layout::Page,
        timeline::{NEW_COMMENT_ANCHOR, ThreadUrls, TimelineOptions, thread},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/issues", get(list).post(create))
        .route("/{owner}/{repo}/issues/new", get(new_form))
        .route("/{owner}/{repo}/issues/{number}", get(show))
        .route("/{owner}/{repo}/issues/{number}/labels", post(set_labels))
        .route("/{owner}/{repo}/issues/{number}/assignees", post(set_assignees))
        .route("/{owner}/{repo}/issues/{number}/milestone", post(set_milestone))
        .route("/{owner}/{repo}/issues/{number}/subscription", post(set_subscription))
        .route("/{owner}/{repo}/issues/{number}/delete", post(delete))
}

/// Values of `key` among submitted form pairs (checkbox lists repeat their name).
fn values<'a>(pairs: &'a [(String, String)], key: &'a str) -> impl Iterator<Item = &'a str> {
    pairs.iter().filter(move |(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Positive database ids; anything else is a 400.
fn ids(raw: impl Iterator<Item = impl AsRef<str>>) -> WebResult<Vec<i64>> {
    raw.filter(|value| !value.as_ref().is_empty())
        .map(|value| {
            value.as_ref().parse::<i64>().ok().filter(|id| *id > 0).ok_or_else(|| WebError::BadRequest("unknown choice".to_owned()))
        })
        .collect()
}

fn parse_number(number: &str) -> WebResult<IssueNumber> {
    IssueNumber::parse(number).map_err(|_| WebError::NotFound)
}

/// Thread `number` of `owner/repo` for `user`; 404 for hidden repositories and unknown
/// or malformed numbers.
async fn open(state: &AppState, user: &User, owner: &str, repo: &str, number: &str) -> WebResult<IssueContext> {
    let number = parse_number(number)?;
    let repo = state.services.repositories.open(Some(user), owner, repo).await?;
    Ok(state.services.timeline.open(Some(user), repo, number).await?)
}

/// `303` to the thread's page, scrolled to `anchor`.
fn back(ctx: &IssueContext, anchor: &str) -> Redirect {
    Redirect::to(&ThreadUrls::for_context(ctx).at(anchor))
}

// ------------------------------------------------------------------ the list

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    page: Option<u32>,
}

/// `GET /{owner}/{repo}/issues[?q=…&page=n]`
async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(params): Query<ListQuery>,
) -> WebResult<IssueListPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let typed = params.q.as_deref().map(str::trim).filter(|q| !q.is_empty()).unwrap_or(DEFAULT_QUERY).to_owned();
    let query = IssueQuery::parse(&typed);
    let issues = &state.services.issues;
    let ctx = &loaded.ctx;
    let (results, labels, milestones, people) = tokio::try_join!(
        issues.search(viewer.user(), ctx, &query, params.page.unwrap_or(1)),
        issues.labels(ctx),
        issues.milestones(ctx, Some(rbit_core::IssueState::Open)),
        issues.assignable(ctx, ""),
    )?;
    let list = format!("{}/issues", loaded.urls.home());
    let clean = IssueQuery { ignored: Vec::new(), ..query.clone() };
    let page_url = |n: u32| {
        let base = list_url(&list, &clean);
        if base.contains('?') { format!("{base}&page={n}") } else { format!("{base}?page={n}") }
    };
    let pagination = Pagination {
        page: results.page,
        page_count: results.page_count(),
        previous_url: (results.page > 1).then(|| page_url(results.page - 1)),
        next_url: (results.page < results.page_count()).then(|| page_url(results.page + 1)),
    };
    let choices = MenuChoices { labels, milestones, people };
    let full_name = ctx.repo.full_name();
    Ok(IssueListPage {
        page: page.titled(format!("Issues · {full_name}")),
        header: loaded.header(&state, "issues"),
        rows: IssueRow::list(&results, ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str(), &list),
        menus: filter_menus(&clean, &list, &choices, viewer.user()),
        query: typed,
        hints: results.hints,
        open_url: list_url(&list, &clean.with_state(StateFilter::Open)),
        closed_url: list_url(&list, &clean.with_state(StateFilter::Closed)),
        open: results.open,
        closed: results.closed,
        showing_open: query.state == Some(StateFilter::Open),
        showing_closed: query.state == Some(StateFilter::Closed),
        pagination,
        new_url: format!("{list}/new"),
        labels_url: format!("{}/labels", loaded.urls.home()),
        milestones_url: format!("{}/milestones", loaded.urls.home()),
        is_filtered: clean.to_string() != DEFAULT_QUERY,
        list_url: list,
    })
}

// ------------------------------------------------------------------ creating

/// What the new-issue form shows (and re-shows after an error).
struct Draft {
    title: String,
    body: String,
    labels: Vec<i64>,
    assignees: Vec<i64>,
    milestone: Option<i64>,
}

async fn new_page(state: &AppState, loaded: &RepoPage, page: Page, draft: Draft, error: FormError) -> WebResult<NewIssuePage> {
    let ctx = &loaded.ctx;
    let can_triage = ctx.access.can_write();
    let (labels, people, milestones) = if can_triage {
        let issues = &state.services.issues;
        tokio::try_join!(issues.labels(ctx), issues.assignable(ctx, ""), issues.milestones(ctx, Some(rbit_core::IssueState::Open)))?
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    let list = format!("{}/issues", loaded.urls.home());
    Ok(NewIssuePage {
        page: page.titled(format!("New issue · {}", ctx.repo.full_name())),
        header: loaded.header(state, "issues"),
        error,
        title: draft.title,
        body: draft.body,
        action_url: list.clone(),
        cancel_url: list,
        can_triage,
        labels: labels
            .iter()
            .map(|l| Choice {
                id: l.id.0,
                label: l.name.clone(),
                color: Some(safe_color(&l.color)),
                selected: draft.labels.contains(&l.id.0),
            })
            .collect(),
        assignees: people
            .iter()
            .map(|p| Choice { id: p.id.0, label: p.name.to_string(), color: None, selected: draft.assignees.contains(&p.id.0) })
            .collect(),
        milestones: milestones
            .iter()
            .map(|m| Choice {
                id: m.milestone.id.0,
                label: m.milestone.title.clone(),
                color: None,
                selected: draft.milestone == Some(m.milestone.id.0),
            })
            .collect(),
    })
}

/// `GET /{owner}/{repo}/issues/new`
async fn new_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<NewIssuePage> {
    let viewer = Viewer(Some(user));
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let draft = Draft { title: String::new(), body: String::new(), labels: Vec::new(), assignees: Vec::new(), milestone: None };
    new_page(&state, &loaded, page, draft, FormError::default()).await
}

/// `POST /{owner}/{repo}/issues`: opens an issue and shows it.
async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Response> {
    let viewer = Viewer(Some(user.clone()));
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let draft = Draft {
        title: value(&pairs, "title").unwrap_or_default().to_owned(),
        body: value(&pairs, "body").unwrap_or_default().to_owned(),
        labels: ids(values(&pairs, "label"))?,
        assignees: ids(values(&pairs, "assignee"))?,
        milestone: ids(value(&pairs, "milestone").into_iter())?.first().copied(),
    };
    let input = NewIssueInput {
        title: draft.title.clone(),
        body: draft.body.clone(),
        labels: draft.labels.iter().copied().map(LabelId).collect(),
        assignees: draft.assignees.iter().copied().map(AccountId).collect(),
        milestone: draft.milestone.map(MilestoneId),
    };
    match state.services.issues.create(&user, &loaded.ctx, input).await {
        Ok(issue) => {
            let repo = &loaded.ctx.repo;
            Ok(Redirect::to(&issue_url(repo.owner.name.as_str(), repo.repo.name.as_str(), issue.kind, issue.number)).into_response())
        }
        Err(ServiceError::Invalid(message)) => {
            Ok(new_page(&state, &loaded, page, draft, FormError::from_message(&message)).await?.into_response())
        }
        Err(error) => Err(error.into()),
    }
}

// ------------------------------------------------------------------ the page

#[derive(Debug, Deserialize)]
pub struct ShowQuery {
    edit: Option<String>,
    after: Option<String>,
}

/// `GET /{owner}/{repo}/issues/{n}[?edit=…&after=…]`: pull requests move to `/pull/{n}`.
/// Viewing marks the viewer's notification about it read, so a signed-in viewer's
/// preload is declined (see [`crate::middleware::decline_preload`]).
async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    headers: HeaderMap,
    mut page: Page,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Query(params): Query<ShowQuery>,
) -> WebResult<Response> {
    if viewer.user().is_some() && crate::middleware::is_preload(&headers) {
        return Ok(crate::middleware::decline_preload());
    }
    let number = parse_number(&number)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let ctx = state.services.timeline.open(viewer.user(), loaded.ctx.clone(), number).await?;
    if ctx.issue.is_pull() {
        return Ok(super::timeline::moved_to_kind(&ThreadUrls::for_context(&ctx).page));
    }
    let after = params.after.as_deref().and_then(TimelineCursor::parse);
    let options = TimelineOptions { edit: params.edit.as_deref(), ..TimelineOptions::default() };
    let (thread_view, sidebar) =
        tokio::try_join!(thread(&state, &ctx, viewer.user(), after, options), sidebar(&state, &ctx, viewer.user()))?;
    if let Some(user) = viewer.user() {
        state.services.notifications.mark_thread_read(user.id(), ctx.issue.id).await;
        if page.has_unread_notifications {
            page.has_unread_notifications = state.services.notifications.has_unread(user.id()).await.unwrap_or(false);
        }
    }
    let list = format!("{}/issues", loaded.urls.home());
    Ok(IssuePage {
        page: page.titled(format!("{} {} · {}", ctx.issue.title, ctx.issue.number, ctx.repo.repo.full_name())),
        header: loaded.header(&state, "issues"),
        title: thread_view.title.clone(),
        badge: thread_view.badge,
        summary: ThreadSummary::new(&ctx),
        thread: thread_view,
        sidebar,
        list_url: list,
    }
    .into_response())
}

// ------------------------------------------------------------------ sidebar forms

/// `POST /{owner}/{repo}/issues/{n}/labels`: the thread's labels become exactly the
/// checked `label`s.
async fn set_labels(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    let labels: Vec<LabelId> = ids(values(&pairs, "label"))?.into_iter().map(LabelId).collect();
    state.services.issues.set_labels(&user, &ctx, &labels).await?;
    Ok(back(&ctx, NEW_COMMENT_ANCHOR))
}

/// `POST /{owner}/{repo}/issues/{n}/assignees`: with `replace=true` the checked
/// `assignee`s become the assignees; otherwise `add` and `remove` are applied.
async fn set_assignees(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    let issues = &state.services.issues;
    let accounts = |key| -> WebResult<Vec<AccountId>> { Ok(ids(values(&pairs, key))?.into_iter().map(AccountId).collect()) };
    if value(&pairs, "replace") == Some("true") {
        issues.set_assignees(&user, &ctx, &accounts("assignee")?).await?;
    } else {
        issues.change_assignees(&user, &ctx, &accounts("add")?, &accounts("remove")?).await?;
    }
    Ok(back(&ctx, NEW_COMMENT_ANCHOR))
}

/// `POST /{owner}/{repo}/issues/{n}/milestone`: `milestone={id}`, or empty for none.
async fn set_milestone(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    let milestone = ids(value(&pairs, "milestone").into_iter())?.first().copied().map(MilestoneId);
    state.services.issues.set_milestone(&user, &ctx, milestone).await?;
    Ok(back(&ctx, NEW_COMMENT_ANCHOR))
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionForm {
    #[serde(default)]
    subscribed: String,
}

/// `POST /{owner}/{repo}/issues/{n}/subscription`: `subscribed=true|false`.
async fn set_subscription(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<SubscriptionForm>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    let notifications = &state.services.notifications;
    if !notifications.enabled() {
        return Err(WebError::NotFound);
    }
    let subscribed = match form.subscribed.as_str() {
        "true" => true,
        "false" => false,
        _ => return Err(WebError::BadRequest("subscribed must be true or false".to_owned())),
    };
    notifications.set_subscribed(user.id(), ctx.issue.id, subscribed).await?;
    Ok(back(&ctx, NEW_COMMENT_ANCHOR))
}

/// `POST /{owner}/{repo}/issues/{n}/delete`: repository administrators delete an issue.
async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let ctx = open(&state, &user, &owner, &repo, &number).await?;
    state.services.issues.delete(&user, &ctx).await?;
    let repo = &ctx.repo.repo;
    Ok(Redirect::to(&format!("/{}/issues", repo.full_name())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_values_and_ids_are_strict() {
        let pairs = vec![("label".to_owned(), "3".to_owned()), ("x".to_owned(), "y".to_owned()), ("label".to_owned(), "5".to_owned())];
        assert_eq!(ids(values(&pairs, "label")).unwrap(), [3, 5]);
        assert_eq!(value(&pairs, "x"), Some("y"));
        assert_eq!(ids(std::iter::once("")).unwrap(), Vec::<i64>::new(), "an empty choice is none");
        for bad in ["0", "-2", "abc"] {
            assert!(matches!(ids(std::iter::once(bad)), Err(WebError::BadRequest(_))), "{bad}");
        }
        assert!(matches!(parse_number("012"), Err(WebError::NotFound)));
    }
}
