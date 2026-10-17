# Milestone 3 — M: Importing and push mirrors (as built)

Workstream M of [milestone 3](../milestone-3.md): importing repositories from GitHub
(github.com or GitHub Enterprise) and Gitea/Forgejo with their history, and push mirrors.
Migrations `0038_imports` and `0039_push_mirrors` only.

## As built

**Domain (`rbit-core`).** `import`: `SourceKind` (`github`, `github_enterprise`, `gitea`),
`ImportSource::parse(kind, host, repository)` (the repository as `owner/name` or as a URL on
the chosen server; API base `https://api.github.com`, `<host>/api/v3` or `<host>/api/v1`;
the server address follows `MirrorUrl`'s rules: http(s), no credentials), `ImportOptions`
(issues and pull requests, labels, milestones, releases, LFS), `ImportStatus`,
`ImportPhase`, `ImportCounts`, `RepoImport`, and the conversions the importer needs
(`parse_timestamp` for RFC 3339 with offsets, `label_color`, `truncate_bytes`/`_chars`,
`clean_message`). `push_mirror`: `PushMirror`, `PushMirrorId`, `PushMirrorUrl` (http(s)
without credentials; `file://` only with the test switch), `validate_username` (no `:`)
and `validate_secret`. Config: `[imports]` and `[push_mirrors]` (`config/imports.rs`).

**Schema.**

| Table | Purpose |
|---|---|
| `repo_imports (repo_id PK, importer_id SET NULL, source_kind, source_url, options JSON, status, phase, counts JSON, warnings JSON, error, created_at, started_at, finished_at)` | One record per imported repository; `CHECK ((status = 'failed') = (error IS NOT NULL))`; partial index on active rows. The token is **not** a column. |
| `repo_push_mirrors (id, repo_id, url, username, secret, enabled, sync_on_push, next_sync_at, last_attempt_at, last_success_at, last_error, created_by SET NULL, created_at, updated_at, UNIQUE(repo_id, url))` | Push mirrors; `secret` stored as given (like webhook secrets), write-only. Partial index on due enabled rows. |

Stores: `ImportStore` (`Database::imports()`: the record's life cycle, `fail_unfinished`
except the repositories this process runs, `insert_pull` (an imported pull request's thread
and `pull_requests` row in one transaction), `backdate_release`, and author lookups by
verified email / user name) and `PushMirrorStore` (`Database::push_mirrors()`: the secret is
read only by `target`; `claim` / `record_attempt` / `schedule_after_push` keep pushes during
an attempt from being absorbed by it). Imported threads are written by a block appended to
`sqlite/issues.rs` (`IssueStore::insert_imported`, `insert_imported_in`, `reserve_numbers`),
so that file stays the only writer of the core tables: the given number (the counter is
raised to at least it), state, reason, `closed_at`, `locked`, comments with their times,
timeline events, `comment_count` and `updated_at`.

**Git (`rbit_git::remote`).** `fetch_import` (`+refs/heads/*`, `+refs/tags/*` and, for
issue imports, `+refs/pull/*/head`, with prune), `push_mirror` (`push --force --prune
--no-verify` of `refs/heads/*` and `refs/tags/*` only) and `lfs_pointers` (every blob of
pointer size via `cat-file --batch-all-objects`, parsed with `LfsPointer::parse`).
**Credentials:** the caller passes an `Authorization` header value; git receives it as
`http.extraHeader` through `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_0`/`GIT_CONFIG_VALUE_0` in its
environment — never in argv, a URL or the repository's config (a test asserts the header
arrives and nothing is written). Git runs with the scrubbed environment, no prompts,
credential helpers, askpass or hooks (`core.hooksPath=<null device>`), no redirects,
`GIT_ALLOW_PROTOCOL=http:https` (`file` only when the caller allows local URLs) and a
timeout that kills it.

**Imports (`rbit_services::imports`).**
- `ImportService::start(actor, ImportInput)`: validates the source and token (≤ 1024
  bytes, no whitespace), checks the API host's addresses (`is_internal_address`, unless
  `[imports] allow_private_addresses`), reads `GET /repos/{owner}/{repo}` — a wrong name, a
  refused token or an internal address is `ServiceError::Invalid` and **nothing is
  created** — then creates the repository with `RepositoryService::create_in` (so owner
  rules, the redirect trigger, `RepositoryCreated` and everything downstream apply; name
  defaults to the source's, description from the source), records the import and spawns
  the job. At most `max_concurrent` jobs run; others stay `queued`.
- `client.rs`: `SourceClient` (reqwest with the webhooks' `GuardedResolver`, so every
  connection — API, Git LFS batch, downloads — is checked at connect time; address literals
  checked before sending; no proxies). API requests follow no redirects; downloads follow
  at most 5, each hop re-checked (reqwest drops `Authorization` when leaving the host). The
  token goes as `Authorization: Bearer` (GitHub) or `token` (Gitea), only to the API's
  origin; `Link: rel="next"` is followed only on that origin (an empty page ends a
  listing). Rate limits: 403/429 with `Retry-After`, or with `X-RateLimit-Remaining: 0` +
  `X-RateLimit-Reset`, are waited out (≤ `rate_limit_max_wait_seconds`, else the import
  fails "rate limit used up until …"); a success that used up the budget pauses until the
  reset; 5xx and connect errors are retried twice. Downloads stream through `Download`, an
  `AsyncRead` fed by a channel, where a transfer error is a read error (a truncated file
  can never pass as complete).
- `job.rs`: phases Repository → Lfs → Labels → Milestones → Issues → Releases. The Git copy
  runs under the repository lease, then the source's default branch is adopted and
  `pushed_at` set. **The token is sent to git/LFS only when the clone URL's host is the API
  or web host of the source.** LFS: batch requests of 100 to `<clone URL>/info/lfs/objects/
  batch` (Basic `x-access-token:<token>`), objects already linked skipped, each download
  stored with `LfsService::store` (size and SHA-256 verified, `[lfs] max_object_bytes`).
  Labels (`#rrggbb`, names ≤ 50 / descriptions ≤ 100 characters; existing names reused)
  and milestones (due date, closed state and time) through `IssueMetaStore`. Issues:
  pull requests and all comments are listed first (`/pulls?state=all`,
  `/issues/comments?sort=created`), the newest issue number reserves the counter, then
  `/issues?state=all&sort=created&direction=asc` is imported page by page. Pull requests
  that are open on the source **and** whose `refs/pull/{n}/head` arrived **and** whose base
  branch exists stay open (`head_repo` = the repository for same-repository heads that came
  along, otherwise `NULL` with the head kept in `refs/pull/{n}/head`; mergeability
  `unknown`, so the worker computes it); merged ones become merged outside rbit
  (`merge_strategy NULL`, merge commit when known, a `Merged` event at `merged_at`); others
  are closed (an open one without its head gets "imported closed" appended). Labels,
  milestone and (mapped) assignees are attached. Releases: oldest first, created through
  `ReleaseStore` (author mapped or the importer) and back-dated; files over `[releases]
  max_asset_bytes` or with invalid names are skipped with a warning; files stream into
  `ReleaseService::upload_asset` (GitHub: the API asset URL with `Accept:
  application/octet-stream`, following the redirect to storage; Gitea:
  `browser_download_url`); a file that arrives with the wrong size is removed again.
- **Authors.** `[imports] author_mapping` = `none` (default), `email` (an active user whose
  *verified* address equals the source user's public one; GitHub needs `GET /users/{login}`
  for that, cached per login) or `email_or_username` (also an active user, not an
  organisation, with the same name). Unmapped: the importer writes the text, starting with
  `*Originally by [@login](<profile URL>) on GitHub.*` (or Gitea) — inside link text, so the
  mention scanner never links a local `@login`. Unknown (deleted) source users are
  `@ghost`. Event actors are the mapped account or none (ghost).
- **No notifications, emails or webhooks for imported history.** The job writes through
  the stores only (never `TimelineService`, `IssueService`, `PullService`,
  `ReleaseService::create` or `PushService`), so no `DomainEvent` is published except the
  `RepositoryCreated` of the empty repository at the start. Consequently imported texts are
  not scanned for cross-references or closing keywords, imported pushes reach no push
  mirror or webhook, and nobody is subscribed to imported threads. A test asserts that only
  `RepositoryCreated` is published.
- **Token lifetime.** In memory only: in the `SourceClient` (an `Arc<str>`, redacted from
  `Debug`) for the job's lifetime and in git's environment for the fetch. It is never
  written to SQLite, redb, logs or the repository. A restart therefore cannot resume an
  import: `spawn_background` marks queued/running imports this process does not run as
  failed ("interrupted … start a new import"); shutdown cancels running jobs (killing git)
  and records "stopped because the server shut down".
- **Failures.** A failed Git copy fails the import; later, a failed *listing* (token
  refused, rate limit outlasting the wait, network) fails it with everything so far kept;
  single items (number already taken, file too large, LFS object missing) become warnings
  (≤ 50 stored, the rest counted). Progress is saved at phase changes and every 25 items.
- `ImportService::status(viewer, repo)`: the importer or a repository administrator; 404
  for everyone else and for repositories that were not imported.

**Push mirrors (`rbit_services::push_mirrors`).** Repository administrators only (404
otherwise, also when `[push_mirrors] enabled = false`); ≤ 10 per repository; URL and
credentials validated and the remote's addresses checked on save and before every push
(`check_public_host`, shared with imports). A push sends `Authorization: Basic
base64(username:secret)` (empty user name → `git`) through git's environment; an empty
repository is skipped (git refuses to push nothing). Sync on push: a `RepositoryPushed`
event with changed refs makes the repository's enabled sync-on-push mirrors due at
`now + debounce_seconds` at the latest (`MIN`, so a burst is sent once) and wakes the
scheduler after the delay; every enabled mirror is also pushed every `interval_minutes`;
"Push now" makes it due at once. The scheduler (30 s tick, woken on demand) runs at most 2
pushes, never two for one mirror, under the repository lease; `claim` moves the next time
forward before pushing so a push during an attempt schedules another. Outcome, time and
error (git's stderr, ≤ 500 characters; the secret never appears) are stored.
`rbit_push_mirror_pushes_total{outcome}` and `rbit_imports_total{outcome}` count attempts.

**Pages.**

| Route | |
|---|---|
| `GET/POST /new/import` | The form (linked from "New repository"): source kind, server address, repository, token (never re-rendered), owner, name, visibility, what to bring along (wiki shown disabled, "not supported"). 422 with the reason on refusal; otherwise 303 to the progress page. With `[imports] enabled = false` a notice replaces the form. |
| `GET /{owner}/{repo}/import` | Progress: status badge, each requested phase (done / current / failed / not reached) with counts, start/end times, warnings, the error ("Everything imported until then is in the repository."). While active it re-fetches itself every 3 s (`hx-trigger="every 3s"`, `hx-select`; no script). Importer and administrators only. |
| `GET/POST /{owner}/{repo}/settings/push-mirrors` | List (URL, Paused / On push badges, last push time with OK/Failed, user name, "secret configured", the error) with Push now / Edit / Remove, and the add form. A "Push mirrors" card is appended to the repository settings page. |
| `GET/POST …/push-mirrors/{id}`, `POST …/{id}/sync`, `POST …/{id}/delete` | Edit (blank secret keeps it; Enabled checkbox), push now, remove. |

All page routes are Origin-checked forms under the usual page layers. Checked at 375 px
(no horizontal scroll) and on desktop.

**Tests.** Core unit tests (sources, timestamps, colours, truncation, push mirror URLs and
credentials, config defaults); `rbit-storage/tests/imports.rs` (record life cycle,
restart sweep with exceptions, imported threads keep numbers/states/times, conflicts,
counter reservation, merged/open pull rows, author lookups) and `push_mirrors.rs`
(write-only secrets, scheduling, claim/record semantics), `upgrade.rs` (tables exist,
start empty, defaults, cascades, `SET NULL`); `rbit-git/tests/remote.rs` (fetch with pull
heads but not `refs/pull/*/merge`, `file://` refused without the switch, push mirror
force + prune, the header reaching an HTTP listener and nothing written, LFS pointers);
`rbit-services/tests/imports.rs` against `tests/support/fake_forge.rs` — an axum fake of
GitHub's API (also `/api/v1`), paged two items per page with `Link` headers, a used-up
rate limit on the first comments request, a dumb-HTTP Git remote served from a local bare
repository that requires the token, an LFS batch endpoint with remote download headers, a
release file behind a redirect: full GitHub import (numbers 1–6 and 8, states, times,
attribution, labels, milestones, open/merged/fork/lost pull requests, new numbers after 8,
releases and the file, LFS object, token placement, only `RepositoryCreated` published),
Gitea flavour with author mapping by user name, refusals before creation (wrong token,
unknown repository, bad token, private address, disabled), a failed Git copy leaving an
empty repository and a clear error, visibility of the status, the restart sweep;
`rbit-services/tests/push_mirrors.rs` (push + prune to a local bare repository, failure
recording without the secret, background sync after a push with debounce, refusals and
permissions); `rbit-web` view tests; and `rbit-e2e/tests/e2e/imports.rs`, which includes
the same fake (`#[path]`) and goes through the pages: refused token (422, token not echoed),
import, progress page, history over HTTP and `git clone`/`ls-remote`, 404 for others, then
a push mirror that receives everything, follows an HTTP push and prunes a deleted branch.

## Deviations

- **`file://` for tests is a push-mirror switch only** (`[push_mirrors] allow_local_urls`).
  Imports never accept local URLs: the import tests serve the Git remote over HTTP (Git's
  dumb protocol from a local bare repository) with `[imports] allow_private_addresses`,
  which also exercises the credential header for real.
- **Two config sections** (`[imports]`, `[push_mirrors]`) added as one marked block at the
  end of `Config`, rather than one field; and both have their own
  `allow_private_addresses` instead of reusing `[mirrors]`'s.
- **Author mapping is configuration** (`[imports] author_mapping`), not a runtime admin
  setting in `/admin/settings`: it is an instance policy like the address switches.
- **Pull request reviews and line comments are not imported**; conversation comments
  (the issue comments API) are. Reactions, issue events other than closing/merging
  (label changes, renames, references) and wikis are not imported either.
- **Issue and pull request rows are written by `ImportStore::insert_pull` and an appended
  block of `sqlite/issues.rs`**; `insert_pull` writes the `pull_requests` row itself (P's
  table) in the same transaction as the thread. Labels, milestones and assignees use
  `IssueMetaStore`'s existing methods (no events).
- **Releases are created with `ReleaseStore::create`** (no `ReleasePublished`) and
  back-dated by `ImportStore::backdate_release`; their tags need not exist (drafts).
- **An import cannot be resumed or cancelled from the page**; a failed or interrupted one
  is repeated into a new repository.
- **Shared service instances.** `Services::new` now builds one `ReleaseService` and one
  `LfsService` and hands clones to the importer (two lines of construction changed): LFS
  placement shares a mutex with the orphan sweep, which a second instance would not.
- `rbit-services` gained `serde` (workspace, derive) as a dependency and `axum` as a
  dev-dependency; `rbit-e2e` gained `base64` as a dev-dependency. No new crates in the
  lock file.
- **An empty repository is not pushed** to its mirrors (git refuses a push of nothing);
  the attempt is recorded as successful and the remote keeps what it has.

## CHANGELOG entry

```markdown
### Added
- **Import from GitHub and Gitea/Forgejo** (`/new/import`, linked from "New repository"):
  copies the Git repository (branches, tags, pull request heads) and, as chosen, Git LFS
  objects, labels, milestones, issues and pull requests with their comments, and releases
  with their files. Issue and pull request numbers, states and dates are kept; open pull
  requests whose commits came along stay open, the rest become history. Authors appear as
  "originally by @login on GitHub" unless `[imports] author_mapping` maps them to local
  accounts. Runs in the background with a progress page (`/{owner}/{repo}/import`), waits
  out the source's rate limits, and sends no notifications, emails or webhooks for imported
  history. The access token is used only while the import runs and is never stored.
- **Push mirrors** (repository Settings → Push mirrors): force-push every branch and tag
  to up to ten remotes (deleting what no longer exists here) a few seconds after each push
  and on an interval, with write-only credentials, "Push now" and the last result.

### Upgrade notes
- Migrations `0038_imports` and `0039_push_mirrors` add `repo_imports` and
  `repo_push_mirrors`; both start empty.
- New config sections `[imports]` and `[push_mirrors]` (defaults: enabled, no private
  addresses). Push mirror secrets are stored as given, like webhook secrets.
- Imports interrupted by a restart are marked failed at start-up (the token is not kept).
```

## README

Feature line:

```markdown
- **Import from GitHub and Gitea** with issues, pull requests (numbers kept), labels,
  milestones, releases and LFS objects, and **push mirrors** that keep a copy elsewhere.
```

Configuration table rows:

| Section | Purpose |
|---|---|
| `[imports]` | Importing from GitHub/Gitea: on/off, private addresses, concurrency, Git and API timeouts, rate-limit wait, author mapping |
| `[push_mirrors]` | Push mirrors: on/off, interval, debounce after pushes, push timeout, private addresses |

## `deploy/rbit.example.toml`

```toml
[imports]
# Offer "Import a repository" (GitHub, GitHub Enterprise, Gitea/Forgejo).
enabled = true
# Sources (API, Git, release files, LFS objects) on loopback, private, link-local and
# unique-local addresses are refused unless this is set.
allow_private_addresses = false
# Imports running at once; more wait in line.
max_concurrent = 2
git_timeout_seconds = 3600
# One API request; release file and LFS downloads get ten times this.
api_timeout_seconds = 60
# Longest wait for the source's rate limit to reset before the import fails.
rate_limit_max_wait_seconds = 900
# "none": the importing user writes everything, naming the original author;
# "email": authors with the same verified email address become local accounts;
# "email_or_username": also accounts with the same user name.
author_mapping = "none"

[push_mirrors]
enabled = true
# Every mirror is pushed at least this often.
interval_minutes = 480
# Pushes to a repository reach its mirrors this many seconds later (a burst is sent once).
debounce_seconds = 10
push_timeout_seconds = 900
allow_private_addresses = false
# (tests only) allow_local_urls = true admits file:// remotes.
```

## ARCHITECTURE text

For §2 (`rbit-core`): "Milestone 3 (M) added `import` (sources, options, progress) and
`push_mirror` (remotes and credential rules)." For `rbit-git`: "`remote` fetches imports
(branches, tags, `refs/pull/*/head`) and pushes push mirrors (force + prune of branches
and tags), passing credentials as an `Authorization` header through git's environment
(`GIT_CONFIG_COUNT`), never in arguments or URLs; `lfs_pointers` lists the LFS objects a
history refers to." For `rbit-services`, add to the services table:

| Service | Responsibility |
|---|---|
| `ImportService` (`imports`) | Imports from GitHub/Gitea: validates the source and creates the repository, then a background job copies Git, LFS objects, labels, milestones, issues and pull requests (numbers kept) and releases through the stores without publishing events; progress on `/{owner}/{repo}/import`; the token lives only in memory. |
| `PushMirrorService` (`push_mirrors`) | Push mirrors: write-only credentials, pushes after `RepositoryPushed` (debounced) and on an interval, outcome per mirror. |

§3.3 schema:

```
-- milestone 3 (M)
repo_imports       repo_id → repositories PK, importer_id SET NULL, source_kind(github|github_enterprise|gitea),
                   source_url, options JSON, status(queued|running|succeeded|failed), phase, counts JSON,
                   warnings JSON, error (iff failed), created_at, started_at, finished_at
repo_push_mirrors  id, repo_id → repositories, url, username, secret (write-only, as given), enabled,
                   sync_on_push, next_sync_at, last_attempt_at, last_success_at, last_error,
                   created_by SET NULL, created_at, updated_at, UNIQUE(repo_id, url)
```

Migrations table: `0038` `imports`, `0039` `push_mirrors`. §3.3 secrets paragraph: "…with
two documented exceptions: webhook secrets and push mirror secrets are stored as given".
§4.1/new page routes: `/new/import`, `/{owner}/{repo}/import`,
`/{owner}/{repo}/settings/push-mirrors` (list, add), `…/{id}` (edit), `…/{id}/sync`,
`…/{id}/delete`. §5 security table:

| Concern | Measure |
|---|---|
| Imports | Every host (API, Git, LFS, downloads) must be public unless `[imports] allow_private_addresses`: checked before the first request and in the HTTP client's resolver at connect time; API requests follow no redirects, downloads at most 5 re-checked hops; `Link` pagination stays on the API origin; the token goes only to the source's hosts, lives only in memory and git's environment, and is never stored or logged. Imported history publishes no events (no notifications, emails, webhooks, reference scans). |
| Push mirrors | Administrators only; remotes without credentials in the URL, internal addresses refused on save and before every push, no redirects; the secret is write-only and reaches git only as an environment-provided header; force + prune of branches and tags only. |

Events table: `RepositoryPushed` gains the push mirror subscriber (no new variants).

## Integration requests

1. `docs/ARCHITECTURE.md`, `README.md`, `CHANGELOG.md`, `deploy/rbit.example.toml`,
   `docs/deployment.md`: the texts above (deployment: mention that imports and push mirrors
   need outbound HTTPS to the source/remote, and the `allow_private_addresses` switches).
2. `docs/HANDOFF.md` §3: `RBIT_IMPORTS__ALLOW_PRIVATE_ADDRESSES=true` to import from a
   local Gitea; `RBIT_PUSH_MIRRORS__ALLOW_PRIVATE_ADDRESSES=true` likewise.
3. Optional: audit log entries for push mirror changes (needs new `AuditAction` variants
   in `rbit-storage/src/sqlite/audit.rs`, owned elsewhere) — like webhook changes, since
   push mirrors carry credentials.
4. Optional: a "Import in progress" banner on the repository home linking to
   `/{owner}/{repo}/import` (repository page template, owned elsewhere), and an API for
   imports and push mirrors.
5. Rebase notes: shared files touched are only appended marked blocks — `rbit-core`
   `lib.rs`/`config/mod.rs`, `rbit-storage` `sqlite/mod.rs` and `sqlite/issues.rs` (the
   imported-thread block before the tests), `rbit-git/lib.rs`, `rbit-services/lib.rs`
   (plus shared `releases`/`lfs` construction), `rbit-web` `routes/mod.rs`, `views/mod.rs`,
   `views/repo_settings.rs` (one card line), `templates/repo/new.html` (the link sentence),
   `rbit-e2e/tests/e2e/main.rs`, the three `Cargo.toml`s and `upgrade.rs`.
