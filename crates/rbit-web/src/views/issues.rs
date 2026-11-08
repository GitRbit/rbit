//! Issue list, issue page and form view models, and the sidebar shared with pull
//! request pages.
//!
//! Milestone 2 workstream I (issues) owns this file and `templates/issues/**`.
//!
//! Hook (`docs/design/milestone-2b.md` §3): P's pull request page renders [`sidebar`]
//! with `templates/issues/sidebar.html` (which expects `sidebar: IssueSidebar`). The
//! sidebar's forms post to `/{owner}/{repo}/issues/{n}/…` for both kinds (the routes in
//! `routes/issues.rs` answer with a redirect to the canonical page).
//!
//! Label colours are user data: they appear only as a validated `#rrggbb` SVG `fill`
//! (`issues/label_chip.html`), never in a `style` attribute.

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{
    Account, AccountId, Issue, IssueKind, IssueState, StateReason, User,
    issue::LabelSnapshot,
    issue_query::{DEFAULT_QUERY, IssueQuery, IssueSort, KindFilter, StateFilter, Who},
};
use rbit_localization::{CollabSentence, IssueFilterMenu, IssueFilterMessage, IssueSidebarMessage, Locale, ThreadMessage};
use rbit_services::{
    ServiceError,
    issues::{IssueMetadata, IssueSearchResults, Label, MilestoneProgress},
    notifications::NotificationReason,
    timeline::IssueContext,
};

use crate::{
    AppState,
    urls::issue_url,
    views::{
        components::{Avatar, FormError, Pagination},
        layout::Page,
        repo::RepoHeader,
        timeline::{ActorView, StateBadge, ThreadView, TitleView},
    },
};

/// `#rrggbb` if `color` is exactly that, else a neutral grey (stored colours are checked
/// by the schema; this keeps the template safe whatever the data).
pub fn safe_color(color: &str) -> String {
    let valid = color.strip_prefix('#').is_some_and(|hex| hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()));
    if valid { color.to_ascii_lowercase() } else { "#888888".to_owned() }
}

/// `page` titled in its own language: `title` gets the page's locale.
pub fn titled(page: Page, title: impl FnOnce(Locale) -> String) -> Page {
    let title = title(page.locale);
    page.titled(title)
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

/// The list at `base` (`/{owner}/{repo}/issues`) for `query`; the default query is the
/// bare URL.
pub fn list_url(base: &str, query: &IssueQuery) -> String {
    let text = query.to_string();
    if text == DEFAULT_QUERY { base.to_owned() } else { format!("{base}?q={}", encode(&text)) }
}

// ------------------------------------------------------------------ pieces

/// A label: its name with its colour as a dot; `url` filters the list by it.
#[derive(Debug, Clone)]
pub struct LabelChip {
    pub name: String,
    /// A validated `#rrggbb`.
    pub color: String,
    pub url: Option<String>,
}

impl LabelChip {
    pub fn new(label: &LabelSnapshot, url: Option<String>) -> Self {
        Self { name: label.name.clone(), color: safe_color(&label.color), url }
    }
}

/// A person shown by avatar and name.
#[derive(Debug, Clone)]
pub struct PersonView {
    pub name: String,
    pub url: String,
    pub avatar: Avatar,
}

impl PersonView {
    pub fn new(account: &Account) -> Self {
        Self { name: account.name.to_string(), url: format!("/{}", account.name), avatar: Avatar::for_account(account) }
    }
}

/// The open / closed icon of a list row, coloured by status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateIcon {
    pub icon: &'static str,
    /// A token utility (`text-success` for open, `text-ink-muted` for closed).
    pub class: &'static str,
    pub label: &'static str,
    pub message: ThreadMessage,
}

impl StateIcon {
    pub fn for_issue(issue: &Issue) -> Self {
        let badge = StateBadge::for_issue(issue);
        let class = match (issue.state, issue.state_reason) {
            (IssueState::Open, _) => "text-success",
            (IssueState::Closed, Some(StateReason::Merged)) => "text-ink",
            (IssueState::Closed, _) => "text-ink-muted",
        };
        Self { icon: badge.icon, class, label: badge.label, message: badge.message() }
    }
}

/// One row of an issue list.
#[derive(Debug, Clone)]
pub struct IssueRow {
    pub title: String,
    pub url: String,
    /// `#12`.
    pub number: String,
    pub state: StateIcon,
    pub labels: Vec<LabelChip>,
    pub author: String,
    pub when: String,
    pub when_iso: String,
    pub closed: bool,
    pub comments: u32,
    pub comments_url: String,
    pub assignees: Vec<PersonView>,
    pub milestone: Option<(String, String)>,
    pub locked: bool,
}

impl IssueRow {
    /// The row of `issue` in `owner/repo`, whose list lives at `list` (label chips link
    /// back to it filtered by the label).
    pub fn new(issue: &Issue, author: Option<&Account>, metadata: Option<&IssueMetadata>, owner: &str, repo: &str, list: &str) -> Self {
        let url = issue_url(owner, repo, issue.kind, issue.number);
        let labels = metadata.map_or_else(Vec::new, |m| {
            m.labels
                .iter()
                .map(|label| {
                    let query = IssueQuery::parse(DEFAULT_QUERY).toggling_label(&label.name);
                    LabelChip::new(label, Some(list_url(list, &query)))
                })
                .collect()
        });
        let (closed, at) = match issue.closed_at {
            Some(closed_at) if !issue.is_open() => (true, closed_at),
            _ => (false, issue.created_at),
        };
        Self {
            title: issue.title.to_string(),
            comments_url: format!("{url}#new-comment"),
            url,
            number: issue.number.to_string(),
            state: StateIcon::for_issue(issue),
            labels,
            author: author.map_or_else(|| "ghost".to_owned(), |a| a.name.to_string()),
            when: at.relative(),
            when_iso: at.iso8601(),
            closed,
            comments: issue.comment_count,
            assignees: metadata.map_or_else(Vec::new, |m| m.assignees.iter().map(PersonView::new).collect()),
            milestone: metadata
                .and_then(|m| m.milestone.as_ref())
                .map(|m| (m.title.clone(), format!("/{owner}/{repo}/milestones/{}", m.id.0))),
            locked: issue.locked,
        }
    }

    /// "#12 opened … by alice" or "#12 closed …", the time in slot 0.
    pub fn meta(&self) -> CollabSentence<'_> {
        let number = &self.number;
        if self.closed { CollabSentence::RowClosed { number } } else { CollabSentence::RowOpened { number, author: &self.author } }
    }

    /// The assignees' names, for a `title`.
    pub fn assignee_names(&self) -> String {
        self.assignees.iter().map(|person| person.name.as_str()).collect::<Vec<_>>().join(", ")
    }

    /// The rows of one page of results.
    pub fn list(results: &IssueSearchResults, owner: &str, repo: &str, list: &str) -> Vec<Self> {
        results
            .issues
            .iter()
            .map(|issue| {
                let author = issue.author.and_then(|id| results.authors.get(&id));
                Self::new(issue, author, results.metadata.get(&issue.id), owner, repo, list)
            })
            .collect()
    }
}

/// One entry of a filter menu: a message, or a name (an account, a label, a milestone).
#[derive(Debug, Clone)]
pub struct MenuItem {
    /// The name; empty for a message.
    pub label: String,
    pub text: Option<IssueFilterMessage>,
    pub url: String,
    pub selected: bool,
    /// A label's colour (label menu only).
    pub color: Option<String>,
}

impl MenuItem {
    fn message(text: IssueFilterMessage, url: String, selected: bool) -> Self {
        Self { label: String::new(), text: Some(text), url, selected, color: None }
    }

    fn named(label: String, url: String, selected: bool) -> Self {
        Self { label, text: None, url, selected, color: None }
    }
}

/// A `<details>` menu above the list.
#[derive(Debug, Clone)]
pub struct FilterMenu {
    pub kind: IssueFilterMenu,
    pub items: Vec<MenuItem>,
    /// Some entry other than "any" is selected.
    pub active: bool,
}

impl FilterMenu {
    /// A stable English key, for element ids.
    pub const fn title(&self) -> &'static str {
        match self.kind {
            IssueFilterMenu::State => "State",
            IssueFilterMenu::Status => "Status",
            IssueFilterMenu::Author => "Author",
            IssueFilterMenu::Label => "Label",
            IssueFilterMenu::Milestone => "Milestone",
            IssueFilterMenu::Assignee => "Assignee",
            IssueFilterMenu::Sort => "Sort",
        }
    }
}

/// Everything the filter menus can offer.
#[derive(Debug, Clone, Default)]
pub struct MenuChoices {
    pub labels: Vec<Label>,
    pub milestones: Vec<MilestoneProgress>,
    /// People who can push (authors and assignees offered by name).
    pub people: Vec<Account>,
}

/// The menus for `query` on the list at `list`.
pub fn filter_menus(query: &IssueQuery, list: &str, choices: &MenuChoices, viewer: Option<&User>) -> Vec<FilterMenu> {
    use IssueFilterMessage as M;
    let link = |query: IssueQuery| list_url(list, &IssueQuery { ignored: Vec::new(), ..query });
    let person = |who: &Option<Who>, name: &str| matches!(who, Some(Who::Name(n)) if n.eq_ignore_ascii_case(name));

    let mut author = vec![MenuItem::message(M::Anyone, link(query.with_author(None)), query.author.is_none())];
    let mut assignee = vec![
        MenuItem::message(M::Anyone, link(query.with_assignee(None)), query.assignee.is_none() && !query.no_assignee),
        MenuItem::message(M::Nobody, link(query.without_assignee()), query.no_assignee),
    ];
    if viewer.is_some() {
        author.push(MenuItem::message(M::You, link(query.with_author(Some(Who::Me))), query.author == Some(Who::Me)));
        assignee.push(MenuItem::message(M::You, link(query.with_assignee(Some(Who::Me))), query.assignee == Some(Who::Me)));
    }
    for account in &choices.people {
        let name = account.name.to_string();
        let who = Some(Who::Name(name.clone()));
        author.push(MenuItem::named(name.clone(), link(query.with_author(who.clone())), person(&query.author, &name)));
        assignee.push(MenuItem::named(name.clone(), link(query.with_assignee(who)), person(&query.assignee, &name)));
    }

    let mut milestones = vec![
        MenuItem::message(M::Any, link(query.with_milestone(None)), query.milestone.is_none() && !query.no_milestone),
        MenuItem::message(M::NoMilestone, link(query.without_milestone()), query.no_milestone),
    ];
    for progress in &choices.milestones {
        let title = progress.milestone.title.clone();
        let selected = query.milestone.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(&title));
        milestones.push(MenuItem::named(title.clone(), link(query.with_milestone(Some(title))), selected));
    }

    let sort = IssueSort::ALL
        .into_iter()
        .map(|sort| MenuItem::message(sort_message(sort), link(query.with_sort(sort)), query.sort == sort))
        .collect();

    let mut menus = state_menus(query, list);
    menus.extend(vec![
        FilterMenu { kind: IssueFilterMenu::Author, active: query.author.is_some(), items: author },
        label_menu(query, list, choices),
        FilterMenu { kind: IssueFilterMenu::Milestone, active: query.milestone.is_some() || query.no_milestone, items: milestones },
        FilterMenu { kind: IssueFilterMenu::Assignee, active: query.assignee.is_some() || query.no_assignee, items: assignee },
        FilterMenu { kind: IssueFilterMenu::Sort, active: query.sort != IssueSort::default(), items: sort },
    ]);
    menus
}

const fn sort_message(sort: IssueSort) -> IssueFilterMessage {
    match sort {
        IssueSort::CreatedDesc => IssueFilterMessage::SortNewest,
        IssueSort::CreatedAsc => IssueFilterMessage::SortOldest,
        IssueSort::UpdatedDesc => IssueFilterMessage::SortRecentlyUpdated,
        IssueSort::UpdatedAsc => IssueFilterMessage::SortLeastRecentlyUpdated,
        IssueSort::CommentsDesc => IssueFilterMessage::SortMostCommented,
    }
}

fn label_menu(query: &IssueQuery, list: &str, choices: &MenuChoices) -> FilterMenu {
    let link = |query: IssueQuery| list_url(list, &IssueQuery { ignored: Vec::new(), ..query });
    let mut clear_labels = query.clone();
    clear_labels.labels.clear();
    clear_labels.not_labels.clear();
    clear_labels.no_label = false;
    let mut no_labels = clear_labels.clone();
    no_labels.no_label = true;
    let any = query.labels.is_empty() && query.not_labels.is_empty() && !query.no_label;
    let mut labels = vec![
        MenuItem::message(IssueFilterMessage::AnyLabel, link(clear_labels), any),
        MenuItem::message(IssueFilterMessage::NoLabels, link(no_labels), query.no_label),
    ];
    labels.extend(choices.labels.iter().map(|label| MenuItem {
        color: Some(safe_color(&label.color)),
        ..MenuItem::named(label.name.clone(), link(query.toggling_label(&label.name)), query.has_label(&label.name))
    }));
    FilterMenu { kind: IssueFilterMenu::Label, active: !any, items: labels }
}

fn state_menus(query: &IssueQuery, list: &str) -> Vec<FilterMenu> {
    use IssueFilterMessage as M;
    let link = |query: IssueQuery| list_url(list, &IssueQuery { ignored: Vec::new(), ..query });
    let state = vec![
        MenuItem::message(M::Open, link(query.with_state(StateFilter::Open)), query.state == Some(StateFilter::Open)),
        MenuItem::message(M::Closed, link(query.with_state(StateFilter::Closed)), query.state == Some(StateFilter::Closed)),
        MenuItem::message(M::AllStates, link(IssueQuery { state: None, ..query.clone() }), query.state.is_none()),
    ];
    let mut menus = vec![FilterMenu { kind: IssueFilterMenu::State, active: query.state != Some(StateFilter::Open), items: state }];
    if query.kind == Some(KindFilter::Pull) {
        let any = IssueQuery { draft: false, merged: false, ..query.clone() };
        let draft = IssueQuery { draft: true, state: Some(StateFilter::Open), ..any.clone() };
        let merged = IssueQuery { merged: true, state: Some(StateFilter::Closed), ..any.clone() };
        menus.push(FilterMenu {
            kind: IssueFilterMenu::Status,
            active: query.draft || query.merged,
            items: vec![
                MenuItem::message(M::AnyStatus, link(any), !query.draft && !query.merged),
                MenuItem::message(M::Draft, link(draft), query.draft),
                MenuItem::message(M::Merged, link(merged), query.merged),
            ],
        });
    }
    menus
}
// ------------------------------------------------------------------ pages

/// `/{owner}/{repo}/issues`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "issues/list.html")]
pub struct IssueListPage {
    pub page: Page,
    pub header: RepoHeader,
    /// The query as shown in the search box.
    pub query: String,
    pub hints: Vec<String>,
    pub list_url: String,
    pub open_url: String,
    pub closed_url: String,
    pub open: u64,
    pub closed: u64,
    /// Which tab is current (neither when the query names no state).
    pub showing_open: bool,
    pub showing_closed: bool,
    pub rows: Vec<IssueRow>,
    pub menus: Vec<FilterMenu>,
    pub pagination: Pagination,
    pub new_url: String,
    pub labels_url: String,
    pub milestones_url: String,
    /// The query differs from the default (offer to clear it).
    pub is_filtered: bool,
}

/// A writer's choices on the new-issue form.
#[derive(Debug, Clone)]
pub struct Choice {
    pub id: i64,
    pub label: String,
    pub color: Option<String>,
    pub selected: bool,
}

/// `/{owner}/{repo}/issues/new`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "issues/new.html")]
pub struct NewIssuePage {
    pub page: Page,
    pub header: RepoHeader,
    pub error: FormError,
    pub title: String,
    pub body: String,
    pub action_url: String,
    pub cancel_url: String,
    pub can_triage: bool,
    pub labels: Vec<Choice>,
    pub assignees: Vec<Choice>,
    pub milestones: Vec<Choice>,
}

/// "alice opened this issue 2 days ago · 3 comments".
#[derive(Debug, Clone)]
pub struct ThreadSummary {
    pub author: ActorView,
    /// The author in slot 0, the time in slot 1.
    pub sentence: CollabSentence<'static>,
    pub when: String,
    pub when_iso: String,
}

impl ThreadSummary {
    pub fn new(ctx: &IssueContext) -> Self {
        let issue = &ctx.issue;
        Self {
            author: ActorView::new(ctx.author.as_ref()),
            sentence: match (issue.kind, u64::from(issue.comment_count)) {
                (IssueKind::Pull, comments) => CollabSentence::OpenedPull { comments },
                (IssueKind::Issue, comments) => CollabSentence::OpenedIssue { comments },
            },
            when: issue.created_at.relative(),
            when_iso: issue.created_at.iso8601(),
        }
    }
}

/// `/{owner}/{repo}/issues/{n}`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "issues/show.html")]
pub struct IssuePage {
    pub page: Page,
    pub header: RepoHeader,
    pub thread: ThreadView,
    pub title: TitleView,
    pub badge: StateBadge,
    pub summary: ThreadSummary,
    pub sidebar: IssueSidebar,
    pub list_url: String,
}

// ------------------------------------------------------------------ the sidebar

/// One choice in a sidebar editor.
#[derive(Debug, Clone)]
pub struct SidebarOption {
    pub value: String,
    pub label: String,
    pub color: Option<String>,
    pub selected: bool,
}

/// Assignees, labels, milestone, subscription and moderation controls beside an issue
/// or pull request. The default renders nothing.
#[derive(Debug, Clone, Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent display flags")]
pub struct IssueSidebar {
    /// Whether anything is rendered at all.
    pub visible: bool,
    pub assignees: Vec<PersonView>,
    pub labels: Vec<LabelChip>,
    pub milestone: Option<(String, String)>,
    /// Labels, assignees and milestone are editable.
    pub can_triage: bool,
    pub label_options: Vec<SidebarOption>,
    pub assignee_options: Vec<SidebarOption>,
    pub milestone_options: Vec<SidebarOption>,
    pub labels_url: String,
    pub assignees_url: String,
    pub milestone_url: String,
    /// The viewer may assign themselves (a writer who is not assigned yet).
    pub can_assign_self: bool,
    pub viewer_id: i64,
    /// Notifications are on and the viewer is signed in.
    pub can_subscribe: bool,
    pub subscribed: bool,
    pub subscription_reason: Option<IssueSidebarMessage>,
    /// The viewer has an unread notification about this thread (not rendered: the page
    /// marks it read).
    pub thread_unread: bool,
    pub subscription_url: String,
    pub can_delete: bool,
    pub delete_url: String,
    /// The thread is a pull request (sentences name the kind).
    pub is_pull: bool,
}

/// The sidebar for `ctx` as `viewer` sees it: current metadata, editors for those who
/// may triage, the subscription toggle and, for administrators of an issue, deletion.
pub async fn sidebar(state: &AppState, ctx: &IssueContext, viewer: Option<&User>) -> Result<IssueSidebar, ServiceError> {
    let issues = &state.services.issues;
    let repo = &ctx.repo.repo;
    let (owner, name) = (repo.owner.name.as_str(), repo.repo.name.as_str());
    let metadata = issues.metadata(&[ctx.issue.id]).await?.remove(&ctx.issue.id).unwrap_or_default();
    let list = format!("/{owner}/{name}/{}", if ctx.issue.is_pull() { "pulls" } else { "issues" });
    let forms = format!("/{owner}/{name}/issues/{}", ctx.issue.number.get());
    let can_triage = ctx.permissions.can_triage;

    let mut sidebar = IssueSidebar {
        visible: true,
        assignees: metadata.assignees.iter().map(PersonView::new).collect(),
        labels: metadata
            .labels
            .iter()
            .map(|label| LabelChip::new(label, Some(list_url(&list, &IssueQuery::parse(DEFAULT_QUERY).toggling_label(&label.name)))))
            .collect(),
        milestone: metadata.milestone.as_ref().map(|m| (m.title.clone(), format!("/{owner}/{name}/milestones/{}", m.id.0))),
        can_triage,
        labels_url: format!("{forms}/labels"),
        assignees_url: format!("{forms}/assignees"),
        milestone_url: format!("{forms}/milestone"),
        subscription_url: format!("{forms}/subscription"),
        delete_url: format!("{forms}/delete"),
        can_delete: ctx.permissions.can_delete,
        is_pull: ctx.issue.is_pull(),
        ..IssueSidebar::default()
    };

    if can_triage {
        let (labels, people, milestones) = tokio::try_join!(
            issues.labels(&ctx.repo),
            issues.assignable(&ctx.repo, ""),
            issues.milestones(&ctx.repo, Some(IssueState::Open))
        )?;
        sidebar.label_options = labels
            .iter()
            .map(|label| SidebarOption {
                value: label.id.0.to_string(),
                label: label.name.clone(),
                color: Some(safe_color(&label.color)),
                selected: metadata.labels.iter().any(|l| l.id == label.id),
            })
            .collect();
        let assigned: Vec<AccountId> = metadata.assignees.iter().map(|a| a.id).collect();
        sidebar.assignee_options = people
            .iter()
            .map(|person| SidebarOption {
                value: person.id.0.to_string(),
                label: person.name.to_string(),
                color: None,
                selected: assigned.contains(&person.id),
            })
            .collect();
        let current = metadata.milestone.as_ref().map(|m| m.id);
        sidebar.milestone_options =
            std::iter::once(SidebarOption { value: String::new(), label: String::new(), color: None, selected: current.is_none() })
                .chain(milestones.iter().map(|progress| SidebarOption {
                    value: progress.milestone.id.0.to_string(),
                    label: progress.milestone.title.clone(),
                    color: None,
                    selected: current == Some(progress.milestone.id),
                }))
                .collect();
        if let Some(user) = viewer {
            sidebar.viewer_id = user.id().0;
            sidebar.can_assign_self = !assigned.contains(&user.id()) && people.iter().any(|p| p.id == user.id());
        }
    }

    let notifications = &state.services.notifications;
    if let Some(user) = viewer.filter(|_| notifications.enabled()) {
        sidebar.can_subscribe = true;
        let (subscription, unread) = notifications.thread_state(user.id(), ctx.issue.id).await?;
        sidebar.thread_unread = unread;
        sidebar.subscribed = subscription.is_some_and(|s| s.subscribed);
        sidebar.subscription_reason = subscription.filter(|s| s.subscribed).map(|s| reason_sentence(s.reason));
    }
    Ok(sidebar)
}

/// Why the viewer receives notifications about a thread.
pub const fn reason_sentence(reason: NotificationReason) -> IssueSidebarMessage {
    match reason {
        NotificationReason::Manual => IssueSidebarMessage::ReasonManual,
        NotificationReason::Comment => IssueSidebarMessage::ReasonComment,
        NotificationReason::Reviewed => IssueSidebarMessage::ReasonReviewed,
        NotificationReason::Author => IssueSidebarMessage::ReasonAuthor,
        NotificationReason::Mention => IssueSidebarMessage::ReasonMention,
        NotificationReason::Assigned => IssueSidebarMessage::ReasonAssigned,
        NotificationReason::ReviewRequested => IssueSidebarMessage::ReasonReviewRequested,
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{AccountKind, IssueId, IssueNumber, IssueText, IssueTitle, LabelId, MilestoneId, RepoId, Timestamp};
    use rbit_services::issues::MilestoneRef;

    use super::*;

    fn account(id: i64, name: &str) -> Account {
        Account {
            id: AccountId(id),
            kind: AccountKind::User,
            name: name.parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        }
    }

    fn issue(state: IssueState) -> Issue {
        Issue {
            id: IssueId(1),
            repo_id: RepoId(1),
            number: IssueNumber::new(12).unwrap(),
            kind: IssueKind::Issue,
            title: IssueTitle::parse("Crash <on> start").unwrap(),
            body: IssueText::default(),
            author: Some(AccountId(1)),
            state,
            state_reason: (state == IssueState::Closed).then_some(StateReason::Completed),
            closed_at: (state == IssueState::Closed).then_some(Timestamp(50)),
            closed_by: None,
            locked: false,
            comment_count: 3,
            created_at: Timestamp(10),
            updated_at: Timestamp(10),
        }
    }

    #[test]
    fn colours_are_never_passed_through_unchecked() {
        assert_eq!(safe_color("#D73A4A"), "#d73a4a");
        for bad in ["red", "#fff", "#12345g", "#123456\" onload=\"x", "url(x)"] {
            assert_eq!(safe_color(bad), "#888888", "{bad}");
        }
    }

    #[test]
    fn rows_link_labels_back_to_the_list() {
        let metadata = IssueMetadata {
            labels: vec![LabelSnapshot { id: LabelId(1), name: "good first issue".into(), color: "#7057ff".into() }],
            assignees: vec![account(2, "bob")],
            milestone: Some(MilestoneRef { id: MilestoneId(3), title: "v1".into() }),
        };
        let row =
            IssueRow::new(&issue(IssueState::Open), Some(&account(1, "alice")), Some(&metadata), "alice", "demo", "/alice/demo/issues");
        assert_eq!(row.url, "/alice/demo/issues/12");
        assert_eq!(row.number, "#12");
        assert_eq!(row.state.class, "text-success");
        assert_eq!(row.labels[0].url.as_deref(), Some("/alice/demo/issues?q=is%3Aopen%20is%3Aissue%20label%3A%22good%20first%20issue%22"));
        assert_eq!(row.milestone, Some(("v1".to_owned(), "/alice/demo/milestones/3".to_owned())));
        assert_eq!(row.assignees[0].name, "bob");
        let closed = IssueRow::new(&issue(IssueState::Closed), None, None, "alice", "demo", "/alice/demo/issues");
        assert!(closed.closed && closed.author == "ghost" && closed.state.class == "text-ink-muted");
    }

    #[test]
    fn menus_link_to_changed_queries() {
        let query = IssueQuery::parse("is:open label:bug");
        let choices = MenuChoices { people: vec![account(2, "bob")], ..MenuChoices::default() };
        let menus = filter_menus(&query, "/a/b/issues", &choices, None);
        let titles: Vec<&str> = menus.iter().map(FilterMenu::title).collect();
        assert_eq!(titles, ["State", "Author", "Label", "Milestone", "Assignee", "Sort"]);
        let author = &menus[1];
        assert_eq!(author.items.len(), 2, "no \"You\" when signed out");
        assert_eq!(author.items[1].url, "/a/b/issues?q=is%3Aopen%20author%3Abob%20label%3Abug");
        assert!(menus[2].active && !menus[1].active);
        assert_eq!(menus[2].items[0].url, "/a/b/issues?q=is%3Aopen");
        assert_eq!(menus[2].items[1].url, "/a/b/issues?q=is%3Aopen%20no%3Alabel");
        assert!(menus[5].items[0].selected);
        assert_eq!(list_url("/a/b/issues", &IssueQuery::parse("")), "/a/b/issues");
    }

    fn page() -> Page {
        crate::views::test_support::page("/alice/demo/issues")
    }

    fn assert_no_inline_styles(html: &str) {
        assert!(!html.contains("style="), "no inline styles: {html}");
        assert!(!html.contains("<script>"), "no inline scripts");
    }

    #[test]
    fn the_list_renders_rows_tabs_and_menus() {
        let metadata = IssueMetadata {
            labels: vec![LabelSnapshot { id: LabelId(1), name: "bug".into(), color: "#d73a4a".into() }],
            ..IssueMetadata::default()
        };
        let rows = vec![IssueRow::new(
            &issue(IssueState::Open),
            Some(&account(1, "alice")),
            Some(&metadata),
            "alice",
            "demo",
            "/alice/demo/issues",
        )];
        let query = IssueQuery::parse("");
        let html = IssueListPage {
            page: page(),
            header: RepoHeader { active_tab: "issues", ..RepoHeader::default() },
            query: DEFAULT_QUERY.to_owned(),
            hints: vec!["“x:y” is not understood and was ignored".to_owned()],
            list_url: "/alice/demo/issues".to_owned(),
            open_url: "/alice/demo/issues".to_owned(),
            closed_url: "/alice/demo/issues?q=is%3Aclosed".to_owned(),
            open: 1,
            closed: 4,
            showing_open: true,
            showing_closed: false,
            rows,
            menus: filter_menus(&query, "/alice/demo/issues", &MenuChoices::default(), None),
            pagination: Pagination::default(),
            new_url: "/alice/demo/issues/new".to_owned(),
            labels_url: "/alice/demo/labels".to_owned(),
            milestones_url: "/alice/demo/milestones".to_owned(),
            is_filtered: false,
        }
        .render()
        .unwrap();
        assert!(html.contains("Crash ") && !html.contains("<on>"), "titles are escaped");
        assert!(html.contains(r##"fill="#d73a4a""##), "label colours are SVG fills");
        assert!(html.contains(r#"aria-current="page""#));
        assert!(html.contains("is not understood"));
        assert_no_inline_styles(&html);
    }

    #[derive(Template)]
    #[template(source = r#"{% let page = crate::views::test_support::page("/") %}{% include "issues/sidebar.html" %}"#, ext = "html")]
    struct SidebarFixture {
        sidebar: IssueSidebar,
    }

    #[test]
    fn the_sidebar_shows_editors_only_to_triagers() {
        assert_eq!(SidebarFixture { sidebar: IssueSidebar::default() }.render().unwrap().trim(), "", "the default renders nothing");
        let reader = IssueSidebar {
            visible: true,
            labels: vec![LabelChip { name: "bug".into(), color: "#d73a4a".into(), url: None }],
            can_subscribe: true,
            subscription_url: "/a/b/issues/1/subscription".into(),
            is_pull: false,
            ..IssueSidebar::default()
        };
        let html = SidebarFixture { sidebar: reader.clone() }.render().unwrap();
        assert!(html.contains("Subscribe") && !html.contains("Edit labels") && !html.contains("Delete issue"));
        assert_no_inline_styles(&html);
        let triager = IssueSidebar {
            can_triage: true,
            can_delete: true,
            subscribed: true,
            subscription_reason: Some(reason_sentence(NotificationReason::Assigned)),
            label_options: vec![SidebarOption { value: "1".into(), label: "bug".into(), color: Some("#d73a4a".into()), selected: true }],
            ..reader
        };
        let html = SidebarFixture { sidebar: triager }.render().unwrap();
        assert!(html.contains("Edit labels") && html.contains("Unsubscribe") && html.contains("You are assigned."));
        assert!(html.contains(r#"name="label" value="1""#) && html.contains("checked"));
        assert!(html.contains("Delete issue"));
        assert!(!html.contains(" id="), "the sidebar can be included twice");
    }
}
