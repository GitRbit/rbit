# Milestone 2 — A real forge

The working contract for the collaboration layer that makes Rustit competitive with
Gitea and Forgejo. It ships in two waves, because pull requests reuse the issue
timeline and comments:

- **Wave A** (this document, in detail): three independent workstreams, built in
  parallel, each on its **own branch in its own git worktree**, each owning a disjoint set
  of files. Every shared registration point (services, stores, routes, views, config
  sections, settings links, domain events, audit actions, crates) is already pre-wired on
  `main` as an inert stub.
- **Wave B** (outline at the end): issues, pull requests and branch protection, specified
  in full once wave A has merged.

## Goals

1. People other than the owner can work on a repository with exactly the access they
   need — collaborators, organisation teams — and repositories can change hands.
2. Rustit can be automated: a versioned, documented REST API and signed webhooks.
3. Repositories feel complete: working README links and images, downloads, releases,
   blame, branch comparison and forks.

Non-goals for wave A: issues, pull requests, notifications, branch protection, API
endpoints for wave A's own new resources (collaborators, teams, releases, forks —
added in wave B once the services exist on `main`), LFS, a JavaScript SPA.

## Wave A at a glance

| | Workstream | Scope | Migrations |
|---|---|---|---|
| **A** | Access | Permission levels, collaborators, organisation teams and base permission, exhaustive `repo_access` policy, repository transfer | `0009` (redirects: `0016`) |
| **W** | API & webhooks | `/api/v1` (token scopes, pagination, errors, OpenAPI), repository and organisation webhooks with HMAC signatures, a persistent delivery queue with retries and a delivery log | `0011`, `0012` |
| **R** | Repository essentials | README relative links and images, `zip`/`tar.gz` archives, releases (notes; attached files as stretch), blame, compare, forks; pull mirrors as stretch | `0013`, `0014`, `0015` |

Migration numbers are reservations, not files: sqlx accepts gaps, so an unused number
simply stays unused. Never create a file outside your range. Files are named
`NNNN_<topic>.sql`, `STRICT` tables, timestamps as Unix seconds, foreign keys with
explicit `ON DELETE` behaviour.

---

## Shared contracts (already on `main`)

### Domain events (`rustit_services::events`)

```rust
pub enum DomainEvent {
    // Published by PushService::complete for HTTP and SSH pushes.
    RepositoryPushed { repo: RepoId, pusher: Option<AccountId>, updates: Vec<RefUpdate> },
    RepositoryCreated { repo: RepoId, actor: AccountId },          // RepositoryService::create_in
    RepositoryDeleted { repo: RepoId, owner: AccountId, full_name: String }, // settings + admin
    RepositoryTransferred { repo: RepoId, from: AccountId, to: AccountId },  // A publishes
    RepositoryForked { repo: RepoId, parent: RepoId, actor: AccountId },     // R publishes
    ReleasePublished { repo: RepoId, release: ReleaseId, actor: AccountId }, // R publishes
    UserSignedUp { user: AccountId },
    PasswordChanged { user: AccountId },
    EmailChanged { user: AccountId, previous: String },
}

// rustit_git::push (re-exported as rustit_services::events::RefUpdate)
pub struct RefUpdate { pub name: String /* refs/heads/main */, pub before: Option<String>, pub after: Option<String> }
impl RefUpdate { fn is_created(); fn is_deleted(); fn branch() -> Option<&str>; fn tag() -> Option<&str> }
impl RepositoryReader {
    async fn ref_snapshot(&self) -> Result<RefSnapshot, GitError>;
    /// Commits reachable from `head` but from none of `excluding`, newest first.
    async fn new_commits(&self, head: String, excluding: Vec<String>, limit: usize) -> Result<Vec<CommitSummary>, GitError>;
}
```

Forks publish `RepositoryForked` only (not `RepositoryCreated`). A fork must not go
through `RepositoryService::create_in`, which initialises an empty repository. Mirror
fetches (R, stretch) report through `PushService::begin/complete` with `pusher: None`.
Subscribers must ignore variants they do not handle (`_ => {}`), so adding variants never
breaks them.

### Background work

`Services::spawn_background()` starts `webhooks`, `releases` and `mirrors` (stubs
today); `Services::shutdown()` awaits `jobs`, `webhooks`, `releases` and `mirrors` and
is called by `serve` before the stores close. Each `shutdown` must stop new work, cancel
or finish what is running, and return within ~10 s (see `JobService` for the pattern:
`CancellationToken` + `TaskTracker`). Remove the `#[expect(clippy::unused_async)]` on your
stub once it does real work (the expectation then fails, by design).

### Settings links

Repository and organisation settings pages render a list of `SettingsCard`s
(`views::components::SettingsCard { url, icon, title, description }`, drawn by the
`ui::settings_card` macro; icons: `server`, `users`, `webhook`, `refresh`, `transfer`).
Each workstream returns its cards from a function in a file it owns:

| Function (stub returns `Vec::new()`) | Owner | Rendered on |
|---|---|---|
| `views::collaborators::repo_settings_cards(&RepoHeader)` | A | `/{owner}/{repo}/settings` |
| `views::transfer::danger_zone_cards(&RepoHeader)` | A | the danger zone of the same page |
| `views::teams::org_settings_cards(settings_url)` | A | `/{org}/settings` |
| `views::webhooks::repo_settings_cards(&RepoHeader)` | W | `/{owner}/{repo}/settings` |
| `views::webhooks::org_settings_cards(settings_url)` | W | `/{org}/settings` |
| `views::mirrors::repo_settings_cards(&RepoHeader)` | R | `/{owner}/{repo}/settings` |

Repository sub-pages load the repository with `routes::repo::RepoPage::load` and build
the header with `loaded.header(&state, "settings")`, exactly like the deploy keys page.
Only repository administrators get these pages; everyone else gets 404.

### Routers

- Page routes (sessions, CSRF, security headers, 30 s timeout, 1 MiB bodies, HTML error
  pages): `collaborators`, `teams`, `transfer`, `webhooks`, `releases`, `blame`,
  `compare`, `forks`, `mirrors` — each `routes()` is merged into `page_routes`.
- Merged **outside** the page router, bringing their own layers:
  `api::routes()` (W), `archives::routes(&state)` (R), `releases::file_routes(&state)` (R).
  They must not set a fallback. `archives` and `file_routes` apply
  `middleware::session` and `middleware::security_headers` themselves (and
  `middleware::csrf_origin_check` for uploads); the API uses no cookies at all.
- New second-level paths under `/{owner}/` must be reserved repository names
  (`RESERVED_REPO_NAMES`; `teams` is already reserved). Ask the integrator for others.

### Configuration (defaults on `main`, documented in each file)

| Section | File | Owner |
|---|---|---|
| `[api]` `enabled`, `default_page_size` (30), `max_page_size` (100) | `config/api.rs` | W |
| `[webhooks]` `enabled`, `allow_private_addresses` (false), `timeout_seconds` (10), `max_attempts` (5), `concurrency` (4), `delivery_retention_days` (14) | `config/webhooks.rs` | W |
| `[releases]` `max_asset_bytes` (512 MiB), `max_assets_per_release` (50), `upload_timeout_seconds` (3600) | `config/releases.rs` | R |
| `[mirrors]` `enabled`, `default_interval_minutes` (480), `min_interval_minutes` (10), `fetch_timeout_seconds` (900), `allow_private_addresses` (false) | `config/mirrors.rs` | R |

`StorageConfig::attachments_dir()` is `<data_dir>/attachments`. You may add fields to your
own sections (with defaults and doc comments); the example config in `deploy/` is updated
by the integrator from your report.

### Audit log and throttling

Declared in `rustit-storage/src/sqlite/audit.rs` (category "Repository access"):
`CollaboratorAdded`, `CollaboratorChanged`, `CollaboratorRemoved`, `TeamCreated`,
`TeamChanged`, `TeamDeleted`, `RepositoryTransferred` (A); `WebhookCreated`,
`WebhookChanged`, `WebhookDeleted` (W). Record them with
`AuditService::record(AuditEntry::new(action).by(actor).on(target))`.
`ThrottleScope::ApiAuth` (presented tokens, per client address) and
`ThrottleScope::ApiRequest` (authenticated requests, per account) exist for W.

---

## Workstream A — Access

### Scope

1. **Permission levels.** `rustit_core::membership::Permission { Read, Write, Admin }`
   with `as_str`/`parse`/`label`/`ALL` and `access() -> RepoAccess`. Used by
   collaborators, teams and the organisation base permission (`None | Read | Write`,
   stored as `none`/`read`/`write`).
2. **Collaborators** on any repository (personal and organisation-owned "outside
   collaborators"): repository admins add a user by username with a permission, change it,
   remove it; a collaborator can remove themselves. No invitations in wave A (direct add;
   suspended users cannot be added). `/{owner}/{repo}/settings/collaborators`.
3. **Teams** inside organisations: owners create/rename/delete teams (`TeamName`
   validated like a slug, unique per organisation, case-insensitive); members must be
   organisation members (leaving or being removed from the organisation removes them from
   its teams); each team grants a `Permission` per repository of that organisation.
   Organisation members see the team list and pages; owners manage them. Pages:
   `/{org}/teams`, `/{org}/teams/new`, `/{org}/teams/{team}` (members, repositories),
   `/{org}/teams/{team}/settings`; a **Teams** tab in `org/header.html`; the
   repository's collaborators page also lists team grants (read-only, linking to the team).
4. **Organisation base permission** (org settings): what every member gets on every
   organisation repository — `write` by default, which preserves today's behaviour.
5. **Policy.** Extend `rustit_auth::policy::repo_access` to take what the caller looked up:

   ```rust
   #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
   pub struct Grants {
       pub org_role: Option<OrgRole>,          // viewer's role in the owning organisation
       pub org_base: Option<Permission>,       // that organisation's base permission (None = none)
       pub team: Option<Permission>,           // best grant among the viewer's teams for this repository
       pub collaborator: Option<Permission>,   // the viewer's collaborator permission
   }
   pub fn repo_access(viewer: Option<&User>, repo: &OwnedRepository, grants: &Grants) -> RepoAccess;
   ```

   Effective access is the **maximum** of: public baseline (`Read` if public), site admin
   → `Admin`, personal owner → `Admin`, organisation owner → `Admin`, organisation member
   → `org_base` and `team`, collaborator → `collaborator`. Organisation fields are ignored
   for personal repositories; nothing ever lowers access below the public baseline. Keep
   it pure and **exhaustively tested** (iterate every combination of actor kind,
   visibility, owner kind and each `Option<…>` field, and assert against an independent
   reference table in the test). Update the module's table.
6. **Every access decision uses it.** `RepositoryService::open` and `list_for_owner`
   look up `Grants` (one query; skip it when the answer cannot change the outcome).
   Listings expressed in SQL (`list_by_owner`, `list_for_member` — the dashboard now
   includes repositories shared through collaborators or teams — and `StarStore::starred_by`)
   use one shared readable-by predicate defined once in `sqlite/repositories.rs`
   (a `macro_rules!` producing a `'static` SQL fragment), with a test that the SQL and the
   pure policy agree on a fixture matrix. Git over HTTP and SSH pick this up automatically
   through `RepositoryService::open`; deploy keys are unchanged.
7. **Transfer** (`/{owner}/{repo}/settings/transfer`, a danger-zone card): a repository
   admin moves the repository to an organisation where they may create repositories
   (same rule as `owner_for_new_repository`) or to their own account; site admins may
   move it to any user or organisation. Confirm by typing `owner/name`. A name clash at
   the target is a 422. Order: move the directory with `rustit_git::relocate`
   (`<root>/<old owner>/<id>.git` → `<root>/<new owner>/<id>.git`, same filesystem
   rename, parent created), then update `owner_id` in a transaction that also drops team
   grants of the old organisation; if the database step fails, move the directory back.
   Collaborators, stars, forks, releases and webhooks stay (they reference the id).
   Publish `RepositoryTransferred` and audit it. **Stretch:** redirects from the old
   `owner/name` (and from renames) to the new location — done after wave A in `0016`
   (`0010` was already taken by `0010_preferences.sql`).

### Migrations

- `0009_access.sql`: `collaborators(repo_id → repositories CASCADE, user_id → accounts
  CASCADE, permission CHECK IN ('read','write','admin'), added_by → accounts SET NULL,
  created_at, PRIMARY KEY (repo_id, user_id))`; `teams(id, org_id → accounts CASCADE,
  name NOCASE, description, created_at, UNIQUE(org_id, name))`; `team_members(team_id
  CASCADE, user_id CASCADE, PK)`; `team_repositories(team_id CASCADE, repo_id CASCADE,
  permission CHECK, PK)`; `org_profiles.member_permission TEXT NOT NULL DEFAULT 'write'
  CHECK IN ('none','read','write')`; indexes for "repositories shared with user".
- `0016_repo_redirects.sql` (moved from the reserved `0010`, which `0010_preferences.sql`
  already used): `repo_redirects(owner_id → accounts CASCADE, name NOCASE, repo_id →
  repositories CASCADE, created_at, PK (owner_id, name))`, kept by triggers on
  `repositories`: leaving a location records it, arriving at one removes its redirect.

### Files A owns

Stubs marked ★ already exist on `main`.

- core: `rustit-core/src/{membership.rs★,access.rs,org.rs}`
- auth: `rustit-auth/src/policy.rs`
- git: `rustit-git/src/relocate.rs★`
- storage: `rustit-storage/src/sqlite/{collaborators.rs★,teams.rs★,repositories.rs,orgs.rs,stars.rs}`,
  `rustit-storage/migrations/0009_*.sql`, new `rustit-storage/tests/{collaborators,teams}.rs`
- services: `rustit-services/src/{collaborators.rs★,teams.rs★,transfers.rs★,repositories.rs,orgs.rs}`,
  new `rustit-services/tests/{access,teams,transfers}.rs`, `rustit-services/tests/orgs.rs`
- web: `rustit-web/src/routes/{collaborators.rs★,teams.rs★,transfer.rs★,orgs.rs}`,
  `rustit-web/src/views/{collaborators.rs★,teams.rs★,transfer.rs★,org.rs}`,
  `rustit-web/templates/org/**`, new `rustit-web/templates/access/**`
- e2e: `rustit-e2e/tests/e2e/access.rs★`

Keep on `main`'s promises: `RepositoryService::owner_for_new_repository` stays
`pub(crate)` with today's semantics (R's forks call it); `create_in` keeps publishing
`RepositoryCreated`; `OrgSettingsPage.cards` and `org_settings_cards` stay (W's link);
do not add fields to `rustit_core::Repository`/`OwnedRepository` (they are built in many
tests across the workspace).

### Tests expected

Exhaustive policy tests; store tests for every query (including the readable-by
predicate matrix); service tests for each rule (who may add/remove, last-owner and
non-member cases, suspended users, transfer rollback when the target name exists); view
render tests; e2e: a read collaborator can clone a private repository but not push, a
write collaborator pushes over HTTP with a token, a team grant works the same way, others
still get 404/401, and after a transfer the repository clones from the new URL.

---

## Workstream W — API & webhooks

### Scope

1. **API conventions** (`/api/v1`, all JSON, UTF-8):
   - Authentication: `Authorization: Bearer rit_…` (also accept `token rit_…`).
     Never cookies. Anonymous requests may read public data. Invalid tokens → 401 with
     `WWW-Authenticate: Bearer`; throttle presented tokens with `ThrottleScope::ApiAuth`
     per client address and authenticated requests with `ThrottleScope::ApiRequest` per
     account (429 + `Retry-After`). Suspended owners and users with expired tokens are
     refused. Two-factor accounts work normally (tokens are the second factor's escape hatch).
   - Scopes: extend `TokenScope` with `Admin` (`admin`). `read` → GET only; `write` →
     mutations that need write access (and Git push, as today); `admin` → repository
     settings, webhooks, deletion. A token never exceeds its owner's access
     (`TokenScope::limit`). The tokens page offers the new scope.
   - Errors: `{"message": "…"}` plus `"errors": [{"field": "…", "message": "…"}]` for 422.
     401 unauthenticated, 403 authenticated but not allowed (only for things the caller may
     see), **404 for anything the caller may not know exists**, 409 conflicts, 413, 422, 429.
   - Pagination: `?page=` (1-based) and `?per_page=` (default/max from `[api]`);
     responses carry `Link` (RFC 8288: `first`, `prev`, `next`, `last`) and `X-Total-Count`.
   - Representation: snake_case fields, numeric `id`s, RFC 3339 UTC timestamps, absolute
     `html_url`/`clone_url` built from `server.public_url`, no nulls omitted silently
     (explicit `null`). Request bodies capped at 1 MiB; 30 s timeout; gzip responses.
   - `GET /api/v1/openapi.json`: a hand-written OpenAPI 3.1 document embedded with
     `include_str!`, plus a test that the set of documented paths/methods equals the set
     the router registers (build both from one route table).
   - Unknown `/api/v1/…` paths answer a JSON 404 from an explicit catch-all route; when
     `[api] enabled = false` everything under `/api` answers 404.
2. **Endpoints in wave A:** `GET /version`; `GET /user`; `GET /users/{name}`;
   `GET /orgs/{org}`, `GET /orgs/{org}/members`; `GET /user/repos`, `GET /users/{name}/repos`,
   `GET /orgs/{org}/repos`; `POST /user/repos`, `POST /orgs/{org}/repos`;
   `GET|PATCH|DELETE /repos/{owner}/{repo}`; `GET /repos/{owner}/{repo}/branches`,
   `…/branches/{branch}` (branch names may contain `/`), `…/tags`, `…/commits?sha=&page=`,
   `…/commits/{ref}`; `GET /user/starred`, `PUT|DELETE /user/starred/{owner}/{repo}`;
   webhook management (below). Every endpoint goes through existing services — no SQL or
   Git access from handlers. Endpoints for collaborators, teams, releases and forks come in
   wave B (the services do not exist on your branch).
3. **Webhooks** for repositories (`/{owner}/{repo}/settings/hooks`) and organisations
   (`/{org}/settings/hooks`, firing for every repository of the organisation), managed by
   repository admins / organisation owners, in the UI and via
   `GET|POST /repos/{owner}/{repo}/hooks`, `GET|PATCH|DELETE …/hooks/{id}`,
   `POST …/hooks/{id}/pings`, `GET …/hooks/{id}/deliveries`,
   `POST …/hooks/{id}/deliveries/{delivery}/attempts` (redeliver), and the same under
   `/orgs/{org}/hooks`. Maximum 20 hooks per repository or organisation.
   - Fields: `url` (`http`/`https` only, ≤ 2048 bytes, no credentials in the URL),
     `secret` (optional), `events` (`push`, `repository`, `release`, `fork`; `ping` is
     always sent on creation), `active`. Content type is always `application/json`.
   - **Secret handling:** HMAC needs the raw secret, so it is the one documented
     exception to "secrets only as digests": stored as given, write-only (never shown or
     returned again; the UI says "configured"), excluded from logs and the delivery log.
   - **Payloads** follow GitHub's shapes where they exist, so existing receivers work:
     `push` (`ref`, `before`, `after` — 40 zeros for creation/deletion — `created`,
     `deleted`, `commits` (≤ 20 via `new_commits`, plus `total_commits`), `head_commit`,
     `pusher`, `repository`, `sender`, `compare` URL); one delivery per `RefUpdate`.
     `repository` (`created`, `deleted`, `transferred`), `release` (`published`),
     `fork`, `ping` (`zen`, `hook_id`, `hook`). Deleted repositories only reach
     organisation hooks.
   - **Headers:** `User-Agent: Rustit-Hookshot/<version>`, `X-Rustit-Event`,
     `X-Rustit-Delivery` (random id), `X-Rustit-Signature-256: sha256=<hex HMAC-SHA256 of
     the body>` and the GitHub-compatible `X-GitHub-Event`, `X-GitHub-Delivery`,
     `X-Hub-Signature-256` when a secret is set.
   - **Delivery queue:** persisted in SQLite so restarts lose nothing. The event subscriber
     only inserts rows (`pending`, `next_attempt_at = now`); a worker claims due rows,
     runs at most `[webhooks] concurrency` requests, treats 2xx as success, does **not**
     follow redirects, applies `timeout_seconds`, and retries with exponential backoff and
     jitter (≈ 10 s, 1 min, 10 min, 1 h, 6 h) until `max_attempts`, then `failed`. Each
     attempt records status, duration, response headers and the first 64 KiB of the body.
     A daily task prunes deliveries older than `delivery_retention_days`. The worker stops
     cleanly in `WebhookService::shutdown`.
   - **SSRF protection:** unless `allow_private_addresses`, refuse targets that resolve to
     loopback, private (RFC 1918), CGNAT (100.64/10), link-local, unique-local (fc00::/7),
     multicast, unspecified or documentation addresses (and IPv4-mapped forms). Check at
     delivery time inside a custom DNS resolver for the HTTP client, so DNS rebinding cannot
     bypass it; reject literal private IPs at configuration time with a clear message.
   - UI: list with last delivery status, create/edit form, delete (confirm), recent
     deliveries with request headers/body and response, redeliver, ping. Monochrome; status
     colour only for success/failure badges.
   - Dependencies (W's blocks): `reqwest` is already in the workspace for tests; enable it
     in `rustit-services/Cargo.toml` with `features = [...]` as needed. Prefer the `ring`
     rustls provider (as lettre and russh use) so Windows builds need no CMake/NASM;
     `hmac` and `sha2` are already workspace dependencies; `serde`/`serde_json` for payloads.

### Migrations

- `0011_webhooks.sql`: `webhooks(id, repo_id → repositories CASCADE NULL, org_id →
  accounts CASCADE NULL, CHECK exactly one set, url, secret, events (JSON array text),
  active, created_by SET NULL, created_at, updated_at)`; `webhook_deliveries(id, hook_id
  CASCADE, guid UNIQUE, event, action, payload, status CHECK IN
  ('pending','delivering','succeeded','failed'), attempts, next_attempt_at,
  last_attempt_at, response_status, response_headers, response_body, duration_ms,
  created_at)` with an index on `(status, next_attempt_at)`.
- `0012_token_scopes.sql`: rebuild `access_tokens` to allow `'admin'` in the scope CHECK
  (SQLite cannot alter a CHECK: create the new table, copy every row with its id, drop,
  rename, recreate indexes — one migration, nothing lost).

### Files W owns

- core: `rustit-core/src/{webhook.rs★,access_token.rs,config/api.rs★,config/webhooks.rs★}`
- storage: `rustit-storage/src/sqlite/{webhooks.rs★,access_tokens.rs}`,
  `rustit-storage/migrations/0011_*.sql`, `0012_*.sql`,
  `rustit-storage/tests/access_tokens.rs`, new `rustit-storage/tests/webhooks.rs`
- services: `rustit-services/src/{webhooks.rs★,tokens.rs}`, `rustit-services/tests/tokens.rs`,
  new `rustit-services/tests/{webhooks,api}*.rs`
- web: `rustit-web/src/routes/api/**★`, `rustit-web/src/routes/{webhooks.rs★,settings.rs}`,
  `rustit-web/src/views/{webhooks.rs★,settings.rs}`, `rustit-web/templates/settings/tokens.html`,
  new `rustit-web/templates/webhooks/**`
- e2e: `rustit-e2e/tests/e2e/api.rs★`, `rustit-e2e/tests/e2e/tokens.rs`

`routes/settings.rs` and `views/settings.rs` hold the whole account settings area: change
only the token parts and keep the rest byte-for-byte.

### Tests expected

Unit tests for the signature (against a known vector), address classification (every
range above, IPv4-mapped IPv6), backoff schedule, payload shapes (field-by-field), Link
header arithmetic and scope checks; store tests for queue claiming (no double delivery
with two workers) and pruning; a service test delivering to a local listener (test config
sets `allow_private_addresses = true`) including a retry after a 500 and the recorded
attempts; API handler tests through `tower::ServiceExt::oneshot`; e2e: create a token,
read `/api/v1/user`, page through repositories (headers), get 404 for a private repository
with a read token of another user, create a hook via the API, `git push`, receive the
signed `push` delivery on a local listener and verify the HMAC.

---

## Workstream R — Repository essentials

### Scope

1. **README relative links and images.** Markdown rendering takes a context (owner,
   repository, revision, directory of the document). Relative link targets resolve against
   that directory: links → `/{owner}/{repo}/blob/{rev}/{path}` (directories:
   `/tree/…`), images → `/{owner}/{repo}/raw/{rev}/{path}`; `..` is normalised and
   anything escaping the repository root is dropped to plain text; absolute URLs, `#anchors`
   and `mailto:` are untouched; heading anchors get stable ids. Sanitisation with `ammonia`
   stays the last step (property tests keep passing). Applies to READMEs on the code tab,
   rendered `.md` blobs and release notes (resolved against the release tag).
2. **Archives:** `GET /{owner}/{repo}/archive/{rev}.zip` and `.tar.gz` (`rev` may contain
   `/`; parse the suffix). `git archive --format=zip|tar --prefix={repo}-{rev}/ {commit}`
   with the scrubbed environment used by the transports; `tar.gz` is gzipped in-process
   (`async-compression`). Streamed, never buffered; `Content-Disposition: attachment` with
   a sanitised file name; `ETag` = commit id; `ArchiveService` bounds concurrent archives
   (e.g. 4; 503 + `Retry-After` when busy). Access exactly like browsing (session
   viewer, 404 when hidden). Links: "Download ZIP" in the Clone menu, archives on tags and
   releases.
3. **Releases:** `/{owner}/{repo}/releases` (newest first, paginated), `/releases/tag/{tag}`,
   `/releases/new`, edit and delete for writers; a **Releases** tab in the repository
   sub-navigation; the latest release in the sidebar. A release names an **existing tag**
   (creating a tag from a branch is a stretch goal), has a title, Markdown notes, draft and
   pre-release flags; drafts are visible to writers only. Publishing (create as non-draft,
   or publish a draft) emits `ReleasePublished`.
4. **Release files — storage design** (binding; the upload UI itself is a **stretch goal**):
   - Rows in `release_assets`; bytes on disk at `<attachments_dir>/<repo_id>/<asset_id>` —
     numeric ids only, never user-supplied names in paths, independent of the owner so
     transfers need no move.
   - Upload (`POST /{owner}/{repo}/releases/{id}/assets`, `multipart/form-data`, in
     `releases::file_routes` with session + CSRF + security headers, no page timeout,
     `upload_timeout_seconds` instead, body limit `max_asset_bytes` + form overhead):
     stream into `<attachments_dir>/tmp/<random>.part` while computing SHA-256 and size,
     abort and delete as soon as the limit is exceeded; then insert the row (name, size,
     content type as declared — informational only —, digest, uploader), fsync and rename
     the temporary file to its final path; if the rename fails, delete the row. At most
     `max_assets_per_release`, names unique per release (≤ 255 bytes, no `/`, `\`, control
     characters or leading dots).
   - Download (`GET /{owner}/{repo}/releases/download/{tag}/{name}`, also in
     `file_routes`): access check as for browsing; always
     `Content-Type: application/octet-stream`, `Content-Disposition: attachment;
     filename*=UTF-8''…`, `X-Content-Type-Options: nosniff`, `Content-Length`, `ETag` from
     the digest; streamed; `download_count` incremented best-effort.
   - Deleting an asset removes the row, then the file. On `RepositoryDeleted`,
     `ReleaseService`'s background task removes `<attachments_dir>/<repo_id>`; at start-up
     it sweeps `tmp/*.part` older than a day. Backups: report the needed change to
     `rustit backup` (integrator).
   - Needs the `multipart` feature of axum: R may change the `axum` line of
     `rustit-web/Cargo.toml`.
5. **Blame:** `/{owner}/{repo}/blame/{rev}/{path}` using gitoxide's `blame` (R may add the
   feature to `rustit-git/Cargo.toml`), bounded by file size and a time budget (fall back to
   a "too large to blame" notice); hunks show commit summary, author, relative date and
   link to the commit; a **Blame** button on the blob view.
6. **Compare:** `/{owner}/{repo}/compare` (pick base and head) and
   `/compare/{base}...{head}`: merge base, ahead/behind counts, the commits in head not in
   base (`new_commits`), and the diff from the merge base to head with the existing
   `DiffLimits`. Same repository only in wave A (cross-fork compare comes with pull requests).
7. **Forks:** `POST /{owner}/{repo}/fork` (choose yourself or an organisation where
   `owner_for_new_repository` allows it; optional new name); any reader may fork. The fork
   keeps the parent's visibility (a private parent yields a private fork), description and
   default branch. Disk: `rustit_git::fork` runs `git clone --bare --local` (hard links
   are safe: objects are immutable) and keeps only branches and tags; on failure the row is
   removed. `repo_forks` remembers the parent; the header shows "forked from owner/name",
   the sub-navigation a fork button with count, `/{owner}/{repo}/forks` lists readable
   forks. Deleting a parent leaves forks intact (parent becomes `NULL`). Publishes
   `RepositoryForked`.
8. **Pull mirrors (stretch):** "Mirror a repository" on `/new` (an `https://` URL,
   optional interval), `/{owner}/{repo}/settings/mirror` (interval, sync now, last result,
   remove mirroring). Fetch with the `git` executable (`--prune`, refs/heads and refs/tags
   only, scrubbed environment, `GIT_TERMINAL_PROMPT=0`, no credential helpers, only
   `https`/`http`, timeout, the same address checks as webhooks unless
   `allow_private_addresses`), report through `PushService` with `pusher: None`. Pushes to a
   mirror must be refused — describe the change to `git_http.rs` / `rustit-ssh` in your
   report rather than editing them.

### Migrations

- `0013_releases.sql`: `releases(id, repo_id CASCADE, tag_name, name, notes, is_draft,
  is_prerelease, author_id SET NULL, created_at, updated_at, published_at,
  UNIQUE(repo_id, tag_name))`; `release_assets(id, release_id CASCADE, repo_id CASCADE,
  name, size, content_type, sha256 BLOB, download_count, uploader_id SET NULL, created_at,
  UNIQUE(release_id, name))`.
- `0014_forks.sql`: `repo_forks(repo_id PRIMARY KEY → repositories CASCADE, parent_id →
  repositories SET NULL, created_at)` with an index on `parent_id`.
- `0015_mirrors.sql` (stretch): `repo_mirrors(repo_id PK CASCADE, url, interval_minutes,
  next_sync_at, last_sync_at, last_error, created_at)`.

### Files R owns

- core: `rustit-core/src/{release.rs★,mirror.rs★,config/releases.rs★,config/mirrors.rs★}`
- git: `rustit-git/src/{archive.rs★,blame.rs★,compare.rs★,fork.rs★,mirror.rs★,changes.rs,diff.rs,model.rs,reader.rs,revision.rs}`,
  `rustit-git/Cargo.toml`, `rustit-git/tests/{changes.rs,storage.rs}` and new test files
- storage: `rustit-storage/src/sqlite/{releases.rs★,forks.rs★,mirrors.rs★}`,
  `rustit-storage/migrations/0013_*.sql`, `0014_*.sql`, `0015_*.sql`, new storage tests
- services: `rustit-services/src/{releases.rs★,forks.rs★,mirrors.rs★,archives.rs★}`, new service tests
- web: `rustit-web/src/routes/{archives.rs★,blame.rs★,compare.rs★,forks.rs★,mirrors.rs★,releases.rs★,repo.rs,history.rs,repo_create.rs}`,
  `rustit-web/src/views/{blame.rs★,compare.rs★,forks.rs★,mirrors.rs★,releases.rs★,repo.rs,refs.rs,commit.rs}`,
  `rustit-web/src/render/markdown.rs`, `rustit-web/src/urls.rs`,
  `rustit-web/templates/repo/**` **except `settings.html`**, new `rustit-web/templates/releases/**`,
  the `axum` line of `rustit-web/Cargo.toml`, `rustit-web/tests/properties.rs` (Markdown cases)
- e2e: `rustit-e2e/tests/e2e/essentials.rs★`

`RepoHeader` derives `Default`, so adding fields to it (forks, releases) does not break
other files' tests. `RepoPage::load`/`header()` are used by A's and W's pages: keep their
signatures.

### Tests expected

Markdown link resolution table (relative, `..`, escaping the root, absolute, anchors,
images) and the existing sanitisation properties; archive contents checked by listing a
produced zip and tar.gz in tests (prefix, files, a path with spaces); release rules
(drafts hidden from readers, tag must exist, unique tag) and, if uploads ship, size-limit
abort, name validation, download headers and clean-up on repository deletion; blame on a
fixture with known authorship; compare ahead/behind and merge-base; fork: refs copied,
visibility kept, fork of a private repository hidden from others, parent deletion; e2e:
download a zip of a private repository only when signed in, fork and push to the fork,
publish a release and see it on the Releases tab.

---

## Rules (all workstreams)

**Ownership.** Edit only files you own (lists above) and new files inside your own
directories. Shared crates' `Cargo.toml`: append only the lines you need, inside your
labelled block in the workspace `Cargo.toml` for new crates. Integrator files —
`rustit-services/src/{lib.rs,events.rs,error.rs,pushes.rs,jobs.rs,repo_settings.rs,admin.rs,audit.rs}`,
`rustit-storage/src/{lib.rs,sqlite/mod.rs,sqlite/rows.rs,sqlite/audit.rs,sqlite/admin.rs}`,
`rustit-git/src/{lib.rs,push.rs,storage.rs}`, `rustit-core/src/{lib.rs,names.rs,config/mod.rs}`,
`rustit-web/src/{lib.rs,error.rs,extract.rs,middleware.rs,state.rs,routes/mod.rs,views/mod.rs,views/components.rs,views/layout.rs,views/repo_settings.rs,routes/repo_settings.rs,routes/git_http.rs}`,
`rustit-web/templates/{base.html,macros.html,repo/settings.html}`, `rustit-web/styles/app.css`,
`rustit-server/**`, `rustit-ssh/**`, `rustit-e2e/src/lib.rs`, `docs/**`, `README.md` — and
everything owned by the deployment work (`Dockerfile`, `.dockerignore`, `deploy/**`,
`.github/**`, `CHANGELOG.md`, `docs/deployment.md`). Need a change there (a re-export, an
icon, a CSS component, an e2e harness helper, a reserved name, a `ServiceError` variant)?
**Describe the exact change in your final report**; work around it on your branch without
editing the file (e.g. import by module path, `rustit_core::membership::Permission`).

**Engineering** (same as milestone 1): layering `web → services → (storage, git, auth) →
core`; thin handlers, rules in services, SQL in stores, no logic in templates; validated
domain types; `//!` module docs; clippy pedantic clean; no `unsafe`; no `unwrap` outside
tests; `'static` SQL (`concat!`, `json_each(?)` for lists); Git work on the blocking pool;
422 for validation errors, 303 after success, 404 for anything the viewer may not know
exists; strict CSP (no inline styles or scripts — SVG attributes for widths); Origin-checked
forms; throttle anything that verifies a secret; secrets stored as digests (webhook secrets
are the documented exception).

**Design: monochrome minimal.** Reuse `.box`, `.box-header`, `.row`, `.btn`
(`-primary`/`-ghost`/`-danger`), `.input`, `.label`, `.hint`, `.badge`, `.tab`, `.menu`,
`ui::settings_card`; colour only for status, diffs, syntax and the language bar. Design at
375 px first. Icons available: `git-fork`, `git-compare`, `package`, `webhook`, `archive`,
`upload`, `trash`, `transfer`, `refresh` (plus the existing ones). Every template that
calls macros starts with `{% import "macros.html" as ui %}`; pass string literals to
`ui::icon`; never add a `macros.html` in a subfolder.

**Commits.** Work on your branch in your worktree; commit early and often. Each commit is
atomic: it builds, is formatted and passes `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` and the tests of the crates you
touched (run `cargo test --workspace` before you finish). Imperative subject ≤ 72
characters, blank line, body explaining what and why (wrapped at 72), trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Typical order: migration +
store → service + tests → routes/views/templates → e2e. Never rewrite `main`, never merge
other branches, never commit `assets/app.css`, `target/` or `data/`. A running dev server
locks `target/debug/rustit.exe`: build with `CARGO_TARGET_DIR` set to a directory of your
own if it is in use.

**Final report.** Commits (one line each), what is done and what is not (stretch goals
included), integration requests with exact changes, config fields added (for
`deploy/rustit.example.toml`), and anything you decided that this contract left open.

## Integration (after each workstream)

Rebase the branch onto `main` inside its worktree (conflicts are expected only in
`Cargo.lock` — take `main`'s and run `cargo metadata` — and in `Cargo.toml` dependency
lists — keep both), run the full gate, fast-forward `main`, remove the worktree, then
apply the integration requests as separate small commits. After all three: verify an
upgrade from a milestone 1 database through `0015`, extend `rustit backup` with the
attachments directory, update `docs/ARCHITECTURE.md` (schema, routes, events) and the
example configuration.

---

## Wave B outline (specified after wave A merges)

| Workstream | Scope |
|---|---|
| **Issues** | Issues per repository: Markdown bodies and comments, labels, milestones, assignees, open/closed, references (`#12`, `fixes #12` in pushed commits via `RepositoryPushed.updates` + `new_commits`), a shared timeline model reusable by pull requests, search and filters, notification events. |
| **Pull requests** | Compare branches including forks (`repo_forks`), the issue timeline, review comments on diff lines, approvals and change requests, merge strategies (merge commit, squash, rebase) run on a temporary worktree, conflict detection, a required-checks placeholder. |
| **Branch protection** | Rules per branch pattern (no force-push, required reviews, restricted pushers using A's permissions) enforced in HTTP and SSH receive paths as a pre-receive check, UI under repository settings. |
| **API (continued)** | Endpoints for collaborators, teams, releases (incl. asset upload), forks, issues and pull requests; webhook events for them. |

Reserved for wave B: migrations `0017`–`0030` (allocated per workstream in its contract;
`0016` holds the repository redirects).
