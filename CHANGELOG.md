# Changelog

All notable changes to rbit are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security

- Update `zstd` to 0.14.3, `zstd-safe` to 8.2.0 and `zstd-sys` to 2.1.2 for
  RUSTSEC-2026-0391, RUSTSEC-2026-0398, RUSTSEC-2026-0403 and RUSTSEC-2026-0410; they reach
  rbit through `async-compression`.
- Update `pulldown-cmark` to 0.13.5 for RUSTSEC-2026-0414. rbit's Markdown never enables
  wikilinks, the only affected option.

### Added

- `rbit` is also a command-line client for an instance's API, in the manner of `gh`: runs,
  logs, workflows, artifacts, pull requests and raw API calls (`docs/cli.md`).
- `rbit-runner install`, `uninstall`, `start`, `stop` and `status` run the runner with
  systemd, launchd or Windows (a service, or a logon task for jobs that need the desktop).
- Actions: reusable workflows (`workflow_call`, `jobs.<id>.uses`), `workflow_run` and
  `repository_dispatch` triggers, environment reviewers and wait timers, container and
  service `options` and named volumes, `runs-on` and matrices from earlier jobs' outputs,
  problem matchers, custom shells, zip artifacts, status badges and the permissions API.
- **rbit Actions**, rbit's own CI/CD, off unless `[actions] enabled = true`
  ([docs/actions.md](docs/actions.md)). Workflows in `.rbit/workflows` (or
  `.forgejo/workflows`, `.github/workflows`) use a GitHub-style subset: `push`,
  `pull_request`, `workflow_dispatch`, `schedule`, `workflow_run` and `repository_dispatch`
  triggers, matrices, `needs`, job and step
  `if:` with GitHub's expressions, `fail-fast`, `max-parallel`, concurrency groups,
  containers and services, and the built-in `rbit/checkout`, `rbit/cache`,
  `rbit/upload-artifact` and `rbit/download-artifact` (with `actions/*` aliases).
- **rbit-runner**, shipped in every release archive and as `ghcr.io/gitrbit/rbit-runner`:
  registers with a single-use token, runs jobs in Docker or Podman containers (no privileges,
  no Docker socket, a network per job) or on the host, masks secrets in logs, and offers
  `rbit-runner exec` to run a workflow locally. With the container backend the runner
  follows no link and opens no pipe or device a job leaves in its mounted directories, trusts
  no `.git` there (every one under a container checkout's path, so a planted submodule
  `.git` cannot run hooks on the host), and pauses the job's container while built-in steps
  work on them; containers drop `MKNOD`, no job variable reaches the container engine's own
  process, and checkout's `git` uses only HTTPS (and the instance's own scheme) with hooks and
  `fsmonitor` disabled, submodules included.
- An **Actions** tab with runs, run pages, job pages with live logs and raw downloads, and
  artifacts; settings for the repository switch, the default token permission, secrets
  (sealed with AES-256-GCM under a key file kept out of the database), variables, trusted
  authors and runners at repository, organisation, account and instance level.
- Pull requests from forks and from people without write access run read-only, without
  secrets or variables, after a writer approves them.
- `workflow_run` and `workflow_job` webhooks, and `/api/v1/repos/{owner}/{repo}/actions/…`
  endpoints for runs, jobs, job logs, artifacts, workflows, dispatch, secrets, variables,
  runners and registration tokens (runners, secrets and variables for organisations and users
  too).
- **Run workflow** on a `workflow_dispatch` workflow's page, with its inputs at the chosen
  branch or tag; workflows on the default branch are listed before they ever ran.
- Workflows post commit statuses and comment on issues and pull requests with their token,
  shown as `rbit-actions[bot]`.
- What steps write to `GITHUB_STEP_SUMMARY` is shown as Markdown on the run page.
- `rbit_actions_*` metrics: finished runs and jobs, queue and job durations, runner calls,
  log bytes, workflow errors, waiting jobs and online runners.

### Upgrade notes

- Migrations `0048` to `0057` add the Actions tables, a `source` column on commit statuses
  and comments (and releases), step summaries, the Actions permissions and owner policies, the
  `workflow_run` and `repository_dispatch` events, access to private reusable workflows and
  environment wait timers. Nothing changes until an administrator enables Actions; with it off no Actions
  table is written and `<data_dir>/actions` is not created.
- With Actions on, `<data_dir>/actions/secrets.key` seals secrets. `rbit backup` does not
  copy it: back it up separately. Job logs and artifacts under `<data_dir>/actions` are not
  backed up.

## [0.1.5] - 2026-10-25

### Security

- Update `crypto-bigint` to 0.7.9, replacing the yanked 0.7.6 release while retaining
  its constant-time arithmetic fixes and incorporating the subsequent upstream corrections.
- Dismiss approvals when a pull request's base branch changes, refuse merges whose
  expected base no longer matches (`base` on the API merge endpoint), rate-limit base
  changes and refuse them on pull requests closed concurrently.
- Throttle current-password checks in settings per account, stop successful sign-ins from
  refilling a client's failure budget, and share the sign-in budget between an account's
  username and email.
- Require a one-time setup token, logged at start-up or set with `security.setup_token`,
  to sign up the first account of a fresh instance. Cap sessions at
  `security.session_max_age_days` (default 90) however active they stay.
- Limit Git over HTTP and SSH with a shared process cap, per-client request limits, idle
  and total timeouts and `repository.max_push_bytes` (default 2 GiB); never run
  repository hooks; limit archive downloads per client with idle and total timeouts.
- Encrypt TOTP seeds, webhook and push mirror secrets, server signing keys and secret
  configuration fields at rest with a key file (`<data_dir>/secret.key`, or
  `storage.secret_key_file`). Existing values are encrypted on the first start; back up
  the key file with the database. Retire obsolete plaintext backups after the rollback
  window is complete.
- Migration `0044` widens push mirror secrets to fit their encrypted form.
- Create backups owner-only (0700 directory, 0600 files) and include the key file.
- Ask for a push mirror's credential again when its URL moves to another host.
- Count commits by a user's private account email on their contribution calendar only
  for that user and site administrators.
- Prefix ids in rendered Markdown with `user-content-` so they cannot collide with page
  anchors; in-document links are rewritten to match.
- Send `Strict-Transport-Security` with an `https://` public URL (`security.hsts`,
  `security.hsts_include_subdomains`), and add `security.external_images` to keep pages
  from loading images on other sites.
- Refuse to make a fork public while a repository it was forked from is private.
- Look up single API comments directly instead of loading their whole timeline.
- Keep local data, databases and keys out of the Docker build context.

### Added

- Collect notification emails into a daily or weekly email digest under Settings →
  Notifications, at a chosen hour (UTC). Digests are grouped by repository, leave out
  threads already read in the app, keep the one-click unsubscribe headers and are never
  sent twice, also after a restart. Immediate emails stay the default. Administrators set
  the default digest hour under Site administration → Settings (`[notifications]
  digest_hour`, default 8), and `rbit_notification_digests_total{outcome}` counts sent and
  failed digests.
- Archive repositories from the settings danger zone or with `PATCH /api/v1/repos/{owner}/{repo}`
  (`"archived": true`). Archived repositories are read-only: pushes over HTTP and SSH,
  merges, browser edits, issues, pull requests, comments, reviews, labels, milestones,
  releases, commit statuses and LFS uploads and locks are refused (the API answers 403
  "Repository was archived so is read-only."), while reading, cloning, forking and starring
  keep working. Pull and push mirrors pause until the repository is unarchived. Pages show
  a banner and hide write controls, lists mark archived repositories, and Explore filters
  them (`?archived=true|false`). Archiving and unarchiving are audited and send a
  `repository` webhook (`archived` / `unarchived`); repository objects gain `archived`.
- Export SQLite contention metrics: connection pool waits, write lock waits, write
  transaction durations and `SQLITE_BUSY`/`SQLITE_BUSY_SNAPSHOT` errors by store
  (`rbit_sqlite_*`, see the deployment guide's metrics table).

### Fixed

- Wait for repository readers and background jobs to release their files before deletion,
  preventing intermittent failures on Windows after a push or while viewing settings.
- Editing an issue or pull request's title or description no longer fails with "database
  is locked" when another write happens at the same moment: the edit now waits for the
  write lock instead of reading first and failing to upgrade.
- Setting an issue's milestone no longer fails with "database is locked" under
  concurrent writes, for the same reason.

### Performance

- Build the bundled SQLite with the query planner stability guarantee
  (`SQLITE_ENABLE_QPSG`, set in `.cargo/config.toml`), so a statement is no longer
  re-parsed and re-planned on every execution once `PRAGMA optimize` has recorded
  `sqlite_stat4` samples or when it has a `LIKE ?`. With 16 concurrent requests this
  more than doubles issue list and issue page throughput. Mention completion seeks its
  name range explicitly, as the planner no longer derives it from the bound pattern.
- Resolve a full commit id without gix's abbreviated-id lookup, which re-read the object
  database's directory state on every call. Repository pages pass the commit they
  resolved to each later read, so this was several file system scans per page.
- Read the repository page lookup, the dashboards' and inbox's repository check, a
  thread's author and a thread's metadata through scalar subqueries by key instead of
  joins, so their plans no longer turn into scans of `repositories`, `accounts` or
  `issues` when the planner's statistics describe tiny tables, as on a new instance.
- Count the stars of both repository lists on a signed-in home page in one statement.
- Remember a pull request's comparison counts from its Files changed tab only when the
  cache does not hold them yet, instead of committing a redb write on every view.
- Recheck access to the repositories of the `/issues` and `/pulls` dashboards' rows and of
  the notification inbox in one statement per page, instead of up to three per row.
- Show a collaborator's "Leave" box on the repository home from the grants read when the
  repository was opened, instead of reading the collaborator row again.
- Read only the labels of the pull request list's rows, the only metadata it shows.
- Decide whether the viewer may delete a pull request's head branch from the access
  already computed for the request when the branch is in the base repository, instead
  of opening that repository again.
- Mark a viewed issue or pull request's notification read only when that thread has an
  unread one (read with the sidebar's subscription), and refresh the header's unread dot
  only when something was marked, instead of whenever the viewer had anything unread.
- Read one issue or pull request's labels, assignees and milestone in one statement
  instead of three, for its page, the API and single-row lists.
- Read an issue or pull request's author together with the thread, in one statement.
- Load a repository page's header (the viewer's grants, stars, readable forks, parent,
  mirror and open issue and pull request counts) in the same statement as the repository:
  one statement instead of six anonymous, seven signed in. Access is still decided from
  that row, on every request, before anything is shown.
- Count the SQLite acquisitions, statements and insights cache writes each request makes
  in tests (`rbit_storage::count_work`), and pin the counts of the busiest pages, so a
  change cannot add a query to them unnoticed.
- Spell highlighted spans' classes from a static table and size highlighted output once,
  so highlighting allocates nothing per span.
- Keep the widest line's width with each cached highlighted file, so file and blame views
  served from the cache no longer rescan every line to size the code column.
- Build diff rows with numeric line numbers and text moved out of the computed diff, so
  large commit, compare and pull request diffs no longer allocate three strings per row.
- Render pull request Files changed rows like any other diff: the file carries where
  comments go and one delegated handler opens the form from a line number (one tab stop
  per file, moved with the arrow keys), instead of two htmx links per line. Without
  scripts, each hunk header links to the form at its first line.
- Mount diff pages from the content an htmx swap brings in, instead of searching the whole
  document after every swap.
- Configure the Markdown sanitiser once per process instead of once per document with
  repository links.
- Cache rendered READMEs, Markdown files and release notes by content and location (8 MiB,
  least recently used first), so viewing a document again skips rendering it. Comments are
  never cached.
- Mark a viewed issue or pull request's notification read only when the viewer has unread
  notifications, so other signed-in views no longer take the database's write lock.
- Load a pull request conversation's sidebar, merge box parts and closed head branch
  concurrently, and count merge box reviews from the ones the page already loaded.

- Skip htmx's scan of every attribute for `hx-on` handlers at startup and after each
  swap; rbit uses none, and its Content Security Policy would block them.

- Correct the performance notes: page JavaScript ships bundled and minified as
  `app.bundle.js`.

- Darken scrollbars through the root's hover state instead of a universal `:hover`
  rule, so moving the pointer no longer restyles each element it crosses.

- Keep htmx history snapshots of at most 64,000 characters (was 200,000); larger
  pages are refetched on back and forward instead of slowing every navigation.

- Size long file and diff placeholders once per page load and history restore instead
  of twice.

- Move the diff file tree's current-file marker without a colour fade, so scrolling a
  large commit, compare or pull request no longer repaints the whole tree for every frame
  of the fade; hovering a row still fades in.

- Hide the activity calendar's tooltip on scroll by remembering which one is shown,
  instead of searching the whole document on every scroll event of every page.

- Let the headless scroll measurements (`scripts/perf/sweep.mjs`, `trace.mjs`) sign in
  as the fixture's account with `--credentials`, to measure signed-in pages.

- Look for diagrams, reply buttons, mirror progress, language listings, repository
  documents and the render time only within newly swapped content, not the whole page.

- Check whether a page is large enough to skip its history snapshot by looking for its
  4,000th element instead of counting all of them.

- Collect links preloaded once visible as htmx swaps them in, instead of searching the
  whole page for them after every navigation.

- Answer asset revalidations carrying a current `ETag` with `304 Not Modified`, and send
  embedded and pre-compressed asset bytes without copying them for each response.

- Keep the 650 KB source artwork of the logo out of the binary: it moved to
  `scripts/icons/logo.png`, where `npm run icons` reads it; nothing served it.

- Disable Nagle's algorithm on accepted HTTP connections, and show an nginx upstream with
  keep-alive connections to rbit in the deployment guide.

- Load the Mermaid renderer from a content-hashed, immutable URL instead of revalidating
  it for every diagram, and cache theme stylesheets linked by the theme picker.

- Leave the inlined stylesheets out of pages fetched by htmx (boosted navigation, preloads,
  history restores), which discards `<head>` anyway, and render them without copying.

- Put the deferred page scripts before the inlined stylesheets so browsers start fetching
  them sooner.

- Keep three htmx history snapshots instead of ten, since htmx rewrites the whole cache on
  every navigation.

- Serve `/assets/branding.css` with the static assets and cache it for good at the URL
  pages link, which names the current branding colours.

- Stop counting commits behind the base for pull request tabs, creation, header counts and
  API statistics; only the compare page shows that number.

- Build pull request webhook payloads only when a webhook subscribes to the event; each
  one diffed the pull request.

- Cache pull request line and file statistics with the header counts, so API pull
  request lists and pull request webhooks reuse them instead of diffing each pull request.
  Counts cached by earlier versions are recomputed once.

- Check review comments for outdated files with one repository read per push instead of
  one per comment.

- Resolve only the default branch of each repository for profile and organization
  contribution calendars instead of every branch.

- Persist the last commit of every entry of any directory listed in full, not only of
  the default branch's root, so revisiting a directory or branch does not walk history
  again once the short in-memory reuse expires.

- Write each repository's commit-graph after pushes and during periodic maintenance, and
  read root trees from it in last-commit walks. Existing repositories get one at the
  first maintenance run after upgrading.

- Count a repository's open issues and pull requests from the open index entries only,
  instead of every thread the repository ever had.

- Build issue and pull request list queries from the filters in use only, and read the
  default newest-first page in index order up to its limit. Upgrade note: migration
  `0045` adds the index `issues_by_repo_created`.

- Search assignees among a repository's possible writers instead of checking every account.

- Resolve `@mentions` through the account name index instead of reading every account.

- Read a signed-in request's user, password-change restriction and unread notification
  indicator in one database query instead of three.

- Report whether opening a thread marked its notification read, so pages only check the
  unread indicator again when it may have changed.

- Load the dashboard's repositories, recent repositories and organizations concurrently.

- Refresh SQLite's query planner statistics (`PRAGMA optimize`) after migrations and on
  the maintenance schedule.

### Upgrade notes

- Back up the database, repositories and attachments before upgrading from 0.1.4.
  Startup applies migrations `0044`–`0047` automatically; migrations only move forward.
  To roll back, restore the complete pre-upgrade backup before starting the older binary.
- Migration `0044` widens push mirror secrets for encryption. On first startup, existing
  TOTP seeds, webhook and push mirror secrets, server signing keys and secret configuration
  fields are encrypted with `<data_dir>/secret.key` (or `storage.secret_key_file`). Keep
  that key with every post-upgrade backup; a database containing sealed values cannot be
  opened without its original key. The backup command includes it as `secret.key`; restore
  it to the configured key-file location together with the matching database. Replace
  obsolete plaintext backups after the upgrade and rollback window are complete.
- Migration `0045` adds the issue-list ordering index. Existing repositories receive a
  commit-graph at the first maintenance run, and SQLite planner statistics refresh after
  migrations and during scheduled maintenance.
- Migration `0046` adds `repo_archives`. Existing repositories remain unarchived.
- Migration `0047` adds email delivery preferences and digest tables. Existing users keep
  immediate notification emails until they choose a daily or weekly digest. Digest hours
  use UTC; the administrator default is `[notifications] digest_hour = 8`.
- Fresh instances require the one-time setup token from startup logs (or
  `security.setup_token`) to register the first administrator. Existing instances need no
  setup token. Active sessions now have a maximum age of 90 days by default
  (`security.session_max_age_days`); older sessions require a new sign-in.
- Restart with the new binary or container image to apply the Git transfer limits,
  security headers and refreshed web assets. Review the documented configuration defaults
  in [deploy/rbit.example.toml](deploy/rbit.example.toml) before changing limits.

## [0.1.4] - 2026-10-24

### Security

- Update `gix` to 0.90.0 to prevent untrusted repository configuration from disabling
  filesystem path protections
  ([GHSA-q54r-jf2p-rfr2](https://github.com/advisories/GHSA-q54r-jf2p-rfr2)).
- Update `crypto-bigint` to 0.7.6 and `ctutils` to 0.4.5 to address constant-time
  arithmetic issues ([RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html)).
- Include the `ctutils` constant-time comparison fixes
  ([RUSTSEC-2026-0385](https://rustsec.org/advisories/RUSTSEC-2026-0385.html)).

### Added

- Resolve and reopen pull request review conversations as a repository writer or the
  conversation's author. Resolved conversations collapse with their replies in the
  conversation and Files changed tabs; state changes appear in the timeline.
- Change an open pull request's base branch from its Conversation tab, with searchable
  branch choices for the author and repository writers.
- Change your username from account settings with current-password confirmation. Old
  profile links, repository URLs and Git HTTP/SSH remotes follow the current account
  until another account claims the old name; private redirects require repository access.
- Preview Markdown before submitting issue and pull request descriptions, comments,
  reviews and browser file edits, with repository links and sanitized HTML.
- Add personal `/issues` and `/pulls` dashboards with involvement tabs, issue-query filters,
  visibility-aware counts and global pagination.
- Notify in the footer and administrator dashboard when a newer stable rbit release is
  available, using a bounded daily background check of the upstream GitHub release feed.
  Release checks never send the running version, instance identity or repository data.
- Add independent `[updates] check_for_updates` and `report_version` controls. Disabling
  reporting hides public version output and omits version numbers from outgoing
  integration User-Agents; administrator-only diagnostics remain available.

### Changed

- Update `async-compression` to 0.4.51 and its compression codec dependencies.
- Move the repository commit calendar from Commits to a dedicated Activity tab, keeping
  commit history focused on the selected branch and its paginated commits.

### Fixed

- Clarify the README introduction to describe rbit as Git hosting and a Git forge.
- Clarify private security reporting, remove the broken conduct-reporting contact,
  and add automated checks for GitHub repository configuration and submission paths.

### Upgrade notes

- Back up the database before upgrading. Startup applies migrations `0041`–`0043`;
  migrations only move forward. To roll back, restore the pre-upgrade backup before
  starting the previous binary.
- Migration `0041` adds review conversation resolution metadata. Existing conversations
  remain unresolved.
- Migration `0042` adds account-name redirects and their maintenance triggers. Existing
  account names and IDs are unchanged; names abandoned before the upgrade cannot be
  reconstructed.
- Migration `0043` stores dashboard mention membership. First startup indexes existing
  Markdown bodies, comments and submitted reviews in batches of 100, so large databases
  can take longer to start. The backfill is restart-safe and runs once. Live mention
  updates use the existing eventual event subscriber; no replay guarantee is introduced.
- Automatic release checks are enabled by default. Set
  `RBIT_UPDATES__CHECK_FOR_UPDATES=false` to prevent them, independently of
  `RBIT_UPDATES__REPORT_VERSION=false` to hide public/outbound version reporting.
  Configuration changes take effect after restart. These controls need no additional
  database migration and do not install updates.

## [0.1.3] - 2026-10-21

### Security

- Update TLS certificate validation dependencies to reject distrusted emSign intermediates
  (RUSTSEC-2026-0381 and RUSTSEC-2026-0382).

### Changed

- Add a searchable file tree to pull request Files changed tabs, with folder controls and links to each diff.

- Match installed app launch colors to the chosen theme and replace beige app icon backgrounds with rbit Light.

- Switch repository creation, mirrors and imports in place while keeping form drafts.

- Cache daily repository activity by default-branch tip and reduce commit graph ordering work.

- Add quoted replies and user, issue and pull request mention choices to discussions.

- Search branch and tag choices on demand when comparing changes. Keep diff highlighting off the request worker.

- Use the issue search syntax and filter menus for pull requests, with draft and merged filters.

- Add searchable issue filter menus, label reset choices and search help.

- Refresh the roadmap for the public open-source project, separate available features
  from planned work, and remove stale first-release guidance from contributor and handoff docs.
- Document how to distinguish large HTTPS push failures at proxies from Git's shallow
  update rejections, with regression coverage for normal and shallow pushes over 100 MiB.

### Upgrade notes

- Back up the instance before upgrading. No new database migrations or configuration
  changes are required when upgrading from 0.1.2.
- Restart the service with the new binary or container image to apply the TLS
  certificate validation updates and refreshed web assets.

## [0.1.2] - 2026-10-19

### Added

- Repository document tabs for README files, licenses, code of conduct, contributing
  guidelines and security policies, with matching About sidebar links, a sticky tab
  bar and a searchable heading outline. Document links resolve against the displayed
  commit and preserve the selected document when reloading or using browser history.
- Mermaid diagrams in Markdown and responsive images with safe relative `srcset`
  URLs. Diagrams render in isolated sandboxed frames with a readable source fallback.
- Mirror creation progress with current fetch stages and automatic navigation to the
  repository when the first fetch completes.
- Immediate contribution calendar tooltips and selectable days with commit details.

### Changed

- Make the DNA light and dark palettes the default rbit themes. The original palettes
  remain available as rbit Classic, and existing saved preferences are preserved.
- Make repository Code, Issues, Pull requests, Commits, Branches, Tags and Releases
  navigation reuse bounded, short-lived preloaded responses. Keep the tab bar mounted
  across navigation, preserving focus, horizontal position and open clone menus.
- Preload repository documents and switch cached tabs immediately through a smaller
  document-only response, preserving the tab header and browser navigation behavior.
- Expand language file dialogs into a consistent split view with an indented file tree
  and a content preview. Improve branch graph spacing, colors and labels on narrow screens.

### Fixed

- Require accounts created by administrators with an initial password to change it
  before accessing repositories or using authenticated services.
- Align sticky repository and document headers to remove gaps, including at fractional
  zoom levels and on mobile screens.
- Update end-to-end coverage for mirror progress redirects and commit-pinned README links.

### Upgrade notes

- Take a full backup before upgrading. Database migration 0040 adds the initial-password
  requirement flag and runs automatically; database migrations only move forward.

## [0.1.1] - 2026-10-18

### Added

- Yearly contribution heatmaps on user and organization profiles and repository Commits
  tabs, with daily counts, monthly activity, active days and longest streaks. Calendars
  rearrange into quarters on small screens without horizontal scrolling. Counts respect
  repository visibility and use default-branch commits by UTC author date.
- White-label instance branding and administrator configuration forms, including
  customizable names, logos and email presentation.

### Fixed

- Resolve strict lint warnings across the workspace.

## [0.1.0] - 2026-10-18

First release of rbit, a self-hosted Git platform distributed as one Rust binary.

### Highlights

- Git over HTTPS and SSH, repository browsing, syntax highlighting, blame, archives,
  forks, Git LFS, and pull and push mirrors.
- Organisations, teams, issues, pull requests, reviews, branch protection, commit status
  checks, email notifications, a REST API and signed webhooks.
- Passkeys and TOTP, scoped access tokens, password recovery, OpenID Connect and GitHub
  sign-in, and imports from GitHub, GitHub Enterprise and Gitea/Forgejo.
- SQLite and redb storage, health checks, metrics, backups, Docker and systemd support.
- A responsive web interface with built-in and tinted themes, bounded rendering caches,
  bundled browser scripts, and initial styles authorized by exact Content Security Policy
  hashes. Installation metadata and offline support load after the initial page.
- MIT OR Apache-2.0 licensing, community contribution files, and release automation for
  Linux, macOS and Windows archives, checksums and a multi-architecture container image.

### Installation and compatibility

- The server requires the Git executable. Source builds require Rust 1.94 or newer and
  Node.js 24 for frontend assets; production does not need Node.js or an external database.
- The release workflow targets Linux x86_64 and aarch64 (glibc 2.35+), macOS Intel and
  Apple silicon, and Windows x86_64. Linux is the supported server platform; Windows
  also has automated development and integration coverage.
- Before upgrading an existing development instance, take a full backup and read the
  detailed upgrade notes below. Database migrations run automatically and only move
  forward. Production setup is described in [docs/deployment.md](docs/deployment.md).

### Known limitations

- Resolving review threads, changing a pull request's base branch, cross-repository
  issue/pull-request dashboards, Markdown preview and account-rename redirects are
  planned follow-up work, not features of 0.1.0.
- rbit accepts status checks from external CI; it does not execute CI jobs or provide a
  package registry. Git LFS over SSH authenticates transfers that then use HTTP.

The sections below record the features, fixes and upgrade details accumulated during
development of this first release.

### Performance

- Page branches, tags, directories and language-file results at 100 entries, with
  search and stable snapshot links. Load commit cells and branch-protection badges
  only for the requested page; do not permanently cache incomplete commit cells.
- Reuse directory-history results for 30 seconds with bounded storage and four
  admitted workers, including cancelled requests. Share repeated requests, avoid
  quadratic cache filtering, and reuse decoded parent trees during history walks.
- Combine branch graph lane segments into one SVG path per row, avoid full branch
  commit summaries, and cap visible branch labels while linking to the full list.
- Share cached highlighted lines through file and blame rendering without copying their
  strings. Coalesce simultaneous requests for the same highlighted file and admit at
  most four CPU jobs, including work whose requesting page has been cancelled.
- Hash borrowed HTML slices for ETags instead of allocating another page-sized buffer;
  existing validators and conditional responses remain compatible.
- Cancel obsolete preloads and queued work when navigating, changing state or scrolling
  after a touch. Bound retained preload records, respect Save-Data for all speculation,
  and consume preload bodies without retaining another complete HTML buffer.
- Add paired HTTP measurements with executable hashes, raw samples and alternating run
  order. Reject invalid scroll probes and keep debug and release measurements separate.
- Load branch/tag picker options only when opened, with 50 results per page and
  server-side search. Keep paged links and search available without JavaScript.
- Index mounted diff files and sidebar links once, coalesce bulk toggle updates, and
  skip diff scroll work on other pages. Refresh indexes and header observation across
  htmx navigation and history restores.
- Reuse bounded blame results and share concurrent calculations for the same file and
  revision. Limit active blame calculations even after a request times out, and keep
  authorization checks fresh.
- Stop retaining tree changes at the diff file limit and skip display-string allocation
  for hunks beyond the line budget, preserving rename detection and line statistics.
- Batch pull-request reviewer permissions and notification recipients, inbox writes and
  email deduplication claims. Keep permission, suspension and email preference checks
  fresh, with bounded notification write transactions.
- Scan expired sessions without holding the database writer, then remove them in small
  batches that recheck expiry and preserve concurrently renewed sessions.

### Security and reliability audit

- Make first-administrator registration, password reset consumption, two-factor state
  changes, access-token limits and release-file limits atomic under concurrent requests.
- Revoke sessions synchronously after credential changes; prevent session renewal from
  restoring revoked sessions, reject stale password proofs, and complete credential
  updates after request cancellation.
- Recheck SSH keys and administrator status for every exec request, and enforce branch
  protection across Windows ref-name case aliases.
- Pin remote Git connections to validated DNS addresses, disable proxy overrides, and
  validate numeric webhook hosts and IPv6 translation ranges consistently.
- Restrict object-scoped Git LFS credentials to their object transfers, and report stored
  object sizes accurately. Clean up cancelled uploads and finish file placement independently
  of disconnected clients.
- Keep private responses out of shared caches, make the offline page anonymous, limit
  service-worker caching to versioned static assets, and prevent speculative thread loads
  from marking notifications read. Reject unsafe redirect targets and invalid API dates.
- Cancel abandoned Git subprocess streams, bound subprocess output and email work queues,
  stream LFS pointer discovery, deduplicate object ids in linear time, and replace
  exponential recursive `.gitattributes` matching with iterative matching.
- Update vulnerable sanitizer and frontend build dependencies and replace a yanked
  cryptography dependency; run browser-script regressions in CI.

### Milestone 3 — ready to open-source

rbit is licensed under **MIT OR Apache-2.0** (development builds previously used
AGPL-3.0-or-later), with its upstream at github.com/GitRbit/rbit. Release packaging targets
Linux (x86_64, aarch64), macOS (Intel, Apple silicon) and Windows with SHA-256 sums, and a multi-architecture image at
`ghcr.io/gitrbit/rbit`. The milestone also brings commit statuses for external CI, more of the
REST API, email notifications, editing files in the browser, Git LFS and sign-in with
OpenID Connect providers and GitHub, importing repositories from GitHub and Gitea/Forgejo,
and push mirrors.

#### Upgrade notes

- Database migrations `0033_commit_statuses`, `0034_notification_email`, `0036_lfs` and
  `0037_external_identities` run automatically at start-up (or with `rbit migrate`). Take a
  backup first; migrations only move forward. They add tables (commit statuses and required
  checks, notification email settings and the email outbox, the `server_secrets` key store,
  Git LFS objects, links and locks, linked sign-in identities) and one column,
  `branch_protections.require_status_checks` (off), so existing data and protection rules
  are unchanged. `0035` is unused on purpose.
- **Personal access tokens now start with `rbit_`.** Existing `rit_…` tokens keep working
  unchanged, over Git and the API; nothing needs to be regenerated. Secret scanners and
  scripts that recognise tokens by prefix should accept both. The token list shows hints
  as `…a1b2`.
- **Email notifications are on by default.** With a `[mail]` transport configured, users
  with a verified address start receiving email for threads they take part in
  (`[notifications] default_email = "participating"`). Set `default_email = "off"` to make
  it opt-in, or `[notifications] email = false` to send none. Unsubscribe links use
  `server.public_url`.
- **Git LFS is on by default** (`[lfs] enabled = false` keeps the endpoints and
  `git-lfs-authenticate` off). Behind a reverse proxy, allow request bodies up to
  `[lfs] max_object_bytes` (default 2 GiB) and turn off request buffering for
  `/*/info/lfs/objects/*` (nginx: `client_max_body_size`, `proxy_request_buffering off`).
  Web uploads (`[editor] max_upload_bytes`, 50 MiB per file) need the same allowance.
- **Backups use format 3.** `rbit backup` also copies Git LFS objects into `lfs/` and the
  manifest gains `lfs_objects` and `missing_lfs_objects`. When restoring, copy `lfs/` along
  with `rbit.db`, `repositories/` and `attachments/` (see
  [docs/deployment.md](docs/deployment.md#backups)). Older backups restore as before.
- New data directory `<data_dir>/lfs` (Git LFS objects, back it up); LFS uploads in
  progress live in `<data_dir>/tmp/lfs`, which is never backed up.
- New configuration sections `[editor]`, `[lfs]`, `[notifications]` and `[auth]`; every
  key is optional and documented in [deploy/rbit.example.toml](deploy/rbit.example.toml).
  External sign-in stays off until `[[auth.providers]]` are configured; providers are
  validated at start-up and every problem is reported at once. Client secrets can also
  come from `RBIT_AUTH__PROVIDERS__<N>__CLIENT_SECRET` (`<N>` the position from 0, or the
  provider's id in upper case with `-` as `_`).
- Webhooks: the events `member` and `status` are new; existing hooks are not subscribed to
  them until you tick them.
- Database migrations `0038_imports` and `0039_push_mirrors` add two tables (import jobs
  and push mirrors); existing data is unchanged. New configuration sections `[imports]`
  and `[push_mirrors]` (both on by default; private addresses refused) are documented in
  [deploy/rbit.example.toml](deploy/rbit.example.toml).

#### Added

- **Commit statuses**, compatible with GitHub's commit status API: external CI reports
  `pending`, `success`, `failure` or `error` per check (`context`) on a commit with
  `POST /api/v1/repos/{owner}/{repo}/statuses/{sha}` (a `write` token); read them with
  `GET …/commits/{ref}/statuses` and the combined status with `GET …/commits/{ref}/status`.
  The pull request merge box lists the head's checks with links to their details; commit
  lists and the commit page show a combined status mark.
- **Required status checks** in branch protection: list the required contexts, and pull
  requests into matching branches merge only when each one's latest status on the head is
  `success` (administrators may bypass where the rule allows it). Statuses nobody requires
  are shown but never block, and status checks never affect pushes.
- **REST API: collaborators, teams, releases and forks.**
  `GET /repos/{owner}/{repo}/collaborators`, `GET|PUT|DELETE …/collaborators/{username}`
  and `…/collaborators/{username}/permission`; organisation teams under
  `/orgs/{org}/teams` with members (`…/memberships/{username}`) and repository grants
  (`…/repos/{owner}/{repo}`); releases (`…/releases`, `/latest`, `/tags/{tag}`, `/{id}`)
  with attached files — uploaded through the release's `upload_url`
  (`POST …/releases/{id}/assets?name=`, the raw body is the file, streamed under the
  `[releases]` limits) and downloaded with `Accept: application/octet-stream`; forks
  (`GET|POST …/forks`). Shaped like GitHub's and documented in the OpenAPI document.
- **Webhooks** `member` (a collaborator added, changing permission or removed) and
  `status` (a commit status reported), with GitHub's payload shapes.
- **Email notifications**: mentions, assignments, review requests, reviews, comments and
  state changes on threads you take part in are also emailed to verified addresses
  (Settings → Notifications: off / participating / everything). Messages thread per issue
  (`Message-ID` / `In-Reply-To`), say why you got them, and carry a one-click unsubscribe
  link (RFC 8058 `List-Unsubscribe-Post`). Rate limited per address, never sent to the
  actor or to anyone who can no longer read the repository.
- **Edit files in the browser**: create (with new directories), edit, rename or move,
  delete and upload files from the repository pages, one commit each, on the current
  branch or on a new branch followed by a pull request. Branch protection applies exactly
  as for pushes; a branch that moved meanwhile is reported, never silently rebased. Commits
  made this way trigger webhooks, `fixes #n` and everything else a push does, and are
  authored with your noreply address, like merges. Git LFS pointer files, binaries and
  files too large to display cannot be edited in the browser (they can be deleted or
  replaced by an upload).
- **Git LFS.** The server side of Git Large File Storage: the batch API and basic
  transfers under `/{owner}/{repo}.git/info/lfs` (also without `.git`), with streamed
  uploads checked against their SHA-256 and size; the locks API (`git lfs lock`, `locks`,
  `unlock`, lock verification on push); and `git-lfs-authenticate` over SSH. The same
  credentials as Git over HTTP (password or personal access token) or SSH keys; transfers
  use short-lived signed headers. Objects are stored once under `<data_dir>/lfs` however
  many repositories use them, forks share their parent's objects, and objects no
  repository uses are removed by maintenance after a day. The blob page shows the stored
  file (with a "Stored with Git LFS" badge) instead of the pointer, and Raw serves it.
- **Sign in with OpenID Connect providers** (Keycloak, Authentik, Google, Entra ID, …) and
  **GitHub** (github.com or Enterprise Server), configured as `[[auth.providers]]`.
  Authorization code flow with PKCE, signed state, nonce and full ID token validation
  (RS256/ES256, key rotation). First sign-ins register an account when allowed
  (`allow_registration`), or link to an existing one from Settings → Security; linking by
  email only with `trust_email` and verified addresses on both sides. Two-factor
  authentication still applies unless `trust_mfa` and the provider reports MFA. Linked
  accounts are listed with unlink; the last way to sign in can't be removed. Accounts
  registered through a provider have no password until a reset link sets one.
- **Import repositories** from github.com, GitHub Enterprise Server, Gitea or Forgejo
  (`/new/import`): the Git history with issues, pull requests, labels, milestones,
  comments, releases with their files and Git LFS objects, keeping issue and pull request
  numbers. The import runs in the background with its progress on
  `/{owner}/{repo}/import`; the token is never stored, and imported history sends no
  notifications or webhooks. Authors map to local accounts only with
  `[imports] author_mapping`.
- **Push mirrors** (Settings → Push mirrors): force-push a repository's branches and tags
  to other `http(s)` remotes after each push (debounced) and on a schedule, or on demand,
  with the last attempt's outcome shown. Secrets are write-only.
- New audit actions "Linked a sign-in provider" and "Unlinked a sign-in provider"
  (category "Sign-in"); sign-ins through a provider record the provider's name.
- **Open-source project files**: `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD-PARTY.md`,
  `CONTRIBUTING.md`, `SECURITY.md` (private vulnerability reports through GitHub security
  advisories), `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1), `ROADMAP.md`, issue forms
  and a pull request template.
- A **release workflow**: version tags build and publish the binaries, their checksums,
  release notes from this changelog, and the container image. The image carries OCI
  labels (source, license, documentation, version).
- `cargo deny` checks dependency licenses and RustSec advisories in CI (`deny.toml`).

#### Changed

- **License:** MIT OR Apache-2.0 instead of AGPL-3.0-or-later, in every crate, the
  front-end package and the OpenAPI document.
- New personal access tokens are `rbit_…` (`rit_…` is still accepted everywhere).
- Pull requests no longer show "No status checks — not supported yet": the checks line
  appears when the head has statuses, and only required checks can block a merge.
- Release objects in webhook payloads and the API carry GitHub's API links: `url`,
  `assets_url`, `upload_url` and each file's `url`.
- **Granting a team `owner/name` of a different owner is refused** instead of silently
  granting the organisation's repository of that name (web form and API).
- The blob page of a Git LFS pointer shows the stored object (text, Markdown, code or an
  image) instead of the pointer text; "Object not on this server" when it is missing.
- Blob and tree pages offer Edit, Delete, Add file and Upload to writers on branches
  (never on pull mirrors).
- Settings has a **Notifications** section; Settings → Security lists sign-in methods
  when providers are configured. Settings pages no longer overflow a 375 px screen.
- Everything under `/login/` (provider sign-in starts included) is no longer preloaded on
  hover.

#### Security

- `server_secrets` holds server-generated keys (the notification unsubscribe key and the
  external sign-in key, 256 bits each, created on first use). Like webhook secrets they
  are stored raw, because HMAC needs the key.
- `POST /email/unsubscribe/{token}` is the single path exempt from the cross-site request
  check: mail clients send RFC 8058 one-click unsubscribes from their own origin. The
  token is an HMAC of the account id and can only turn that account's notification emails
  off; `GET` only shows a confirmation, so link scanners unsubscribe nobody.
- Git LFS: object ids are validated before they name a path; uploads are checked against
  size and SHA-256 before they are placed; an object stored for an unrelated repository is
  never linked without a fresh upload, so knowing an object id never grants its content.
  Transfer headers are HMAC-signed, short-lived, scoped to one repository (and one
  object), re-check the holder's current access, and failures are throttled like Git
  credentials.
- External sign-in: identities are keyed by the provider's stable subject, never by
  email; an unverified address never links anything; the flow state lives in a signed,
  10-minute `HttpOnly; SameSite=Lax` cookie and must match exactly; starting a link from
  settings needs a session-bound token; callbacks are throttled per address; suspended
  accounts are refused. Provider requests use the webhooks' outbound address guard, no
  redirects and a 1 MiB response cap.
- Web edits never check files out (scratch repository, scratch index and `commit-tree`),
  validate every path (no `..`, no `.git` in any spelling), stream uploads to disk, and
  reach the branch only through one compare-and-swap `update-ref`. No email address is
  written into commits made in the browser.
- Notification emails go only to verified addresses, re-check read access at send time,
  are deduplicated and rate limited per address, and carry no `Reply-To`.

### Speed — large pages and navigation

Long files, blame, large diffs and big pull requests scroll without blank lines or a
jumping scrollbar, and appear much sooner; moving between pages sends less and preloads
smarter. Measured on the fixture in `scripts/perf/` (a 20,000-line file, a 152-file
change): the file and blame pages went from about 17.5 MB and 356,000 elements to about
4 MB and 92,000, the compare and pull request diffs to less than half their size.

- Code, blame and diff lines render in fixed-height blocks of 128 that the browser skips
  as a unit while off screen, with exact placeholder heights; changed files are skipped
  as a whole with their exact height, and only diffs of more than 512 rows also use
  blocks. Find in page, `#L…` links, text selection and screen readers still see every
  line. Diffs are no longer tables.
- Syntax highlighting markup is about 4.5× smaller (one flat span per token, short
  classes), and highlighted files are kept in a 32 MiB in-memory cache, so a large file
  renders in milliseconds after its first view; blame reuses the file's entry.
- Pull request tabs no longer compute the diff twice (Files changed) or at all
  (Conversation, Commits): the counts come from a cache keyed by the base and head
  commits, or fill in right after the page paints.
- The commit page reads its statuses while the diff is computed, the compare page the
  fork network while comparing, the Files changed tab the reviews while diffing.
- Navigating within a repository fetches only the changed part of the page (the title,
  the content and the tabs): about half the bytes.
- Preloading is rewritten: a link is fetched at most once per 30 seconds however often it
  is hovered, nothing is fetched while scrolling on a touch screen, and the repository
  tabs and "next page" links are preloaded when the browser is idle (not with Save-Data
  or on 2G). The htmx preload extension is no longer vendored.
- Commit-pinned fragments of public repositories (last commits in the file list,
  pull request counts) are cached by the browser for good; private ones still revalidate.

#### Upgrade notes

- The insights cache (`rbit.redb`) gains a `comparison_counts` table, created on start-up
  and pruned with the other cached insights; nothing needs to be migrated or rebuilt.
- The server keeps up to 32 MiB of highlighted files in memory.

### Fixed

- Collapsing or expanding a file on a pull request's **Files changed** tab no longer
  throws a script error.
- A webhook created right after an event (for example a push) no longer receives that
  event's delivery: hooks only get events that happened while they existed, as on GitHub.
  A hook deleted or deactivated before an event is queued does not receive it either.

### Milestone 2 — wave B

The collaboration layer, second half: issues, pull requests with review and merging, and
protected branches — completing milestone 2. Pull requests are issues with a branch
attached: they share the repository's number sequence, comments and timeline.

#### Upgrade notes

- Database migrations `0017_issues_core` through `0024_branch_protection` run
  automatically at start-up (or with `rbit migrate`). Take a backup first; migrations only
  move forward. They add tables only (and fill the new full-text index from existing rows,
  which are none), so existing data is untouched; the upgrade test migrates a milestone 1
  database through them. `0023` is not used in this wave.
- **Clients can no longer push to `refs/pull/*`.** The namespace now holds pull request
  heads (`refs/pull/{n}/head`) and is managed by rbit; such pushes are refused with
  "refs/pull/ is managed by rbit", over HTTP and SSH.
- **Force-push protection affects the whole push.** When a push updates a branch protected
  against force pushes, git checks every ref of that push for fast-forwards, so a
  non-fast-forward update of another, unprotected ref in the *same* push is refused too
  ("non-fast-forward"). Push it separately. Pushes are also all or nothing: if branch
  protection refuses one ref, the other refs of the push are declined with it.
- New scratch directory `<data_dir>/tmp` (merges and cross-fork overlays). Its contents are
  disposable, swept at start-up and **excluded from backups**; it needs no backup or
  restore.
- New configuration sections `[issues]` and `[pulls]`; every key is optional and
  documented in [deploy/rbit.example.toml](deploy/rbit.example.toml). Commits rbit makes
  (merges, squashes, rebases) use `<username>@<[pulls] noreply_domain>`, by default
  `noreply.<host of server.public_url>` — set `server.public_url` before merging.
- `issues` and `pulls` are now reserved account names. An existing account with such a
  name keeps its data but should be renamed.
- Webhooks: the events `issues`, `issue_comment`, `pull_request` and
  `pull_request_review` are new; existing hooks are not subscribed to them until you tick
  them.

#### Added

- **Issues**: list, new and issue pages with a Markdown body, the shared timeline
  (comments, edits, close as completed or not planned, reopen, lock, "close with
  comment"), and a sidebar with assignees, labels, milestone and a notification
  subscription. Administrators can delete issues.
- **Labels** (with a default set) and **milestones** (description, due date, progress),
  **assignees** (writers only).
- **References**: `#12`, `owner/repo#12` and `@name` link in Markdown; mentions between
  threads appear on the target's timeline; commits mentioning `#12` are listed on the
  issue, and `fixes #12` closes it when the commit reaches the default branch or a pull
  request naming it is merged into the default branch.
- **Search and filters** for issues and pull requests: `is:open|closed|issue|pr|merged|draft|locked`,
  `author:`, `assignee:` (`@me`), `label:` / `-label:`, `milestone:`, `no:`, `sort:` and
  full-text search over titles and bodies.
- **Notifications**: an in-app inbox (`/notifications`, a dot on the header bell) for
  threads you take part in, are assigned to, mentioned in or asked to review; mark read,
  mark all read, unsubscribe.
- **Pull requests** within a repository and across its fork network
  (`compare/main...owner:branch`), drafts, Conversation / Commits / Files changed tabs,
  **line comments** with threads (outdated after new pushes), **reviews** (comment,
  approve, request changes; pending reviews), review requests and dismissals,
  background mergeability checks with conflicting paths, and **merging** by merge
  commit, squash or rebase, optionally deleting the head branch. Pushes to the head branch
  update the pull request; a pull request merged with plain `git` is recognised and marked
  merged.
- **Branch protection** (Settings → Branches): rules per branch name or glob that forbid
  force pushes and deletion, require a pull request with up to 10 approvals (optionally
  dismissing stale approvals and blocking on requested changes), restrict who may push
  (users and teams), with an optional administrator bypass. Enforced for pushes over
  HTTP and SSH and for merges; protected branches show a **Protected** badge on the
  branches page.
- **REST API**: issues, comments, timeline, labels, milestones, assignees, notifications,
  pull requests (commits, files, merge, reviews, review comments, requested reviewers) and
  branch protection rules; all in the OpenAPI document.
- **Webhooks** `issues`, `issue_comment`, `pull_request` (including `synchronize` and
  `closed` with `merged`) and `pull_request_review`, shaped like GitHub's.
- New audit actions: branch protection rule created, changed, deleted and bypassed
  ("Repository access"); issue deleted (new category "Repository content").

#### Changed

- Repositories have **Issues** and **Pull requests** tabs with open counts.
- The compare page accepts heads from other repositories of the fork network
  (`owner:branch`, `owner/name:branch`) and offers "Create pull request".
- Git over SSH receives pushes in two steps (advertisement, then a stateless
  `receive-pack`), exactly like smart HTTP, so pushes can be checked before git runs.
  Clients notice no difference.

#### Security

- The push gate runs in-process in both receive paths before git starts; refused pushes
  leave the repository untouched, and refusal messages never say who may push.
- Merges never check files out: they are computed with `git merge-tree --write-tree` and
  `git commit-tree` in scratch repositories under `<data_dir>/tmp`, with hooks disabled
  and a hardened `git`, and reach the base branch only through one compare-and-swap
  `update-ref`. They are bounded in time and concurrency.
- No email address appears in any issue or pull request page, API object, webhook payload
  or commit rbit makes. Notifications only reach people who can read the repository at
  that moment; mentions never reveal whether someone has access; cross-references are
  shown only to people who can read their source.
- Creating and editing issues, pull requests, comments and reviews is rate limited per
  account.

### Passkeys for two-factor authentication

Passkeys and hardware security keys (WebAuthn) can now be used as a second factor, on
their own or next to an authenticator app. Add them under **Settings → Security**; the
second sign-in step then offers **Use a passkey** as well as the code field.

- Supported: platform passkeys (Apple, Google, Windows Hello, password managers) and FIDO2
  security keys, with ES256, EdDSA or RS256 keys. No attestation is requested.
- Recovery codes now belong to the account rather than to the authenticator app: they are
  issued with the first factor (passkey or app) and deleted with the last one. Removing the
  authenticator app keeps two-factor authentication on while passkeys remain.
- Removing a passkey or the app, and generating new recovery codes, need the password plus
  an authenticator code, a recovery code or a passkey.
- New audit actions: "Added a passkey", "Removed a passkey".
- `/settings/security` and `/login/two-factor` are no longer preloaded on hover, since
  they issue single-use challenges.

#### Upgrade notes

- Migration `0032_passkeys` adds the `passkeys` table.
- Passkeys are bound to the host of `server.public_url` and only work when the site is
  opened at exactly that origin (scheme, host and port). Set it correctly before users
  register passkeys: changing the host later invalidates them.

### Renamed to rbit

The project, its crates and its binary (`rbit`) are now called rbit. The footer shows how
long the server took for each page, and keeps the figure current across htmx swaps (it is
also sent as a `Server-Timing` header).

#### Upgrade notes

- Existing installations keep working unchanged: `rustit.db` and `rustit.redb` are used
  when `rbit.db` / `rbit.redb` do not exist, `RUSTIT_*` environment variables (including
  `RUSTIT_CONFIG`) are still read with `RBIT_*` taking precedence, and the
  `rustit_session` and `rustit-theme` cookies are still accepted, so nobody is signed out.
  Rename the files and variables at your convenience.
- **Breaking for integrations:** webhook headers are now `X-Rbit-Event`,
  `X-Rbit-Delivery`, `X-Rbit-Hook-Id` and `X-Rbit-Signature-256` (`X-Hub-Signature-256`
  is unchanged), the user agent is `rbit-Hookshot/…`, and Prometheus metrics are prefixed
  `rbit_` instead of `rustit_`.
- The systemd unit is now `deploy/rbit.service` and the example config
  `deploy/rbit.example.toml`.

### Diff view

- Commit and compare pages show the changed files as a folder tree beside the diffs, with
  a filter, the file being read marked as you scroll, and a button to hide it (remembered
  per browser). Diffs are listed in the tree's order.
- File headers stick below the top bar while their diff scrolls past and collapse in
  place; Alt-click or **Collapse all** toggles every file. Each header shows the change
  type, a copy-path button and a change bar.
- Scrollbars are thin and follow the active theme's colours.

### Milestone 2 — wave A

The collaboration layer, first half: other people can work on a repository with exactly
the access they need, rbit can be automated through a REST API and signed webhooks, and
repositories gain downloads, releases, blame, comparison, forks and mirrors. Issues, pull
requests and branch protection follow in wave B.

#### Upgrade notes

- Database migrations `0009_access` through `0016_repo_redirects` run automatically at
  start-up (or explicitly with `rbit migrate`, which now also works while the server is
  running). Take a backup first; migrations only move forward. They were tested against a
  milestone 1 database: every row is kept. `0012_token_scopes` rebuilds the
  `access_tokens` table to allow the new `admin` scope, copying every token with its id,
  so existing tokens keep working unchanged.
- **Nobody loses access.** Organisation members keep write access to every repository of
  their organisation: the new organisation base permission defaults to `write`, the
  milestone 1 behaviour. Owners can lower it to `read` or `none` under the organisation's
  settings and grant access through teams instead.
- **Nobody is signed out** by this upgrade.
- **Backups use format 2.** `rbit backup` now also copies release files into
  `attachments/` and writes `"format": 2` to `manifest.json`. When restoring, copy
  `attachments/` along with `rbit.db` and `repositories/` (see
  [docs/deployment.md](docs/deployment.md#backups)). Backups in format 1 restore as before.
- New data directory: `<data_dir>/attachments` holds release files (uploads in progress
  in `attachments/tmp`). It is created on demand; back it up with the rest of the data
  directory (the backup command does).
- New configuration sections `[api]`, `[webhooks]`, `[releases]` and `[mirrors]`; every key
  is optional and documented in [deploy/rbit.example.toml](deploy/rbit.example.toml).
  The REST API is on by default (`[api] enabled = false` turns it off).
- Webhooks and pull mirrors make **outbound** HTTP(S) requests. Internal addresses
  (loopback, private, link-local, …) are refused unless `allow_private_addresses` is set
  in the respective section.
- Release uploads can be large (up to `[releases] max_asset_bytes`, 512 MiB by default):
  make sure the reverse proxy's request body limit allows them.
- `themes` is now a reserved account name and `teams` a reserved repository name. An
  existing account or repository with such a name keeps its data, but its page is
  shadowed by the new one; rename it before or after upgrading.

#### Added

- **Collaborators** on any repository with `read`, `write` or `admin` permission
  (Settings → Collaborators); collaborators can leave a repository from its sidebar, and
  repositories shared with you appear on the dashboard.
- **Teams** inside organisations, with members and a permission per repository, and an
  organisation **base permission** (`none`, `read` or `write`) for every member. Leaving an
  organisation removes its team memberships and collaborator entries.
- **Repository transfer** to another user or organisation (Settings → Danger zone).
- **Old names keep working**: after a rename or transfer, pages under the old
  `owner/name` redirect to the new location, and Git over HTTP and SSH keep serving the
  repository there until another repository takes the name.
- **REST API** under `/api/v1`: users, organisations and members, repositories (list,
  create, update, delete), branches, tags, commits, stars and webhooks, authenticated with
  personal access tokens (`Authorization: Bearer rit_…`), paginated with `Link` and
  `X-Total-Count`, with JSON errors, rate limits and an OpenAPI 3.1 document at
  `/api/v1/openapi.json`.
- **Admin scope** for personal access tokens, required for repository settings, webhooks
  and deletion through the API.
- **Webhooks** for repositories and organisations (`push`, `repository`, `release`, `fork`,
  `ping`) with GitHub-compatible payloads and headers, HMAC-SHA256 signatures, a persistent
  delivery queue with retries and exponential backoff, a delivery log with redelivery, and
  the metric `rbit_webhook_attempts_total{outcome}`.
- **Archives**: download any revision as `.zip` or `.tar.gz` (Clone menu, tags, releases).
- **Releases** with Markdown notes, draft and pre-release flags, a Releases tab and the
  latest release in the sidebar; a release can create its tag from a branch. **Release
  files** can be uploaded and downloaded.
- **Blame** view (Blame button on every file) and **compare** two revisions (ahead/behind,
  commits and diff from the merge base).
- **Forks** into your account or an organisation, with "forked from" and a forks list.
- **Pull mirrors**: create a repository that fetches an upstream on a schedule, with sync
  now, last result and interval in Settings → Mirror. Pushes to a mirror are refused.
- README and Markdown files resolve **relative links and images** within the repository;
  images in the raw view are served with their media types.
- **Themes**: a theme gallery (`/themes`, Settings → Appearance) with the built-in light and
  dark themes and ~500 tinted-theming schemes, applied without a flash and remembered
  per account.
- A redesigned landing page and dashboard with public instance statistics, live search on
  Explore, and the rbit version in the footer.

#### Changed

- Members who create a repository in an organisation become its administrators (as a
  collaborator entry, which leaving the organisation removes). Repositories created
  before the upgrade keep their milestone 1 access.
- Repository deletion and transfer are recorded with the person who did it; changing an
  organisation's base permission is audited.
- `rbit user create` and `rbit migrate` no longer need the server to be stopped.
- Logs are plain text (no ANSI colours) when standard output is not a terminal.
- Sign-in, recovery and error pages are drawn as bordered cards.

#### Security

- Every access decision — pages, API, Git over HTTP and SSH — goes through one exhaustively
  tested policy; repositories a caller may not know about answer 404 in the API as well.
- **Webhook secrets are the one exception to "secrets are stored only as digests"**: HMAC
  signing needs the raw value. They are write-only: never shown again, returned by the
  API, logged or copied into the delivery log.
- Webhook deliveries refuse internal addresses at connection time (inside the DNS
  resolver, so DNS rebinding does not help), follow no redirects and ignore proxy
  variables; mirror fetches check the upstream's addresses before every fetch, follow no
  redirects and never prompt for credentials.
- The API never reads cookies, so it needs no CSRF protection; presented tokens are
  throttled per client address.
- Release files are stored under numeric ids and always downloaded as attachments
  (`application/octet-stream`, `nosniff`), so uploaded HTML or SVG can never run on the
  site's origin.

### Milestone 1 — safe to self-host

The first release meant to run on the internet. It builds on the foundation (accounts,
repositories, browsing, smart HTTP, installable PWA) and the second phase (commit diffs,
branches and tags, access tokens, organisations, stars, language statistics, settings,
administration panel).

#### Upgrade notes

- **Everyone is signed out once.** Login sessions moved to a new store (the redb table
  `sessions_v2`, keyed by a digest of the session token and indexed per account). Sessions
  created by earlier builds are not carried over, so after upgrading every user signs in
  again. Nothing else is lost: accounts, repositories, tokens and settings live in SQLite.
- Database migrations `0006_security`, `0007_email` and `0008_ssh_keys` run automatically
  at start-up (or explicitly with `rbit migrate`). Take a backup first; migrations only
  move forward.
- Behind a reverse proxy, set `[security] trusted_proxies` to the proxy's address.
  Without it every request appears to come from the proxy, so per-IP rate limits and the
  audit log see a single client.
- With two-factor authentication enabled, Git over HTTP no longer accepts the account
  password; use a personal access token.

#### Added

- **Rate limiting** of sign-in, sign-up, two-factor codes, Git HTTP credentials, SSH
  authentication, email requests and form posts; `429` responses carry `Retry-After`.
- **Trusted proxies** (`[security] trusted_proxies`): `X-Forwarded-For`, `Forwarded` and
  `X-Forwarded-Proto` are honoured only from listed CIDRs.
- **Session management** under Settings → Security: active sessions with client and last
  use, revoke one, sign out everywhere. Changing or resetting a password revokes all other
  sessions.
- **Two-factor authentication** (TOTP, RFC 6238): QR-code enrolment, ten single-use
  recovery codes stored hashed, replay protection, a second sign-in step.
- **Audit log** of sign-ins, two-factor changes, session revocations, token changes and
  every administrator action, with filtering in Site administration → Audit log.
- **Email** (`[mail]`): SMTP (STARTTLS, implicit TLS or plain) and a log transport for
  development. Password reset by email, email verification on sign-up and address change,
  and administrator actions to send a reset link or set a password. With
  `transport = "disabled"` every flow explains that email is unavailable.
- **Built-in SSH server** (`[ssh]`): clone and push over SSH with public-key
  authentication, an Ed25519 host key generated on first start, user SSH keys and
  per-repository deploy keys (read-only or read-write), SSH clone URLs in the Clone menu.
- **Background jobs**: cache warming after every push, periodic `git gc --auto`, insights
  cache pruning, bounded concurrency and graceful shutdown.
- **Prometheus metrics** at `/metrics` (`[operations] metrics`, optional bearer token):
  HTTP requests and latency, Git operations, job runs, cache size.
- **Health checks**: `/healthz` (SQLite and redb) and `/readyz` (also the repository
  directory), answering `503` on failure.
- **Backups**: `rbit backup --output DIR` writes a consistent SQLite snapshot, a mirror of
  every repository and a manifest while the server keeps running. Restore is documented.
- **Deployment**: a multi-stage `Dockerfile` (non-root, `git` included, health check), a
  hardened systemd unit, a commented production configuration
  (`deploy/rbit.example.toml`) and [docs/deployment.md](docs/deployment.md) covering
  Caddy, nginx, TLS, backups and upgrades.
- **Continuous integration**: formatting, Clippy with warnings denied, tests on Linux and
  Windows, the front-end build and a release build.
- **End-to-end and Git compatibility tests** that boot the full server and drive it with
  an HTTP client and the real `git` and OpenSSH clients.
- PNG application icons (192, 512, maskable) for the progressive web app.

#### Security

- Session tokens are stored only as SHA-256 digests; a leaked database never yields usable
  cookies.
- Password-reset tokens (one hour) and email-verification tokens (two days) are single-use
  and stored as digests; the reset form answers identically whether or not an address
  exists.
