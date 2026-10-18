//! Pull request pages: `/{owner}/{repo}/pulls`, `/pulls/new`, `/pull/{n}` with its
//! Commits and Files changed tabs, review forms and the merge box (an issue number
//! answers 301 to `/issues/{n}`).
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository pages load the repository with
//! `super::repo::RepoPage::load` and build the header with `loaded.header(&state, tab)`.
//! Hidden repositories and unknown or malformed numbers are 404; forms answer `303` back
//! to the page with an anchor; refused actions are 403 (may not) or 400 (cannot now,
//! with the reason). Title, description, comments, state and lock use T0's routes under
//! `/issues/{n}/…`.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{
    IssueNumber, IssueState, ReviewId, User,
    issue::MergeStrategy,
    pull::{HeadSpec, ReviewCommentId, ReviewSide},
};
use rbit_git::DiffLimits;
use rbit_services::{
    RepoContext, ServiceError,
    pulls::{LineComment, MergeFailure, MergeInput, NewPullInput, PullContext, PullFilter, PullStateFilter, ReviewData, ReviewVerdict},
    timeline::{IssueContext, TimelineCursor},
};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    render::references::RefContext,
    urls::{DocumentBase, RepoUrls, issue_url},
    views::{
        commit::FileDiffView,
        components::Pagination,
        layout::Page,
        pulls::{
            ClosedBoxView, LabelView, ListTabs, MergeBoxView, NewPullPage, PullCommitsPage, PullCountsFragment, PullCountsView,
            PullFileView, PullFilesPage, PullHeaderView, PullListPage, PullPage, PullRowView, PullUrls, ReviewCommentView, ReviewFormView,
            ReviewView, ReviewersView, files_at, interleave, line_anchor, review_anchor,
        },
        repo::CommitView,
        timeline::{TimelineOptions, thread},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/pulls", get(list).post(create))
        .route("/{owner}/{repo}/pulls/new", get(new_page))
        .route("/{owner}/{repo}/pull/{number}", get(conversation))
        .route("/{owner}/{repo}/pull/{number}/commits", get(commits))
        .route("/{owner}/{repo}/pull/{number}/files", get(files))
        .route("/{owner}/{repo}/pull/{number}/counts/{spec}", get(counts))
        .route("/{owner}/{repo}/pull/{number}/merge", post(merge))
        .route("/{owner}/{repo}/pull/{number}/draft", post(draft))
        .route("/{owner}/{repo}/pull/{number}/base", post(change_base))
        .route("/{owner}/{repo}/pull/{number}/comments", post(line_comment))
        .route("/{owner}/{repo}/pull/{number}/reviews", post(submit_review))
        .route("/{owner}/{repo}/pull/{number}/reviews/discard", post(discard_review))
        .route("/{owner}/{repo}/pull/{number}/reviews/{id}/dismiss", post(dismiss_review))
        .route("/{owner}/{repo}/pull/{number}/reviewers", post(request_reviewers))
        .route("/{owner}/{repo}/pull/{number}/reviewers/remove", post(remove_reviewer))
        .route("/{owner}/{repo}/pull/{number}/branch/delete", post(delete_branch))
}

// ------------------------------------------------------------------ loading

/// What opening a `/pull/{n}` page found.
enum Opened {
    Pull(Box<PullContext>),
    /// The number is an issue: redirect there.
    Issue(String),
}

async fn open_thread(state: &AppState, viewer: Option<&User>, repo: RepoContext, number: &str) -> WebResult<Opened> {
    let number = IssueNumber::parse(number).map_err(|_| WebError::NotFound)?;
    let thread: IssueContext = state.services.timeline.open(viewer, repo, number).await?;
    if !thread.issue.is_pull() {
        let repo = &thread.repo.repo;
        return Ok(Opened::Issue(issue_url(repo.owner.name.as_str(), repo.repo.name.as_str(), thread.issue.kind, number)));
    }
    Ok(Opened::Pull(Box::new(state.services.pulls.context(thread).await?)))
}

/// Opens pull request `number` for a form post (an unpinned reader: merges write).
async fn open_for_post(state: &AppState, user: &User, owner: &str, repo: &str, number: &str) -> WebResult<PullContext> {
    let repo = state.services.repositories.open(Some(user), owner, repo).await?;
    match open_thread(state, Some(user), repo, number).await? {
        Opened::Pull(ctx) => Ok(*ctx),
        Opened::Issue(_) => Err(WebError::NotFound),
    }
}

fn ref_context(ctx: &PullContext, refs: rbit_services::references::ReferenceSet) -> RefContext {
    let repo = &ctx.repo().repo;
    let urls = RepoUrls::new(repo.owner.name.as_str(), repo.repo.name.as_str());
    RefContext {
        base: DocumentBase::new(urls, repo.repo.default_branch.clone(), ""),
        owner: repo.owner.name.to_string(),
        repo: repo.repo.name.to_string(),
        refs,
    }
}

/// The reviews of `ctx` as views, with references resolved in one call.
async fn review_views(
    state: &AppState,
    ctx: &PullContext,
    data: &ReviewData,
    viewer: Option<&User>,
) -> WebResult<(Vec<(i64, ReviewView)>, Vec<ReviewCommentView>)> {
    let mut texts: Vec<&str> = data.reviews.iter().map(|entry| entry.review.body.as_str()).collect();
    texts.extend(data.visible_comments().map(|(comment, _)| comment.body.as_str()));
    let refs = state.services.references.resolve(ctx.repo(), &texts).await?;
    let refs = ref_context(ctx, refs);
    let urls = PullUrls::for_context(ctx);
    let can_dismiss = viewer.is_some() && ctx.repo().access.can_write();
    let reviews = data
        .reviews
        .iter()
        .map(|entry| {
            let at = entry.review.submitted_at.unwrap_or(entry.review.created_at).seconds();
            (at, ReviewView::new(entry, can_dismiss, &urls, &refs))
        })
        .collect();
    let mut comments: Vec<ReviewCommentView> = Vec::new();
    for entry in &data.reviews {
        comments
            .extend(entry.comments.iter().map(|(comment, author)| ReviewCommentView::new(comment, author.as_ref(), false, &urls, &refs)));
    }
    if let Some(pending) = &data.pending {
        comments
            .extend(pending.comments.iter().map(|(comment, author)| ReviewCommentView::new(comment, author.as_ref(), true, &urls, &refs)));
    }
    Ok((reviews, comments))
}

fn review_form(ctx: &PullContext, data: &ReviewData, viewer: Option<&User>) -> Option<ReviewFormView> {
    let viewer = viewer?;
    let can_comment = ctx.thread.permissions.can_comment;
    let pending_comments = data.pending.as_ref().map_or(0, |entry| entry.comments.len());
    (can_comment && (ctx.issue().is_open() || pending_comments > 0)).then(|| {
        let urls = PullUrls::for_context(ctx);
        ReviewFormView {
            action_url: urls.reviews,
            discard_url: urls.discard_review,
            pending_comments,
            can_decide: ctx.issue().is_open() && !ctx.issue().is_author(viewer),
        }
    })
}

/// The header of a tab that does not diff (Conversation, Commits): the counts a Files
/// changed tab remembered for the compared commits, or slots that [`counts`] fills in
/// after the page has painted. `ahead` is what the tab already knows.
async fn header_without_diff(state: &AppState, ctx: &PullContext, active: &'static str, ahead: Option<usize>) -> PullHeaderView {
    let pulls = &state.services.pulls;
    let (counts, counts_url) = match pulls.compared_ids(ctx).await {
        Ok((base, head)) => {
            if let Some(counts) = pulls.cached_counts(&base, &head).await {
                (PullCountsView::known(counts.ahead, counts.files), None)
            } else {
                let url = format!("{}/counts/{base}...{head}", PullUrls::for_context(ctx).page);
                (PullCountsView { commits: ahead, files: None }, Some(url))
            }
        }
        Err(error) => {
            tracing::debug!(%error, "cannot compare the pull request");
            (PullCountsView::known(ahead.unwrap_or(0), 0), None)
        }
    };
    PullHeaderView::new(ctx, counts, counts_url, active)
}

fn title(ctx: &PullContext, suffix: &str) -> String {
    format!("{}{suffix} · Pull request {} · {}", ctx.issue().title, ctx.issue().number, ctx.repo().repo.full_name())
}

// ------------------------------------------------------------------ list

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    state: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    page: Option<u32>,
}

/// `GET /{owner}/{repo}/pulls?state=open|closed|merged&author=&page=`
async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> WebResult<PullListPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (filter_state, state_name) = match query.state.as_str() {
        "closed" => (PullStateFilter::Closed, "closed"),
        "merged" => (PullStateFilter::Merged, "merged"),
        _ => (PullStateFilter::Open, "open"),
    };
    let author_name = query.author.trim();
    let author = if author_name.is_empty() { None } else { state.services.accounts.find_account(author_name).await.ok() };
    if !author_name.is_empty() && author.is_none() {
        // An unknown author matches nothing.
        return Ok(list_page(
            &state,
            &loaded,
            page,
            Vec::new(),
            ListTabs { open_url: String::new(), closed_url: String::new(), open: 0, closed: 0, state: state_name },
            Pagination { page: 1, page_count: 1, previous_url: None, next_url: None },
            Some(author_name.to_owned()),
        ));
    }
    let filter = PullFilter { state: filter_state, author: author.as_ref().map(|account| account.id), ..PullFilter::default() };
    let number = query.page.unwrap_or(1).max(1);
    let listed = state.services.pulls.list(&loaded.ctx, &filter, number).await?;
    let ids: Vec<_> = listed.items.iter().map(|item| item.issue.id).collect();
    let metadata = state.services.issues.metadata(&ids).await?;
    let rows = listed
        .items
        .iter()
        .map(|item| {
            let labels = metadata
                .get(&item.issue.id)
                .map(|meta| {
                    meta.labels.iter().map(|label| LabelView { name: label.name.clone(), color: valid_color(&label.color) }).collect()
                })
                .unwrap_or_default();
            PullRowView::new(item, &owner_name(&loaded), &repo_name(&loaded), labels)
        })
        .collect();
    let base = format!("{}/pulls", loaded.urls.home());
    let author_query = author.as_ref().map(|account| format!("&author={}", account.name)).unwrap_or_default();
    let page_url = |n: u32| format!("{base}?state={state_name}{author_query}&page={n}");
    let page_count = u32::try_from(listed.total.div_ceil(u64::from(rbit_services::pulls::PAGE_SIZE))).unwrap_or(u32::MAX).max(1);
    let pagination = Pagination {
        page: number,
        page_count,
        previous_url: (number > 1).then(|| page_url(number - 1)),
        next_url: (number < page_count).then(|| page_url(number + 1)),
    };
    let tabs = ListTabs {
        open_url: format!("{base}?state=open{author_query}"),
        closed_url: format!("{base}?state=closed{author_query}"),
        open: listed.open,
        closed: listed.closed,
        state: if state_name == "merged" { "closed" } else { state_name },
    };
    Ok(list_page(&state, &loaded, page, rows, tabs, pagination, author.map(|account| account.name.to_string())))
}

fn list_page(
    state: &AppState,
    loaded: &RepoPage,
    page: Page,
    rows: Vec<PullRowView>,
    tabs: ListTabs,
    pagination: Pagination,
    author_filter: Option<String>,
) -> PullListPage {
    PullListPage {
        page: page.titled(format!("Pull requests · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "pulls"),
        tabs,
        rows,
        pagination,
        new_url: format!("{}/compare", loaded.urls.home()),
        author_filter,
    }
}

fn owner_name(loaded: &RepoPage) -> String {
    loaded.ctx.repo.owner.name.to_string()
}

fn repo_name(loaded: &RepoPage) -> String {
    loaded.ctx.repo.repo.name.to_string()
}

/// `#rrggbb`, or a neutral grey for anything else (it lands in an SVG `fill`).
fn valid_color(color: &str) -> String {
    let hex = color.strip_prefix('#').unwrap_or_default();
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) { color.to_owned() } else { "#888888".to_owned() }
}

// ------------------------------------------------------------------ new

#[derive(Debug, Deserialize)]
pub struct NewQuery {
    #[serde(default)]
    base: String,
    #[serde(default)]
    head: String,
}

/// `GET /{owner}/{repo}/pulls/new?base=…&head=…`: the comparison and the form.
async fn new_page(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<NewQuery>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let base = if query.base.trim().is_empty() { loaded.default_branch().to_owned() } else { query.base.trim().to_owned() };
    let head = query.head.trim().to_owned();
    if head.is_empty() {
        return Ok(Redirect::to(&format!("{}/compare", loaded.urls.home())).into_response());
    }
    let spec = HeadSpec::parse(&head).ok_or(WebError::NotFound)?;
    let pulls = &state.services.pulls;
    let head_ctx = pulls.resolve_head(viewer.user(), &loaded.ctx, &spec).await?;
    let head_full_name = head_ctx.repo.full_name();
    let picker = super::compare::pickers(&state, &viewer, &loaded, &base, &spec.branch, &head_full_name).await;
    let mut view = NewPullPage {
        page: page.titled(format!("Open a pull request · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "pulls"),
        picker,
        action_url: format!("{}/pulls", loaded.urls.home()),
        base: base.clone(),
        head: head.clone(),
        title: spec.branch.clone(),
        body: String::new(),
        problem: None,
        can_create: false,
        signed_in: viewer.user().is_some(),
        sign_in_url: format!("/login?next={}", crate::views::pulls::new_pull_url(&owner_name(&loaded), &repo_name(&loaded), &base, &head)),
        commits: Vec::new(),
        files: Vec::new(),
        additions: 0,
        deletions: 0,
        truncated: false,
    };
    let head_urls = RepoUrls::new(head_ctx.repo.owner.name.as_str(), head_ctx.repo.repo.name.as_str());
    match pulls
        .compare(&loaded.ctx, &format!("refs/heads/{base}"), &head_ctx, &format!("refs/heads/{}", spec.branch), DiffLimits::default(), 100)
        .await
    {
        Ok(comparison) if comparison.merge_base.is_none() => {
            view.problem = Some("These branches have entirely different histories.".to_owned());
        }
        Ok(comparison) if comparison.ahead == 0 => view.problem = Some(format!("{base} is up to date with all commits from {head}.")),
        Ok(comparison) => {
            if comparison.ahead == 1 {
                view.title = comparison.commits.first().map_or_else(|| spec.branch.clone(), |commit| commit.summary.clone());
            }
            view.commits = comparison.commits.iter().map(|commit| CommitView::new(commit, &head_urls)).collect();
            view.files = FileDiffView::list(&comparison.files, &comparison.head.id, &head_urls);
            view.additions = comparison.additions();
            view.deletions = comparison.deletions();
            view.truncated = comparison.truncated;
            view.can_create = viewer.user().is_some();
        }
        Err(ServiceError::Internal(source)) => return Err(WebError::Internal(source)),
        Err(_) => view.problem = Some("Choose two existing branches to compare.".to_owned()),
    }
    Ok(view.into_response())
}

#[derive(Debug, Deserialize)]
pub struct CreateForm {
    #[serde(default)]
    base: String,
    #[serde(default)]
    head: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: Option<String>,
}

/// `POST /{owner}/{repo}/pulls`: opens the pull request; `303` to its page.
async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<CreateForm>,
) -> WebResult<Redirect> {
    let base = state.services.repositories.open(Some(&user), &owner, &repo).await?;
    let spec = HeadSpec::parse(&form.head).ok_or_else(|| WebError::BadRequest("choose a branch to propose".to_owned()))?;
    let pulls = &state.services.pulls;
    let head = pulls.resolve_head(Some(&user), &base, &spec).await?;
    let input =
        NewPullInput { base_branch: form.base, head_branch: spec.branch, title: form.title, body: form.body, draft: form.draft.is_some() };
    let (issue, _) = pulls.create(&user, &base, &head, input).await?;
    Ok(Redirect::to(&PullUrls::new(&owner_of(&base), &name_of(&base), issue.number.get()).page))
}

fn owner_of(repo: &RepoContext) -> String {
    repo.repo.owner.name.to_string()
}

fn name_of(repo: &RepoContext) -> String {
    repo.repo.repo.name.to_string()
}

// ------------------------------------------------------------------ the pull request pages

#[derive(Debug, Deserialize)]
pub struct ConversationQuery {
    edit: Option<String>,
    after: Option<String>,
}

/// `GET /{owner}/{repo}/pull/{n}`: the conversation, reviews and the merge box. Viewing
/// marks the viewer's notification about it read, so a signed-in viewer's preload is
/// declined (see [`crate::middleware::decline_preload`]).
async fn conversation(
    State(state): State<AppState>,
    viewer: Viewer,
    headers: HeaderMap,
    mut page: Page,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Query(query): Query<ConversationQuery>,
) -> WebResult<Response> {
    if viewer.user().is_some() && crate::middleware::is_preload(&headers) {
        return Ok(crate::middleware::decline_preload());
    }
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let ctx = match open_thread(&state, viewer.user(), loaded.ctx.clone(), &number).await? {
        Opened::Pull(ctx) => *ctx,
        Opened::Issue(url) => return Ok(super::timeline::moved_to_kind(&url)),
    };
    let user = viewer.user();
    let after = query.after.as_deref().and_then(TimelineCursor::parse);
    let options =
        TimelineOptions { edit: query.edit.as_deref(), head_branch: Some(&ctx.pull.head_branch), base_branch: Some(&ctx.pull.base_branch) };
    let pulls = &state.services.pulls;
    let (thread, data, pull_header) = tokio::join!(
        thread(&state, &ctx.thread, user, after, options),
        pulls.reviews(user, &ctx),
        header_without_diff(&state, &ctx, "conversation", None),
    );
    let (thread, data) = (thread?, data?);
    let (reviews, _) = review_views(&state, &ctx, &data, user).await?;
    let urls = PullUrls::for_context(&ctx);
    let is_open = ctx.issue().state == IssueState::Open;
    let merge_box = if is_open { Some(MergeBoxView::new(&pulls.merge_box(user, &ctx).await?, &ctx, &urls, user)) } else { None };
    let closed_box = if is_open {
        None
    } else {
        let head_exists = pulls.head_branch_exists(&ctx).await;
        let can_write_head = match user {
            Some(user) => pulls.may_delete_head(user, &ctx).await?,
            None => false,
        };
        let repo_urls = RepoUrls::new(&owner, &repo);
        Some(ClosedBoxView {
            merged: ctx.pull.is_merged(),
            merge_commit: ctx.pull.merge_commit_sha.as_ref().map(|sha| (sha.chars().take(7).collect(), repo_urls.commit(sha))),
            can_delete_branch: head_exists && can_write_head,
            delete_url: urls.delete_branch.clone(),
            head_label: ctx.head_label(),
        })
    };
    let permissions = ctx.thread.permissions;
    let can_request =
        user.is_some() && (permissions.can_triage || user.is_some_and(|user| ctx.issue().is_author(user) && permissions.can_comment));
    let reviewers = ReviewersView::new(&data, can_request && is_open, &urls);
    let review_form = review_form(&ctx, &data, user);
    let can_convert_to_draft =
        is_open && !ctx.pull.is_draft && user.is_some_and(|user| ctx.issue().is_author(user) || ctx.repo().access.can_write());
    if let Some(user) = user {
        state.services.notifications.mark_thread_read(user.id(), ctx.issue().id).await;
        if page.has_unread_notifications {
            page.has_unread_notifications = state.services.notifications.has_unread(user.id()).await.unwrap_or(false);
        }
    }
    let sidebar = crate::views::issues::sidebar(&state, &ctx.thread, user).await?;
    let entries = interleave(thread.items.clone(), reviews);
    let view = PullPage {
        page: page.titled(title(&ctx, "")),
        header: loaded.header(&state, "pulls"),
        pull: pull_header,
        thread,
        entries,
        merge_box,
        closed_box,
        review_form,
        reviewers,
        sidebar,
        can_convert_to_draft,
    };
    Ok(view.into_response())
}

/// `GET /{owner}/{repo}/pull/{n}/commits`
async fn commits(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, number)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let ctx = match open_thread(&state, viewer.user(), loaded.ctx.clone(), &number).await? {
        Opened::Pull(ctx) => *ctx,
        Opened::Issue(url) => return Ok(super::timeline::moved_to_kind(&url)),
    };
    let changes = state.services.pulls.changes(&ctx, DiffLimits { max_files: 0, ..DiffLimits::default() }).await.ok();
    let pull_header = header_without_diff(&state, &ctx, "commits", changes.as_ref().map(|c| c.ahead)).await;
    let commits: Vec<CommitView> =
        changes.as_ref().map(|c| c.commits.iter().map(|commit| CommitView::new(commit, &loaded.urls)).collect()).unwrap_or_default();
    let more_commits = changes.as_ref().is_some_and(|c| c.ahead > c.commits.len());
    let view = PullCommitsPage {
        page: page.titled(title(&ctx, " · Commits")),
        header: loaded.header(&state, "pulls"),
        pull: pull_header,
        commits,
        more_commits,
    };
    Ok(view.into_response())
}

#[derive(Debug, Deserialize)]
pub struct FilesQuery {
    /// A line anchor: show the inline comment form there.
    comment: Option<String>,
}

/// `GET /{owner}/{repo}/pull/{n}/files[?comment=<anchor>]`
async fn files(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Query(query): Query<FilesQuery>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let ctx = match open_thread(&state, viewer.user(), loaded.ctx.clone(), &number).await? {
        Opened::Pull(ctx) => *ctx,
        Opened::Issue(url) => return Ok(super::timeline::moved_to_kind(&url)),
    };
    let user = viewer.user();
    let pulls = &state.services.pulls;
    // The diff and the reviews are independent; a failed diff shows an empty tab.
    let (changes, reviews) = tokio::join!(pulls.changes(&ctx, DiffLimits::default()), async {
        let data = pulls.reviews(user, &ctx).await?;
        let (_, comments) = review_views(&state, &ctx, &data, user).await?;
        WebResult::Ok((data, comments))
    });
    let changes = changes.ok();
    let (data, comments) = reviews?;
    let counts = changes.as_ref().map_or_else(|| PullCountsView::known(0, 0), |c| PullCountsView::known(c.ahead, c.files.len()));
    let urls = PullUrls::for_context(&ctx);
    let can_comment = user.is_some() && ctx.thread.permissions.can_comment;
    let (files, additions, deletions, truncated) = match &changes {
        Some(changes) => (
            PullFileView::list(&changes.files, &changes.head.id, &loaded.urls, &urls, &comments, query.comment.as_deref(), can_comment),
            changes.additions(),
            changes.deletions(),
            changes.truncated,
        ),
        None => (Vec::new(), 0, 0, false),
    };
    let view = PullFilesPage {
        page: page.titled(title(&ctx, " · Files changed")),
        header: loaded.header(&state, "pulls"),
        pull: PullHeaderView::new(&ctx, counts, None, "files"),
        files,
        additions,
        deletions,
        truncated,
        can_comment,
        signed_in: user.is_some(),
        sign_in_url: format!("/login?next={}", urls.files),
        review_form: review_form(&ctx, &data, user),
        outdated_count: comments.iter().filter(|comment| comment.outdated).count(),
    };
    Ok(view.into_response())
}

/// `GET /{owner}/{repo}/pull/{n}/counts/{base}...{head}`: the header counts of comparing
/// two commit ids, as out-of-band swaps into the slots of the Conversation and Commits
/// tabs. The ids pin the answer, so it never changes. Only the pair the pull request
/// compares right now answers; any other is 404 (and an outdated slot keeps its no-JS
/// state).
async fn counts(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, number, spec)): Path<(String, String, String, String)>,
) -> WebResult<Response> {
    let (base, head) = spec
        .split_once("...")
        .filter(|(base, head)| super::history::is_full_commit_id(base) && super::history::is_full_commit_id(head))
        .ok_or(WebError::NotFound)?;
    let repo = super::repo::open_pinned(&state, &viewer, &owner, &repo).await?;
    let Opened::Pull(ctx) = open_thread(&state, viewer.user(), repo, &number).await? else {
        return Err(WebError::NotFound);
    };
    let counts = state.services.pulls.counts_between(&ctx, base, head).await.map_err(|error| match error {
        ServiceError::Internal(_) => WebError::from(error),
        _ => WebError::NotFound,
    })?;
    let public = ctx.repo().repo.repo.visibility.is_public();
    Ok(crate::middleware::cache_pinned(
        PullCountsFragment { counts: PullCountsView::known(counts.ahead, counts.files) }.into_response(),
        public,
    ))
}

// ------------------------------------------------------------------ forms

#[derive(Debug, Deserialize)]
pub struct MergeForm {
    #[serde(default)]
    strategy: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    expected_head: String,
    #[serde(default)]
    delete_branch: Option<String>,
    #[serde(default)]
    bypass: Option<String>,
}

/// `POST /{owner}/{repo}/pull/{n}/merge`
async fn merge(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<MergeForm>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    let strategy = MergeStrategy::parse(&form.strategy).ok_or_else(|| WebError::BadRequest("choose a merge method".to_owned()))?;
    let input = MergeInput {
        strategy,
        title: form.title,
        message: form.message,
        expected_head: Some(form.expected_head).filter(|head| !head.is_empty()),
        delete_head_branch: form.delete_branch.is_some(),
        bypass: form.bypass.is_some(),
    };
    match state.services.pulls.merge(&user, &ctx, input).await {
        Ok(_) => Ok(Redirect::to(&PullUrls::for_context(&ctx).page)),
        Err(MergeFailure::Service(error)) => Err(error.into()),
        Err(failure) => Err(WebError::BadRequest(failure.to_string())),
    }
}

#[derive(Debug, Deserialize)]
pub struct DraftForm {
    #[serde(default)]
    draft: String,
}

/// `POST /{owner}/{repo}/pull/{n}/draft`: `draft=true` converts to a draft, `false` marks ready.
async fn draft(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<DraftForm>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    let draft = match form.draft.as_str() {
        "true" => true,
        "false" => false,
        _ => return Err(WebError::BadRequest("draft must be true or false".to_owned())),
    };
    state.services.pulls.set_draft(&user, &ctx, draft).await?;
    Ok(Redirect::to(&format!("{}#merge-box", PullUrls::for_context(&ctx).page)))
}

#[derive(Debug, Deserialize)]
pub struct BaseForm {
    #[serde(default)]
    base: String,
}

/// `POST /{owner}/{repo}/pull/{n}/base`: points the pull request at another base branch.
async fn change_base(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<BaseForm>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    state.services.pulls.set_base(&user, &ctx, &form.base).await?;
    Ok(Redirect::to(&PullUrls::for_context(&ctx).page))
}

#[derive(Debug, Deserialize)]
pub struct LineCommentForm {
    #[serde(default)]
    path: String,
    #[serde(default)]
    side: String,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    in_reply_to: Option<i64>,
    /// `single` (post now) or `review` (add to the pending review).
    #[serde(default)]
    action: String,
}

/// `POST /{owner}/{repo}/pull/{n}/comments`: a comment on a diff line, or a reply.
async fn line_comment(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<LineCommentForm>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    let in_reply_to = form.in_reply_to.filter(|id| *id > 0).map(ReviewCommentId);
    let side = match (form.side.as_str(), in_reply_to) {
        ("left", None) => ReviewSide::Left,
        (_, Some(_)) | ("right", None) => ReviewSide::Right,
        _ => return Err(WebError::BadRequest("unknown diff side".to_owned())),
    };
    let line = match (form.line, in_reply_to) {
        (_, Some(_)) => 1,
        (Some(line), None) if line > 0 => line,
        _ => return Err(WebError::BadRequest("choose a line to comment on".to_owned())),
    };
    let input = LineComment { path: form.path, side, line, body: form.body, in_reply_to, start_review: form.action == "review" };
    let comment = state.services.pulls.comment_on_line(&user, &ctx, input).await?;
    Ok(Redirect::to(&files_at(&PullUrls::for_context(&ctx), &line_anchor(&comment.path, comment.side, comment.line))))
}

#[derive(Debug, Deserialize)]
pub struct ReviewForm {
    #[serde(default)]
    verdict: String,
    #[serde(default)]
    body: String,
}

/// `POST /{owner}/{repo}/pull/{n}/reviews`: Comment, Approve or Request changes.
async fn submit_review(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<ReviewForm>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    let verdict = ReviewVerdict::parse(&form.verdict).ok_or_else(|| WebError::BadRequest("choose how to submit the review".to_owned()))?;
    let review = state.services.pulls.submit_review(&user, &ctx, verdict, &form.body).await?;
    Ok(Redirect::to(&format!("{}#{}", PullUrls::for_context(&ctx).page, review_anchor(review.id.0))))
}

/// `POST /{owner}/{repo}/pull/{n}/reviews/discard`: throws the pending review away.
async fn discard_review(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    state.services.pulls.discard_review(&user, &ctx).await?;
    Ok(Redirect::to(&PullUrls::for_context(&ctx).files))
}

#[derive(Debug, Deserialize)]
pub struct DismissForm {
    #[serde(default)]
    message: String,
}

/// `POST /{owner}/{repo}/pull/{n}/reviews/{id}/dismiss`
async fn dismiss_review(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number, id)): Path<(String, String, String, String)>,
    Form(form): Form<DismissForm>,
) -> WebResult<Redirect> {
    let id = id.parse::<i64>().ok().filter(|id| *id > 0).map(ReviewId).ok_or(WebError::NotFound)?;
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    state.services.pulls.dismiss_review(&user, &ctx, id, &form.message).await?;
    Ok(Redirect::to(&format!("{}#{}", PullUrls::for_context(&ctx).page, review_anchor(id.0))))
}

#[derive(Debug, Deserialize)]
pub struct ReviewersForm {
    /// Names separated by commas or spaces.
    #[serde(default)]
    reviewers: String,
    #[serde(default)]
    reviewer: String,
}

impl ReviewersForm {
    fn names(&self) -> Vec<String> {
        self.reviewers
            .split([',', ' '])
            .chain(std::iter::once(self.reviewer.as_str()))
            .map(|name| name.trim().trim_start_matches('@').to_owned())
            .filter(|name| !name.is_empty())
            .collect()
    }
}

/// `POST /{owner}/{repo}/pull/{n}/reviewers`
async fn request_reviewers(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<ReviewersForm>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    state.services.pulls.request_reviewers(&user, &ctx, &form.names()).await?;
    Ok(Redirect::to(&PullUrls::for_context(&ctx).page))
}

/// `POST /{owner}/{repo}/pull/{n}/reviewers/remove`
async fn remove_reviewer(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
    Form(form): Form<ReviewersForm>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    state.services.pulls.remove_reviewers(&user, &ctx, &form.names()).await?;
    Ok(Redirect::to(&PullUrls::for_context(&ctx).page))
}

/// `POST /{owner}/{repo}/pull/{n}/branch/delete`: deletes the head branch of a closed or
/// merged pull request.
async fn delete_branch(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, number)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let ctx = open_for_post(&state, &user, &owner, &repo, &number).await?;
    state.services.pulls.delete_head(&user, &ctx).await?;
    Ok(Redirect::to(&PullUrls::for_context(&ctx).page))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewer_names_and_colours_are_cleaned() {
        let form = ReviewersForm { reviewers: "@alice, bob  carol".to_owned(), reviewer: String::new() };
        assert_eq!(form.names(), ["alice", "bob", "carol"]);
        assert_eq!(valid_color("#a1b2c3"), "#a1b2c3");
        for bad in ["red", "#12345", "#zzzzzz", "url(x)"] {
            assert_eq!(valid_color(bad), "#888888");
        }
    }
}
