# Milestone 2, wave B — Issues, pull requests, branch protection

The working contract for the second half of [milestone 2](milestone-2.md): the workflow
layer (issues, pull requests with review and merge, protected branches) on top of wave A's
access model, API, webhooks, compare and forks. House rules from
[milestone-2.md](milestone-2.md#rules-all-workstreams) and [../HANDOFF.md](../HANDOFF.md)
§4 apply unchanged unless this document says otherwise.

Three workstreams, each on its **own branch in its own git worktree**, each owning a
disjoint set of files:

| | Workstream | Scope | Migrations |
|---|---|---|---|
| **I** | Issues | Issue pages, labels, milestones, assignees, references (`#12`, `owner/repo#12`, `@user`, `fixes #12` in pushed commits), search and filters, in-app notifications, `issues`/`issue_comment` webhooks and REST endpoints | `0018`, `0019`, `0020` |
| **P** | Pull requests | Cross-fork compare, PR creation and pages, `refs/pull/N/head`, review comments on diff lines, reviews (approve / request changes), mergeability checks, merge strategies (merge commit, squash, rebase) on a temporary worktree, required-checks placeholder, `pull_request`/`pull_request_review` webhooks and REST endpoints | `0021`, `0022`, (`0023` spare) |
| **B** | Branch protection | Rules per branch pattern (no force-push, no deletion, require a pull request with N approvals, restrict pushers), a pre-receive gate in the HTTP **and** SSH receive paths, merge requirements for P, settings UI, REST endpoints | `0024`, (`0025` spare) |

Pull requests are issues with a branch attached: they share the per-repository number
sequence, comments and the timeline. That shared core is **not** left to I or P. It is
built first, by the integrator, as step **T0** (below), so I and P can then run in
parallel without touching each other's files.

Migration `0016` belongs to the wave A integration (old-name redirects). Migration numbers `0017`–`0031` are reserved for wave B: `0017` for T0, the rest as in the
table; `0026`–`0031` stay free for the API continuation. Never create a file outside your
range. Files are `NNNN_<topic>.sql`, `STRICT` tables, Unix-second timestamps, foreign keys
with explicit `ON DELETE`.

## Goals

1. Anyone who can read a repository can report and discuss problems; writers triage them
   with labels, milestones and assignees; commits close issues.
2. Changes arrive as pull requests, also from forks, are reviewed line by line and are
   merged by Rbit itself, safely, with the strategy the merger picks.
3. Important branches cannot be rewritten, deleted or pushed to by the wrong people, no
   matter which transport is used.
4. Everything above is visible to automation: REST endpoints and GitHub-shaped webhooks.
5. People hear about what concerns them: an in-app notification inbox.

**In scope, decided:** in-app notifications (participation-based inbox, unread indicator in
the header). **Out of scope:** email notifications (the `MailService` seam stays; no
digests), watching repositories, reactions, issue templates, issue transfer, confidential
issues, file uploads in comments, commit statuses / CI (only a required-checks
placeholder), tag protection, CODEOWNERS, auto-merge, suggested changes, multi-line review
comments, the `pull_request_review_comment` webhook event, API endpoints for collaborators,
teams, releases and forks (the "API continued" row of the wave B outline moves to a later
wave), GraphQL, project boards.

## Sequencing

```
I1 (integration agent, in flight: mirror push refusal + old-name redirects in git_http.rs / rbit-ssh)
 └─► P0  pre-wire: stubs, events, push-gate glue (HTTP + SSH), config, nav, icons, CSS      (integrator)
      ├─► B  starts immediately (needs only P0)
      └─► T0 timeline core: migration 0017, store, service, generic routes, timeline views (integrator)
           ├─► I starts
           └─► P starts
```

P0 is done on `main` **after I1 has merged**, because it edits the same receive paths. T0
is real, tested code (not a stub) and is frozen once I and P start: changes to it go
through integration requests. B does not depend on T0.

---

## Shared contracts (on `main` after P0 and T0)

### 1. The issue model (T0, frozen)

#### Migration `0017_issues_core.sql`

```sql
CREATE TABLE issue_counters (
    repo_id     INTEGER PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
    last_number INTEGER NOT NULL
) STRICT;

CREATE TABLE issues (
    id            INTEGER PRIMARY KEY,
    repo_id       INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    number        INTEGER NOT NULL CHECK (number > 0),        -- shared by issues and PRs
    kind          TEXT    NOT NULL CHECK (kind IN ('issue', 'pull')),
    title         TEXT    NOT NULL,
    body          TEXT    NOT NULL DEFAULT '',
    author_id     INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    state         TEXT    NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'closed')),
    state_reason  TEXT    CHECK (state_reason IN ('completed', 'not_planned', 'merged')),
    closed_at     INTEGER,
    closed_by     INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    locked        INTEGER NOT NULL DEFAULT 0 CHECK (locked IN (0, 1)),
    comment_count INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    UNIQUE (repo_id, number),
    CHECK ((state = 'open') = (closed_at IS NULL)),
    CHECK ((state = 'open') = (state_reason IS NULL)),
    CHECK (state_reason IS NOT 'merged' OR kind = 'pull')
) STRICT;
CREATE INDEX issues_by_repo   ON issues (repo_id, kind, state, number DESC);
CREATE INDEX issues_by_author ON issues (author_id, state);

CREATE TABLE issue_comments (
    id         INTEGER PRIMARY KEY,
    issue_id   INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    author_id  INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    body       TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    edited_at  INTEGER                                    -- NULL until edited
) STRICT;
CREATE INDEX issue_comments_by_issue ON issue_comments (issue_id, created_at, id);

CREATE TABLE issue_events (
    id         INTEGER PRIMARY KEY,
    issue_id   INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    actor_id   INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    kind       TEXT    NOT NULL,                          -- TimelineEvent tag, snake_case
    data       TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(data)),
    created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX issue_events_by_issue ON issue_events (issue_id, created_at, id);
```

Numbers come from `issue_counters` (upsert + `RETURNING` inside the inserting
transaction), so a deleted issue's number is never reused. **Only
`sqlite/issues.rs` writes these four tables.** Other stores may read them (joins, counts)
and reference `issues(id)` by foreign key; I and P keep their own data in their own tables
keyed by `issue_id`.

#### Core types — `rbit-core/src/issue.rs` (lands in P0, so events can use it)

```rust
pub struct IssueId(pub i64);  pub struct CommentId(pub i64);  pub struct EventId(pub i64);
pub struct LabelId(pub i64);  pub struct MilestoneId(pub i64); pub struct ReviewId(pub i64);

/// Per-repository number, `> 0`; parsed from URL segments ("12"), displayed "#12".
pub struct IssueNumber(u32);           // parse(&str) -> Result<Self, NameError>, get() -> u32
pub enum IssueKind { Issue, Pull }     // as_str "issue" | "pull"
pub enum IssueState { Open, Closed }
pub enum StateReason { Completed, NotPlanned, Merged }   // Merged only for kind Pull
pub enum MergeStrategy { Merge, Squash, Rebase }          // "merge" | "squash" | "rebase"
pub enum ReviewState { Pending, Commented, Approved, ChangesRequested, Dismissed }

/// Trimmed, 1..=256 characters, no control characters.
pub struct IssueTitle(String);
/// Markdown bodies and comments: ≤ 65 536 bytes, CRLF normalised to LF. May be empty
/// for issue bodies, never for comments (`IssueText::comment`).
pub struct IssueText(String);

pub struct Issue {
    pub id: IssueId, pub repo_id: RepoId, pub number: IssueNumber, pub kind: IssueKind,
    pub title: IssueTitle, pub body: IssueText, pub author: Option<AccountId>,
    pub state: IssueState, pub state_reason: Option<StateReason>,
    pub closed_at: Option<Timestamp>, pub closed_by: Option<AccountId>,
    pub locked: bool, pub comment_count: u32, pub created_at: Timestamp, pub updated_at: Timestamp,
}
pub struct Comment { pub id: CommentId, pub issue_id: IssueId, pub author: Option<AccountId>,
    pub body: IssueText, pub created_at: Timestamp, pub updated_at: Timestamp, pub edited_at: Option<Timestamp> }

/// A label as it was when an event happened (labels can be renamed or deleted later).
pub struct LabelSnapshot { pub id: LabelId, pub name: String, pub color: String /* "#rrggbb" */ }

/// Everything the timeline can show besides comments. Stored as `kind` + `data` (the
/// variant's fields as a JSON object, via serde); unknown kinds are skipped on read.
pub enum TimelineEvent {
    // T0
    Closed { reason: StateReason, commit: Option<String>, pull: Option<IssueNumber> },
    Reopened,
    Renamed { from: String, to: String },
    Locked, Unlocked,
    // I
    Labeled { label: LabelSnapshot }, Unlabeled { label: LabelSnapshot },
    Milestoned { milestone: MilestoneId, title: String }, Demilestoned { milestone: MilestoneId, title: String },
    Assigned { assignee: AccountId }, Unassigned { assignee: AccountId },
    /// A commit in this repository mentions the issue ("referenced this in abc1234").
    CommitReferenced { commit: String, summary: String },
    /// Another issue or PR mentions this one; shown only to viewers who can read the source.
    CrossReferenced { source: IssueId },
    // P
    HeadPushed { before: String, after: String, count: u32, forced: bool },
    BaseChanged { from: String, to: String },
    Merged { commit: String, strategy: Option<MergeStrategy> },   // None: merged outside Rbit
    ReviewRequested { reviewer: AccountId }, ReviewRequestRemoved { reviewer: AccountId },
    ReviewDismissed { review: ReviewId, reviewer: Option<AccountId> },
    HeadRefDeleted { branch: String },
    ConvertedToDraft, ReadyForReview,
}

/// What a viewer may do with an issue or PR. Pure; exhaustively tested in T0.
pub struct IssuePermissions {
    pub can_comment: bool, pub can_edit: bool /* title + body */, pub can_set_state: bool,
    pub can_lock: bool, pub can_triage: bool /* labels, milestones, assignees, reviewers */,
    pub can_delete: bool, pub can_moderate: bool /* delete others' comments */,
}
pub fn issue_permissions(viewer: Option<&User>, access: RepoAccess, issue: &Issue) -> IssuePermissions;
```

Permission rules (the `issue_permissions` table T0 tests exhaustively over viewer kind ×
`RepoAccess` × author/not × locked/not × kind):

| Action | Who |
|---|---|
| read, list | `access ≥ Read` (everyone else: 404 for the whole repository) |
| open an issue, comment | signed in, `access ≥ Read`; on a locked thread only `access ≥ Write` |
| edit title/body | the author, or `access ≥ Write` |
| close / reopen | the author, or `access ≥ Write` (P adds a reopen precondition, see hooks) |
| lock / unlock, triage | `access ≥ Write` |
| delete an issue | `access ≥ Admin`, `kind = Issue` only (PRs cannot be deleted) |
| edit a comment | its author; delete: its author or `access ≥ Write` |

Token scopes are already folded into `access` by callers (`TokenScope::limit`).

#### Store — `rbit-storage/src/sqlite/issues.rs`, `Database::issues() -> IssueStore`

```rust
pub struct NewIssue { pub repo: RepoId, pub kind: IssueKind, pub title: IssueTitle, pub body: IssueText, pub author: AccountId, pub at: Timestamp }
pub struct IssueRef { pub id: IssueId, pub number: IssueNumber, pub kind: IssueKind, pub state: IssueState, pub title: String }

impl IssueStore {
    pub async fn create(&self, new: &NewIssue) -> Result<Issue, StorageError>;
    /// The same insert inside a caller's transaction (P creates the PR row atomically with it).
    pub(crate) async fn insert_in(conn: &mut SqliteConnection, new: &NewIssue) -> Result<Issue, StorageError>;
    pub async fn find(&self, id: IssueId) -> Result<Option<Issue>, StorageError>;
    pub async fn find_by_number(&self, repo: RepoId, number: IssueNumber) -> Result<Option<Issue>, StorageError>;
    /// Batch lookup for reference rendering (`json_each(?)`).
    pub async fn find_numbers(&self, repo: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, StorageError>;
    /// Writes a `Renamed` event in the same transaction when the title changes.
    pub async fn update_text(&self, id: IssueId, title: Option<&IssueTitle>, body: Option<&IssueText>, actor: AccountId, at: Timestamp) -> Result<Issue, StorageError>;
    /// Conditional (`WHERE state <> ?`); `None` when nothing changed. `event` is recorded in the same transaction.
    pub async fn set_state(&self, id: IssueId, state: IssueState, reason: Option<StateReason>, actor: Option<AccountId>, event: &TimelineEvent, at: Timestamp) -> Result<Option<Issue>, StorageError>;
    pub async fn set_locked(&self, id: IssueId, locked: bool, actor: AccountId, at: Timestamp) -> Result<bool, StorageError>;
    pub async fn touch(&self, id: IssueId, at: Timestamp) -> Result<(), StorageError>;
    pub async fn delete(&self, id: IssueId) -> Result<Option<Issue>, StorageError>;
    pub async fn open_counts(&self, repo: RepoId) -> Result<(u64, u64), StorageError>;   // (issues, pulls)
    pub async fn add_comment(&self, issue: IssueId, author: AccountId, body: &IssueText, at: Timestamp) -> Result<Comment, StorageError>;
    pub async fn find_comment(&self, issue: IssueId, id: CommentId) -> Result<Option<Comment>, StorageError>;
    pub async fn edit_comment(&self, id: CommentId, body: &IssueText, at: Timestamp) -> Result<Comment, StorageError>;
    pub async fn delete_comment(&self, id: CommentId) -> Result<Option<Comment>, StorageError>;
    pub async fn record_event(&self, issue: IssueId, actor: Option<AccountId>, event: &TimelineEvent, at: Timestamp) -> Result<EventId, StorageError>;
    /// Comments and events merged (`UNION ALL`), ordered by (created_at, id), actor accounts joined.
    pub async fn timeline(&self, issue: IssueId, after: Option<TimelineCursor>, limit: u32) -> Result<Vec<StoredTimelineItem>, StorageError>;
}
```

`comment_count` and `updated_at` are maintained in the same transaction as the comment
change.

#### Service — `rbit-services/src/timeline.rs`, `Services::timeline: TimelineService`

```rust
pub struct IssueContext { pub repo: RepoContext, pub issue: Issue, pub author: Option<Account>, pub permissions: IssuePermissions }
pub enum StateChange { Close(StateReason /* Completed | NotPlanned */), Reopen }
pub enum TimelineEntry {
    Comment { comment: Comment, author: Option<Account>, can_edit: bool, can_delete: bool },
    Event { id: EventId, actor: Option<Account>, event: TimelineEvent, at: Timestamp, subjects: EventSubjects },
}
/// Accounts named by the event (assignee, reviewer) and, for `CrossReferenced`, the source.
pub struct EventSubjects { pub account: Option<Account>, pub source: Option<CrossReferenceSource> }
pub struct CrossReferenceSource { pub full_name: String, pub number: IssueNumber, pub kind: IssueKind, pub state: IssueState, pub title: String }

impl TimelineService {
    /// 404 (`ServiceError::NotFound`) when the issue does not exist; `repo` is already access-checked.
    pub async fn open(&self, viewer: Option<&User>, repo: RepoContext, number: IssueNumber) -> Result<IssueContext, ServiceError>;
    pub async fn open_counts(&self, repo: RepoId) -> Result<OpenCounts, ServiceError>;
    /// Creates the row only; the caller publishes `IssueAction::Opened` once its own data is in place.
    pub(crate) async fn create(&self, actor: &User, repo: &RepoContext, kind: IssueKind, title: &str, body: &str) -> Result<Issue, ServiceError>;
    pub async fn edit(&self, actor: &User, ctx: &IssueContext, title: Option<&str>, body: Option<&str>) -> Result<Issue, ServiceError>;
    pub async fn set_state(&self, actor: &User, ctx: &IssueContext, change: StateChange) -> Result<Issue, ServiceError>;
    /// For closing by commit (I) and by merge (P): no permission check, no throttle; records
    /// `event` and publishes `IssueAction::Closed { reason }`. `false` if already closed.
    pub(crate) async fn close_as_system(&self, issue: IssueId, actor: Option<AccountId>, reason: StateReason, event: TimelineEvent) -> Result<bool, ServiceError>;
    pub async fn set_locked(&self, actor: &User, ctx: &IssueContext, locked: bool) -> Result<(), ServiceError>;
    pub async fn comment(&self, actor: &User, ctx: &IssueContext, body: &str) -> Result<Comment, ServiceError>;
    pub async fn edit_comment(&self, actor: &User, ctx: &IssueContext, id: CommentId, body: &str) -> Result<Comment, ServiceError>;
    pub async fn delete_comment(&self, actor: &User, ctx: &IssueContext, id: CommentId) -> Result<(), ServiceError>;
    /// At most `limit` (≤ 200) entries after `cursor`; `CrossReferenced` events whose source
    /// the viewer cannot read are dropped here.
    pub async fn timeline(&self, viewer: Option<&User>, ctx: &IssueContext, cursor: Option<TimelineCursor>, limit: u32) -> Result<Timeline, ServiceError>;
    pub(crate) async fn record(&self, issue: IssueId, actor: Option<AccountId>, event: TimelineEvent) -> Result<EventId, ServiceError>;
}
```

T0 publishes, for both kinds: `IssueComment` (created, edited, deleted), `Issue` with
`Edited`, `Closed`, `Reopened`, `Locked`, `Unlocked`. Creating, editing and commenting
consume `ThrottleScope::Content` for the actor (429 + `Retry-After` when exhausted).
`set_state` refuses `StateReason::Merged` (only P's merge produces it) and, for
`Reopen` of a `kind = Pull`, first calls the hook `pulls::lifecycle::check_reopen`.

#### Generic routes — `rbit-web/src/routes/timeline.rs` (T0)

Page router, HTML forms, Origin-checked, 303 to the canonical page
(`/{owner}/{repo}/issues/{n}` or `/{owner}/{repo}/pull/{n}` by kind) plus an anchor:

| Route | Does |
|---|---|
| `POST /{owner}/{repo}/issues/{n}/comments` | comment; optional `state=close_completed\|close_not_planned\|reopen` applies after the comment ("Close with comment") |
| `POST /{owner}/{repo}/issues/{n}/comments/{id}` | edit a comment |
| `POST /{owner}/{repo}/issues/{n}/comments/{id}/delete` | delete a comment |
| `POST /{owner}/{repo}/issues/{n}/title`, `…/body` | edit |
| `POST /{owner}/{repo}/issues/{n}/state` | `close_completed`, `close_not_planned`, `reopen` |
| `POST /{owner}/{repo}/issues/{n}/lock` | `locked=true\|false` |

PR pages post to these same URLs (they use the shared number). Anchors:
`#issuecomment-{id}`, `#event-{id}`. `urls.rs` gains `issue_url(owner, repo, kind, number)`.

#### Timeline views (T0)

`rbit-web/src/views/timeline.rs` and `templates/timeline/{item,comment,event,comment_form,state_badge}.html`.
`TimelineItemView` renders one entry; `timeline/item.html` expects `item: TimelineItemView`
so P can interleave its reviews with core items in its own loop. Every `TimelineEvent`
variant, including I's and P's, has its sentence written by T0 from this contract
("alice added the `bug` label 2 days ago", "alice merged commit `abc1234` into `main`
(squash)", "alice force-pushed the `feature` branch from `abc1234` to `def5678`"). Comment
bodies are rendered through the I-owned hook `render::references::render_issue_markdown`.
Edit forms appear in place with `?edit=comment-{id}` (no JavaScript needed; htmx swaps the
same fragment).

Deleted accounts appear as **ghost** (no link, neutral avatar).

### 2. Domain events (P0, `rbit_services::events`)

```rust
DomainEvent::Issue {
    repo: RepoId, issue: IssueId, number: IssueNumber, kind: IssueKind,
    actor: Option<AccountId>,           // None: a deploy-key push or a mirror
    action: IssueAction,
}
DomainEvent::IssueComment { repo: RepoId, issue: IssueId, number: IssueNumber, kind: IssueKind, comment: CommentId, actor: AccountId, action: CommentAction }
DomainEvent::PullRequestReview { repo: RepoId, pull: IssueId, number: IssueNumber, review: ReviewId, actor: AccountId, action: ReviewAction }

// rbit-core/src/issue.rs
pub enum IssueAction {
    Opened,                                                        // I (issues), P (pulls)
    Edited { previous_title: Option<String>, previous_body: Option<String>, previous_base: Option<String> },
    Closed { reason: StateReason },                                // Merged ⇒ webhook "closed" with merged: true
    Reopened, Locked, Unlocked,                                    // T0
    Deleted { snapshot: Box<Issue> },                              // I
    Labeled { label: LabelSnapshot }, Unlabeled { label: LabelSnapshot },          // I
    Assigned { assignee: AccountId }, Unassigned { assignee: AccountId },          // I
    Milestoned { milestone: MilestoneId }, Demilestoned { milestone: MilestoneId, title: String }, // I
    Synchronized { before: String, after: String },               // P
    ReviewRequested { reviewer: AccountId }, ReviewRequestRemoved { reviewer: AccountId }, // P
    ConvertedToDraft, ReadyForReview,                              // P
}
pub enum CommentAction { Created, Edited { previous_body: String }, Deleted { snapshot: Comment } }
pub enum ReviewAction { Submitted, Edited { previous_body: String }, Dismissed }
```

Labels, milestones and assignees work on PRs too (they attach to the issue row): I's
services publish `Issue { kind: Pull, … }` for them, and the webhook dispatcher routes
those to P's `pull_request` payloads. Subscribers keep ignoring variants they do not handle.

### 3. Hook points (P0 stubs; the owner replaces the body, never the signature)

| Function | Owner | Called by | Stub |
|---|---|---|---|
| `rbit_web::render::references::render_issue_markdown(source, &RefContext) -> String`, `RefContext { base: DocumentBase, owner, repo, refs: ReferenceSet }` | I | T0 timeline views, I, P | `render_markdown(source, Some(&ctx.base))` |
| `ReferenceService::resolve(&self, repo: &RepoContext, texts: &[&str]) -> Result<ReferenceSet, ServiceError>` (`Services::references`; existing `#N` and `@name` among the texts, one batch query each) | I | T0 views, P | `ReferenceSet::default()` |
| `IssueService::metadata(&self, issues: &[IssueId]) -> Result<HashMap<IssueId, IssueMetadata>, ServiceError>` (`labels`, `assignees`, `milestone`) | I | P (lists, payloads, API) | empty map |
| `views::issues::sidebar(&AppState, &IssueContext, Option<&User>) -> IssueSidebar` + `templates/issues/sidebar.html` | I | P's PR page | `IssueSidebar::default()`, template renders nothing |
| `NotificationService::has_unread(&self, user: AccountId) -> Result<bool, ServiceError>` | I | `Page` extractor | `Ok(false)` |
| `NotificationService::mark_thread_read(&self, user: AccountId, issue: IssueId)` | I | I's issue page, P's PR page | no-op |
| `rbit_services::issues::hooks::payload(&ServiceContext, &DomainEvent, base_url) -> Result<Option<HookPayload>, ServiceError>` | I | `WebhookService::handle_event` | `Ok(None)` |
| `rbit_services::issues::json::{issue, label, milestone}` | I | P (embedded objects) | minimal objects from core fields |
| `rbit_services::pulls::hooks::payload(…)` (same signature) | P | `WebhookService::handle_event` | `Ok(None)` |
| `rbit_services::pulls::lifecycle::check_reopen(&ServiceContext, &Issue) -> Result<(), ServiceError>` | P | `TimelineService::set_state` | `Ok(())` |
| `ProtectionService::check_push(&self, &RepoContext, &PushActor, &[RefUpdate]) -> Result<PushVerdict, ServiceError>` | B | push-gate glue (HTTP + SSH) | `PushVerdict::allow()` |
| `ProtectionService::check_merge(&self, &RepoContext, merger: &User, branch: &str) -> Result<MergeRules, ServiceError>` | B | P's merge | `MergeRules::unprotected()` |
| `ProtectionService::may_delete_branch(&self, &RepoContext, actor: &User, branch: &str) -> Result<bool, ServiceError>` | B | P ("delete head branch") | `Ok(true)` |
| `ProtectionService::protected_branches(&self, &RepoContext) -> Result<Vec<String>, ServiceError>` (names among existing branches) | B | branches page badges | `Ok(vec![])` |
| `rbit_transport::receive::intercept(input, CommandLimits) -> Result<Intercept<R>, ReceiveError>` | B | push-gate glue | `Ok(Intercept::Unread(input))` |
| `rbit_transport::receive::refusal(&CommandList, &[Refusal]) -> Vec<u8>` | B | push-gate glue | empty report |

```rust
pub struct HookPayload { pub repo: OwnedRepository, pub event: WebhookEvent, pub body: serde_json::Value }

pub struct PushActor<'a> { pub user: Option<&'a User>, pub deploy_key: bool, pub access: RepoAccess /* after the token scope */ }
pub struct Refusal { pub name: String /* full ref */, pub reason: String }
pub struct PushVerdict { pub refusals: Vec<Refusal>, pub deny_non_fast_forwards: bool }
pub struct MergeRules {
    pub patterns: Vec<String>,          // matching rules, for display
    pub require_pull_request: bool,
    pub required_approvals: u8,
    pub dismiss_stale_approvals: bool,
    pub block_on_changes_requested: bool,
    pub merger_may_push: bool,          // restrict_pushers allows the merger
    pub merger_may_bypass: bool,        // admin with allow_admin_bypass
}
```

### 4. The push gate (P0 glue, B's logic)

`git receive-pack` runs without hooks, and Rbit's tests boot the server in-process, so a
`pre-receive` hook executable is not an option. The gate is in-process instead:

1. **Read the command list before git sees it.** The client's request starts with
   pkt-lines `old new ref[\0capabilities]` (optionally preceded by `shallow <id>` lines)
   ending in a flush packet; push options and the pack follow. `receive::intercept` reads
   exactly that section (bounded: ≤ 10 000 commands, ≤ 4 MiB, `ReceiveError` beyond), keeps
   the raw bytes and returns `Intercept::Read { commands, input: Replay<R> }`, where
   `Replay` yields the saved bytes and then the rest of the stream, byte for byte.
2. **Decide.** The glue converts commands to `RefUpdate`s and calls
   `ProtectionService::check_push`. Rules that need no objects (deletion, restricted
   pushers, "changes must go through a pull request", the reserved `refs/pull/*`
   namespace) are decided here.
3. **Refuse all-or-nothing** (like a declining `pre-receive` hook): when any ref is
   refused, the glue drains the rest of the request (as much as git would have read),
   does not start git, and answers `receive::refusal(...)`: `unpack ok`, then
   `ng <ref> <reason>` for every refused ref and `ng <ref> push declined: another ref in
   this push was refused` for the others, honouring `report-status` / `report-status-v2`
   and `side-band-64k` / `side-band` (with a band-2 `Rbit: …` explanation per refusal).
   Clients that requested no `report-status` get HTTP 403 with a text body, or over SSH a
   stderr message and exit status 1. The client then prints
   `! [remote rejected] main -> main (protected branch: force pushes are not allowed)`.
4. **Force pushes** need the new objects, which arrive after the commands. When
   `verdict.deny_non_fast_forwards` is set (some update of an existing branch is protected
   against force pushes for this pusher), git is started with
   `-c receive.denyNonFastForwards=true`, which git applies after unpacking and before
   updating refs. Documented consequence: in that one push, other branches' non-fast-forward
   updates are refused too ("non-fast-forward"); pushing them separately works.
5. Otherwise git runs as today with the replayed input; `PushService::begin` is taken after
   the verdict, `complete` as today.

P0 glue, written once by the integrator in both paths, **after** I1's mirror refusal and
redirect handling (so it runs on the resolved repository and never for mirrors):

```rust
let (input, options) = match receive::intercept(input, CommandLimits::default()).await {
    Err(error) => /* HTTP 400 / SSH stderr + exit 1 */,
    Ok(Intercept::Unread(input)) => (boxed(input), ReceiveOptions::default()),
    Ok(Intercept::Read { commands, input }) => {
        let verdict = services.protection.check_push(&ctx, &actor, &commands.updates()).await?;
        if !verdict.refusals.is_empty() {
            receive::drain(input).await;
            return respond(receive::refusal(&commands, &verdict.refusals));
        }
        (boxed(input), ReceiveOptions { deny_non_fast_forwards: verdict.deny_non_fast_forwards })
    }
};
```

**SSH runs receive-pack in two steps** (P0): the SSH server writes the advertisement from
`git receive-pack --stateless-rpc --advertise-refs` itself (without HTTP's
`# service=` header), reads the command list from the channel, decides, and only then
starts `git receive-pack --stateless-rpc` with the replayed input — the same exchange smart
HTTP performs, which is why it is equivalent for clients. git's stderr is forwarded as
extended data as today; an empty command list (`Everything up-to-date`) ends the exchange
with exit status 0 without starting git. `git-upload-pack` over SSH stays interactive.
`ProcessTransport` gains `ReceiveOptions { deny_non_fast_forwards }` on its receive entry
points and exposes stderr for the SSH path. P0 must keep every existing SSH/HTTP e2e test
green with the stub (`Intercept::Unread`).

### 5. Webhooks (P0 dispatch, payloads by I and P)

`rbit_core::webhook::WebhookEvent` gains `Issues` (`issues`), `IssueComment`
(`issue_comment`), `PullRequest` (`pull_request`), `PullRequestReview`
(`pull_request_review`), all in `SUBSCRIBABLE` with labels and descriptions; the webhook
form lists them. `WebhookService::handle_event` gains:

```rust
DomainEvent::Issue { kind: IssueKind::Issue, .. } | DomainEvent::IssueComment { .. }
    => crate::issues::hooks::payload(&ctx, event, &base).await?,
DomainEvent::Issue { kind: IssueKind::Pull, .. } | DomainEvent::PullRequestReview { .. }
    => crate::pulls::hooks::payload(&ctx, event, &base).await?,
```

then, for `Some(payload)`, finds hooks with `subscribed(Some(repo), &org_of(owner), payload.event)`
and queues `payload.body` (the delivery's `action` is the payload's `action` field). The
`issue_comment` payload for a comment on a PR is I's (its `issue` object carries a
`pull_request` key). Shapes follow GitHub, field by field (see I and P).

### 6. REST API plug-in (P0)

`routes/api/mod.rs::table()` extends with `issues::routes()`, `notifications::routes()`,
`pulls::routes()` and `protection::routes()` (child modules, so they can use the parent's
private `route()` helper and `Caller`, `Pager`, `ApiError`, `ApiJson`). Each returns
`Vec<ApiRoute>`; stubs return `Vec::new()`.

`openapi.json` is no longer edited by workstreams: each owns a fragment
`routes/api/openapi/{issues,notifications,pulls,protection}.json` shaped
`{"paths": {}, "components": {"schemas": {}}}`. P0 serves the base document merged with
the fragments (built once, `LazyLock<String>`), and the existing
"documents exactly the routes" test runs on the merged document and fails on duplicate
paths or schema names.

### 7. Routers, navigation, reserved names (P0)

- Page routes merged into `page_routes`: `timeline`, `issues`, `labels`, `milestones`,
  `notifications`, `pulls`, `protection`. All repository pages load the repository with
  `RepoPage::load` and `loaded.header(&state, tab)`; tabs `"issues"`, `"pulls"`.
- `templates/repo/subnav.html`: **Issues** and **Pull requests** tabs after Code, each with
  an open count (`RepoHeader.open_issues`, `RepoHeader.open_pulls`, filled from
  `TimelineService::open_counts`, one indexed query).
- `templates/base.html`: for signed-in users a bell link to `/notifications`
  (`aria-label="Notifications"`), with a dot when `page.has_unread_notifications`.
- Repository settings: `views::protection::repo_settings_cards(&RepoHeader)` (B, icon
  `shield`) is added to `settings_cards`.
- Reserved account names gain `issues` and `pulls` (dashboards, stretch);
  `notifications` is already reserved. No new second-level names under `/{owner}/`.
- E2E modules `issues`, `pulls`, `protection` declared in `tests/e2e/main.rs` (empty files).

### 8. Configuration (P0 defaults, documented in each file)

| Section | File | Owner |
|---|---|---|
| `[issues]` `scan_commit_limit` (250 commits per ref update), `notifications` (true), `notification_retention_days` (90) | `config/issues.rs` | I |
| `[pulls]` `merge_timeout_seconds` (120), `max_concurrent_merges` (2), `mergeability_concurrency` (2), `max_commits` (250), `noreply_domain` (`""` = `noreply.<host of server.public_url>`) | `config/pulls.rs` | P |

`StorageConfig::tmp_dir()` is `<data_dir>/tmp` (merges, overlays; never backed up).

### 9. Audit, throttling, icons, CSS (P0)

- Audit actions: `BranchProtectionCreated`, `BranchProtectionChanged`,
  `BranchProtectionDeleted`, `BranchProtectionBypassed` (category "Repository access",
  B and P), `IssueDeleted` (new category "Repository content", I).
- `ThrottleScope::Content`: issues, pull requests, comments and reviews created or edited,
  keyed by account (burst 30, one unit per 10 s, no failure cost).
- Icons: `circle-dot`, `circle-check`, `circle-slash`, `git-pull-request`,
  `git-pull-request-closed`, `git-pull-request-draft`, `git-merge`, `tag`, `milestone`,
  `message-square`, `bell`, `shield`, `lock`, `unlock`, `eye`, `eye-off`, `check`, `x`,
  `file-diff`, `user-plus`; `ui::settings_card` knows `shield`.
- `app.css` components: `.state-badge` (`-open`, `-closed`, `-merged`, `-draft`),
  `.label-chip`, `.timeline`, `.timeline-item`, `.notification-dot`, `.diff-comment`.
  Ask for more in your report; meanwhile use Tailwind token utilities.

---

## Workstream I — Issues

### Scope

1. **Issue pages.** `/{owner}/{repo}/issues` (list), `/issues/new`, `/issues/{n}`. A PR
   number under `/issues/{n}` answers 301 to `/pull/{n}`. Creation: title, Markdown body;
   writers may also set labels, assignees and a milestone (one transaction after the
   core row; then `Opened` is published once). The page shows title, number, state badge,
   "alice opened this issue 2 days ago · 3 comments", the T0 timeline, the comment form,
   and the **sidebar** (assignees, labels, milestone, notifications subscribe/unsubscribe,
   lock, delete for admins). The same sidebar renders on PR pages through the hook.
   Visiting an issue marks its notification thread read.
2. **Labels** (`/{owner}/{repo}/labels`): name (1–50 chars, unique per repository,
   case-insensitive), colour `#rrggbb` (lower-cased, validated strictly), description
   (≤ 100). Writers create, edit, delete; "Create default labels" adds `bug`,
   `documentation`, `duplicate`, `enhancement`, `good first issue`, `help wanted`,
   `invalid`, `question`, `wontfix` when the repository has none. Max 200 per repository,
   25 per issue. Deleting a label removes it from issues without timeline events.
3. **Milestones** (`/{owner}/{repo}/milestones`, `/milestones/new`, `/milestones/{id}`,
   edit, close/reopen, delete): title (1–100, unique per repository, case-insensitive),
   Markdown description (≤ 10 000 bytes), optional due date (a calendar day), open/closed.
   The list shows open/closed counts and a progress bar; an issue has at most one.
4. **Assignees**: at most 10 per issue; only users with `Write` access to the repository
   (owner, collaborators, organisation owners, members whose base permission or team grant
   is `write`/`admin`) — `IssueService::assignable(repo, query)` lists them (≤ 100).
5. **References and mentions** (`rbit-core/src/references.rs`, pure):
   - `#12` → this repository; `owner/repo#12` → another repository; `@name` → a user.
     Ignored inside code spans, code blocks and link text (scan only pulldown-cmark text
     events). Closing keywords: `close`, `closes`, `closed`, `fix`, `fixes`, `fixed`,
     `resolve`, `resolves`, `resolved`, case-insensitive, followed by `:`? and a
     same-repository reference.
   - **Rendering** (`render_issue_markdown`): `#12` becomes a link (with the title as
     `title=`) only if it exists in this repository (batch `find_numbers`); `owner/repo#12`
     always links (no existence check, so nothing leaks); `@name` links to the profile when
     the account exists. Ammonia sanitisation stays the last step. `render/markdown.rs`
     gains a text-rewriting entry point (I owns it in wave B).
   - **Cross-references**: an issue, PR or comment that mentions `#N` or `owner/repo#N`
     records `CrossReferenced { source }` on the target (once per source issue and target),
     shown only to viewers who can read the source (T0 filters).
   - **Commits**: I's background subscriber handles `RepositoryPushed` (skipping pull
     mirrors): for each branch update, the commits it introduced
     (`new_commits(after, [before])`, or excluding all other tips for new branches, at most
     `scan_commit_limit`). Every same-repository `#N` in a commit message records
     `CommitReferenced` once per (issue, commit) (`issue_commit_refs` dedupes). Closing
     keywords close the issue with `close_as_system(reason Completed, Closed { commit })`
     **only when the branch is the default branch**; actor is the pusher (`None` for deploy
     keys). Cross-repository references in commits are ignored. When a PR is merged into
     the default branch (`Issue { kind: Pull, action: Closed { reason: Merged } }`), closing
     keywords in the PR's body close the referenced issues with `Closed { pull: Some(n) }`.
6. **Search and filters.** One query box (default `is:open is:issue`), parsed by
   `rbit-core/src/issue_query.rs` (pure): `is:open|closed|issue|pr|merged|draft|locked`,
   `author:name`, `assignee:name|@me`, `no:assignee`, `label:name` (repeatable, AND;
   quoted names), `-label:name`, `no:label`, `milestone:"title"`, `no:milestone`,
   `sort:created-desc|created-asc|updated-desc|updated-asc|comments-desc`, free text
   (title and body, SQLite **FTS5**, prefix match on the last word). ≤ 20 terms; unknown
   qualifiers are reported in a hint and ignored; `@me` needs sign-in. Open/closed tab
   counts honour the other filters. Filter menus (Author, Label, Milestone, Assignee, Sort)
   are links whose URLs the view model computes. 25 per page. The store supports both
   kinds, so P's list can adopt it after merge.
7. **Notifications** (in-app). Threads, not individual events: one row per (user, issue).
   - Participants (`issue_subscriptions`): the author, commenters, assignees, mentioned
     users, requested reviewers, reviewers; anyone can subscribe or unsubscribe manually
     (an explicit unsubscribe sticks until they subscribe again).
   - A background subscriber handles `Issue`, `IssueComment` and `PullRequestReview`
     events: recipients are the subscribed participants, plus the newly mentioned users
     (comment created or edited: only mentions not notified before), the assignee on
     `Assigned`, the reviewer on `ReviewRequested`; minus the actor, suspended users and
     **anyone without read access to the repository at that moment** (policy with grants).
     The row is upserted unread with the strongest reason (`review_requested` > `assigned`
     > `mention` > `author` > `comment` > `manual`). At most 50 mentions per text are
     honoured.
   - `/notifications`: Unread / All, grouped by repository; each row shows the state icon,
     `owner/repo #N title`, reason, relative time; mark read, mark all read, unsubscribe.
     Rows of repositories the user can no longer read are hidden at display time and
     deleted lazily. Read rows older than `notification_retention_days` are pruned daily.
   - `has_unread` is one indexed `EXISTS`. Disabled entirely by `[issues] notifications = false`.
8. **Dashboards (stretch):** `/issues` and `/pulls` for "created by me / assigned to me /
   mentioning me / review requested" across readable repositories.
9. **Markdown preview (stretch):** `POST /{owner}/{repo}/preview` returns the rendered
   fragment (htmx "Preview" tab on every Markdown textarea; Origin-checked, `Content`
   throttle).
10. **Webhooks** (`issues/hooks.rs`, `issues/json.rs`): `issues` with actions `opened`,
    `edited` (`changes.title.from`, `changes.body.from`), `closed`, `reopened`, `labeled`,
    `unlabeled` (`label`), `assigned`, `unassigned` (`assignee`), `milestoned`,
    `demilestoned` (`milestone`), `locked`, `unlocked`, `deleted`; `issue_comment` with
    `created`, `edited` (`changes.body.from`), `deleted`. Each carries `action`, `issue`
    (`id`, `number`, `title`, `body`, `state`, `state_reason`, `locked`, `user`, `labels`,
    `assignee`, `assignees`, `milestone`, `comments`, `created_at`, `updated_at`,
    `closed_at`, `html_url`, `url`, `repository_url`, `comments_url`, and `pull_request:
    {url, html_url, merged_at}` for PRs), `comment` where relevant, `repository`, `sender`
    and `organization` for organisation repositories. No email addresses anywhere.
11. **REST API** (`api/issues.rs`, `api/notifications.rs`, fragments):
    `GET|POST /repos/{owner}/{repo}/issues` (GitHub filters `state`, `labels`,
    `milestone`, `assignee`, `creator`, `sort`, `direction`, `since`; PRs included with a
    `pull_request` key, as GitHub does), `GET|PATCH …/issues/{number}` (title, body, state,
    state_reason, labels, assignees, milestone), `PUT|DELETE …/issues/{number}/lock`,
    `DELETE …/issues/{number}` (admin scope), `GET|POST …/issues/{number}/comments`,
    `GET|PATCH|DELETE …/issues/comments/{id}`, `GET …/issues/{number}/timeline`,
    `GET|POST …/labels`, `GET|PATCH|DELETE …/labels/{name}`,
    `GET|POST|PUT|DELETE …/issues/{number}/labels`, `DELETE …/issues/{number}/labels/{name}`,
    `GET|POST …/milestones`, `GET|PATCH|DELETE …/milestones/{id}` (by id, a documented
    deviation from GitHub's per-repository milestone numbers), `GET …/assignees`,
    `POST|DELETE …/issues/{number}/assignees`, `GET /notifications`,
    `PUT /notifications` (mark all read), `PATCH /notifications/threads/{id}`,
    `DELETE /notifications/threads/{id}/subscription`. `read` scope for GET, `write` for
    mutations (also for readers commenting), `admin` for deletion.

### Migrations

- `0018_issue_metadata.sql`: `labels(id, repo_id CASCADE, name NOCASE, color CHECK
  (color GLOB '#[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]'), description,
  created_at, updated_at, UNIQUE(repo_id, name))`; `issue_labels(issue_id CASCADE,
  label_id CASCADE, created_at, PK)` + index on `label_id`; `milestones(id, repo_id
  CASCADE, title NOCASE, description, due_on NULL, state CHECK IN ('open','closed'),
  closed_at, created_at, updated_at, UNIQUE(repo_id, title))`; `issue_milestones(issue_id
  PK CASCADE, milestone_id CASCADE)` + index; `issue_assignees(issue_id CASCADE, user_id
  CASCADE, assigned_by SET NULL, created_at, PK)` + index on `user_id`;
  `issue_commit_refs(issue_id CASCADE, commit_sha, created_at, PK)`;
  `issue_cross_refs(target_id → issues CASCADE, source_id → issues CASCADE, created_at, PK)`
  (dedupes `CrossReferenced`).
- `0019_notifications.sql`: `issue_subscriptions(issue_id CASCADE, user_id CASCADE,
  reason CHECK IN ('author','comment','assigned','mention','review_requested','reviewed','manual'),
  subscribed CHECK IN (0,1), updated_at, PK)`; `notifications(id, user_id CASCADE,
  issue_id CASCADE, repo_id CASCADE, reason, unread CHECK IN (0,1), last_actor_id SET
  NULL, updated_at, UNIQUE(user_id, issue_id))` + index `(user_id, unread, updated_at DESC)`.
- `0020_issue_search.sql`: `issues_fts` (FTS5, `content='issues'`, `content_rowid='id'`,
  columns `title`, `body`, `tokenize='unicode61 remove_diacritics 2'`), insert/update/delete
  triggers on `issues`, then `INSERT INTO issues_fts(issues_fts) VALUES ('rebuild')`. The
  bundled SQLite is built with `SQLITE_ENABLE_FTS5`; a store test asserts it.

### Files I owns

Stubs marked ★ exist on `main` after P0.

- core: `rbit-core/src/{references.rs★,issue_query.rs★,config/issues.rs★}`
- storage: `rbit-storage/src/sqlite/{issue_meta.rs★,issue_search.rs★,notifications.rs★}`,
  `rbit-storage/migrations/0018_*.sql`, `0019_*.sql`, `0020_*.sql`, new
  `rbit-storage/tests/{issue_meta,issue_search,notifications}.rs`
- services: `rbit-services/src/{issues.rs★,issues/**★,references.rs★,notifications.rs★}`,
  new `rbit-services/tests/{issues,references,notifications}*.rs`
- web: `rbit-web/src/routes/{issues.rs★,labels.rs★,milestones.rs★,notifications.rs★}`,
  `rbit-web/src/routes/api/{issues.rs★,notifications.rs★}`,
  `rbit-web/src/routes/api/openapi/{issues,notifications}.json★`,
  `rbit-web/src/views/{issues.rs★,labels.rs★,milestones.rs★,notifications.rs★}`,
  `rbit-web/src/render/{references.rs★,markdown.rs}`, `rbit-web/tests/properties.rs`
  (Markdown cases), `rbit-web/templates/{issues,notifications}/**★`
- e2e: `rbit-e2e/tests/e2e/issues.rs★`

### Tests expected

Reference parser table (each form, code spans/blocks, link text, punctuation, `#0`,
overlong numbers, closing keywords with and without colon); query parser table; store
tests for every query, including the FTS5 match, label AND/NOT filters, counts per tab and
`json_each` lists; service rules (label limits and uniqueness, assignee must be a writer,
milestone per issue, deletion by admins only, events published once per change); commit
references: default branch closes, other branches only reference, dedupe across branches,
deploy-key pusher, mirrors skipped; notification fan-out: actor excluded, user without read
access never notified (private repository, and after losing access), mentions in code
ignored, unsubscribe sticks, strongest reason wins; payload shapes field by field; API
handler tests (`oneshot`); view render tests; e2e: an outsider gets 404 for a private
repository's issues (pages and API); open an issue, label it, assign a collaborator, who
sees an unread notification while a mentioned outsider does not; `git push` a commit
"Fixes #1" to a feature branch (issue shows the reference, stays open), then to `main`
(issue closed via commit); an `issues` webhook delivery arrives signed; page through
`GET /repos/{owner}/{repo}/issues` with `Link` headers.

---

## Workstream P — Pull requests

### Scope

1. **Fork network and cross-fork compare.** Two repositories are in the same network when
   their `repo_forks` chains (followed up to 10 levels) reach the same root. PRs may go
   from any readable repository of the network to any other (fork → parent, parent →
   fork, between siblings). `/{owner}/{repo}/compare/{base}...{head}` accepts
   `head` as `branch`, `owner:branch` (that owner's repository in the network) or
   `owner/name:branch`; the page gains base and head repository pickers. Cross-repository
   compare and checks read through an **object overlay** (`rbit-git/src/overlay.rs`): a
   throw-away bare repository under `<tmp_dir>/overlays/<random>` whose
   `objects/info/alternates` lists both repositories' object directories, so nothing is
   written to either repository before a PR exists; dropped (and swept at start-up) after use.
2. **Creating a PR** (`/{owner}/{repo}/pulls/new?base=&head=`, and a "Create pull request"
   button on the compare page): needs read access to both repositories, a head branch that
   exists, base ≠ head, at least one commit ahead, the base repository not a pull mirror,
   and no other **open** PR for the same (base repository, base branch, head repository,
   head branch) — checked inside the creating transaction. Title defaults to the single
   commit's summary or the branch name; Markdown body; draft checkbox. `PullStore::create`
   inserts the issue row (`IssueStore::insert_in`) and the PR row in one transaction, then
   the PR head is stored as **`refs/pull/{n}/head`** in the base repository (same
   repository: `update-ref`; fork: `git fetch --no-tags <fork path>
   +refs/heads/<branch>:refs/pull/{n}/head`, verifying the fetched id), so the PR keeps
   working when the fork is deleted. Publishes `Issue { kind: Pull, action: Opened }`.
3. **Pages.** `/{owner}/{repo}/pulls` (list: Open / Closed tabs, `state=merged`,
   `author=`, 25 per page; rows show state icon, title, labels via `IssueService::metadata`,
   `#n opened … by …`, review summary, comment count) and `/{owner}/{repo}/pull/{n}` with
   tabs **Conversation**, **Commits** (`/commits`, ≤ `max_commits`), **Files changed**
   (`/files`). An issue number under `/pull/{n}` answers 301 to `/issues/{n}`. Header:
   title, `#n`, state badge (Open / Draft / Closed / Merged), "alice wants to merge 3
   commits into `main` from `alice:feature`" (cross-repository heads are prefixed with the
   owner). Conversation = T0 timeline items interleaved by time with P's review entries
   (review state, body and its comments with their stored diff hunk), then the merge box.
   Sidebar: reviewers (request / remove, the review state of each), then I's sidebar hook.
   Visiting marks the notification thread read. Title, body, close/reopen, lock and
   comments use T0's routes.
4. **Head and base updates** (P's background subscriber on `RepositoryPushed`, indexed
   lookups by `(head_repo_id, head_branch)` and `(repo_id, base_branch)` of open PRs):
   - head branch moved: refresh `refs/pull/{n}/head`, record `HeadPushed { before, after,
     count, forced }` (`forced` when `before` is not an ancestor of `after`), publish
     `Synchronized`, mark mergeability unknown;
   - head branch deleted: record `HeadRefDeleted`; the PR stays open but cannot be merged
     until the branch is pushed again;
   - base branch moved: mark open PRs into it unknown; if a PR's head is now an ancestor of
     the base tip, it was **merged outside Rbit**: mark it merged (`merged_by` = pusher,
     strategy `NULL`), record `Merged { strategy: None }`, close it with reason `Merged`.
5. **Mergeability.** A bounded worker (`mergeability_concurrency`) computes, for open
   non-draft PRs with `mergeable = 'unknown'`, a trial merge of head into base on a
   temporary worktree (below) and stores `clean`, `conflict` (with the conflicting paths,
   ≤ 100) or `error`, together with the base and head ids it ran on, so stale results are
   recognisable. Woken by the subscriber; at start-up and every minute it also picks up
   `unknown` rows (nothing is lost across restarts). The page shows "Checking…" while
   unknown.
6. **Reviews.**
   - Line comments on **Files changed**: anchored to `(commit_sha, path, side
     left|right, line)` of the diff from the merge base to head; the stored `diff_hunk`
     (up to 4 lines of context ending at the line) is kept for display. A comment becomes
     **outdated** when the PR head moves and the file's blob at the new head differs from
     the blob at the comment's commit; outdated comments show only in the conversation.
     Replies (`in_reply_to`) form threads. Every diff line has a stable anchor
     `#diff-{sha256(path)[..8]}-{L|R}{line}`; on mobile the line number is the tap target.
     Without JavaScript `?comment=<anchor>` renders the inline form server-side; with htmx
     it is fetched as a fragment.
   - A user may "Add single comment" (an immediate `commented` review) or "Start a review"
     (comments stay **pending**, visible only to their author, one pending review per user
     and PR) and then **submit** with Comment / Approve / Request changes and an optional
     body. The PR author can only comment. Anyone with read access may review; only reviews
     by users who currently have `Write` access count towards requirements.
   - Writers and the author request reviewers (users with read access, ≤ 15; records
     `ReviewRequested`, publishes `ReviewRequested`); submitting a review clears the
     request. Writers may dismiss an approval or change request with a message (records
     `ReviewDismissed`, publishes `PullRequestReview { action: Dismissed }`).
   - Resolving threads is a **stretch** goal.
7. **Merge requirements** (the merge box lists each with a check or a cross):
   not a draft; open; head branch exists; mergeability `clean`; protection
   (`ProtectionService::check_merge`): approvals = distinct writers whose **latest**
   non-pending, non-dismissed review is `approved` (only on the current head if
   `dismiss_stale_approvals`) ≥ `required_approvals`, no writer's latest review is
   `changes_requested` when `block_on_changes_requested`, and `merger_may_push`;
   **checks**: always "No status checks — not supported yet" (`ChecksStatus::NotConfigured`;
   the enum also has `Pending`, `Passing`, `Failing` for a future status API, which nothing
   produces in this wave). A merger with `merger_may_bypass` may merge anyway after
   confirming; that is audited as `BranchProtectionBypassed`.
8. **Merging** (`POST /{owner}/{repo}/pull/{n}/merge`: strategy, commit title and message
   for merge/squash, the expected head id, "delete head branch"): needs `Write` on the base
   repository and all requirements. Serialised per base repository (a per-repository async
   mutex; at most `max_concurrent_merges` overall). `rbit-git/src/merge.rs` runs it on a
   **temporary worktree**:
   1. `git clone --shared --no-checkout --no-tags <base repo> <tmp_dir>/merges/<random>`
      (objects through alternates, nothing copied), `checkout --detach <base tip>`;
   2. **merge**: `merge --no-ff --no-edit -m <message> <head id>`; **squash**:
      `merge --squash <head id>` then `commit -m <message>` authored by the PR author;
      **rebase**: `checkout --detach <head id>`, `rebase --onto <base tip> <merge base>`
      (each commit keeps its author; committer = merger); a conflict aborts with the
      conflicting paths;
   3. push the result back: `git push --porcelain <base repo>
      HEAD:refs/heads/<base> --force-with-lease=refs/heads/<base>:<base tip>` (a
      fast-forward of the base branch). A lease failure is `BaseMoved`: retry once from step
      1, then report "the base branch changed; try again";
   4. wrapped in `PushService::begin`/`complete` with the merger as pusher, so
      `RepositoryPushed` (webhooks, I's commit references) fires as for any push;
   5. record `Merged { commit, strategy }`, `close_as_system(reason Merged)` (publishes
      `Closed { reason: Merged }`), store merge commit, merged base and head ids;
   6. optionally delete the head branch (if `ProtectionService::may_delete_branch` and the
      merger has write access to the head repository): `update-ref -d` with the expected
      id, reported through `PushService` of the head repository.

   Every git invocation: the transports' scrubbed environment, `GIT_CONFIG_NOSYSTEM=1`,
   `GIT_CONFIG_GLOBAL=<null device>`, `GIT_TERMINAL_PROMPT=0`, `LC_ALL=C`, and
   `-c core.autocrlf=false -c core.hooksPath=<null device> -c commit.gpgSign=false
   -c rerere.enabled=false -c gc.auto=0 -c maintenance.auto=false -c core.fsmonitor=false`,
   `--no-verify` on commits, `--end-of-options` before revisions, revisions as full ids or
   `refs/heads/<validated name>`, never options from user input. Identities: name = display
   name (or username), email = `<username>@<noreply_domain>` — Rbit never writes a real
   email address into a commit. Fixed `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE` from the
   service clock. `merge_timeout_seconds` bounds the whole merge (`kill_on_drop`). The
   temporary directory is removed on every exit path (a guard) and swept at start-up;
   nothing in the base repository changes until the final, atomic ref update.
   Default messages: merge `Merge pull request #n from owner/branch` + blank line + title;
   squash `title (#n)` + the list of squashed commit summaries.
9. **Draft PRs**: cannot be merged or have mergeability computed; "Ready for review" and
   "Convert to draft" (author or writers) record and publish the matching actions.
10. **Changing the base branch (stretch):** author or writers pick another branch of the
    base repository; records `BaseChanged`, publishes `Edited { previous_base }`.
11. **Reopening** (`pulls::lifecycle::check_reopen`): refused (422) if it was merged, the
    head branch no longer exists, or another open PR has the same base and head.
12. **Webhooks** (`pulls/hooks.rs`, `pulls/json.rs`): `pull_request` with `opened`,
    `edited` (`changes.title|body|base.ref.from`), `closed` (`pull_request.merged`
    true/false), `reopened`, `synchronize` (`before`, `after`), `labeled`, `unlabeled`,
    `assigned`, `unassigned`, `milestoned`, `demilestoned`, `review_requested`,
    `review_request_removed` (`requested_reviewer`), `ready_for_review`,
    `converted_to_draft`, `locked`, `unlocked`; `pull_request_review` with `submitted`,
    `edited`, `dismissed` and `review` (`id`, `user`, `body`, `state` lower-case,
    `commit_id`, `submitted_at`, `html_url`). The `pull_request` object: `id`, `number`,
    `state`, `locked`, `title`, `body`, `user`, `labels`, `milestone`, `assignees`,
    `requested_reviewers`, `draft`, `head`/`base` (`label`, `ref`, `sha`, `repo`), `merged`,
    `mergeable` (true/false/null), `mergeable_state` (`clean`, `dirty`, `blocked`,
    `unknown`, `draft`), `merged_at`, `merged_by`, `merge_commit_sha`, `comments`,
    `review_comments`, `commits`, `additions`, `deletions`, `changed_files`, `html_url`,
    `url`, `created_at`, `updated_at`, `closed_at`.
13. **REST API** (`api/pulls.rs`, fragment): `GET|POST /repos/{owner}/{repo}/pulls`
    (`state`, `head`, `base`, `sort`, `direction`), `GET|PATCH …/pulls/{number}`,
    `GET …/pulls/{number}/commits`, `GET …/pulls/{number}/files`,
    `GET|PUT …/pulls/{number}/merge` (GET: 204 merged / 404 not; PUT body `merge_method`,
    `commit_title`, `commit_message`, `sha` → 200 `{sha, merged, message}`, 405 when not
    mergeable or blocked, 409 when `sha` is not the head), `GET|POST …/pulls/{number}/reviews`,
    `GET …/pulls/{number}/reviews/{id}`, `PUT …/pulls/{number}/reviews/{id}/dismissals`,
    `GET|POST …/pulls/{number}/comments`, `GET|POST|DELETE …/pulls/{number}/requested_reviewers`.

### Migrations

- `0021_pull_requests.sql`: `pull_requests(issue_id INTEGER PRIMARY KEY → issues CASCADE,
  repo_id → repositories CASCADE, head_repo_id → repositories SET NULL, head_branch,
  base_branch, head_sha, is_draft CHECK IN (0,1), mergeable CHECK IN
  ('unknown','clean','conflict','error') DEFAULT 'unknown', mergeable_base_sha,
  mergeable_head_sha, conflict_paths (JSON array text), merged_at, merged_by → accounts
  SET NULL, merge_strategy CHECK IN ('merge','squash','rebase') NULL, merge_commit_sha,
  merged_base_sha, merged_head_sha, created_at, updated_at)`; indexes on
  `(head_repo_id, head_branch)`, `(repo_id, base_branch)`, `(repo_id, mergeable)`.
- `0022_reviews.sql`: `pull_reviews(id, pull_id → pull_requests CASCADE, reviewer_id SET
  NULL, state CHECK IN ('pending','commented','approved','changes_requested','dismissed'),
  body, commit_sha, created_at, submitted_at, dismissed_by SET NULL, dismissed_at,
  dismissal_message)` + a partial unique index on `(pull_id, reviewer_id) WHERE state =
  'pending'`; `pull_review_comments(id, review_id CASCADE, pull_id CASCADE, author_id SET
  NULL, path, side CHECK IN ('left','right'), line CHECK (line > 0), commit_sha, diff_hunk,
  in_reply_to → pull_review_comments SET NULL, body, outdated CHECK IN (0,1), created_at,
  updated_at)` + index `(pull_id, path)`; `pull_review_requests(pull_id CASCADE, user_id
  CASCADE, requested_by SET NULL, created_at, PK)`.
- `0023`: spare (thread resolution stretch).

### Files P owns

- core: `rbit-core/src/{pull.rs★,config/pulls.rs★}`
- git: `rbit-git/src/{merge.rs★,overlay.rs★,pull_refs.rs★,compare.rs,diff.rs}` (the last
  two additively), new `rbit-git/tests/{merge,overlay}.rs`
- storage: `rbit-storage/src/sqlite/{pulls.rs★,reviews.rs★,forks.rs}`,
  `rbit-storage/migrations/0021_*.sql`, `0022_*.sql`, `0023_*.sql`, new storage tests
- services: `rbit-services/src/{pulls.rs★,pulls/**★,forks.rs}` (`forks.rs` additively:
  network queries), new `rbit-services/tests/pulls*.rs`
- web: `rbit-web/src/routes/{pulls.rs★,compare.rs}`, `rbit-web/src/routes/api/pulls.rs★`,
  `rbit-web/src/routes/api/openapi/pulls.json★`,
  `rbit-web/src/views/{pulls.rs★,compare.rs,commit.rs}` (`commit.rs` additively: optional
  fields with defaults), `rbit-web/templates/pulls/**★`,
  `rbit-web/templates/repo/{compare.html,file_diffs.html}`
- e2e: `rbit-e2e/tests/e2e/pulls.rs★`

### Tests expected

Git tests for each strategy on fixtures (merge commit has two parents and the message;
squash has one parent and the head's tree; rebase is linear, keeps authors, sets the
committer), conflicts reported with their paths and the base repository untouched,
`BaseMoved` when the base changes mid-merge, temporary directories removed after success,
conflict, timeout and panic, no hooks executed (a hook planted in the fixture never runs),
`core.autocrlf` ignored on Windows; overlay compare across a fork; `refs/pull/{n}/head`
follows head pushes and survives fork deletion; approval counting table (latest review per
reviewer, stale with and without `dismiss_stale_approvals`, dismissed, non-writer, own PR,
changes requested); requirements evaluation against `MergeRules` values (P tests with
hand-built `MergeRules`, since B's logic is not on its branch); "merged outside Rbit"
detection; reopen preconditions; payload shapes; API handler tests; view render tests;
e2e: fork a repository, push a branch to the fork, open a PR to the parent in the UI,
leave a line comment, request changes, push a fix (timeline shows the push, the comment
is outdated), approve, squash-merge, `git pull` in a parent clone shows the squash commit,
the PR shows Merged and the head branch can be deleted; a conflicting PR lists the file and
refuses to merge; a private repository's PRs are 404 for outsiders (pages and API); a
`pull_request` webhook `closed` delivery has `merged: true`.

---

## Workstream B — Branch protection

### Scope

1. **Rules** per repository, edited by repository admins: `pattern` (an exact branch name,
   or a glob: `*` matches within one path segment, `**` across segments, `?` one
   character; ≤ 255 bytes; otherwise the characters a branch name may contain), and
   - `deny_force_push` (default on), `deny_deletion` (default on),
   - `require_pull_request` (default off) with `required_approvals` 0–10 (> 0 requires
     `require_pull_request`), `dismiss_stale_approvals`, `block_on_changes_requested`
     (default on),
   - `restrict_pushers` with allowed users (who must have `Write` access) and, for
     organisation repositories, allowed teams of that organisation (≤ 50 each),
   - `allow_admin_bypass` (default **off**: rules apply to administrators too).
   At most 50 rules per repository; patterns unique per repository. Branches are
   case-sensitive. Rules only concern `refs/heads/*`.
2. **Combination:** every matching rule applies; the strictest value wins (any denial
   denies, the highest approval count, restrictions intersect: a pusher must be allowed by
   every restricting rule).
3. **Push decisions** (`check_push`, pure core in `rbit-core/src/protection.rs`,
   exhaustively tested over rule flags × actor (anonymous/deploy key/reader/writer/admin
   with and without bypass, listed user, team member) × update kind (create,
   fast-forward-or-unknown update, delete) × reserved refs):
   - any update of `refs/pull/*` by a client: refused ("refs/pull/ is managed by Rbit");
   - delete with `deny_deletion`: refused ("protected branch: deleting it is not allowed");
   - pusher not allowed by `restrict_pushers` (deploy keys never are): refused, also for
     creation ("protected branch: you are not allowed to push to it");
   - update of an existing branch with `require_pull_request`: refused ("protected branch:
     changes must go through a pull request"); creating such a branch is allowed;
   - update of an existing branch with `deny_force_push`: allowed, and sets
     `deny_non_fast_forwards`;
   - an admin with `allow_admin_bypass` passes every branch rule (never the `refs/pull/`
     rule); each bypass that changed the outcome is audited as `BranchProtectionBypassed`
     with the ref in the detail.
   Refusal texts never list who is allowed.
4. **Protocol** (`rbit-transport/src/receive.rs`): `intercept`, `Replay`, `drain`,
   `refusal` and `CommandList` as in shared contract §4 (capabilities from the first
   command line, `shallow` lines passed through, malformed or oversized input →
   `ReceiveError`, no allocation proportional to the pack).
5. **Merge rules** (`check_merge`, `may_delete_branch`, `protected_branches`) as in §3,
   from the same pure core.
6. **UI**: `/{owner}/{repo}/settings/branches` (list: pattern in monospace, summary badges
   "No force pushes", "No deletion", "2 approvals", "Restricted"; "Add rule"),
   `/settings/branches/new`, `/settings/branches/{id}` (edit, with "Matches: main,
   release/1.0" computed from existing branches), delete with confirmation. Allowed users
   are added by username, teams picked from a list. A settings card (icon `shield`) on the
   repository settings page. The branches page shows a "Protected" badge (shield icon)
   via `protected_branches`. Changes are audited (`BranchProtectionCreated`/`Changed`/`Deleted`).
7. **REST API** (`api/protection.rs`, fragment, `admin` scope, repository admins only;
   others get 404 when they cannot read the repository and 403 otherwise):
   `GET|POST /repos/{owner}/{repo}/branch_protections`,
   `GET|PATCH|DELETE /repos/{owner}/{repo}/branch_protections/{id}`. Object: `id`,
   `pattern`, the flags above, `required_approvals`, `push_allowlist_users` (logins),
   `push_allowlist_teams` (names), `created_at`, `updated_at`.

### Migrations

- `0024_branch_protection.sql`: `branch_protections(id, repo_id CASCADE, pattern,
  deny_force_push, deny_deletion, require_pull_request, required_approvals CHECK
  (required_approvals BETWEEN 0 AND 10), dismiss_stale_approvals,
  block_on_changes_requested, restrict_pushers, allow_admin_bypass (all flags `CHECK IN
  (0,1)`), created_by SET NULL, created_at, updated_at, UNIQUE(repo_id, pattern),
  CHECK (required_approvals = 0 OR require_pull_request = 1))`;
  `branch_protection_users(rule_id CASCADE, user_id CASCADE, PK)`;
  `branch_protection_teams(rule_id CASCADE, team_id → teams CASCADE, PK)`.
- `0025`: spare.

### Files B owns

- core: `rbit-core/src/protection.rs★`
- transport: `rbit-transport/src/receive.rs★`, new `rbit-transport/tests/receive.rs`
- storage: `rbit-storage/src/sqlite/protection.rs★`, `rbit-storage/migrations/0024_*.sql`,
  `0025_*.sql`, new `rbit-storage/tests/protection.rs`
- services: `rbit-services/src/protection.rs★`, new `rbit-services/tests/protection.rs`
- web: `rbit-web/src/routes/{protection.rs★,api/protection.rs★}`,
  `rbit-web/src/routes/api/openapi/protection.json★`,
  `rbit-web/src/views/{protection.rs★,refs.rs}`, `rbit-web/templates/protection/**★`,
  `rbit-web/templates/repo/refs.html`
- e2e: `rbit-e2e/tests/e2e/protection.rs★`

### Tests expected

Pattern matching table; the exhaustive decision matrix against an independent reference
table in the test; protocol: parsing with and without capabilities, `shallow` lines, push
options and pack bytes left untouched by `Replay` (byte-for-byte), oversize and malformed
input, and the exact refusal bytes for `report-status`, `report-status-v2`,
`side-band-64k`, `side-band` and none; store and service tests (limits, validation, team
must belong to the organisation, audit entries); view render tests; e2e **over HTTP and
over SSH** (SSH tests skip without the OpenSSH client): a force push to a protected `main`
is rejected and `main` is unchanged; deleting it is rejected; a non-allowed writer is
rejected while an allowed one pushes; a direct push to a `require_pull_request` branch is
rejected; a force push of an unprotected branch in a separate push still works; pushing
`refs/pull/1/head` is rejected; an admin with bypass can force-push and the audit log shows
it; a push to a pull mirror is still refused by I1's check first.

---

## Security rules (all workstreams)

- **404 for anything the viewer may not know exists**: every new page and endpoint opens
  the repository through `RepositoryService::open` (pages) or `Caller::open_repo` (API);
  issue and PR numbers that do not exist are 404; numbers are parsed as `IssueNumber`
  (anything else is 404). Inside a readable repository every issue and PR is visible.
  403 only for actions on things the caller can see.
- **Mutations** are POST forms on the page router (Origin check, session) or token-scoped
  API calls (no cookies). No GET has side effects except marking a notification read.
- **Throttle** creation and editing with `ThrottleScope::Content`; the push gate adds no
  new secret check.
- **Markdown** always goes through `render_issue_markdown` (ammonia last). Label colours
  appear only as an SVG `fill` on a dot (validated `#rrggbb`), progress bars as SVG
  `width` attributes: **no inline `style=`, no inline scripts**.
- **Privacy:** no email address in any page, payload, API object or Rbit-made commit;
  notifications only for users who can read the repository; mentions never reveal whether
  a user can read it; cross-references shown only to viewers of the source; refusal
  messages never list allowed pushers.
- **Git safety:** validated names only, `--end-of-options`, scrubbed environments, no
  hooks, temporary directories with random names under `tmp_dir()` only, atomic final ref
  updates, bounded time and concurrency.
- **Limits:** titles 256 characters, bodies and comments 64 KiB, timeline pages of 200,
  PR commits `max_commits`, diffs with the existing `DiffLimits`, ≤ 10 000 push commands.

## UI notes (all workstreams)

Monochrome minimal; mobile-first at **375 px**, then `sm:`/`md:`. Reuse `.box`,
`.box-header`, `.row`, `.btn` (`-primary`/`-ghost`/`-danger`), `.input`, `.label`,
`.hint`, `.badge`, `.tab`, `.menu`, `ui::settings_card` and P0's new components. **Colours
come only from theme tokens** — Tailwind token utilities (`bg-surface`, `bg-subtle`,
`border-line`, `text-ink`, `text-ink-muted`, `text-ink-faint`, `text-success`,
`text-danger`, `text-warning`, `bg-inverted text-inverted-ink`) or `var(--…)` in P0's CSS
— never palette classes or literal colours, so all ~500 themes work. Status mapping: open
`--success`, closed `--ink-muted`, merged `--inverted` badge, draft `--ink-faint` outline,
conflicts / changes requested `--danger`, approved / mergeable `--success`, pending /
checking `--warning`. Label text stays `--ink` on `--surface` with a coloured dot, so labels
are legible in every theme.

- Lists: one row per item; on phones labels wrap under the title and secondary metadata
  collapses to one muted line. Filter menus are `<details class="menu">`.
- Issue and PR pages: the sidebar sits beside the timeline from `md:`; on phones it is a
  `<details>` "Details" block under the header. Comment forms are full width; buttons at
  least 44 px tall on touch.
- Files changed: unified diffs only (they fit phones); each file is a `<details open>`.
- Everything works without JavaScript; htmx only swaps fragments (edit forms, inline
  review forms, preview). Links with side effects or downloads carry `hx-boost="false"`.
- Every template that calls macros starts with `{% import "macros.html" as ui %}`; never
  add a `macros.html` in a subfolder.

## Rules (all workstreams)

As in [milestone 2](milestone-2.md#rules-all-workstreams), with this ownership update.
**Integrator files** now also include everything wave A created that is not reassigned
above — in particular `rbit-core/src/{issue.rs,webhook.rs}`,
`rbit-services/src/{timeline.rs,webhooks.rs,webhooks/**}`,
`rbit-storage/src/sqlite/issues.rs`, `rbit-transport/src/{lib.rs,process.rs}`,
`rbit-web/src/routes/{timeline.rs,api/mod.rs,api/openapi.json,repo.rs}`,
`rbit-web/src/views/{timeline.rs,repo.rs}`, `rbit-web/src/urls.rs`,
`rbit-web/templates/{timeline/**,repo/subnav.html}` and the rest of
`templates/repo/**`. Requests for these go in your final report with the exact change.
Only `sqlite/issues.rs` writes the T0 tables. Use only the hook signatures of §3 across
workstreams; never call another workstream's internals.

## Integration

Merge order: **B** (independent), then **I** and **P** in either order. After each:
rebase in its worktree, full gate, fast-forward, apply its integration requests as small
commits. After all three, the integrator adds `rbit-e2e/tests/e2e/workflow.rs`: `main`
protected with `require_pull_request` and one approval; direct pushes refused over HTTP
and SSH; a PR from a fork whose body says "Fixes #1" is approved by a writer and
squash-merged; issue #1 closes with "closed via #2"; the issue author has an unread
notification; `pull_request` (`closed`, `merged: true`) and `issues` (`closed`) webhooks
are delivered. Then: verify an upgrade from a wave A database through `0024`, exclude
`tmp/` from `rbit backup`, update `docs/ARCHITECTURE.md` (schema, routes, events, the
push gate) and `deploy/rbit.example.toml`.

---

## P0 pre-wire checklist (integrator, on `main` after I1)

Core and storage
- [x] `rbit-core/src/issue.rs` complete (types, `TimelineEvent`, `IssueAction`,
      `CommentAction`, `ReviewAction`, `issue_permissions` with exhaustive tests); stub
      modules `references.rs`, `issue_query.rs`, `pull.rs`, `protection.rs`;
      `config/{issues,pulls}.rs` with defaults; `StorageConfig::tmp_dir()`; reserved account
      names `issues`, `pulls`.
- [x] `WebhookEvent::{Issues, IssueComment, PullRequest, PullRequestReview}` (+ `parse`,
      labels, descriptions, `SUBSCRIBABLE`).
- [x] Stub stores with `Database` accessors: `issue_meta()`, `issue_search()`,
      `notifications()`, `pulls()`, `reviews()`, `branch_protection()`.
- [x] Audit actions and the "Repository content" category; `ThrottleScope::Content`.

Services
- [x] `DomainEvent::{Issue, IssueComment, PullRequestReview}`.
- [x] Stub services and `Services` fields: `issues` (`IssueService`), `references`,
      `notifications`, `pulls` (`PullService`), `protection` (`ProtectionService`), with
      the §3 signatures; modules `issues/{hooks,json}.rs`, `pulls/{hooks,json,lifecycle}.rs`;
      `spawn_background`/`shutdown` include `issues`, `notifications`, `pulls`.
- [x] `WebhookService::handle_event` dispatch to `issues::hooks` / `pulls::hooks`.

Git and transport
- [x] Stub `rbit-git/src/{merge.rs,overlay.rs,pull_refs.rs}`; `rbit-transport/src/receive.rs`
      (`Intercept`, `Replay` complete, `CommandList`, `CommandLimits`, `Refusal`,
      `ReceiveError`, stub `intercept`/`refusal`, real `drain`).
- [x] `ProcessTransport` receive entry points take `ReceiveOptions`; stderr exposed for SSH.
- [x] Push-gate glue in `routes/git_http.rs` and `rbit-ssh/src/session.rs` (after I1's
      checks); SSH receive-pack in two steps; existing transport e2e tests green.

Web
- [x] Stub route modules merged into `page_routes`: `issues`, `labels`, `milestones`,
      `notifications`, `pulls`, `protection` (and `timeline` with T0).
- [x] Stub view modules: `issues` (incl. `sidebar` + `IssueSidebar`), `labels`,
      `milestones`, `notifications`, `pulls`, `protection` (incl. `repo_settings_cards`);
      `render/references.rs`; empty template folders with the sidebar stub.
- [x] API: `api/{issues,notifications,pulls,protection}.rs` stubs in `table()`;
      `api/openapi/*.json` fragments; merged OpenAPI document and its test.
- [x] `RepoHeader.{open_issues, open_pulls}` (zero until T0), Issues / Pull requests tabs;
      bell + `Page.has_unread_notifications`; `shield` settings card; icons; CSS components.
- [x] E2E module stubs `issues`, `pulls`, `protection`.

### P0 as built (where reality refined the contract)

P0 is done. These details differ from, or pin down, the sections above; they are
binding like the rest of this document.

- **`RefUpdate` lives in `rbit_core::ref_update`** (re-exported unchanged as
  `rbit_git::RefUpdate` and `rbit_services::events::RefUpdate`), so
  `rbit-transport` (which now depends on `rbit-core` only) can return it from
  `CommandList::updates()`.
- **The push-gate types live in `rbit_core::protection`**: `Refusal`, `PushActor`,
  `PushVerdict` (`allow()`, `is_refused()`) and `MergeRules` (`unprotected()`,
  `is_protected()`). `rbit_services::protection` and `rbit_transport::receive`
  re-export them; B may add its pure decision functions next to them.
- **`receive.rs` extras:** `PushCommand { old, new, name }` (`update()`), `ZERO_ID`;
  `CommandList::{new, commands, updates, capabilities, has_capability, shallow, is_empty,
  wants_report}`; `Replay::new(saved, inner, Remaining)` where `Remaining::{Nothing,
  ToEnd}` tells `drain` whether anything follows the saved bytes (over SSH a deletion-only
  push never sends EOF, so `intercept` must say `Nothing` when every command is a deletion
  and any push options are already among the saved bytes); `refusal` returns an **empty**
  vector when the client asked for no report, and the glue then answers `403` (HTTP) or
  stderr + exit 1 (SSH) with `refusal_message(&refusals)`.
- **`ProcessTransport`:** `serve(service, path, protocol, ReceiveOptions, request)` (HTTP;
  options ignored for upload-pack), `advertisement(service, path, protocol)` (raw, without
  `# service=`), `receive_pack(path, protocol, ReceiveOptions, input, output, errors)`
  (stateless, stderr streamed: the second SSH step). `advertise_refs` and
  `serve_interactive` are unchanged.
- **Glue behaviour:** an `Intercept::Read` with an empty command list ends the SSH
  exchange with status 0 without git; a refusal with a report is sent as git would (HTTP
  200 with the result content type, SSH exit status 0); `check_push` errors are HTTP 500 /
  SSH "internal error"; `intercept` errors are HTTP 400 / SSH `ERROR: <message>` + exit 1.
  HTTP pushes pass `PushActor { user: Some(..), deploy_key: false, .. }` (deploy keys
  exist only over SSH).
- **`IssueNumber`:** `parse` accepts canonical decimals only (`"012"`, `"+1"`, `"0"` are
  errors — a new `NameError::NotANumber`); `Display` writes `#12`, use `get()` in URLs.
  Titles and texts fail with `IssueTextError`. `IssueText` also turns lone `\r` into `\n`.
  `TimelineEvent::kind()` gives the `kind` column; serde's adjacent tagging
  (`{"kind": …, "data": {…}}`, unit variants without `data`) is the stored shape.
- **`comment_permissions(viewer, IssuePermissions, &Comment) -> CommentPermissions {
  can_edit, can_delete }`** joins `issue_permissions`: editing your own comment needs
  `can_comment` (a locked thread freezes readers' comments), deleting needs ownership
  with `can_comment`, or `can_moderate`.
- **`IssueContext`** is declared in P0 (`rbit_services::timeline`, re-exported at the
  crate root), because the sidebar hook takes it; T0 adds `TimelineService` to that file.
- **Hook shapes:** `HookPayload` is defined in `rbit_services::webhooks`;
  `ReferenceSet` is an empty struct I fills; `IssueMetadata { labels: Vec<LabelSnapshot>,
  assignees: Vec<Account>, milestone: Option<MilestoneRef { id, title }> }`;
  `issues::json::{issue(&Issue, &OwnedRepository, base_url), label(&LabelSnapshot, ..),
  milestone(&MilestoneRef, ..)}`; `NotificationService::mark_thread_read` returns `()`
  (it logs). The `Page` extractor skips `has_unread` when `[issues] notifications = false`.
- **Stub lint markers:** async stubs returning `Result` carry
  `#[expect(clippy::unused_async, clippy::unused_async_trait_impl, …)]` (this clippy
  fires both), `()` stubs `#[expect(clippy::unused_async, …)]`, stub fields
  `#[expect(dead_code, …)]`. Remove them when the code is real.
- **CSS:** besides the listed components P0 adds `.timeline-event` and `.timeline-badge`
  (an event row and its icon circle on the timeline rule).

T0 (before launching I and P)
- [ ] `0017_issues_core.sql`, `IssueStore`, `TimelineService`, `routes/timeline.rs`,
      `views/timeline.rs`, `templates/timeline/**`, `urls::issue_url`, open counts in the
      header — with store tests (concurrent creation yields distinct numbers, counters
      survive deletion, comment counts, timeline order and cursor), service tests
      (permissions, events published, locked threads, throttling, `Merged` refused),
      route tests (404 for hidden repositories and unknown numbers, 303 targets by kind)
      and view render tests.
