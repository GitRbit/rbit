//! One running import: the Git copy, LFS objects, labels, milestones, issues and pull
//! requests with their comments, and releases with their files, in that order.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! **Nothing here publishes a [`crate::DomainEvent`].** History is written straight
//! through the stores (never through `TimelineService`, `IssueService`, `PullService`,
//! `ReleaseService::create` or `PushService`), so imported issues, comments, pull requests,
//! releases and the copied refs send no notifications, emails or webhooks and are not
//! scanned for references or closing keywords. The only event of an import is the
//! `RepositoryCreated` of the empty repository it starts with.
//!
//! A failure in the Git copy stops the import. Later, a failure to *list* something
//! (the token was refused, the rate limit outlasts the wait) stops it too, keeping
//! everything imported so far; a problem with a single item (an issue number already
//! taken, a release file that is too large) becomes a warning and the import goes on.

use std::{collections::HashMap, time::Duration};

use rbit_core::{
    AccountId, IssueKind, IssueNumber, IssueState, IssueText, IssueTitle, LabelId, MilestoneId, StateReason, TimelineEvent, Timestamp,
    User,
    config::AuthorMapping,
    import::{ImportOptions, ImportPhase, parse_timestamp, truncate_bytes},
    issue::{MAX_TEXT_BYTES, MAX_TITLE_CHARS},
    lfs::LfsOid,
    mirror::MirrorUrl,
    release::{MAX_RELEASE_NAME_CHARS, MAX_RELEASE_NOTES_CHARS, validate_asset_name},
};
use rbit_git::{
    RefKind,
    remote::{Remote, fetch_import, lfs_pointers},
};
use rbit_repositories::imports::{
    authors::{AuthorDirectory, AuthorMap, AuthorRule, SourceAuthor, attributed},
    items::{self, Closing, Limits, lookup_key},
    progress::{ImportProgress, phase_plan},
};
use rbit_storage::sqlite::{
    imports::ImportedPull,
    issue_meta::{LabelFields, MilestoneFields},
    issues::{ImportedClosing, ImportedComment, ImportedEvent, ImportedThread},
    releases::NewRelease,
    repo_settings::RepoGeneralSettings,
};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};

use super::{
    api,
    client::{SourceClient, SourceError},
};
use crate::{LfsService, ReleaseService, RepoContext, ServiceContext, ServiceError, push_mirrors::basic_authorization};

/// Objects per LFS batch request.
const LFS_BATCH: usize = 100;
/// The user name sent with the token over Git and LFS (the token is the password).
const TOKEN_USERNAME: &str = "x-access-token";
/// The text limits of threads and releases, which their features own.
const LIMITS: Limits = Limits {
    title_chars: MAX_TITLE_CHARS,
    text_bytes: MAX_TEXT_BYTES,
    release_name_chars: MAX_RELEASE_NAME_CHARS,
    release_notes_chars: MAX_RELEASE_NOTES_CHARS,
};

/// Why an import stopped, as shown on its page.
pub type Fatal = String;

/// What a job needs from its service.
#[derive(Debug, Clone)]
pub struct JobServices {
    pub ctx: ServiceContext,
    pub lfs: LfsService,
    pub releases: ReleaseService,
}

/// A running import.
#[derive(Debug)]
pub struct Job {
    services: JobServices,
    client: SourceClient,
    source: api::Repo,
    /// The repository, opened for the importer (who administers it).
    repo: RepoContext,
    importer: User,
    options: ImportOptions,
    progress: ImportProgress,
    authors: AuthorMap<AccountId>,
    labels: HashMap<String, LabelId>,
    milestones: HashMap<String, MilestoneId>,
}

impl Job {
    pub fn new(
        services: JobServices,
        client: SourceClient,
        source: api::Repo,
        repo: RepoContext,
        importer: User,
        options: ImportOptions,
    ) -> Self {
        let rule = match services.ctx.config.imports.author_mapping {
            AuthorMapping::None => AuthorRule::None,
            AuthorMapping::Email => AuthorRule::Email,
            AuthorMapping::EmailOrUsername => AuthorRule::EmailOrUsername,
        };
        Self {
            services,
            client,
            source,
            repo,
            importer,
            authors: AuthorMap::new(rule),
            options,
            progress: ImportProgress::default(),
            labels: HashMap::new(),
            milestones: HashMap::new(),
        }
    }

    fn ctx(&self) -> &ServiceContext {
        &self.services.ctx
    }

    fn repo_id(&self) -> rbit_core::RepoId {
        self.repo.repo.repo.id
    }

    /// Runs every phase the options ask for. `Err` carries why the import stopped.
    pub async fn run(&mut self) -> Result<(), Fatal> {
        self.enter(ImportPhase::Repository).await;
        self.copy_repository().await?;
        for phase in phase_plan(self.options, self.ctx().config.lfs.enabled) {
            self.enter(phase).await;
            match phase {
                ImportPhase::Lfs => self.copy_lfs().await,
                ImportPhase::Labels => self.import_labels().await,
                ImportPhase::Milestones => self.import_milestones().await,
                ImportPhase::Issues => self.import_issues().await,
                ImportPhase::Releases => self.import_releases().await,
                ImportPhase::Repository | ImportPhase::Finished => Ok(()),
            }
            .map_err(|e| e.to_string())?;
        }
        self.save().await;
        Ok(())
    }

    // ------------------------------------------------------------------ progress

    async fn enter(&mut self, phase: ImportPhase) {
        self.progress.enter(phase);
        self.save().await;
    }

    async fn save(&mut self) {
        self.progress.saved();
        let store = self.ctx().database.imports();
        let progress = &self.progress;
        if let Err(error) = store.save_progress(self.repo_id(), progress.phase, &progress.counts, &progress.warnings).await {
            tracing::warn!(%error, repo = %self.repo.repo.full_name(), "cannot save import progress");
        }
    }

    /// Counts one imported item and saves now and then.
    async fn tick(&mut self) {
        if self.progress.tick() {
            self.save().await;
        }
    }

    fn warn(&mut self, message: impl AsRef<str>) {
        let message = self.progress.warn(message.as_ref());
        tracing::info!(repo = %self.repo.repo.full_name(), warning = %message, "import warning");
    }

    // ------------------------------------------------------------------ Git

    /// Whether the token may travel to `url`: only to the source's own origins.
    fn token_for(&self, url: &MirrorUrl) -> Option<String> {
        self.client.git_token(url.as_str()).map(|token| basic_authorization(TOKEN_USERNAME, token))
    }

    fn clone_url(&self) -> Result<MirrorUrl, Fatal> {
        self.source.clone_url.parse().map_err(|e: rbit_core::mirror::MirrorUrlError| format!("the source's clone URL: {e}"))
    }

    async fn copy_repository(&mut self) -> Result<(), Fatal> {
        let url = self.clone_url()?;
        let config = &self.ctx().config;
        let resolution = super::check_source_host(config, url.host(), url.port()).await?;
        let authorization = self.token_for(&url);
        let remote =
            Remote { url: url.as_str(), authorization: authorization.as_deref(), allow_local: false, resolution: Some(&resolution) };
        let timeout = Duration::from_secs(config.imports.git_timeout_seconds.max(1));
        let fetched = {
            let _lease = self.ctx().repositories.lease(self.repo_id()).await;
            fetch_import(&config.repository.git_binary, &self.repo.disk_path, remote, self.options.issues, timeout).await
        };
        if let Err(error) = fetched {
            return Err(match error {
                rbit_git::remote::RemoteError::Git(error) => {
                    tracing::error!(%error, "an import could not run git");
                    "copying the Git repository could not run; see the server log".to_owned()
                }
                other => format!("copying the Git repository failed: {other}"),
            });
        }
        if let Err(error) = self.ctx().database.repositories().mark_pushed(self.repo_id()).await {
            tracing::warn!(%error, "cannot record an imported repository as pushed");
        }
        self.adopt_default_branch().await;
        Ok(())
    }

    /// Makes the source's default branch this repository's (or the first branch, when the
    /// configured default does not exist).
    async fn adopt_default_branch(&mut self) {
        let Ok(refs) = self.repo.reader.refs().await else { return };
        let branches: Vec<&str> = refs.iter().filter(|r| r.kind == RefKind::Branch).map(|r| r.name.as_str()).collect();
        let current = &self.repo.repo.repo;
        let wanted = self.source.default_branch.as_deref().filter(|b| branches.contains(b));
        let branch = match wanted {
            Some(branch) => branch.to_owned(),
            None if branches.contains(&current.default_branch.as_str()) => return,
            None => match branches.first() {
                Some(branch) => (*branch).to_owned(),
                None => return,
            },
        };
        if branch == current.default_branch {
            return;
        }
        let settings = RepoGeneralSettings {
            description: current.description.clone(),
            visibility: current.visibility,
            default_branch: branch.clone(),
        };
        let ctx = self.ctx();
        let updated = async {
            ctx.repositories.set_head(current.owner_id, current.id, &branch).await?;
            ctx.database.repo_settings().update_general(current.id, &settings).await?;
            Ok::<_, ServiceError>(())
        };
        if let Err(error) = updated.await {
            tracing::warn!(%error, repo = %self.repo.repo.full_name(), "cannot adopt the source's default branch");
            return;
        }
        self.repo.repo.repo.default_branch = branch;
    }

    // ------------------------------------------------------------------ LFS

    async fn copy_lfs(&mut self) -> Result<(), SourceError> {
        let git = self.ctx().config.repository.git_binary.clone();
        let pointers = match lfs_pointers(&git, &self.repo.disk_path).await {
            Ok(pointers) => pointers,
            Err(error) => {
                tracing::error!(%error, "cannot list LFS pointers of an imported repository");
                self.warn("Git LFS objects were not copied: the repository could not be read");
                return Ok(());
            }
        };
        if pointers.is_empty() {
            return Ok(());
        }
        let Ok(url) = self.clone_url() else { return Ok(()) };
        let endpoint = items::lfs_batch_endpoint(url.as_str());
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.git-lfs+json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/vnd.git-lfs+json"));
        if let Some(mut value) = self.token_for(&url).and_then(|header| HeaderValue::from_str(&header).ok()) {
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        let store = self.ctx().database.lfs();
        for chunk in pointers.chunks(LFS_BATCH) {
            let oids: Vec<LfsOid> = chunk.iter().map(|p| p.oid.clone()).collect();
            let present = store.linked_objects(self.repo_id(), &oids).await.unwrap_or_default();
            let wanted: Vec<_> = chunk.iter().filter(|p| !present.contains_key(&p.oid)).collect();
            if wanted.is_empty() {
                continue;
            }
            let request = api::BatchRequest {
                operation: "download",
                transfers: ["basic"],
                objects: wanted.iter().map(|p| api::BatchObjectRequest { oid: p.oid.as_str(), size: p.size }).collect(),
            };
            let response: api::BatchResponse = match self.client.post_json(&endpoint, &headers, &request).await {
                Ok(response) => response,
                Err(error @ (SourceError::Cancelled | SourceError::RateLimited(_) | SourceError::Unauthorized)) => return Err(error),
                Err(error) => {
                    self.warn(format!("Git LFS objects were not copied: {error}"));
                    return Ok(());
                }
            };
            for object in response.objects {
                self.copy_lfs_object(object).await?;
            }
        }
        Ok(())
    }

    async fn copy_lfs_object(&mut self, object: api::BatchObject) -> Result<(), SourceError> {
        let Ok(oid) = LfsOid::parse(&object.oid) else { return Ok(()) };
        if let Some(error) = object.error {
            self.warn(format!("Git LFS object {oid}: {}", error.message));
            return Ok(());
        }
        let Some(action) = object.actions.and_then(|actions| actions.download) else {
            self.warn(format!("Git LFS object {oid}: the source offered no download"));
            return Ok(());
        };
        let mut headers = HeaderMap::new();
        for (name, value) in &action.header {
            if let (Ok(name), Ok(mut value)) = (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) {
                value.set_sensitive(true);
                headers.insert(name, value);
            }
        }
        let download = match self.client.download(&action.href, &headers, false).await {
            Ok(download) => download,
            Err(error @ (SourceError::Cancelled | SourceError::RateLimited(_))) => return Err(error),
            Err(error) => {
                self.warn(format!("Git LFS object {oid}: {error}"));
                return Ok(());
            }
        };
        match self.services.lfs.store(&self.repo, &oid, object.size, download).await {
            Ok(()) => {
                self.progress.counts.lfs_objects += 1;
                self.tick().await;
            }
            Err(error) => self.warn(format!("Git LFS object {oid}: {}", user_message(&error))),
        }
        Ok(())
    }

    // ------------------------------------------------------------------ labels and milestones

    async fn import_labels(&mut self) -> Result<(), SourceError> {
        let labels: Vec<api::Label> = self.client.all("/labels", "").await?;
        let meta = self.ctx().database.issue_meta();
        let now = Timestamp::now();
        for label in labels {
            let item = match items::label(&label.name, &label.color, label.description.as_deref()) {
                Ok(item) => item,
                Err(warning) => {
                    self.warn(warning);
                    continue;
                }
            };
            let name = item.name.clone();
            let fields = LabelFields { color: item.color, description: item.description, name: item.name };
            let created = match meta.create_label(self.repo_id(), &fields, now).await {
                Ok(created) => Some(created),
                Err(rbit_storage::StorageError::Conflict(_)) => meta.find_label_by_name(self.repo_id(), &name).await.ok().flatten(),
                Err(error) => {
                    self.warn(format!("label {name}: {}", user_message(&error.into())));
                    None
                }
            };
            if let Some(created) = created {
                self.labels.insert(lookup_key(&name), created.id);
                self.progress.counts.labels += 1;
                self.tick().await;
            }
        }
        Ok(())
    }

    async fn import_milestones(&mut self) -> Result<(), SourceError> {
        let milestones: Vec<api::Milestone> = self.client.all("/milestones", "state=all").await?;
        let meta = self.ctx().database.issue_meta();
        for milestone in milestones {
            let Some((title, description)) = items::milestone(&milestone.title, milestone.description.as_deref(), LIMITS) else { continue };
            let due_on = milestone.due_on.as_deref().and_then(parse_timestamp).map(|at| items::due_date(&at.iso8601()));
            let fields = MilestoneFields { title: title.clone(), description, due_on };
            let created_at = milestone.created_at.as_deref().and_then(parse_timestamp).unwrap_or_else(Timestamp::now);
            let created = match meta.create_milestone(self.repo_id(), &fields, created_at).await {
                Ok(created) => created,
                Err(error) => {
                    self.warn(format!("milestone {title}: {}", user_message(&error.into())));
                    continue;
                }
            };
            if milestone.state == "closed" {
                let at = milestone.closed_at.as_deref().and_then(parse_timestamp).unwrap_or(created_at);
                if let Err(error) = meta.set_milestone_state(self.repo_id(), created.id, IssueState::Closed, at).await {
                    self.warn(format!("milestone {title} stays open: {}", user_message(&error.into())));
                }
            }
            self.milestones.insert(lookup_key(&title), created.id);
            self.progress.counts.milestones += 1;
            self.tick().await;
        }
        Ok(())
    }

    // ------------------------------------------------------------------ authors

    /// The local account `user` maps to, if the instance maps authors at all.
    async fn author(&mut self, user: Option<&api::User>) -> Option<AccountId> {
        let user = user?;
        let directory = Directory { client: &self.client, ctx: &self.services.ctx };
        self.authors.resolve(&directory, &user.login, user.email.as_deref()).await
    }

    /// `text` as written by `user` (see [`attributed`]).
    fn attributed(&self, text: &str, user: Option<&api::User>, mapped: bool) -> String {
        let source = self.client.source();
        let author = user.map(|user| SourceAuthor { login: &user.login, html_url: user.html_url.as_deref() });
        attributed(text, author, mapped, &source.web_base, source.kind.platform())
    }

    // ------------------------------------------------------------------ issues and pull requests

    async fn import_issues(&mut self) -> Result<(), SourceError> {
        let pulls: Vec<api::Pull> = self.client.all("/pulls", "state=all").await?;
        let mut pulls: HashMap<u32, api::Pull> = pulls.into_iter().map(|pull| (pull.number, pull)).collect();
        let mut comments: HashMap<u32, Vec<api::Comment>> = HashMap::new();
        for comment in self.client.all::<api::Comment>("/issues/comments", "sort=created&direction=asc").await? {
            if let Some(number) = comment.number() {
                comments.entry(number).or_default().push(comment);
            }
        }

        // Threads created here meanwhile must not take a number the import still needs.
        let mut newest = self.client.pages("/issues", "state=all&sort=created&direction=desc");
        let newest: Vec<api::Issue> = newest.next().await?.unwrap_or_default();
        let highest = items::highest_number(newest.iter().map(|issue| issue.number).chain(pulls.keys().copied()));
        if let Some(highest) = highest.and_then(IssueNumber::new)
            && let Err(error) = self.ctx().database.issues().reserve_numbers(self.repo_id(), highest).await
        {
            tracing::warn!(%error, "cannot reserve issue numbers for an import");
        }

        let mut pages = self.client.pages("/issues", "state=all&sort=created&direction=asc");
        while let Some(issues) = pages.next::<api::Issue>().await? {
            for issue in issues {
                let number = issue.number;
                let pull = pulls.remove(&number);
                let thread_comments = comments.remove(&number).unwrap_or_default();
                self.import_thread(issue, pull, thread_comments).await;
                self.tick().await;
            }
        }
        Ok(())
    }

    async fn import_thread(&mut self, issue: api::Issue, pull: Option<api::Pull>, comments: Vec<api::Comment>) {
        let Some(number) = IssueNumber::new(issue.number) else { return };
        let is_pull = issue.pull_request.is_some() || pull.is_some();
        if is_pull && pull.is_none() {
            self.warn(items::unknown_branches(number.get()));
            return;
        }
        let author = self.author(issue.user.as_ref()).await;
        let created_at = parse_timestamp(&issue.created_at).unwrap_or_else(Timestamp::now);
        let updated_at = issue.updated_at.as_deref().and_then(parse_timestamp).unwrap_or(created_at).max(created_at);
        let closed_at = issue.closed_at.as_deref().and_then(parse_timestamp);

        // An open pull request stays open only when its head commit came along.
        let mut note = None;
        let mut open_pull = None;
        if let Some(pull) = &pull {
            open_pull = self.open_pull_heads(number, pull).await;
            if issue.state == "open" && pull.merged_at.is_none() && open_pull.is_none() {
                note = Some(items::closed_pull_note(self.client.source().kind.platform()));
            }
        }

        let threads_comments = self.comments(comments, created_at).await;
        let comment_count = u64::try_from(threads_comments.len()).unwrap_or_default();

        let merged_at = pull.as_ref().and_then(|p| p.merged_at.as_deref()).and_then(parse_timestamp);
        let merged_by = match pull.as_ref().and_then(|p| p.merged_by.clone()) {
            Some(user) => self.author(Some(&user)).await,
            None => None,
        };
        let not_planned = issue.state_reason.as_deref() == Some("not_planned");
        let closing =
            items::closing(merged_at.is_some(), issue.state == "closed", is_pull && open_pull.is_none(), not_planned, note.is_some());
        let at = closed_at.unwrap_or(updated_at);
        let closed = match (closing, merged_at) {
            (Closing::Merged, Some(merged)) => Some(ImportedClosing { at: merged, reason: StateReason::Merged, by: merged_by }),
            (Closing::Completed, _) => Some(ImportedClosing { at, reason: StateReason::Completed, by: None }),
            (Closing::NotPlanned, _) => Some(ImportedClosing { at, reason: StateReason::NotPlanned, by: None }),
            (Closing::Open | Closing::Merged, _) => None,
        };
        let events = closing_events(closed.as_ref(), pull.as_ref());

        let title = items::thread_title(&issue.title, LIMITS);
        let title = IssueTitle::parse(&title).or_else(|_| IssueTitle::parse(items::UNTITLED));
        let body = issue.body.as_deref().unwrap_or_default();
        let mut text = self.attributed(body, issue.user.as_ref(), author.is_some());
        if let Some(note) = &note {
            text.push_str(note);
        }
        let (Ok(title), Ok(body)) = (title, IssueText::body(truncate_bytes(&text, MAX_TEXT_BYTES))) else { return };
        let thread = ImportedThread {
            repo: self.repo_id(),
            number,
            kind: if is_pull { IssueKind::Pull } else { IssueKind::Issue },
            title,
            body,
            author: author.unwrap_or_else(|| self.importer.id()),
            created_at,
            updated_at,
            closed,
            locked: issue.locked,
            comments: threads_comments,
            events,
        };
        let inserted = match &pull {
            Some(pull) => {
                let row = self.pull_row(pull, open_pull.as_ref(), merged_at, merged_by);
                self.ctx().database.imports().insert_pull(&thread, &row).await
            }
            None => self.ctx().database.issues().insert_imported(&thread).await,
        };
        let issue_id = match inserted {
            Ok(inserted) => inserted.id,
            Err(rbit_storage::StorageError::Conflict(_)) => {
                self.warn(items::thread_exists(number.get()));
                return;
            }
            Err(error) => {
                tracing::warn!(%error, number = number.get(), "cannot import a thread");
                self.warn(items::thread_failed(number.get()));
                return;
            }
        };
        if is_pull {
            self.progress.counts.pulls += 1;
        } else {
            self.progress.counts.issues += 1;
        }
        self.progress.counts.comments += comment_count;
        self.attach_metadata(issue_id, &issue, created_at).await;
    }

    /// A thread's comments with their original times, attributed to their authors.
    async fn comments(&mut self, comments: Vec<api::Comment>, fallback: Timestamp) -> Vec<ImportedComment> {
        let mut imported = Vec::with_capacity(comments.len());
        for comment in comments {
            let mapped = self.author(comment.user.as_ref()).await;
            let text = self.attributed(items::comment_text(comment.body.as_deref()), comment.user.as_ref(), mapped.is_some());
            let Ok(body) = IssueText::comment(truncate_bytes(&text, MAX_TEXT_BYTES)) else { continue };
            let at = parse_timestamp(&comment.created_at).unwrap_or(fallback);
            let edited = comment.updated_at.as_deref().and_then(parse_timestamp).unwrap_or(at);
            imported.push(ImportedComment {
                author: mapped.unwrap_or_else(|| self.importer.id()),
                body,
                created_at: at,
                updated_at: edited,
            });
        }
        imported
    }

    /// The branch data of an imported pull request: live when it stays open (or its head
    /// came along), otherwise history with the source's names and ids.
    fn pull_row(
        &self,
        pull: &api::Pull,
        open: Option<&OpenPull>,
        merged_at: Option<Timestamp>,
        merged_by: Option<AccountId>,
    ) -> ImportedPull {
        let merged = merged_at.map(|at| (at, items::merge_commit(pull.merge_commit_sha.as_deref())));
        match open {
            Some(open) => ImportedPull {
                head_repo: open.same_repository.then_some(self.repo_id()),
                head_branch: pull.head.name.clone(),
                base_branch: pull.base.name.clone(),
                head_sha: open.head_sha.clone(),
                is_draft: pull.draft,
                merged,
                merged_by,
            },
            None => ImportedPull {
                head_repo: None,
                head_branch: items::non_empty(&pull.head.name, "unknown"),
                base_branch: items::non_empty(&pull.base.name, &self.repo.repo.repo.default_branch),
                head_sha: items::history_head(&pull.head.sha),
                is_draft: false,
                merged,
                merged_by,
            },
        }
    }

    async fn attach_metadata(&mut self, issue_id: rbit_core::IssueId, issue: &api::Issue, at: Timestamp) {
        let meta = self.ctx().database.issue_meta();
        for label in &issue.labels {
            if let Some(id) = self.labels.get(&lookup_key(&label.name))
                && let Err(error) = meta.add_label(issue_id, *id, at).await
            {
                tracing::warn!(%error, "cannot label an imported thread");
            }
        }
        if let Some(milestone) = &issue.milestone
            && let Some(id) = self.milestones.get(&lookup_key(&milestone.title))
            && let Err(error) = meta.set_milestone(issue_id, Some(*id)).await
        {
            tracing::warn!(%error, "cannot set the milestone of an imported thread");
        }
        for assignee in issue.assignees.clone().unwrap_or_default() {
            if let Some(user) = self.author(Some(&assignee)).await
                && let Err(error) = meta.add_assignee(issue_id, user, self.importer.id(), at).await
            {
                tracing::warn!(%error, "cannot assign an imported thread");
            }
        }
    }

    /// For a pull request that is open on the source: its head commit
    /// (`refs/pull/{n}/head`) when it was fetched and the base branch exists here.
    async fn open_pull_heads(&self, number: IssueNumber, pull: &api::Pull) -> Option<OpenPull> {
        if pull.merged_at.is_some() {
            return None;
        }
        let reader = &self.repo.reader;
        let head = reader.commit_id(format!("refs/pull/{}/head", number.get())).await.ok()?;
        reader.commit_id(format!("refs/heads/{}", pull.base.name)).await.ok()?;
        let source = self.client.source().full_name();
        let same_repository = items::same_repository(pull.head.repo.as_ref().map(|repo| repo.full_name.as_str()), &source)
            && reader.commit_id(format!("refs/heads/{}", pull.head.name)).await.is_ok();
        Some(OpenPull { head_sha: head, same_repository })
    }

    // ------------------------------------------------------------------ releases

    async fn import_releases(&mut self) -> Result<(), SourceError> {
        let mut releases: Vec<api::Release> = self.client.all("/releases", "").await?;
        // Listed newest first; oldest first gives ids in the order they were made.
        releases.reverse();
        for release in releases {
            self.import_release(release).await?;
        }
        Ok(())
    }

    async fn import_release(&mut self, release: api::Release) -> Result<(), SourceError> {
        let store = self.ctx().database.releases();
        let Some((tag, name, notes)) = items::release(&release.tag_name, release.name.as_deref(), release.body.as_deref(), LIMITS) else {
            return Ok(());
        };
        if store.find_by_tag(self.repo_id(), &tag).await.ok().flatten().is_some() {
            self.warn(items::release_exists(&tag));
            return Ok(());
        }
        let author = self.author(release.author.as_ref()).await;
        let new = NewRelease {
            repo_id: self.repo_id(),
            tag_name: &tag,
            name: &name,
            notes: &notes,
            is_draft: release.draft,
            is_prerelease: release.prerelease,
            author_id: Some(author.unwrap_or_else(|| self.importer.id())),
            source: rbit_core::release::ReleaseSource::User,
        };
        let id = match store.create(new).await {
            Ok(id) => id,
            Err(error) => {
                tracing::warn!(%error, tag, "cannot import a release");
                self.warn(items::release_failed(&tag));
                return Ok(());
            }
        };
        let created_at = release.created_at.as_deref().and_then(parse_timestamp).unwrap_or_else(Timestamp::now);
        let published_at = release.published_at.as_deref().and_then(parse_timestamp);
        if let Err(error) = self.ctx().database.imports().backdate_release(id, created_at, published_at).await {
            tracing::warn!(%error, tag, "cannot keep a release's original dates");
        }
        self.progress.counts.releases += 1;
        self.tick().await;
        for asset in release.assets {
            self.import_asset(id, &tag, asset).await?;
        }
        Ok(())
    }

    async fn import_asset(&mut self, release: rbit_core::ReleaseId, tag: &str, asset: api::Asset) -> Result<(), SourceError> {
        let name = asset.name.clone();
        let max = self.ctx().config.releases.max_asset_bytes;
        if let Some(refusal) = items::asset_refusal(tag, &name, asset.size, max, validate_asset_name(&name)) {
            self.warn(refusal);
            return Ok(());
        }
        // GitHub: the API URL with `Accept: application/octet-stream` (works for private
        // repositories); Gitea: the download URL.
        let gitea = self.client.source().kind == rbit_core::import::SourceKind::Gitea;
        let Some((url, accept)) = items::asset_download(gitea, asset.url.as_deref(), asset.browser_download_url.as_deref()) else {
            self.warn(format!("release {tag}: {name} has no download URL"));
            return Ok(());
        };
        let (url, api_auth) = (url.to_owned(), true);
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static(accept));
        let download = match self.client.download(&url, &headers, api_auth).await {
            Ok(download) => download,
            Err(error @ (SourceError::Cancelled | SourceError::RateLimited(_))) => return Err(error),
            Err(error) => {
                self.warn(format!("release {tag}: {name}: {error}"));
                return Ok(());
            }
        };
        let content_type = asset.content_type.clone().unwrap_or_default();
        let releases = self.services.releases.clone();
        match releases.upload_asset(&self.repo, &self.importer, release, &name, &content_type, download).await {
            Ok(stored) => {
                if let Some(warning) = items::incomplete_asset(tag, &name, stored.size, asset.size) {
                    self.warn(warning);
                    if let Err(error) = releases.delete_asset(&self.repo, release, stored.id).await {
                        tracing::warn!(%error, "cannot remove an incomplete imported release file");
                    }
                } else {
                    self.progress.counts.assets += 1;
                    self.tick().await;
                }
            }
            Err(error) => self.warn(format!("release {tag}: {name}: {}", user_message(&error))),
        }
        Ok(())
    }
}

/// The timeline event that explains how an imported thread was closed (merged pull
/// requests: "merged …", recorded as merged outside rbit).
fn closing_events(closed: Option<&ImportedClosing>, pull: Option<&api::Pull>) -> Vec<ImportedEvent> {
    let Some(closing) = closed else { return Vec::new() };
    let event = match (closing.reason, pull) {
        (StateReason::Merged, Some(pull)) => {
            TimelineEvent::Merged { commit: items::merged_event_commit(pull.merge_commit_sha.as_deref(), &pull.head.sha), strategy: None }
        }
        (reason, _) => TimelineEvent::Closed { reason, commit: None, pull: None },
    };
    vec![ImportedEvent { actor: closing.by, event, at: closing.at }]
}

/// What an open pull request needs to stay open.
#[derive(Debug)]
struct OpenPull {
    head_sha: String,
    /// The head branch is a branch of the source repository itself (and came along).
    same_repository: bool,
}

/// A service error in words for a warning (internal details stay in the log).
fn user_message(error: &ServiceError) -> String {
    match error {
        ServiceError::Internal(inner) => {
            tracing::warn!(error = %inner, "import item failed");
            "an internal error occurred; see the server log".to_owned()
        }
        other => other.to_string(),
    }
}

/// Author lookups for the feature's author map: the source's profile and local accounts.
struct Directory<'a> {
    client: &'a SourceClient,
    ctx: &'a ServiceContext,
}

impl AuthorDirectory for Directory<'_> {
    type Account = AccountId;

    async fn source_email(&self, login: &str) -> Option<String> {
        self.client.user(login).await.ok().and_then(|profile| profile.email)
    }

    async fn verified_email(&self, email: &str) -> Option<AccountId> {
        self.ctx.database.imports().user_with_verified_email(email).await.ok().flatten()
    }

    async fn named(&self, login: &str) -> Option<AccountId> {
        self.ctx.database.imports().user_named(login).await.ok().flatten()
    }
}
