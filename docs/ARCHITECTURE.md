# rbit — Architecture & Design

rbit is a self-hosted Git forge written in Rust. It competes with Gitea, Forgejo and
GitLab CE on three axes: **raw speed**, **operational simplicity** (one static binary, no
external services), and **a modern, mobile-first interface**.

This document is the source of truth for how the system is put together and *why*.
When the code and this document disagree, fix one of them.

---

## 1. Guiding principles

| Principle | What it means in practice |
|---|---|
| **One binary, zero services** | SQLite + redb + bare Git repos on local disk. No Postgres, Redis, or JS runtime in production. |
| **Server-rendered first** | HTML is rendered by Askama (compiled templates, zero runtime parsing). HTMX adds interactivity without a SPA bundle. Pages work with JavaScript disabled. |
| **Layered, one-directional dependencies** | `web → services → (storage, git, auth) → core`. Lower layers never know about HTTP. |
| **Self-describing code** | Types carry meaning (`RepoName`, `Username`, `RepoAccess`) instead of raw `String`/`bool`. Validation happens once, at construction. |
| **Secure by default** | Argon2id, hashed session tokens, strict CSP, Origin-checked mutations, isolated Git config, path validation at the type level. |
| **Blocking work off the async runtime** | Git object access and password hashing always run in `spawn_blocking`. |
| **Mobile first** | Every template is designed at 360 px first, then enhanced with `sm:`/`md:`/`lg:` breakpoints. |

---

## 2. Workspace layout

```
rbit/
├── Cargo.toml                 # workspace: shared deps, lints, release profile
├── rustfmt.toml               # formatting standard (checked in CI)
├── package.json               # Tailwind v4 CLI, htmx, icon rendering (build-time only)
├── Dockerfile, deploy/        # container image, systemd unit, example production config
├── deny.toml                  # cargo deny: allowed licenses, advisories
├── .github/workflows/ci.yml   # fmt, clippy -D warnings, tests (Linux + Windows), builds, cargo deny
├── .github/workflows/release.yml  # v* tags: binaries, SHA256SUMS, GitHub release, ghcr.io image
├── docs/                      # ARCHITECTURE.md, deployment.md, design/ (per-phase contracts)
└── crates/
    ├── rbit-core/           # Domain model and configuration. No I/O, no async.
    ├── rbit-storage/        # SQLite (relational) + redb (key/value) persistence
    ├── rbit-auth/           # Passwords, session/token secrets, TOTP, passkeys, permission rules
    ├── rbit-git/            # Repository read model, diffs, languages, maintenance (gitoxide)
    ├── rbit-transport/      # Git smart protocol: stateless (HTTP) and interactive (SSH)
    ├── rbit-services/       # Application use-cases, domain events, background jobs
    ├── rbit-mail/           # Email rendering (Askama) and delivery (SMTP, log)
    ├── rbit-ssh/            # Built-in SSH server (russh) for Git over SSH
    ├── rbit-web/            # Axum router, handlers, Askama templates, assets, metrics
    ├── rbit-server/         # The `rbit` binary: CLI, config, wiring, backups, shutdown
    └── rbit-e2e/            # End-to-end and Git compatibility tests (test-only)
```

### Dependency graph

```
                    rbit-server (bin)
                   ┌────────┴─────────┐
              rbit-web          rbit-ssh
                   └────────┬─────────┘
             rbit-services ─────── rbit-transport
     ┌──────────┬───────┼─────────┬────────────┐
rbit-storage rbit-auth rbit-git rbit-mail
     └──────────┴───────┬─────────┴────────────┘
                    rbit-core
```

Each crate exposes a small, documented public API from `lib.rs`; internals live in
sub-modules grouped by concept (e.g. `storage::sqlite::repositories::users`).

### Crate responsibilities

**`rbit-core`** — Validated value types (`Username`, `RepoName`, `Email`, `BranchName`,
`TeamName`, `WebhookUrl`, `MirrorUrl`), domain entities (`User`, `Repository`,
`Visibility`, `Release`, `Webhook`), the `RepoAccess` permission lattice with the grant
levels `Permission` (`read`/`write`/`admin`) and `BasePermission` (`none`/`read`/`write`),
`network::is_internal_address` (the one classifier of addresses outbound requests must not
reach, shared by webhooks and mirrors), and the shared `Config` model. Milestone 2 wave B
added the issue model (`issue`: `IssueNumber`, `IssueKind`, `TimelineEvent`, `IssueAction`,
and the exhaustively tested `issue_permissions` / `comment_permissions` tables), the
reference and mention scanner (`references`: `#12`, `owner/repo#12`, `@name`, closing
keywords, outside code only), the issue search language (`issue_query`), pull request rules
(`pull`: approval tally, merge requirements via `evaluate`), and the pure branch protection
decisions (`protection`: patterns, `check_push`, merge rules) together with `ref_update`'s
`RefUpdate`. Milestone 3 added `status` (commit status states, contexts, reports and the
pure combined-state and required-checks decisions), `file_edit` (`FilePath` — no `..`, `.`
or empty parts, no `.git` in any spelling, no control characters or backslashes, ≤ 255 bytes
per part and 1024 in all — line endings, default commit titles), `lfs` (object ids, pointer
files, locks) and the `[editor]`, `[lfs]`, `[notifications]` and `[auth]` configuration
sections. Pure Rust, trivially unit-testable.

**`rbit-storage`**
- `sqlite/` — `sqlx` pool (WAL, foreign keys, busy timeout), embedded migrations, and one
  *store* per aggregate (`UserStore`, `RepositoryStore`). Stores map rows ↔ domain types.
  Wave B: `IssueStore` (`Database::issues()`, the only writer of the thread core),
  `issue_meta()`, `issue_search()` (FTS5 and filters), `notifications()`, `pulls()`,
  `reviews()` and `branch_protection()`. Milestone 3: `commit_statuses()`,
  `notification_email()`, `server_secrets()` (named, server-generated keys, created on
  first use with `get_or_insert`), `lfs()` and `external_identities()`.
- `sealing` — AES-256-GCM encryption of the credentials stores must read back, under a key
  file outside the database (`SealingKey`; see "Secrets" in §3.3).
- `kv/` — a `redb` database with typed table definitions. Used for sessions and derived,
  rebuildable data (caches). Everything in redb can be lost without losing user data.

**`rbit-auth`** — `PasswordHasher` (Argon2id, OWASP parameters), `SessionToken` and
`AccessTokenSecret` (256-bit random, only SHA-256 digests are persisted), TOTP (RFC 6238,
own implementation verified against the RFC test vectors) with recovery codes, a minimal
WebAuthn relying party for passkeys (`webauthn`: no attestation, ES256/EdDSA/RS256, a small
CBOR/COSE reader; origin, RP ID, challenge, user presence, signature and counter checked on
every ceremony), OpenID Connect support for external sign-in (`oidc`: the sealed `Flow`
cookie, PKCE, JWK sets and `verify_id_token`), and the pure `repo_access(viewer, repo, &Grants)` policy (who may read, write or administer a
repository). Callers look up `Grants` — the viewer's organisation role, the organisation's
base permission, the best team grant and the collaborator permission — in one query, and
the policy takes the **maximum** of the public baseline, site admin, owner, organisation
role, base permission, team and collaborator grants; nothing lowers access below the
public baseline. It is tested exhaustively against an independent reference table, and
the SQL listings use one readable-by predicate that a test checks against the policy.

**`rbit-git`** — `RepositoryStorage` (where bare repos live on disk, creation, deletion,
`relocate` for transfers) and `RepositoryReader` (refs, trees, blobs, commit history, ref
snapshots, `new_commits`, blame, compare/merge base) built on **gitoxide** (`gix`), plus
`archive` (`git archive`, streamed), `fork` (`git clone --bare --local`, branches and tags
only) and `mirror` (`git fetch --prune` of branches and tags, no redirects, no prompts or
credential helpers). Repos are always opened with *isolated* options: no system/global
config, no hooks executed on read; every `git` process gets a scrubbed environment. Wave B
added `overlay` (throw-away bare repositories under `<data_dir>/tmp/overlays` whose
`objects/info/alternates` list two repositories, for cross-fork compares), `merge` (merges,
squashes, rebases and trial merges without a working tree, §3.7) and `pull_refs`
(`refs/pull/{n}/head`). Milestone 3 added `edit` (`EditWorkspace`: commits made in the
browser, §3.8), which shares the merge's hand-over (fetch + `update-ref` compare-and-swap).

**`rbit-transport`** — The Git smart protocol. `ProcessTransport` runs
`git upload-pack/receive-pack` with a scrubbed environment in two modes: *stateless*
(`--stateless-rpc`, for HTTP `info/refs` and POSTs) and *interactive* (one bidirectional
conversation with exit status, for SSH). Both stream; pack data is never buffered. A
pure-Rust gitoxide transport could replace it without touching callers. `receive` reads a
push's command list before git sees it and writes refusals in git's own report format (the
push gate, §4.4); receive-pack entry points take `ReceiveOptions { deny_non_fast_forwards }`.

**`rbit-ssh`** — The built-in SSH server (russh): public-key authentication against user
SSH keys and repository deploy keys, strictly parsed `git-upload-pack` / `git-receive-pack`
exec requests and Git LFS's `git-lfs-authenticate` (§4.7), the same authorisation as HTTP, connection limits, throttled failures, and
no shells, PTYs or forwarding. The Ed25519 host key is generated on first start.
Each exec request rechecks the authenticated key and the holder's current permissions,
so an existing connection cannot outlive key revocation or retain administrator status.
`git-receive-pack` runs in two stateless steps so the push gate can decide between them
(§4.4); `git-upload-pack` stays interactive.

**`rbit-mail`** — A `Mailer` trait with SMTP (lettre over rustls), log and disabled
transports; every message is rendered from Askama templates with text and HTML parts.
`Mail.headers` (`MailHeaders`: `Message-ID`, `In-Reply-To`/`References`, one-click
`List-Unsubscribe`) is written by the SMTP and log transports; `ThreadNotification` is the
notification email (§5.1).

`MailService` admits at most 1,024 queued or active messages and opens at most eight
deliveries concurrently. Best-effort notifications that exceed the queue are dropped with
a warning and `rbit_mail_dropped_total`; account recovery and verification wait for a slot
up to the configured timeout. Delivery's timeout includes waiting for a relay slot.

**`rbit-services`** — Use-cases with business rules, one service per area, all built from
a shared `ServiceContext` (config, database, key/value store, repository storage):
`AccountService` (sign-up, sign-in, sessions, suspension), `RepositoryService` (create,
membership-aware access, listings, explore search), `OrgService` (organisations, members,
roles), `AccessTokenService`, `ProfileService`, `RepoSettingsService`, `StarService`,
`InsightsService` (languages, last commit per entry, cached), `InstanceService` (runtime
settings), `AdminService` (statistics, moderation), `ThrottleService` (rate limits),
`TwoFactorService`, `AuditService`, `MailService`, `RecoveryService` (password reset, email
verification), `SshKeyService`, `JobService` (background work) and
`PreferenceService` (the stored theme). Milestone 2 wave A added `CollaboratorService`,
`TeamService` and `TransferService` (access), `WebhookService` (hooks, queue, delivery
worker), and `ArchiveService` (at most 4 archives at once, 503 + `Retry-After` when busy),
`ReleaseService` (releases and attached files), `ForkService` and `MirrorService`
(scheduled fetches). Wave B added:

| Service | Responsibility |
|---|---|
| `TimelineService` (`timeline`) | The core shared by issues and pull requests: opening a thread for a viewer (404 when it does not exist or the repository is unreadable), comments, title/body edits, close/reopen, lock, and the merged timeline of comments and events. Permissions are recomputed for the actor on every change. `close_as_system` closes by commit or merge. |
| `IssueService` (`issues`) | Creating issues with labels, assignees and a milestone; labels (≤ 200 per repository, 25 per issue, default set), milestones, assignees (writers only, ≤ 10), deletion (admins), search; the metadata hook P uses; the background subscriber that turns references into timeline entries and closes issues from commits on the default branch and from pull requests merged into it. |
| `ReferenceService` (`references`) | Resolves `#N` and `@name` in a batch of texts for rendering; records `CrossReferenced` on targets the mention's author can read. |
| `NotificationService` (`notifications`) | The participation-based inbox (§5.1): subscriptions, fan-out from issue, comment and review events, `has_unread` for the header bell, retention pruning. |
| `PullService` (`pulls`) | The fork network, cross-repository compare through overlays, creating pull requests (`refs/pull/{n}/head`), following head and base pushes (including "merged outside rbit"), the mergeability worker, reviews and line comments, review-thread resolution, review requests, drafts, the merge box and merging (§3.7). |
| `ProtectionService` (`protection`) | Branch protection rules (admins only, audited), the push gate's verdict (`check_push`), merge requirements (`check_merge`), `may_delete_branch`, and `protected_branches` for the branches page badge. |

Milestone 3 added:

| Service | Responsibility |
|---|---|
| `StatusService` (`statuses`) | Commit statuses: `create` (write access, a full id that exists in the repository — for fork pull requests the head is found in the base repository as `refs/pull/{n}/head`), lists, the combined status per commit and for many commits, and the `status` webhook payload (`statuses::hooks`). `PullService::handle_status` re-evaluates open pull requests whose head received a status. |
| `NotificationEmailService` (`notification_email`) | Mails what the inbox fan-out writes (§5.1): preferences, the outbox/dedup claim, the per-address budget, threading headers and signed unsubscribe links. |
| `EditService` (`edits`) | Editing files in the browser (§3.8): authorisation, read-only repositories, the push gate, the author identity, line endings, uploads, and the `PushService` wrapping. |
| `LfsService` (`lfs`) | Git LFS: objects stored once under `<data_dir>/lfs` and linked per repository (fork networks share where readable), the batch decisions, signed transfer tokens (`lfs::tokens`), locks (`lfs::locks`), `store` for anything that brings objects into a repository, link copies for new forks and orphan clean-up. |
| `ExternalAuthService` (`external_auth`) | Sign-in with OpenID Connect providers and GitHub (§5.2): the flow cookie, the code exchange and ID token checks (HTTP in `external_auth/client.rs`), linking, unlinking and the register step; `Services::complete_external_sign_in` applies the linking rules. |

`CollaboratorService` gained `set` (idempotent add-or-change) and `access_of` (a user's
effective access from every grant), `TeamService` `ensure_member`, `ReleaseService` and
`ForkService` paged listings — what the REST API continuation needed.

`Services::spawn_background()` starts the webhook, release, mirror, issue, notification,
pull request and Git LFS tasks; `Services::shutdown()` stops them (and the job queue) within about ten
seconds.

Services communicate through an in-process `EventBus` of `DomainEvent`s, so producers never
depend on reactions such as cache warming, verification email or webhooks. Session
revocation is a synchronous credential-update step: security enforcement cannot depend on
delivery through a lossy broadcast channel. Subscribers ignore variants they do not handle,
so adding one breaks nothing.
`subscribe()` yields bare events; `subscribe_published()` yields them stamped with the
newest webhook id reserved at publish time (`Published`), for the webhook subscriber (§4.3).

| Event | Fields | Published by |
|---|---|---|
| `RepositoryPushed` | `repo`, `pusher: Option<AccountId>` (`None` for deploy keys and mirror fetches), `updates: Vec<RefUpdate>` (`name`, `before`, `after`; ordered by name) | `PushService::complete` (HTTP, SSH, mirror fetches, tags created for releases, pull request merges and head-branch deletions, commits made in the browser) |
| `Issue` | `repo`, `issue`, `number`, `kind` (`issue` \| `pull`), `actor: Option<AccountId>` (`None` for deploy-key pushes and mirrors), `action: IssueAction` | `TimelineService` (`Edited`, `Closed`, `Reopened`, `Locked`, `Unlocked`, for both kinds), `IssueService` (`Opened` for issues, `Deleted`, `Labeled`/`Unlabeled`, `Assigned`/`Unassigned`, `Milestoned`/`Demilestoned` — also on pull requests), `PullService` (`Opened` for pulls, `Synchronized`, `ReviewRequested`/`ReviewRequestRemoved`, `ConvertedToDraft`/`ReadyForReview`, `Closed { reason: Merged }`) |
| `IssueComment` | `repo`, `issue`, `number`, `kind`, `comment`, `actor`, `action` (`Created`, `Edited { previous_body }`, `Deleted { snapshot }`) | `TimelineService` (comments on issues and pull requests) |
| `PullRequestReview` | `repo`, `pull`, `number`, `review`, `actor`, `action` (`Submitted`, `Edited`, `Dismissed`) | `PullService` (reviews) |
| `RepositoryCreated` | `repo`, `actor` | `RepositoryService::create_in` (not forks) |
| `RepositoryDeleted` | `repo`, `owner`, `full_name` (as it was), `actor` | repository settings, site administration |
| `RepositoryTransferred` | `repo`, `from`, `to`, `actor` | `TransferService` |
| `RepositoryForked` | `repo`, `parent`, `actor` | `ForkService` |
| `ReleasePublished` | `repo`, `release`, `actor` | `ReleaseService` (created as non-draft, or a draft published) |
| `UserSignedUp`, `PasswordChanged`, `EmailChanged` | `user` (`previous` address for email) | `AccountService`, `AdminService`, `ProfileService`, `RecoveryService` |
| `CollaboratorChanged` (milestone 3) | `repo`, `user`, `actor`, `change` (`Added(permission)`, `Changed { from, to }`, `Removed`) | `CollaboratorService` (added, permission changed, removed; nothing when the permission stays the same) |
| `CommitStatus` (milestone 3) | `repo`, `status`, `sha` (full id), `actor` | `StatusService::create` (webhooks; the pull request service re-evaluates open pull requests with that head) |

Events carry ids, plus only what cannot be read back later (previous titles and bodies,
snapshots of deleted rows); subscribers (webhooks, notifications, references, pull request
updates) read the current state, so a reaction that runs twice does no harm.

Both Git transports report pushes through `PushService`, which diffs ref snapshots taken
before and after `receive-pack` (it runs without hooks). This is the only layer that
combines storage, git and auth. It is HTTP-agnostic: the web pages, the REST API, CLI
admin commands and background jobs all reuse it.

**`rbit-web`** — Axum 0.8 application:
```
rbit-web/src/
├── lib.rs              # build_router()
├── state.rs            # AppState (cheaply cloneable Arc)
├── error.rs            # WebError → status page / HTMX-friendly response
├── assets.rs           # embedded static files with content-hash cache busting
├── client_ip.rs        # ClientIp (trusted-proxy aware), Https detection
├── metrics.rs          # request metrics (Prometheus)
├── extract.rs          # Viewer, SignedIn, SiteAdmin, Page extractors
├── middleware.rs       # sessions, rate limiting, security headers, CSRF origin check
├── routes/             # one module per area; each feature exposes its own routes()
│   └── api/            # REST API /api/v1: route table, token auth, JSON errors, pagination, openapi.json
├── themes/             # generated theme catalog (tinted-theming schemes)
├── views/              # view models + Askama template structs
└── templates/          # *.html (Askama), organised by area
```

**`rbit-server`** — `clap` CLI (`serve`, `migrate`, `backup`, `user create`), layered
config (`figment`: defaults → TOML file → `RBIT_*` env), tracing and metrics setup, the
SSH server and background jobs, graceful shutdown.

---

## 3. Storage design

### 3.1 Why three stores?

| Data | Store | Reason |
|---|---|---|
| Git objects & refs | **Bare repositories on disk** | Packfiles *are* a highly optimised content-addressed KV store with mmap'ed indexes. Keeping Git's native format means `git gc`, `git fsck`, backup tools, mirroring and disaster recovery all keep working, and gitoxide reads them at native speed. Re-implementing an ODB inside another KV store would cost performance, not gain it. |
| Users, orgs, repos, memberships, issues… | **SQLite** (`sqlx`, WAL) | Relational data with constraints, joins, uniqueness and migrations. |
| Sessions, caches, derived indexes, job queue | **redb** | Embedded, ACID, zero-copy reads, MVCC. Perfect for hot, simple key/value lookups that would otherwise create write contention on SQLite. All of it is either ephemeral or rebuildable from Git + SQLite. |

### 3.2 On-disk layout

```
<data_dir>/
├── rbit.db            # SQLite (+ -wal, -shm)
├── rbit.redb          # redb key/value store
├── repositories/
│   └── <owner_id>/<repo_id>.git   # bare repositories, addressed by immutable IDs
├── attachments/
│   ├── <repo_id>/<asset_id>       # files attached to releases, named by id only
│   └── tmp/<random>.part          # uploads in progress
├── lfs/<aa>/<bb>/<oid>            # Git LFS objects, named by SHA-256, stored once
└── tmp/                           # scratch space, swept at start-up, never backed up
    ├── merges/<random>            # scratch repositories of merges, trial merges and web edits
    ├── overlays/<random>          # object overlays for cross-fork compares
    └── lfs/<random>.part          # LFS uploads in progress
```

Data directories created before the rename to rbit keep working: `rustit.db` and
`rustit.redb` are used when `rbit.db` / `rbit.redb` do not exist (`StorageConfig`).

Repositories are addressed by numeric IDs, not names, so renaming a user or repository
is a single SQL update — no filesystem moves, no path-traversal surface. A transfer moves
the directory to the new owner's folder (`rbit_git::relocate`, a same-filesystem
rename) before the owner changes in SQL, and moves it back if the database step fails.

Release files never use a user-supplied name on disk and do not depend on the owner, so
transfers move nothing. An upload streams into `tmp/<random>.part` while its SHA-256 and
size are computed and is abandoned the moment it exceeds `[releases] max_asset_bytes`;
only a complete, synced file gets a `release_assets` row, with the per-release file limit
checked atomically by the insert. A tracked task owns the row and rename into place after
the body is received, so client cancellation cannot interrupt placement (the row is removed
again if the rename fails). Incomplete uploads remove their temporary files on cancellation.
Placement and deletion hold an exclusive asset lock; downloads share read access until
their file is open, then stream independently. This keeps reused numeric file ids from
mixing old metadata with replacement bytes. Delayed repository cleanup preserves any
currently referenced assets if the repository id has been reused.
Deleting a file removes the row
first, then the bytes, so a crash leaves at worst an orphaned file, never a row without
bytes. `ReleaseService` removes `attachments/<repo_id>` when a repository is deleted and
sweeps `.part` files older than a day at start-up. Downloads are always
`application/octet-stream` with `Content-Disposition: attachment`, `nosniff` and an
`ETag` from the digest.

Git LFS objects are content-addressed: `<data_dir>/lfs/<aa>/<bb>/<oid>` holds an object
once however many repositories use it, and `lfs_repo_objects` says which may. An upload
streams into `tmp/lfs/<random>.part` while its SHA-256 and size are computed, is refused
unless both match the announced object (and the size fits `[lfs] max_object_bytes`), and is
then renamed into place (or discarded when the bytes exist) and linked; bytes are placed
before the row is written, under a process-wide mutex shared with the orphan sweep, so a
sweep never deletes bytes an upload just relied on. **Which objects a repository may use:**
those linked to it and, where the requester can read them, those of its fork network
(`ForkStore::network_readable`), which are then linked to it too (lazily, on first use), so
they survive the other repository's deletion; new forks copy their parent's links in the
background (`RepositoryForked`). The same rule decides whether an upload can be skipped: an
object stored only for an unrelated or unreadable repository must be uploaded again —
proving possession — before it is linked, so an object id alone never grants content.
Deleting a repository unlinks its objects (cascade; a trigger stamps
`lfs_objects.unlinked_at`); maintenance removes objects unlinked for 24 hours, row first
(re-checked in the `DELETE`, so a link added meanwhile wins), then bytes. Interrupted
uploads older than a day are swept at start-up.

### 3.3 SQLite schema

```
accounts           id, kind(user|organization), name UNIQUE NOCASE, display_name, bio, created_at, updated_at
user_logins        account_id → accounts, email UNIQUE NOCASE, password_hash, is_admin, suspended_at
account_profiles   account_id → accounts, website, location                       (users)
org_profiles       account_id → accounts, description, website,
                   member_permission(none|read|write) DEFAULT 'write'             (organisations)
org_members        org_id → accounts, user_id → accounts, role(owner|member)
repositories       id, owner_id → accounts, name NOCASE, description, visibility(public|private),
                   default_branch, created_at, updated_at, pushed_at, UNIQUE(owner_id, name)
stars              account_id → accounts, repo_id → repositories, created_at
access_tokens      id, account_id → accounts, name, token_digest BLOB UNIQUE, hint,
                   scope(read|write|admin), created_at, last_used_at, expires_at
instance_settings  key PRIMARY KEY, value, updated_at       (administrator overrides of config defaults)
two_factor         account_id → accounts, secret (sealed), created_at, confirmed_at, last_used_step (replay guard)
recovery_codes     account_id → accounts, code_digest, used_at
audit_events       id, created_at, category, action, actor_id, actor_name, target_name, ip_address, detail
email_tokens       digest PRIMARY KEY, account_id → accounts, purpose(reset|verify), email, created_at,
                   expires_at, used_at                       (+ user_logins.email_verified_at)
ssh_keys           id, account_id → accounts, title, public_key, algorithm, fingerprint UNIQUE,
                   created_at, last_used_at
deploy_keys        id, repo_id → repositories, title, public_key, algorithm, fingerprint UNIQUE,
                   read_only, created_at, last_used_at

-- milestone 2 wave A
collaborators      (repo_id → repositories, user_id → accounts) PK, permission(read|write|admin),
                   added_by → accounts SET NULL, created_at
teams              id, org_id → accounts, name NOCASE, description, created_at, UNIQUE(org_id, name)
team_members       (team_id → teams, user_id → accounts) PK
team_repositories  (team_id → teams, repo_id → repositories) PK, permission(read|write|admin)
user_preferences   account_id → accounts PK, theme (slug, 1–64 chars or NULL), updated_at
webhooks           id, repo_id → repositories | org_id → accounts (exactly one), url, secret,
                   events (JSON array), active, created_by → accounts SET NULL, created_at, updated_at
webhook_deliveries id, hook_id → webhooks, guid UNIQUE, event, action, payload,
                   status(pending|delivering|succeeded|failed), attempts, next_attempt_at,
                   last_attempt_at, request_headers, response_status, response_headers,
                   response_body, duration_ms, error, created_at
releases           id, repo_id → repositories, tag_name, name, notes, is_draft, is_prerelease,
                   author_id → accounts SET NULL, created_at, updated_at, published_at,
                   UNIQUE(repo_id, tag_name)
release_assets     id, release_id → releases, repo_id → repositories, name, size, content_type,
                   sha256 BLOB(32), download_count, uploader_id → accounts SET NULL, created_at,
                   UNIQUE(release_id, name)
repo_forks         repo_id → repositories PK, parent_id → repositories SET NULL, created_at
repo_mirrors       repo_id → repositories PK, url, interval_minutes, next_sync_at, last_sync_at,
                   last_error, created_at
repo_redirects     (owner_id → accounts, name NOCASE) PK, repo_id → repositories, created_at

-- milestone 2 wave B: the shared issue core (T0; only sqlite/issues.rs writes these)
issue_counters     repo_id → repositories PK, last_number            (numbers are never reused)
issues             id, repo_id → repositories, number > 0, kind(issue|pull), title, body,
                   author_id SET NULL, state(open|closed), state_reason(completed|not_planned|merged),
                   closed_at, closed_by SET NULL, locked, comment_count, created_at, updated_at,
                   UNIQUE(repo_id, number); merged only for kind pull
issue_comments     id, issue_id → issues, author_id SET NULL, body, created_at, updated_at, edited_at
issue_events       id, issue_id → issues, actor_id SET NULL, kind (TimelineEvent tag), data (JSON), created_at

-- wave B: issues (I)
labels             id, repo_id → repositories, name NOCASE (1–50), color '#rrggbb', description (≤ 100),
                   created_at, updated_at, UNIQUE(repo_id, name)
issue_labels       (issue_id → issues, label_id → labels) PK, created_at
milestones         id, repo_id → repositories, title NOCASE (1–100), description, due_on (YYYY-MM-DD),
                   state(open|closed), closed_at, created_at, updated_at, UNIQUE(repo_id, title)
issue_milestones   issue_id → issues PK, milestone_id → milestones          (at most one per thread)
issue_assignees    (issue_id → issues, user_id → accounts) PK, assigned_by SET NULL, created_at
issue_commit_refs  (issue_id → issues, commit_sha) PK, created_at        (dedupes CommitReferenced)
issue_cross_refs   (target_id → issues, source_id → issues) PK, created_at (dedupes CrossReferenced)
issue_subscriptions (issue_id → issues, user_id → accounts) PK,
                   reason(author|comment|assigned|mention|review_requested|reviewed|manual),
                   subscribed (0 = explicit unsubscribe, sticks), updated_at
notifications      id, user_id → accounts, issue_id → issues, repo_id → repositories, reason, unread,
                   last_actor_id SET NULL, updated_at, UNIQUE(user_id, issue_id)
issues_fts         FTS5 over issues(title, body), external content, unicode61 remove_diacritics 2

-- wave B: pull requests (P)
pull_requests      issue_id → issues PK, repo_id → repositories (base), head_repo_id SET NULL,
                   head_branch, base_branch, head_sha, is_draft,
                   mergeable(unknown|clean|conflict|error), mergeable_base_sha, mergeable_head_sha,
                   conflict_paths (JSON), merged_at, merged_by SET NULL,
                   merge_strategy(merge|squash|rebase, NULL = merged outside rbit),
                   merge_commit_sha, merged_base_sha, merged_head_sha, created_at, updated_at
pull_reviews       id, pull_id → pull_requests, reviewer_id SET NULL,
                   state(pending|commented|approved|changes_requested|dismissed), body, commit_sha,
                   created_at, submitted_at, dismissed_by SET NULL, dismissed_at, dismissal_message;
                   one pending review per (pull, reviewer) (partial unique index)
pull_review_comments id, review_id → pull_reviews, pull_id → pull_requests, author_id SET NULL, path,
                   side(left|right), line > 0, commit_sha, diff_hunk, in_reply_to SET NULL, body,
                   outdated, created_at, updated_at
pull_review_requests (pull_id → pull_requests, user_id → accounts) PK, requested_by SET NULL, created_at

-- wave B: branch protection (B)
branch_protections id, repo_id → repositories, pattern (1–255 bytes), deny_force_push (default 1),
                   deny_deletion (default 1), require_pull_request, required_approvals 0–10
                   (> 0 needs require_pull_request), dismiss_stale_approvals,
                   block_on_changes_requested (default 1), restrict_pushers, allow_admin_bypass,
                   created_by SET NULL, created_at, updated_at, UNIQUE(repo_id, pattern)
branch_protection_users (rule_id → branch_protections, user_id → accounts) PK
branch_protection_teams (rule_id → branch_protections, team_id → teams) PK

-- passkeys
passkeys           id, account_id → accounts, credential_id BLOB UNIQUE, public_key (COSE), algorithm
                   (-7 | -8 | -257), sign_count, transports, name, created_at, last_used_at

-- milestone 3: commit statuses (S)
commit_statuses    id, repo_id → repositories, sha (40 or 64 lower-case hex), state(pending|success|
                   failure|error), context (1–255), description (≤ 140), target_url (≤ 2048),
                   creator_id SET NULL, created_at, updated_at; index (repo_id, sha, context, id):
                   the highest id per (repo, sha, context) is current, ≤ 1000 rows per triple
                   (+ branch_protections.require_status_checks, default 0)
branch_protection_required_checks (rule_id → branch_protections, context) PK       (≤ 50 per rule)

-- milestone 3: email notifications (N)
notification_email_settings account_id → accounts PK, email(off|participating|all), updated_at
notification_emails id, account_id → accounts, issue_id → issues, dedup_key, created_at,
                   UNIQUE(account_id, issue_id, dedup_key); index on created_at (pruned with the inbox)
server_secrets     name PK, secret BLOB, created_at        (server-generated keys, sealed — §5)

-- milestone 3: Git LFS (L)
lfs_objects        oid PK (64 hex), size, created_at, unlinked_at (stamped when a link is deleted)
lfs_repo_objects   (repo_id → repositories, oid → lfs_objects) PK, created_at; WITHOUT ROWID, index on oid
lfs_locks          id, repo_id → repositories, path, owner_id → accounts, created_at, UNIQUE(repo_id, path)

-- milestone 3: external sign-in (O)
external_identities id, account_id → accounts, provider_id, subject, email, username, created_at,
                   last_used_at, UNIQUE(provider_id, subject), UNIQUE(account_id, provider_id)
```

Unless marked `SET NULL`, references cascade on delete.

**Triggers.** `ssh_keys_fingerprint_not_a_deploy_key` / `deploy_keys_fingerprint_not_a_user_key`
keep a fingerprint in only one of the two key tables. `org_members_revoke_grants` (after a
row of `org_members` is deleted, however that happens) removes the user's team memberships
in that organisation and their collaborator entries on its repositories in the same
transaction. `repo_redirects_on_create` (after insert into `repositories`) and
`repo_redirects_on_move` (after `owner_id`/`name` change, ignoring changes of letter case
only) keep `repo_redirects` right for every way a repository arrives at or leaves a
location: leaving `owner/name` records it as a redirect, arriving there removes any
redirect from it, so an existing repository always wins over an old name.
`issues_fts_insert` / `issues_fts_update` (title, body) / `issues_fts_delete` keep the
full-text index in step with `issues`, including rows removed by cascades.
`branch_protection_teams_follow_owner` (after `repositories.owner_id` changes) drops allowed
teams that do not belong to the new owner, so a transferred repository keeps its rules but
the former organisation's teams no longer let anyone push. `lfs_repo_objects_unlinked`
(after a row of `lfs_repo_objects` is deleted, cascades included) stamps the object's
`unlinked_at`, which starts its 24-hour grace period.

**Migrations** live in `crates/rbit-storage/migrations/` and are embedded in the binary:

| | Milestone 1 | | Milestone 2 wave A | | Milestone 2 wave B | | Milestone 3 |
|---|---|---|---|---|---|---|---|
| `0001` | `initial` (accounts, logins, members, repositories) | `0009` | `access` (collaborators, teams, base permission, revoke trigger) | `0017` | `issues_core` (counters, issues, comments, events) | `0033` | `commit_statuses` (statuses, required checks) |
| `0002` | `access_tokens` | `0010` | `preferences` (`user_preferences`) | `0018` | `issue_metadata` (labels, milestones, assignees, reference dedupe) | `0034` | `notification_email` (settings, outbox, `server_secrets`) |
| `0003` | `organizations` (`org_profiles`) | `0011` | `webhooks` (hooks, delivery queue) | `0019` | `notifications` (subscriptions, inbox) | `0036` | `lfs` (objects, links, locks, unlink trigger) |
| `0004` | `administration` (instance settings, suspension) | `0012` | `token_scopes` (rebuilds `access_tokens` for `admin`, keeping every id) | `0020` | `issue_search` (FTS5 index and triggers, rebuilt from existing rows) | `0037` | `external_identities` |
| `0005` | `profiles_and_stars` | `0013` | `releases` (releases, assets) | `0021` | `pull_requests` | | |
| `0006` | `security` (2FA, recovery codes, audit) | `0014` | `forks` | `0022` | `reviews` (reviews, line comments, review requests) | | |
| `0007` | `email` (email tokens, verified address) | `0015` | `mirrors` | `0024` | `branch_protection` (rules, allowed users and teams, owner trigger) | | |
| `0008` | `ssh_keys` (user and deploy keys) | `0016` | `repo_redirects` (table and triggers) | `0032` | `passkeys` (WebAuthn credentials as a second factor) | | |

`crates/rbit-storage/tests/upgrade.rs` builds a database at the milestone 1 schema
(`0001`–`0008`) with representative rows and migrates it to the current schema, asserting
that every row survives, the new defaults and triggers apply to old rows, and the tables of
wave B and milestone 3 exist and start empty. `0026`–`0031` (reserved for the API
continuation, which needed none) and `0035` (spare) are unused; `0023` and `0025` are
reserved for milestone 3's W, still in progress, and `0038`, `0039` hold M's imports and
push mirrors. `0032` was
taken by passkeys before wave B landed (sqlx applies missing lower versions in order, so
the gaps are harmless).

Secrets (passwords, sessions, tokens, recovery codes, email links) are only ever stored
hashed — except values rbit must read back: TOTP seeds, **webhook secrets** and **push
mirror secrets** (HMAC, TOTP and Git need the raw value), **`server_secrets`** (keys rbit
generates itself, 256 bits from the OS, created on first use: `notification_unsubscribe`
signs unsubscribe links and `external-sign-in` seals the sign-in flow cookie, see §5), and
the secret fields (`*password`, `*secret`, `*token`) of the administrator-saved
configuration in `instance_settings`. These are **sealed** (`rbit-storage/src/sealing.rs`):
AES-256-GCM with a random 96-bit nonce per value, stored as `enc:v1:` + base64(nonce ‖
ciphertext ‖ tag), under a 256-bit key in a file outside the database —
`<data_dir>/secret.key` (created `0600` on first start) or `[storage] secret_key_file`.
A copy of `rbit.db` alone reveals none of them. Values without the prefix are legacy
plaintext and read as is; `Database::open` seals them in one transaction at start-up
(idempotent), and refuses to start when sealed values exist but the key file is missing
or does not open them, instead of generating a key that would orphan them. Migration
`0042` only widens the push mirror secret column for the longer sealed form.

Users and organisations share one namespace (`accounts`), exactly like GitHub URLs
(`/rbit` can be either). Timestamps are Unix seconds (`INTEGER`).

### 3.4 redb tables

| Table | Key → Value | Purpose |
|---|---|---|
| `sessions_v2` | `sha256(token)` → session record (account, expiry, created, last seen, client) | Login sessions. A leaked DB never leaks usable cookies. |
| `account_sessions` | `(account id, sha256(token))` → `()` | Lists and revokes an account's sessions (sign out everywhere, password changes). |
| `language_stats` | `commit id` → language shares | Language bar. Content-addressed, so never stale (and shared by forks). |
| `tree_last_commit` | `(commit id, directory)` → `name → commit summary` | "Last commit" column in tree views. Results cut short by the time budget are not cached. |
| `insights_entry_written`, `insights_entries_by_age` | entry → write time, and the reverse | Age- and size-based pruning of the two insights caches by background maintenance. |

### 3.5 Runtime instance settings

The TOML/env configuration supplies *defaults*. Site administrators override the site
name, registration (open/closed), who may create organisations and the default repository
visibility at runtime from `/admin/settings`. Overrides are stored in `instance_settings`
and served from memory by `InstanceService` (a synchronous snapshot per request).

### 3.6 Backups

`rbit backup --output DIR` (in `rbit-server`) writes a directory that mirrors the data
directory, so restoring is a copy:

```
DIR/
├── rbit.db                      # SQLite, written with VACUUM INTO (one read transaction)
├── repositories/<owner>/<id>.git  # git clone --mirror --no-local of every repository in that snapshot
├── attachments/<repo>/<asset>     # every release file listed in that snapshot (not attachments/tmp)
├── lfs/<aa>/<bb>/<oid>            # every Git LFS object listed in that snapshot
├── secret.key                     # the sealing key, when it lives in the data directory
└── manifest.json                  # written last; its presence marks a complete backup
```

The directory is created `0700`, and the database, key and manifest `0600` (Unix). The key
is copied only from its default place: with `[storage] secret_key_file` it is meant to be
kept apart, so the manifest's `secret_key` names where it is instead, and a warning reminds
the operator to back it up separately.

The manifest records `format` (currently **4**; 3 had no `secret.key`, 2 no `lfs/`, 1 no `attachments/`), the
rbit version, the time, and counts (accounts, repositories, attachments, `lfs_objects`) plus
the ids of repositories, attachments and LFS objects that disappeared between the snapshot
and the copy (`missing_lfs_objects` for the last). Release files and LFS objects are written
once and never changed, so a plain copy is consistent; uploads in progress live in
`attachments/tmp` and `<data_dir>/tmp` and are skipped. redb is not
backed up: it holds only sessions and rebuildable caches, and neither is `<data_dir>/tmp`
(merge and overlay scratch space). Issues, pull requests, reviews and protection rules are
rows in `rbit.db`; pull request heads (`refs/pull/{n}/head`) are refs of the base
repository, so the repository mirrors carry them.

### 3.7 Issues, pull requests and merges

**One thread model.** An issue and a pull request are both a row of `issues` (`kind`
`issue` or `pull`). They share the repository's number sequence — `issue_counters` is
upserted with `RETURNING` inside the inserting transaction, so concurrent creations get
distinct numbers and a deleted issue's number is never reused — and they share comments
(`issue_comments`) and the timeline (`issue_events`: a `TimelineEvent` variant name plus
its fields as JSON; kinds a running version does not know are skipped). The timeline is
comments and events merged in `(created_at, comments before events, id)` order and paged by
a cursor (`1760000000-c42`). Labels, milestones, assignees, subscriptions and notifications
attach to the thread row, so they work on both kinds. `pull_requests` adds the branch
data, keyed by `issue_id`; `PullStore::create` inserts the thread row and the pull request
row in one transaction. Pull requests cannot be deleted; issues only by administrators.
`/issues/{n}` for a pull request redirects permanently to `/pull/{n}`, and the other way
round.

**Permissions** come from one pure table, `issue_permissions(viewer, access, issue)`
(exhaustively tested): readers may open issues and comment (writers only, on locked
threads); authors and writers edit and close/reopen; writers lock and triage (labels,
milestones, assignees, reviewers); administrators delete issues; comment authors edit
their comments, writers delete anyone's. Services recompute it for the actor on every
change; content creation and edits spend `ThrottleScope::Content` (burst 30, one per 10 s).

**References.** `#12`, `owner/repo#12` and `@name` are found only in Markdown text (never
in code spans, code blocks or link text). Rendering links `#12` only when it exists in this
repository, always links `owner/repo#12` (no lookup, so nothing leaks) and links `@name` when
the account exists. A thread or comment mentioning another records `CrossReferenced` on the
target once, shown only to viewers who can read the source. Commits pushed to any branch
record `CommitReferenced` (once per issue and commit, at most `[issues] scan_commit_limit`
commits per ref update, mirrors skipped); closing keywords (`fixes #12`, …) close the issue
only for commits on the default branch, and a pull request merged into the default branch
closes the issues its body names ("closed via #2").

**Pull request heads.** The head commit is kept in the base repository as
`refs/pull/{n}/head` (`update-ref` for same-repository branches, `git fetch` of the one
branch from a fork, verifying the fetched id), so a pull request keeps its commits when the
fork is deleted. Clients can never push to `refs/pull/*` (§4.4). A background subscriber on
`RepositoryPushed` refreshes the ref when the head branch moves (`HeadPushed`, `forced` when
the old head is not an ancestor), records `HeadRefDeleted`, and marks pull requests whose
head became reachable from their base as **merged outside rbit** (strategy `NULL`).

**Cross-fork compare.** Repositories whose `repo_forks` chains (≤ 10 levels) reach the same
root form a network; pull requests may go between any readable members. Compares and checks
across repositories read through an **overlay**: a throw-away bare repository under
`<data_dir>/tmp/overlays/<random>` whose `objects/info/alternates` lists both object
directories, so nothing is written to either repository before a pull request exists. It is
removed on drop (also while unwinding) and swept at start-up.

**Mergeability.** A bounded worker (`[pulls] mergeability_concurrency`) runs trial merges of
open, non-draft pull requests whose state is `unknown` and stores `clean`, `conflict` (≤ 100
paths) or `error` with the base and head ids it used; it is woken by pushes and rescans
every minute, so nothing is lost across restarts.

**Merging** (`rbit_git::merge`, behind `PullService`'s merge box):
1. A scratch repository is created under `<data_dir>/tmp/merges/<random>` whose only object
   source is the base repository (which already has the head as `refs/pull/{n}/head`).
2. The merge is computed **without a working tree**: `git merge-tree --write-tree` merges
   the trees and writes the result's objects into the scratch repository, `git
   commit-tree` makes the commits — a merge commit (base tip and head as parents), a squash
   (one parent, authored by the pull request's author) or a rebase (each non-merge head
   commit replayed with `merge-tree --merge-base=<its parent>`, authors kept, the merger as
   committer, empty commits dropped). No file is checked out, so `.gitattributes` filters,
   line-ending conversion, hooks and editors never run. A conflict ends the merge with the
   conflicting paths.
3. The base repository fetches the result's objects from the scratch repository and moves
   `refs/heads/<base>` with `git update-ref <new> <expected tip>` — an atomic
   compare-and-swap (the lease). If the branch moved meanwhile the merge is retried once,
   then reported ("the base branch changed; try again"). Nothing else in the base
   repository changes.

Review conversations keep resolution metadata on their root line comment (migration
`0041`). Writers and the root author may resolve or reopen submitted conversations via
`POST /{owner}/{repo}/pull/{n}/comments/{id}/resolution`; pending reviews answer 404.
The store changes the state and records `review_thread_resolved` or
`review_thread_reopened` in one transaction, with repeated state requests doing nothing.
Both PR views group replies with their root and collapse resolved conversations.
Resolution does not affect merge requirements.

The ref update does not go through `receive-pack`, so no hooks run and the push gate is not
involved: protection is applied before merging by `ProtectionService::check_merge`
(approvals from writers' latest reviews, changes requested, allowed pushers, required status
checks (milestone 3), admin bypass — audited as `BranchProtectionBypassed`). The update is wrapped in `PushService::begin` /
`complete` with the merger as pusher, so `RepositoryPushed` (webhooks, commit references)
fires as for any push. Merges are serialised per base repository and bounded instance-wide
(`max_concurrent_merges`), limited by `merge_timeout_seconds` (children are killed on drop),
and the scratch directory is removed on every exit path and swept at start-up. Every `git`
runs hardened: the transports' scrubbed environment without system or global configuration,
no prompts, `LC_ALL=C`, no replace refs, `-c core.hooksPath=<null device>`,
`core.autocrlf=false`, no GPG signing, rerere, auto-gc or fsmonitor, revisions only as
validated full ids or `refs/heads/<validated name>` after `--end-of-options`. rbit-made
commits use `<username>@<[pulls] noreply_domain>` (default `noreply.<host of
server.public_url>`), never a real address, and dates from the service clock. Deleting the
head branch afterwards needs `ProtectionService::may_delete_branch` and write access to the
head repository; it is an `update-ref -d` with the expected id, reported through
`PushService` too.

**Checks (milestone 3).** The merge box reads the head's latest statuses in the base
repository (where fork heads live as `refs/pull/{n}/head`). Rules with
`require_status_checks` contribute their contexts (`MergeRules.required_contexts`: the
union of the matching rules, sorted, deduplicated) and `Requirement::RequiredChecks` is met
only when each required context's latest status on the head is `success` — a protection
requirement, so an administrator whose rules allow bypass may skip it. Statuses nobody
requires produce an informational line ("All checks have passed", "Some checks were not
successful") that never blocks, as on GitHub; status checks never affect the push gate.
When a status arrives, `PullService::handle_status` marks stale or unknown mergeability of
open pull requests with that head as unknown and wakes the worker; the checks themselves are
always read live.

### 3.8 Editing files in the browser

Writers edit, create, rename, delete and upload files at
`/{owner}/{repo}/{new,edit,delete,upload}/{branch}/{path…}` (pages answer 404 to everyone
else, to pull mirrors and for anything that is not a branch). Like merges, a web edit never
checks anything out:

1. **Checks before any object is written** (`EditService::check`): write access, not a pull
   mirror (`EditService::read_only_reason` is the one place archived repositories will join),
   the branch exists, for a direct commit the form's commit is still the tip, for a new
   branch a valid unused name (`{user}-patch-{n}` by default), then the push gate —
   `ProtectionService::check_push` with a `PushActor` for the user and one `RefUpdate`,
   exactly as a push would be decided, bypasses audited. A refusal of the branch itself
   makes the form preselect a new branch (and the form shows that up front through
   `branch_update_refusal`, which does not audit); after committing to a new branch the user
   lands on the new pull request page.
2. **Workspace** (`rbit_git::edit::EditWorkspace`): a scratch bare repository under
   `<data_dir>/tmp/merges/<random>` whose only object source is the repository (the merge's
   overlay), holding a repository lease so a transfer cannot move the repository meanwhile.
   Contents become blobs with `git hash-object -w --no-filters --stdin`, streamed and
   bounded (uploads never sit in memory; no attributes or line-ending filters apply).
3. **Tree checks** against the parent's tree: new files and move targets must be free (a
   file the same edit removes frees its name), edited, moved and deleted paths must be files
   (not submodules or directories), no path runs through an existing file or another written
   path, none is written twice. Edits and moves keep the mode (executable, symlink).
4. **Tree and commit**: `read-tree <parent>`, one `update-index -z --add --remove
   --index-info` on a scratch index (paths only ever on stdin), `write-tree` (a tree equal
   to the parent's is refused: "there are no changes to commit"), then `commit-tree` with
   the parent as the only parent. Author = committer = the user with their **noreply
   address** (`<username>@<[pulls] noreply_domain>`), exactly as for merges.
5. **Hand-over** (shared with merges): the repository fetches the commit from the workspace,
   then `update-ref refs/heads/<branch> <new> <commit the form was opened at>`. A branch that
   moved is a 409 page (nothing committed, the text kept, the form rebased on the new tip),
   never a rebase; a new branch expects the zero id, so an existing branch is never
   overwritten.
6. **Downstream**: the git work is wrapped in `PushService::begin`/`complete`, so
   `RepositoryPushed` drives webhooks, commit references (`fixes #n`), cache warm-up and
   mergeability exactly as for a push.

Browsers submit textareas with CRLF: submitted text is normalised to LF and written in the
file's existing style (CRLF when at least as many lines end in CRLF as in LF), LF for new
files; nothing else changes. Uploads are stored byte for byte. The editor is offered only
for text the browser would display — not for binaries, files over `[repository]
max_render_bytes` or **Git LFS pointer files**, which can still be deleted or replaced by an
upload. Limits: editor forms accept `6 × max_render_bytes + 256 KiB`; uploads stream
outside the page router (like release files) under `[editor] max_upload_bytes` per file,
`max_upload_files` per commit and `upload_timeout_seconds`; committing is bounded by
`[pulls] merge_timeout_seconds`. Commit titles default to `Create <path>`, `Update <path>`,
`Rename <a> to <b>`, `Delete <path>` and `Add files via upload`.

---

## 4. Request lifecycle

```
TCP → hyper → tower layers:
  TraceLayer → metrics::track (route template, status, latency)
→ Router
  ├── /assets/*, /sw.js, /manifest.webmanifest   immutable, content-hashed, embedded
  ├── /healthz, /readyz, /metrics                operations (metrics optional, token-gated)
  ├── /{owner}/{repo}.git/…                       smart HTTP (Basic auth or token; throttled; streaming)
  ├── /{owner}/{repo}[.git]/info/lfs/…           Git LFS API (Basic auth, token or signed lfs1 header; streaming, §4.7)
  ├── /api/v1/…                                   REST API (own layers, below; no cookies, no CSRF)
  ├── /{owner}/{repo}/archive/{rev}.zip|.tar.gz   archives (session + security headers; streamed, no timeout)
  ├── /{owner}/{repo}/releases/{id}/assets        uploads (session + CSRF + headers; upload_timeout_seconds,
  │   /{owner}/{repo}/releases/download/…           body limit max_asset_bytes + 1 MiB) and downloads
  ├── /{owner}/{repo}/upload/{branch}/{dir…}       browser uploads (session + CSRF + headers; [editor] limits, §3.8)
  └── HTML routes
        Compression → Timeout (30 s) → body limit (1 MiB) → SecurityHeaders → CsrfOriginCheck
        → rate_limit (unsafe requests, per client address) → session → error pages
        → extractors → handler → service → view model → Askama

SSH (separate listener, rbit-ssh): public-key auth → exec git-upload-pack (interactive)
  or git-receive-pack (advertisement → push gate → stateless receive-pack, §4.4)
  or git-lfs-authenticate (prints a signed LFS header, §4.7)
```

**Account names redirect.** Migration `0042` adds `account_redirects` keyed by the old
case-insensitive name and referencing the stable account ID. Rename triggers record old
names; account creation or renaming into a name removes its former redirect permanently.
Every alias resolves directly to the current account, so repeated renames and renaming
back cannot loop. The same invariants apply to organisation accounts. Account settings
allow a signed-in user to change their own username with a current-password check that
is atomic against password replacement; attempts share a five-request budget per account,
refilling once every ten minutes. Names still use the reserved-path validation. Account
IDs, repository disk paths, permissions, sessions, keys and tokens remain unchanged.
Sign-in and mentions use current names only. An old profile URL redirects with its query;
repository lookups compose account aliases with repository aliases and enforce visibility
before exposing a destination. Git HTTP and SSH serve the permitted repository directly.
No names from before the migration are inferred or backfilled.

**Old names redirect.** When a page answers 404 and its first two path segments name a
repository's *old* location (`repo_redirects`) that the viewer may read, the error-page
layer answers `301` (`308` for anything but `GET`/`HEAD`, so forms keep their method) to the
same path under the current name; redirects are not cacheable, because a new repository
may take the old name at any time and then wins. Git over HTTP and SSH serve the
repository directly under an old name (`RepositoryService::open_following_renames`).

**Pull mirrors refuse pushes** in both transports (`403` over HTTP, an error message over
SSH), checked after access so only people who could otherwise push learn it is a mirror.
Pushes that get past these checks then pass the push gate (§4.4).

**HTMX conventions**
- `<body hx-boost="true">` turns every link and form into an AJAX swap with history support,
  giving SPA-like navigation with server-rendered HTML.
- Fragments (e.g. branch switcher list, file-tree expansion) are returned when the
  `HX-Request` header is present; full pages otherwise. The same handler serves both.
- `htmx.config.allowEval = false` and no inline scripts, so CSP can forbid `unsafe-inline`
  and `unsafe-eval` for scripts.

### 4.1 Page routes added in milestone 2 wave A

| Area | Routes |
|---|---|
| Collaborators | `/{owner}/{repo}/settings/collaborators` (list, add), `…/{user}/permission`, `…/{user}/remove`; a collaborator can leave from the repository sidebar |
| Teams | `/{org}/teams`, `/{org}/teams/new`, `/{org}/teams/{team}` (members, repositories), `/{org}/teams/{team}/settings`, `…/settings/delete`; base permission at `/{org}/settings/permission` |
| Transfer | `/{owner}/{repo}/settings/transfer` (danger zone; confirm by typing `owner/name`) |
| Webhooks | `/{owner}/{repo}/settings/hooks` and `/{org}/settings/hooks`: list, `new`, `{id}` (edit), `{id}/ping`, `{id}/delete`, `{id}/deliveries/{delivery}` and `…/redeliver` |
| Releases | `/{owner}/{repo}/releases`, `…/releases/tag/{tag}`, `…/releases/new`, `…/releases/{id}/edit`, `…/{id}/delete`, `…/{id}/assets` (upload), `…/{id}/assets/{asset}/delete`, `…/releases/download/{tag}/{name}` |
| Code | `/{owner}/{repo}/blame/{rev}/{path}`, `/{owner}/{repo}/compare` and `/compare/{base}...{head}`, `/{owner}/{repo}/archive/{rev}.zip` / `.tar.gz` |
| Forks | `/{owner}/{repo}/fork` (form, create), `/{owner}/{repo}/forks` |
| Mirrors | `/new/mirror`, `/{owner}/{repo}/settings/mirror` (interval), `…/sync`, `…/remove` |
| Appearance | `/themes` (gallery, choose), `/settings/appearance` |

Repository settings sub-pages answer 404 to everyone but repository administrators.
New second-level paths under `/{owner}/` are reserved repository names.

### 4.2 REST API (`/api/v1`)

`rbit-web/src/routes/api/` is merged outside the page router with its own layers:
1 MiB bodies, a 30 s timeout, gzip, `X-Content-Type-Options: nosniff`, `Cache-Control:
no-store`. Every handler goes through the same services as the pages.

- **Authentication** — `Authorization: Bearer rbit_…` (or GitHub's `token rbit_…`; tokens
  issued before 0.1 as `rit_…` are accepted too). Cookies
  are never read, so the API needs no CSRF protection and a browser session cannot be
  ridden. Anonymous callers read public data. Unknown or expired tokens and tokens of
  suspended accounts are refused with 401 and `WWW-Authenticate: Bearer`. Presented tokens
  are throttled per client address (`ApiAuth`, failures cost extra); requests per account,
  or per address when anonymous (`ApiRequest`); 429 with `Retry-After`.
- **Scopes** — `read` allows `GET` only; `write` also mutations that need write access (and
  Git push); `admin` also repository settings, webhooks and deletion. A token never exceeds
  its owner's access (`TokenScope::limit`).
- **Errors** — `{"message": …}`, plus `"errors": [{"field", "message"}]` for 422. 401, 403
  (only for things the caller may see), **404 for anything the caller may not know exists**,
  409, 413, 422, 429. Unknown `/api/…` paths answer a JSON 404 from an explicit catch-all;
  with `[api] enabled = false` everything under `/api` answers 404.
- **Representation and pagination** — `snake_case`, numeric `id`s, RFC 3339 UTC
  timestamps, absolute `html_url`/`clone_url` from `server.public_url`, explicit `null`s.
  Lists take `?page=` (1-based) and `?per_page=` (`[api] default_page_size` /
  `max_page_size`) and answer with `Link` (`first`, `prev`, `next`, `last`) and
  `X-Total-Count`.
- **Endpoints (wave A)** — `GET /version`, `GET /openapi.json`; `GET /user`,
  `GET|POST /user/repos`, `GET /user/starred`, `PUT|DELETE /user/starred/{owner}/{repo}`;
  `GET /users/{name}`, `GET /users/{name}/repos`; `GET /orgs/{org}`, `GET /orgs/{org}/members`,
  `GET|POST /orgs/{org}/repos`; `GET|PATCH|DELETE /repos/{owner}/{repo}`,
  `…/branches`, `…/branches/{branch}` (may contain `/`), `…/tags`, `…/commits?sha=&page=`,
  `…/commits/{ref}`; webhooks under `/repos/{owner}/{repo}/hooks` and `/orgs/{org}/hooks`:
  `GET|POST`, `GET|PATCH|DELETE /{id}`, `POST /{id}/pings`, `GET /{id}/deliveries`,
  `GET /{id}/deliveries/{delivery}`, `POST /{id}/deliveries/{delivery}/attempts` (redeliver).
- **Endpoints (wave B)**, all under `/repos/{owner}/{repo}` unless absolute; `read` scope for
  `GET`, `write` for mutations (also readers commenting), `admin` for deletions and
  protection rules:
  - *Issues* — `GET|POST /issues` (GitHub's filters `state`, `labels`, `milestone`,
    `assignee`, `creator`, `sort`, `direction`, `since`; pull requests included with a
    `pull_request` key, as on GitHub), `GET|PATCH|DELETE /issues/{number}`,
    `PUT|DELETE /issues/{number}/lock`, `GET|POST /issues/{number}/comments`,
    `GET|PATCH|DELETE /issues/comments/{id}`, `GET /issues/{number}/timeline`,
    `GET|POST|PUT|DELETE /issues/{number}/labels`, `DELETE /issues/{number}/labels/{name}`,
    `POST|DELETE /issues/{number}/assignees`, `GET /assignees`, `GET|POST /labels`,
    `GET|PATCH|DELETE /labels/{name}`, `GET|POST /milestones`, `GET|PATCH|DELETE
    /milestones/{id}` (by id — a documented deviation from GitHub's per-repository
    milestone numbers).
  - *Notifications* — `GET|PUT /notifications` (list; mark all read),
    `PATCH /notifications/threads/{id}`, `DELETE /notifications/threads/{id}/subscription`.
  - *Pull requests* — `GET|POST /pulls` (`state`, `head`, `base`, `sort`, `direction`),
    `GET|PATCH /pulls/{number}`, `GET /pulls/{number}/commits`, `GET /pulls/{number}/files`,
    `GET|PUT /pulls/{number}/merge` (GET 204 merged / 404 not; PUT `merge_method`,
    `commit_title`, `commit_message`, `sha` → `{sha, merged, message}`, 405 when blocked or
    not mergeable, 409 when `sha` is not the head), `GET|POST /pulls/{number}/reviews`,
    `GET /pulls/{number}/reviews/{id}`, `PUT /pulls/{number}/reviews/{id}/dismissals`,
    `GET|POST /pulls/{number}/comments`, `GET|POST|DELETE /pulls/{number}/requested_reviewers`.
  - *Branch protection* — `GET|POST /branch_protections`, `GET|PATCH|DELETE
    /branch_protections/{id}` (repository administrators with an `admin` token; 404 when the
    repository is hidden, 403 otherwise). Since milestone 3 the rule carries
    `require_status_checks` and `required_status_checks`.
- **Endpoints (milestone 3)** — `read` for `GET`, `write` for statuses, release and fork
  changes (deleting a release or a release file included, as for labels), `admin` for
  collaborators (reads included: the data is administrators-only) and team management:
  - *Commit statuses* — `POST /repos/{owner}/{repo}/statuses/{sha}` (write access; 201 with
    `Location`; 422 naming `state`, `target_url`, `description`, `context` or `sha`),
    `GET …/commits/{ref}/statuses` (newest first) and `GET …/commits/{ref}/status` (combined
    state, latest per context). A reference may contain `/`, so both are served below the
    `…/commits/{*reference}` wildcard by `statuses::commit_or_statuses`, which hands every
    other reference to the commit endpoint; the table lists them as `route_in_wildcard`
    entries (documented, not registered). A branch whose name ends in `/status` or
    `/statuses` can be addressed there only by commit id.
  - *Collaborators* — `GET /repos/{owner}/{repo}/collaborators` (direct collaborators),
    `GET|PUT|DELETE …/collaborators/{username}` (`PUT` `{permission}` → 201 added / 204
    changed; `pull`/`push`/`admin`, `triage`/`maintain` are 422; a collaborator may remove
    themselves), `GET …/collaborators/{username}/permission` (the effective access from every
    grant, `none` included).
  - *Teams* — `GET|POST /orgs/{org}/teams`, `GET|PATCH|DELETE /orgs/{org}/teams/{team_slug}`
    (slug = name), `GET …/members`, `GET|PUT|DELETE …/memberships/{username}` (idempotent
    `PUT`), `GET …/repos`, `GET|PUT|DELETE …/repos/{owner}/{repo}` (`{permission}`; only the
    organisation's own repositories — another owner's `owner/name` is refused). Visible to
    organisation members and site administrators only.
  - *Releases* — `GET|POST …/releases`, `GET …/releases/latest`, `GET …/releases/tags/{tag}`,
    `GET|PATCH|DELETE …/releases/{release_id}`, `GET|POST …/releases/{release_id}/assets`
    (`POST ?name=` with the file as the raw body, GitHub's `upload_url`),
    `GET|DELETE …/releases/assets/{asset_id}` (`Accept: application/octet-stream` streams
    the file). Drafts exist only for writers; a release whose tag does not exist creates it at
    `target_commitish`, reported like a push.
  - *Forks* — `GET|POST …/forks` (`{organization, name}` → 202 with the repository and its
    `parent`).
- **Streaming routes** — a table entry marked `.streaming()` (the release file upload and
  download) is served without the 1 MiB body limit, the 30 s timeout and compression:
  `api::routes()` merges those routes after the shared layers. Their handlers bound
  themselves: `[releases] max_asset_bytes` (413, at once from `Content-Length`, otherwise
  as the bytes arrive) and `upload_timeout_seconds` (408).
- **OpenAPI** — `openapi.json` is a hand-written OpenAPI 3.1 document embedded in the
  binary, merged at start-up with one fragment per area (`api/openapi/{issues,
  notifications,pulls,protection,collaborators,teams,releases,forks,statuses}.json`). The router is built from one route table
  (`api::table()`), and a test asserts that the documented paths and methods equal the
  registered ones and that fragments never define a path or schema twice.

### 4.3 Webhooks

Repository hooks fire for that repository; organisation hooks for every repository of the
organisation (and are the only ones to see deletions). Repository administrators and
organisation owners manage them in the UI and the API; at most 20 per repository or
organisation; changes are audited.

- **Fields** — `url` (`http`/`https`, ≤ 2048 bytes, no credentials in it; internal address
  literals and `localhost` names are rejected when saved), optional `secret` (write-only), `events` (`push`,
  `repository`, `release`, `fork`, since wave B `issues`, `issue_comment`,
  `pull_request`, `pull_request_review`, and since milestone 3 `member` and `status`; a
  `ping` is sent on creation), `active`. Bodies are
  always `application/json`.
- **Payloads** follow GitHub's shapes so existing receivers work: `push` (one delivery per
  changed ref: `ref`, `before`/`after` with 40 zeros for creation and deletion, `created`,
  `deleted`, up to 20 `commits` plus `total_commits`, `head_commit`, `pusher`, `compare`),
  `repository` (`created`, `deleted`, `transferred`), `release` (`published`), `fork`,
  `ping` (`zen`, `hook_id`, `hook`); all with `sender`, and with `repository` (and
  `organization` for organisation-owned repositories) where one applies. Wave B:
  - `issues` — `opened`, `edited` (`changes.title.from`, `changes.body.from`), `closed`,
    `reopened`, `labeled`/`unlabeled` (`label`), `assigned`/`unassigned` (`assignee`),
    `milestoned`/`demilestoned` (`milestone`), `locked`, `unlocked`, `deleted`, with the
    `issue` object (labels, assignees, milestone, comment count, URLs);
  - `issue_comment` — `created`, `edited` (`changes.body.from`), `deleted`, with `issue` and
    `comment`; comments on pull requests are sent here too, their `issue` carrying a
    `pull_request` key;
  - `pull_request` — `opened`, `edited` (`changes.title|body|base.ref.from`), `closed`
    (`pull_request.merged` true or false), `reopened`, `synchronize` (`before`, `after`),
    `labeled`, `unlabeled`, `assigned`, `unassigned`, `milestoned`, `demilestoned`,
    `review_requested`, `review_request_removed` (`requested_reviewer`),
    `ready_for_review`, `converted_to_draft`, `locked`, `unlocked`, with the
    `pull_request` object (`head`/`base` with `label`, `ref`, `sha`, `repo`; `draft`,
    `merged`, `mergeable`, `mergeable_state`, `merged_by`, `merge_commit_sha`, counts);
  - `pull_request_review` — `submitted`, `edited`, `dismissed`, with `review` (`state` in
    lower case, `commit_id`, `submitted_at`, `html_url`).

  Milestone 3:
  - `member` — `added`, `edited` (`changes.permission.from`/`to` plus GitHub's older
    `old_permission.from`), `removed`, with `member`; collaborators only (team grants and
    memberships are not reported);
  - `status` — no `action`; `id`, `sha`, `name`, `state`, `context`, `description`,
    `target_url`, `commit` (message, author, committer, parents), `branches` whose tip it is
    (with `protected`), timestamps;
  - the `release` object (also in the API) now carries `url`, `assets_url`, `upload_url`
    (`…/assets{?name,label}`) and each asset's `url`.

  No payload contains an account's email address; like `push`, `status` carries the commit
  author's name and address from the commit itself (`commit.commit.author`).
  `WebhookService::handle_event` routes `DomainEvent::Issue { kind: issue }` and
  `IssueComment` to `issues::hooks::payload`, `Issue { kind: pull }` and
  `PullRequestReview` to `pulls::hooks::payload`, and `CommitStatus` to
  `statuses::hooks::payload`.
- **Headers** — `User-Agent: rbit-Hookshot/<version>`, `X-Rbit-Event`,
  `X-Rbit-Delivery` (random id), `X-Rbit-Hook-Id` and GitHub's `X-GitHub-Event`,
  `X-GitHub-Delivery`, `X-GitHub-Hook-ID`. With a secret: `X-Rbit-Signature-256` and
  `X-Hub-Signature-256`, both `sha256=<lower-case hex HMAC-SHA256 of the body>`.
- **Delivery queue** — persisted in `webhook_deliveries`, so a restart loses nothing. The
  `DomainEvent` subscriber only inserts `pending` rows; a worker claims due rows
  (`delivering`; rows claimed when the process stopped return to the queue at start-up),
  runs at most `[webhooks] concurrency` requests at once, applies `timeout_seconds` to the
  whole exchange, never follows redirects, and treats 2xx as success. Anything else is
  retried with exponential backoff and jitter (≈ 10 s, 1 min, 10 min, 1 h, 6 h) until
  `max_attempts`, then `failed`; a refused target fails at once. Every attempt records the
  request headers, response status, headers (≤ 64) and the first 64 KiB of the body,
  duration and error. A daily task drops finished deliveries older than
  `delivery_retention_days`. With `[webhooks] enabled = false` hooks can be configured but
  nothing is queued or sent. Each attempt increments
  `rbit_webhook_attempts_total{outcome}` (`succeeded`, `retrying`, `failed`).
- **Which hooks get an event** — as on GitHub, only hooks that existed when it happened,
  although the subscriber may handle it later. `WebhookService::create` reserves the new
  hook's id from the `EventBus` (an atomic counter, seeded at start-up with the largest
  stored id), and `publish` stamps each event with the newest id reserved so far; the
  subscriber skips hooks with a larger id. One counter orders both, so this is exact
  without timestamps (whose seconds could not order a push and a hook created just after
  it). Deleting or deactivating a hook takes effect at once: an event the subscriber has
  not yet queued is not delivered to it, even if it happened while the hook was active
  (switching a hook off is how an administrator stops traffic, and the gap is normally
  milliseconds). `active` is judged when queueing, so a hook reactivated in that gap does
  get the event; deliveries already queued for a deactivated hook are still sent.
- **SSRF** — unless `allow_private_addresses`, every address a target resolves to must pass
  `rbit_core::network::is_internal_address`: loopback, private (RFC 1918), shared/CGNAT
  (`100.64.0.0/10`), link-local, unique-local (`fc00::/7`), multicast, unspecified,
  broadcast, reserved, documentation and benchmarking ranges are refused, and IPv6 forms
  embedding an IPv4 address (mapped, compatible, NAT64, 6to4) are judged by that address.
  The check runs inside the HTTP client's DNS resolver at connection time, so DNS rebinding
  cannot slip past it; address literals are checked using the HTTP client's parsed URL,
  including numeric IPv4 spellings. The locally translated NAT64 range is refused.
  Proxy environment variables are ignored. Pull mirrors, imports and push mirrors resolve
  and validate addresses before invoking Git, pin its connections with `http.curloptResolve`,
  disable proxies and refuse redirects, closing the second-DNS-lookup gap.

### 4.4 The push gate (branch protection)

On Windows and macOS, branch protection also matches case aliases because repositories
commonly use case-insensitive filesystems. Noncanonical `refs/heads/` namespace spellings
are refused before Git, whose non-fast-forward check requires the canonical prefix;
case aliases of the managed `refs/pull/` namespace are reserved too.

`git receive-pack` runs without hooks and the tests boot the server in-process, so a
`pre-receive` executable is not an option; the gate is in-process, in **both** receive
paths (`rbit-web/src/routes/git_http.rs` and `rbit-ssh/src/session.rs`), after the access,
mirror and old-name checks:

1. **Intercept.** `rbit_transport::receive::intercept` reads the push's command list (the
   pkt-lines `old new ref[\0capabilities]`, optional `shallow` lines, the flush) before git
   sees it — bounded to 10 000 commands and 4 MiB, malformed input is a 400 / SSH error —
   and returns the commands plus a `Replay` that yields the saved bytes and then the rest of
   the stream byte for byte (push options and the pack are never buffered).
2. **Decide.** `ProtectionService::check_push` loads the repository's rules and decides,
   purely (`rbit_core::protection`), everything that needs no objects: updates of
   `refs/pull/*` are always refused ("refs/pull/ is managed by rbit" — the namespace is
   reserved for pull request heads); deletions under `deny_deletion`; pushers not allowed by
   `restrict_pushers` (deploy keys never are, also when creating the branch); updates of an
   existing branch under `require_pull_request`. Every matching rule applies and the
   strictest value wins; an administrator with `allow_admin_bypass` passes the branch rules
   (never the `refs/pull/` rule) and each bypass that changed the outcome is audited as
   `BranchProtectionBypassed`. Refusal texts never say who is allowed.
3. **Refuse all or nothing**, like a declining `pre-receive` hook: if any ref is refused,
   the rest of the request is drained, git is never started, and rbit answers in git's own
   format — `unpack ok`, `ng <ref> <reason>` for each refused ref and `ng <ref> push
   declined: another ref in this push was refused` for the others — honouring
   `report-status` / `report-status-v2` and `side-band-64k` / `side-band` (with a band-2
   `rbit: …` line per refusal). Clients that asked for no report get HTTP 403 with a text
   body, or over SSH a stderr message and exit status 1.
4. **Force pushes** need the new objects, which arrive after the commands. When some
   update of an existing branch is protected against force pushes for this pusher, git
   runs with `-c receive.denyNonFastForwards=true`, which git applies after unpacking.
   Documented side effect: in that one push, non-fast-forward updates of *other* refs are
   refused too ("non-fast-forward"); pushing them separately works.
5. Otherwise git runs on the replayed input and `PushService` reports the push as before.

**SSH receives in two steps.** The SSH server writes the advertisement from `git
receive-pack --stateless-rpc --advertise-refs` itself, reads the command list from the
channel, decides, and only then starts `git receive-pack --stateless-rpc` with the replayed
input — the exchange smart HTTP performs, so it is equivalent for clients. An empty command
list (`Everything up-to-date`) ends with exit status 0 without starting git; a
deletion-only push never sends EOF, so the reader stops after the commands.

Merges made by rbit move the base branch with `update-ref`, not `receive-pack`; they are
checked by `check_merge` instead (§3.7).

### 4.5 Page routes added in milestone 2 wave B

| Area | Routes |
|---|---|
| Timeline (shared) | `POST /{owner}/{repo}/issues/{n}/comments` (optional `state=close_completed\|close_not_planned\|reopen`: "Close with comment"), `…/comments/{id}` (edit), `…/comments/{id}/delete`, `…/title`, `…/body`, `…/state`, `…/lock`; pull request pages post to the same URLs. Anchors `#issuecomment-{id}`, `#event-{id}`; edit forms open in place with `?edit=…` |
| Issues | `/{owner}/{repo}/issues` (list with the search box: `is:`, `author:`, `assignee:`/`@me`, `label:`/`-label:`, `milestone:`, `no:`, `sort:`, free text through FTS5; 25 per page), `/issues/new`, `/issues/{n}`, `…/{n}/labels`, `…/assignees`, `…/milestone`, `…/subscription`, `…/delete` |
| Labels | `/{owner}/{repo}/labels` (list, create), `…/labels/defaults`, `…/labels/{id}` (edit), `…/labels/{id}/delete` |
| Milestones | `/{owner}/{repo}/milestones`, `…/new`, `…/{id}` (show, update), `…/{id}/edit`, `…/{id}/state`, `…/{id}/delete` |
| Notifications | `/notifications` (Unread / All by repository), `/notifications/read-all`, `/notifications/{id}/read`, `/notifications/{id}/unsubscribe`; the header bell shows a dot when something is unread |
| Pull requests | `/{owner}/{repo}/pulls` (list, create), `/pulls/new?base=&head=`, `/pull/{n}` (conversation), `/pull/{n}/commits`, `/pull/{n}/files` (line comments, anchors `#diff-{sha256(path)[..8]}-{L\|R}{line}`), `…/merge`, `…/draft`, `…/base`, `…/comments`, `…/reviews`, `…/reviews/discard`, `…/reviews/{id}/dismiss`, `…/reviewers`, `…/reviewers/remove`, `…/branch/delete` |
| Compare | `/{owner}/{repo}/compare/{base}...{head}` accepts `head` as `branch`, `owner:branch` or `owner/name:branch` within the fork network, with base and head repository pickers and a "Create pull request" button |
| Branch protection | `/{owner}/{repo}/settings/branches` (list, add), `…/new`, `…/{id}` (edit, with the existing branches it matches), `…/{id}/delete`; a "Protected" badge on `/{owner}/{repo}/branches` |

The repository tabs gain **Issues** and **Pull requests** with open counts (one indexed
query). Every new page opens the repository through `RepositoryService::open`, so hidden
repositories, unknown numbers and malformed ids are 404.

### 4.6 Page routes added in milestone 3

| Area | Routes |
|---|---|
| Editing files | `/{owner}/{repo}/new/{branch}/{dir…}`, `…/edit/{branch}/{path…}`, `…/delete/{branch}/{path…}` (page router, body limit `6 × max_render_bytes + 256 KiB`), `…/upload/{branch}/{dir…}` (outside the page router, streaming); pencil and trash buttons on the blob page, "Add file" and "Upload files" on tree pages, for writers on branches of non-mirrors |
| Commit statuses | the merge box lists the head's checks (state, context, description, "Required", "Expected" for required contexts without a report, "Details" with `rel="nofollow noopener noreferrer"`); `/{owner}/{repo}/commit/{revision}` shows a combined mark and the checks (`#checks`); commit lists show the mark per commit; the branch protection form gains "Require status checks to pass" and the required contexts |
| Email notifications | `GET\|POST /settings/notifications` (off / participating / all, the address and why mail may not arrive), `GET\|POST /email/unsubscribe/{token}` (confirmation page; the POST turns emails off) |
| External sign-in | `GET /login/{id}` (start), `GET /login/{id}/callback`, `GET\|POST /login/{id}/register` (choose a username), `GET /settings/security/linked/{id}?token=…` (link start, token bound to the session), `POST /settings/security/linked/{id}/unlink`; buttons on the sign-in page, "Sign-in methods" under Settings → Security |
| Git LFS | the blob page shows the object a pointer names (with a "Stored with Git LFS" badge, LFS images inline) and `raw` streams it |

Starting a sign-in or a link is a link, not a form: CSP `form-action 'self'` also applies to
the redirect a form post leads to, which would block the way to the provider. Everything
under `/login/` is excluded from hover preloading.

### 4.7 Git LFS

`routes/lfs.rs` (and `lfs/locks.rs`) serve the batch API, basic transfers and the locks API
under both `/{owner}/{repo}.git/info/lfs/…` and `/{owner}/{repo}/info/lfs/…` (old names from
renames and transfers work as for smart HTTP), merged beside smart HTTP without the page
layers: no sessions, CSRF check, compression or timeout; JSON bodies are capped at 1 MiB,
uploads stream.

| Route | |
|---|---|
| `POST …/objects/batch` | `operation` `upload`/`download`, `transfers` must include `basic` (422), `hash_algo` must be `sha256` (409), ≤ 1000 objects. Per object: `actions.download`, or `actions.upload` + `actions.verify` (each with `href`, `header.Authorization`, `expires_in`), nothing when already stored, or `error` 404 / 422 / 413. |
| `GET …/objects/{oid}` | The bytes, streamed (`application/octet-stream`, `Content-Length`, `nosniff`). |
| `PUT …/objects/{oid}` | The body is the object; its size is the transfer token's, else `Content-Length` (411 without). |
| `POST …/objects/{oid}/verify` | `{oid, size}` → 200 once stored and linked. |
| `GET …/locks`, `POST …/locks`, `POST …/locks/verify`, `POST …/locks/{id}/unlock` | Locks per repository (`ref` is accepted and ignored); 409 when a path is locked already; forcing someone else's unlock needs administrator access. |

Authentication is smart HTTP's own (`git_http::authenticate`: Basic with password or
personal access token, throttled, two-factor accounts need a token, the token's scope caps
access) or a short-lived `Authorization: Bearer lfs1.<payload>.<mac>` header that the batch
API and `git-lfs-authenticate` hand out: a fixed binary payload (repository, subject —
anonymous, user or deploy key —, read or write, expiry, and for transfer actions the one
object and its size), HMAC-SHA256 under a 256-bit key drawn at start-up (a restart
invalidates outstanding headers; clients ask again), verified in constant time. Nothing is
stored, and `LfsService::open_granted` recomputes the holder's access on every use
(suspension, removed collaborators and deleted deploy keys end a token early), capped at
the token's read or write. Object-scoped headers only authorize that object's transfer;
batch and lock endpoints require repository-scoped credentials. Download batch replies use
the stored object's size. Invalid tokens count as failed Git authentication. Downloads need
read access; uploads, verify and locks write access (and an account for locks); pull mirrors
refuse uploads; hidden repositories are 404 (401 with `LFS-Authenticate` to anonymous
callers, so clients ask for credentials). Errors are `{"message": …}` as
`application/vnd.git-lfs+json`. With `[lfs] enabled = false` every route answers 404 and the
SSH command is refused.

Over SSH, `git-lfs-authenticate <repo> <upload|download> [oid]` is parsed as strictly as the
Git commands and authorised exactly like a fetch (download) or push (upload) — deploy keys
(read-only ones cannot upload), mirrors, old names, suspension; it prints `{"href":
"<public_url>/<owner>/<repo>.git/info/lfs", "header": {"Authorization": "Bearer lfs1…"},
"expires_in": …}`. The pure-SSH `git-lfs-transfer` protocol is not implemented; clients fall
back to `git-lfs-authenticate` + HTTP.

`LfsService::store(&RepoContext, &LfsOid, size, reader)` is the one way objects enter a
repository (HTTP uploads use it; an importer can too): it verifies size and SHA-256 and
links the object.

---

## 5. Security model

| Concern | Measure |
|---|---|
| Passwords | Argon2id (m = 19 MiB, t = 2, p = 1 — OWASP 2024), hashed on the blocking pool. |
| Sessions | 32 random bytes from the OS CSPRNG, base64url in an `HttpOnly; SameSite=Lax` cookie (`Secure` behind HTTPS). Only SHA-256 digest stored. Sliding expiry. |
| Access tokens | `rbit_` + 256 random bits (`rit_` from earlier releases is still accepted: the digest covers only the random part); only the SHA-256 digest is stored, plus a 4-character hint shown as `…a1b2`. Used as the Git HTTP password and as the API bearer token; scope (`read`/`write`/`admin`) caps the owner's access, so a read token can clone but never push, and only an admin token changes settings, webhooks or deletes. Optional expiry; `last_used_at` is recorded at most hourly. Max 50 per user. |
| Repository access | One pure policy (`repo_access`) decides every check — pages, API, Git over HTTP and SSH — from the viewer's grants (organisation role and base permission, teams, collaborator entry); SQL listings use one readable-by predicate tested against it. Leaving an organisation revokes its team memberships and collaborator entries in the same transaction (trigger). |
| Webhook secrets | **An exception to "secrets only as digests":** HMAC signing needs the raw value, so the secret is stored sealed (§3.3), not hashed. It is write-only — never shown, returned by the API, logged or copied into the delivery log (the UI only says "configured"). |
| Server keys | The other exception: `server_secrets` holds 256-bit keys rbit generates itself on first use and never shows — `notification_unsubscribe` (unsubscribe links) and `external-sign-in` (the sign-in flow cookie, PKCE verifiers and nonces). HMAC needs them raw, so they are sealed (§3.3) rather than hashed. The Git LFS token key is never stored (drawn at start-up). |
| Outbound requests (SSRF) | Webhook deliveries, remote Git operations and external sign-in requests refuse internal addresses (`network::is_internal_address`) unless `allow_private_addresses`; sign-in providers use the webhooks' guarded resolver (no redirects, no proxy, `https` only, 1 MiB responses); webhooks check inside the DNS resolver at connection time, follow no redirects and ignore proxy variables; remote Git pins validated addresses, refuses redirects/proxies, and runs without prompts or credential helpers. See §4.3. |
| Release files | Stored by numeric id only; downloads are `application/octet-stream` + `attachment` + `nosniff`, so an uploaded HTML file can never render on the site's origin. The size limit is enforced while the upload streams; names are validated (no `/`, `\`, control characters or leading dots). |
| CSRF | Unsafe methods require `Sec-Fetch-Site: same-origin` or a matching `Origin` header. Combined with `SameSite=Lax` cookies. **One exemption:** `POST /email/unsubscribe/{token}` (exactly that path shape, `middleware::is_one_click_unsubscribe`), which mail clients send from their own origin (RFC 8058); it needs no session, the signed token is the authorisation, and all it can do is turn that account's notification emails off. |
| XSS | Askama auto-escapes. Markdown is rendered with `pulldown-cmark` then sanitised with `ammonia`. Strict CSP: `default-src 'self'`. |
| Path traversal | `RepoName`/`Username` reject anything outside `[A-Za-z0-9._-]`, leading dots and reserved names. On-disk paths use numeric IDs only. |
| Git config injection | Repos opened with isolated gix options; transport processes run with a scrubbed environment and `GIT_CONFIG_NOSYSTEM=1`. |
| Enumeration | Private repositories return **404**, never 403, to unauthorised viewers — in pages and in the API. Old-name redirects are only issued to viewers who may read the repository. |
| Headers | `X-Content-Type-Options`, `Referrer-Policy`, `frame-ancestors 'none'`, `Permissions-Policy`. |
| Two-factor | Passkeys / security keys (WebAuthn, bound to `server.public_url`; single-use challenges, signature counters must advance) and/or TOTP (RFC 6238, ±1 step, each code accepted once, confirmed at enrolment). 10 single-use recovery codes stored hashed, shared by all factors and removed with the last one. Removing a factor or regenerating codes requires the password plus a code or passkey. Enrolled accounts must use access tokens for Git over HTTP. |
| Brute force | `ThrottleService` budgets per scope (sign-in, sign-up, 2FA, Git auth, SSH auth, email requests) keyed by account and by client address (larger shared budget); failures cost extra, success resets; 429 with `Retry-After`. Form posts are limited per address; issues, pull requests, comments and reviews per account (`Content`). |
| Client address | `X-Forwarded-For` / `Forwarded` / `X-Forwarded-Proto` are honoured only from `[security] trusted_proxies`. |
| Sessions | Listed per account; revoke one or all others; renewal atomically updates an existing, unexpired session and cannot restore a revoked one. Password recovery and administrator resets immediately revoke every session; self-service changes preserve only the initiating session. Credential finalization continues after request cancellation. |
| Registration and recovery | First-administrator assignment and closed-registration bootstrap are serialized in the account-creation transaction, including external sign-in. Password resets consume the token, validate its current email and account status, update the password and invalidate other reset tokens in one transaction. |
| Recovery | Reset and verification links are single-use, expire (1 h / 2 days), stored as digests, invalidated by address changes; reset requests never reveal whether an account exists. |
| SSH | Public keys only; strictly parsed exec commands; no shell, PTY, agent or port forwarding; deploy keys scoped to one repository and read-only unless allowed. |
| Branch protection | Enforced in-process in both receive paths before git starts (§4.4), all or nothing; `refs/pull/*` can never be pushed by clients; rules apply to administrators unless `allow_admin_bypass`, and every bypass that mattered is audited; deploy keys are never allowed pushers; refusals never name who may push. Merges by rbit are checked against the same rules (`check_merge`). |
| Web edits | Never check files out (scratch repository, scratch index, `commit-tree`); paths are validated `FilePath`s (no `..`, `.git` in any spelling, control characters or backslashes) and reach git only on stdin; uploads stream to disk under `[editor]` limits; the push gate decides exactly as for a push; the branch moves only through the merge's lease-protected `update-ref`; commits carry the noreply address. Mirrors are read-only. |
| Commit statuses | Write access and a full commit id that exists in the repository; `target_url` follows the webhook URL rules (absolute `http`/`https`, no credentials) and is linked with `rel="nofollow noopener noreferrer"`; ≤ 1000 statuses per (repository, commit, context); required checks only block merges, never pushes. |
| Git LFS | Object ids are validated SHA-256 hex before they name a path; uploads are checked against size and digest before they are placed; an object stored for another repository is never linked without a fresh upload (or read access through the fork network). Transfer headers are HMAC-signed, short-lived, scoped to one repository (and one object), verified in constant time, and re-check the holder's current access; failures are throttled like Git credentials. Objects are served as `application/octet-stream` with `nosniff`; the raw view keeps its sandbox CSP. |
| External sign-in | Code flow with PKCE `S256`; state in a signed, 10-minute `rbit_oauth` cookie (`HttpOnly; SameSite=Lax; Path=/login`), matched exactly in constant time; nonce and verifier derived server-side from the state; ID tokens RS256 (≥ 2048 bits) / ES256 only with `iss`, `aud`/`azp`, `exp`/`iat`/`nbf` (± 60 s) and `nonce` checked; identities keyed by the provider's subject; linking by email only with `trust_email` and verified addresses on both sides; link starts need a session-bound token; callbacks throttled per address (`ExternalSignIn`); suspended accounts refused; the last way to sign in cannot be unlinked (enforced in SQL); two-factor still applies unless `trust_mfa` and the token reports MFA. Client secrets never appear in `Debug` output. |
| Merges | Computed without a working tree in a scratch repository under `<data_dir>/tmp` (`merge-tree --write-tree` + `commit-tree`), so no checkout filters, hooks or editors run; the base repository changes only through one lease-protected `update-ref`; hardened `git` (no system/global config, null hooks path, no prompts, `--end-of-options`, validated ids and names only); bounded time and concurrency; scratch directories removed on every exit path and swept at start-up. |
| Issues and pull requests | 404 for hidden repositories and unknown or malformed numbers; one pure permission table (§3.7); every Markdown body goes through reference rendering and then `ammonia`; label colours appear only as validated `#rrggbb` SVG `fill`s (no inline `style=`); creation and edits are throttled (`ThrottleScope::Content`). Pending reviews are visible to their author only. |
| Privacy | No email address in any page, payload, API object or rbit-made commit (merges and web edits use noreply addresses); notification emails go to verified addresses only, without `Reply-To`; notifications only reach people who can read the repository (§5.1); `@name` links for any existing account, so a mention never reveals whether someone has access; cross-references are shown only to viewers of the source; `owner/repo#N` is never looked up when rendering. |
| Audit | Sign-ins, 2FA changes, session revocations, token changes, password changes and every administrator action are recorded in `audit_events` (`/admin/audit`), as are repository access changes (category "Repository access": collaborators added/changed/removed, teams created/changed/deleted, transfers, webhooks created/changed/deleted, an organisation's member permission changed, branch protection rules created/changed/deleted and bypassed) and deleted issues (category "Repository content"). External sign-ins record the provider (and "provider MFA" when it replaced rbit's second step); linking and unlinking a provider are audited under "Sign-in". |

### 5.1 Notifications

In the app, and by email since milestone 3 (below). One row per (user, thread) in `notifications`, upserted unread with
the strongest reason (`review_requested` > `assigned` > `mention` > `author` > `comment` >
`manual`).

- **Who takes part:** the author, commenters, assignees, mentioned users, requested
  reviewers and reviewers are subscribed automatically (`issue_subscriptions`); anyone can
  subscribe or unsubscribe by hand, and an explicit unsubscribe sticks until they subscribe
  again.
- **Who is notified:** a background subscriber on `Issue`, `IssueComment` and
  `PullRequestReview` events notifies the subscribers plus the people the event names (new
  mentions — at most 50 per text, only in Markdown text, never in code, and only people not
  notified for that text before — the assignee, the requested reviewer), **minus the actor,
  suspended users and anyone who cannot read the repository at that moment** (decided by the
  access policy with the user's grants).
- **Display:** rows of repositories the user can no longer read are hidden and deleted
  lazily; visiting a thread marks it read (the one GET with a side effect); read rows older
  than `[issues] notification_retention_days` (90) are pruned daily. The header's unread dot
  is one indexed `EXISTS`. `[issues] notifications = false` turns everything off.

**Email (milestone 3).** `NotificationEmailService` (`rbit-services/src/notification_email.rs`)
mails what the inbox fan-out writes: `NotificationService::handle_event` passes the
recipients and reasons it stored to `deliver`, which keeps verified, non-suspended
recipients other than the actor whose preference (`notification_email_settings`, default
`[notifications] default_email`) wants the reason (`participating` = every reason but
manual subscriptions, `all` adds those), re-checks read access with grants from the current
repository row, claims a dedup key in `notification_emails` (`opened`, `comment-{id}`,
`review-{id}`, or `{action}-{unix second}` for repeatable actions such as `closed`), spends
the recipient's `ThrottleScope::NotificationEmail` budget (30, then one every two minutes;
throttled mail is skipped, the inbox keeps it) and queues the message on `MailService`. Mail
failures never fail the inbox fan-out. Nothing is sent or claimed with `[mail] transport =
"disabled"`, `[notifications] email = false` or `[issues] notifications = false`.

Messages (`[owner/repo] Title (#N)`, `Re: ` after the first; a headline, an excerpt of at
most 1000 characters of the Markdown source, a link to the comment or review, why the
recipient got it) thread on `<owner/repo/{issues|pulls}/N@host>`: the opening message
carries it as `Message-ID`, later ones get their own id plus `In-Reply-To`/`References`.
Renaming or transferring a repository therefore starts new threads in mail clients. Each
message carries `List-Unsubscribe` and `List-Unsubscribe-Post: List-Unsubscribe=One-Click`
pointing at `/email/unsubscribe/{account id}.{HMAC-SHA256}` (key `notification_unsubscribe`
in `server_secrets`, verified in constant time); `GET` confirms (link scanners must not
unsubscribe), `POST` sets the preference to `off` — the one CSRF exemption (§5). Outbox rows
are pruned with the inbox (`notification_retention_days`).

### 5.2 External sign-in

Providers are configured as `[[auth.providers]]` (`kind = "oidc"` with an `issuer`, or
`"github"` with an optional Enterprise `base_url`) and validated at start-up, every problem
reported at once. Because figment cannot index into an array of tables, `rbit-server` takes
`RBIT_AUTH__PROVIDERS__<N>__CLIENT_SECRET` / `…__CLIENT_ID` out of figment and applies them
with `AuthConfig::apply_provider_env` (`<N>` is the position or the upper-cased id).

`ExternalAuthService` runs the authorization code flow with PKCE `S256`: a `Flow`
(provider, random 256-bit state, `next`, linking account, issue time) is sealed with
HMAC-SHA256 under the `external-sign-in` key into the `rbit_oauth` cookie; the PKCE verifier
and the nonce are HMACs of the state, so nothing is stored server-side. One flow per browser
at a time. The callback requires the exact state, exchanges the code and verifies the ID
token with `rbit_auth::oidc` (discovery cached for an hour, issuer equal to the configured
one; keys by `kid`, one refetch on an unknown key, which is how rotation works); email from
`email` + `email_verified`, or the userinfo endpoint with the same `sub`. GitHub uses
`/login/oauth/authorize` with PKCE, `/user` (the numeric id is the subject) and only the
verified primary address from `/user/emails`.

`Services::complete_external_sign_in` decides by the provider's subject: (1) a known
identity signs in as its account (suspended accounts are refused; an account with 2FA
continues at `/login/two-factor`, unless `trust_mfa` and `amr` contains `mfa`); (2) an
unknown identity arriving from a link started in Settings → Security (same session) is
linked to that account; (3) with `trust_email`, a provider-verified address equal to an
account's verified address links and signs in; (4) when registration is allowed
(`allow_registration`: `true` follows the site's registration setting, `"always"` ignores
it, `false` never) the user chooses a username at `/login/{id}/register` (pending state in
memory for 15 minutes); (5) otherwise a page explains how to link. An unverified address
never links anything. Accounts registered this way store an empty password hash, which never
verifies, until a reset link sets one (Git over HTTPS needs a token meanwhile). Unlinking
refuses the last way to sign in, atomically in SQL.

---

## 6. Front-end design system

**Stack:** Tailwind CSS v4 (CSS-first `@theme` config, built by the standalone CLI),
htmx 2, inline SVG icons (Lucide-style, rendered from an Askama macro), system font stack
(no third-party font requests — important for self-hosting and privacy).

**Visual language: monochrome minimal.** Black and white do the work; colour appears only
for state (success, danger, warning), syntax highlighting, diffs and the language bar
(GitHub Linguist colours, so languages stay recognisable). Hairline 1px borders instead of
shadows, 6px radius, dense 13–14px type, underline tabs, inverted primary buttons.

| Token | Light | Dark |
|---|---|---|
| `--canvas` (page) / `--surface` (containers) | #f7f7f7 / #ffffff | #000000 / #0d0d0d |
| `--subtle` / `--hover` | #fafafa / #f0f0f0 | #141414 / #1c1c1c |
| `--line` / `--line-strong` | #e3e3e3 / #cccccc | #2a2a2a / #3d3d3d |
| `--ink` / `--ink-muted` / `--ink-faint` | #0a0a0a / #5c5c5c / #787878 | #ededed / #a3a3a3 / #7a7a7a |
| `--inverted` (primary buttons) | #0a0a0a | #ededed |

Every token is defined once with CSS `light-dark()`, so components never need `dark:`
variants, and components use only tokens — never literal colours. Two elevation levels:
the page `--canvas` and containers `--surface`, separated by a `--line` that stays visible
in every theme. `scrollbar-gutter: stable` keeps page width identical whether or not a
page scrolls.

**Themes.** The active theme is `<html data-theme="SLUG">` (plus `data-scheme` with its
light/dark variant): none follows the OS, `light` / `dark` pin the built-in monochrome
themes, and `base16-…` / `base24-…` are ~500 [tinted-theming](https://github.com/tinted-theming/schemes)
schemes mapped onto the same tokens by `npm run themes <schemes-checkout>` (generated,
committed: `assets/themes/*.css`, `src/themes/catalog.rs`, `assets/themes/LICENSE.txt`).
Theme selectors are scoped (`[data-theme="…"]`), so gallery preview cards render in their
own theme. The choice lives in the `rbit-theme` cookie, which the server reads to render
the attribute and the single ~600-byte theme stylesheet into the first byte of HTML — no
flash, no blocking script. Signed-in users' choices are also stored (`user_preferences`)
and restored into the cookie on sign-in. `/themes` (and Settings → Appearance) is the
gallery: by family and variant, searchable, live preview on hover, plain-form fallback. The header's
sun/moon flips light/dark in one click: the generator pairs each scheme with its sibling of
the other variant in the same family (Gruvbox dark, hard ↔ light, hard; Catppuccin Latte ↔
Mocha), falling back to rbit Light/Dark. The palette button beside it opens a modal theme
picker (fetched from `/themes/picker` on first use: families, recent themes, search, a
Dark/Light filter and a live preview on a miniature repository page); without scripts it
links to the gallery.

**Components** (`styles/app.css`): `.box`, `.box-header`, `.row`, `.btn` (`-primary`,
`-ghost`, `-danger`, `-icon`), `.input`, `.label`, `.hint`, `.badge`, `.tab`, `.menu`,
`.code-lines`, `.diff-view`, `.prose-rbit`, and for wave B `.state-badge` (`-open`,
`-closed`, `-merged`, `-draft`), `.label-chip`, `.timeline`, `.timeline-item`,
`.timeline-event`, `.timeline-badge`, `.notification-dot`, `.diff-comment`. Status colours:
open and approved `--success`, closed `--ink-muted`, merged an inverted badge, draft a
`--ink-faint` outline, conflicts and changes requested `--danger`, pending `--warning`;
labels keep `--ink` text with a coloured dot, legible in every theme. Icons and shared fragments are Askama macros in
`templates/macros.html` — never add another `macros.html` inside a template subfolder
(Askama resolves imports relative to the importing template first).

**Layout rules**
- Mobile: single column, sticky top bar, repository tabs scroll horizontally,
  file tables collapse secondary columns.
- `md:` and up: repository sidebar ("About", languages, releases) appears beside content.
- Minimum touch target 44 px; focus rings always visible for keyboard users.

**PWA**
- `manifest.webmanifest` with maskable icons, `display: standalone`, theme colours.
- `sw.js`: cache-first for content-hashed `/assets/*`, network-first for HTML with an
  offline fallback page. Never caches authenticated Git traffic.

---

## 7. Performance strategy

- Compiled templates (Askama) and embedded, pre-compressed-on-the-fly assets with
  `Cache-Control: immutable` + content hashes.
- `mimalloc` global allocator; `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` in release.
- gitoxide with `max-performance-safe` (parallel pack decoding, fast SHA-1, zlib-rs).
- SQLite tuned: WAL, `synchronous = NORMAL`, memory-mapped I/O, prepared statement cache.
- Streaming Git transport: pack data is never buffered in memory.
- One pinned repository handle per page request (`RepositoryReader::pinned()`): a page's
  Git reads share opened config, refs and pack indexes instead of reopening them.
- Blame shares matching calculations and keeps at most 128 results with a 32 MiB
  retained-size budget. At most four calculations run at once; a response timeout leaves
  the work counted against that limit until it actually finishes. Every request checks
  access and resolves the revision again; cached results hold no permissions or Git
  repository readers. Reuse also checks repository identity, effective configuration,
  HEAD, shallow boundaries, local attributes and the index. External filters/attributes,
  custom diff drivers, replacements and inputs too large to fingerprint bypass reuse.
- Diff file limits bound retained changes and stop traversal when possible. Rename
  discovery can still require a full-tree scan. Hunk line budgets are checked before
  allocating display strings; omitted hunks still contribute exact addition/deletion
  counts.
- Pull-request review tallies fetch unique reviewers and current grants in batches of
  at most 256 IDs. Notification fan-out uses 100-recipient batches for permission reads,
  inbox transactions and email deduplication claims; email refreshes active, verified
  accounts, preferences and permissions after the inbox is written.
- Session expiry scans use a read snapshot. Write transactions recheck and remove at
  most 256 candidates at a time, so a scan does not monopolize the writer or remove a
  session renewed since the snapshot.
- Navigation: `hx-boost` plus a purpose-built preloader (`assets/preload.js`, delegated
  document events and native XHR terminal cleanup): ~80 ms hover or mousedown, taps but not scrolls on touch screens,
  `data-preload="visible"` links (repository tabs, next pages) when idle within a small
  budget and never under Save-Data or 2G; each URL at most once per freshness window
  (`PRELOAD_MAX_AGE_SECONDS`, shared with the server), never links with their own
  `hx-get`/`hx-post`. A preload is a `fetch` with exactly the headers responses vary on,
  so the click is answered from the browser cache. Within a repository only `<main>`
  swaps and the tabs update out of band, and the request says so (`Rbit-Partial`):
  `partial::partial_responses` then sends only the title, `<main>` and the tabs, cut at
  markers the layout emits. `middleware::preload_guard` answers preloads of
  side-effecting or heavy paths with `204`; `middleware::html_caching` makes HTML
  `private, no-cache` with a weak `ETag` (revisits cost a `304`), preloads fresh for the
  window, varying on `HX-Request`, `Rbit-Partial` and a per-page-load navigation epoch so
  nothing preloaded before a form post is shown after it. Commit-pinned fragments of
  public repositories are `immutable` and vary on `HX-Request` only. Secret-showing pages
  carry `hx-history="false"`.
- Large files and diffs: rows of exactly one `--row-h` grouped in blocks of 128
  (`views::blocks`, `.vblock`), each one `content-visibility: auto` container whose
  placeholder height comes from `data-rows` (typed `attr()`, or `assets/blocks.js`
  through the CSSOM), so nothing moves as blocks are laid out; line numbers and diff
  signs drawn by CSS. Each open changed file is contained as a whole the same way
  (`.diff-file[data-rows]`: header, bars and placeholders are whole rows; blocks.js adds
  the diff's scrollbar where it takes space), and only diffs of more than
  `BLOCKED_ABOVE_ROWS` (512) rows also use blocks inside it. See [design/frontend-virtualization.md](design/frontend-virtualization.md).
- Syntax highlighting resolves syntect's scopes to one short class per token in Rust
  (flat spans, styled categories only) and caches highlighted files by blob id
  (`render::highlight_cache`, 32 MiB). File and blame views share the cached allocation;
  concurrent misses for one key share a calculation. At most four distinct jobs are
  admitted, and cancellation does not release a slot before its CPU work finishes.
  Preload bookkeeping retains at most 128 records, six queued requests and two active
  requests; navigation and epoch changes discard obsolete work, and Save-Data disables
  all speculative requests. See [performance-followups.md](performance-followups.md)
  for measurements and their limits. Pull request counts are cached by base and head
  commit ids in redb (`comparison_counts`).
- JavaScript modules: relative imports are rewritten to content-hashed URLs when served,
  so every module can be `immutable` and still update on deploy.
- redb caches for last-commit-per-path and language statistics, keyed by commit id (never
  stale) and warmed after pushes.
- Archives and release downloads stream; uploads are hashed while they stream to disk.

---

## 8. Extensibility

- **Trait seams** at every external boundary: `GitTransport`, store traits in services,
  `PasswordHasher`. Swapping an implementation never changes callers.
- **Event bus** — `services` emits `DomainEvent`s (§2) on a `tokio::broadcast` channel;
  cache warming, verification email, release-file clean-up and
  webhooks subscribe independently, and CI triggers or search indexing can join the same way.
- **REST API** — new resources add rows to `api::table()` and paths to `openapi.json`; the
  route/document test keeps them in step.
- **Feature areas** — issues, pull requests and branch protection were added inside the
  existing crates (one module per layer, each with its own `routes()`, store, service and
  API fragment). Larger future areas (CI, packages) can still arrive as crates contributing
  services and a `Router` that `rbit-web` merges.
- **Status checks** — any external CI reports through the commit status API (§4.2) and
  branch protection can require its contexts (§3.7); a runner protocol could build on the
  same statuses without changing merges.
- **Server keys** — `server_secrets` is generic: a feature that needs its own HMAC key
  calls `get_or_insert` with a new name.

---

## 9. Releases and packaging

A `v*` tag runs `.github/workflows/release.yml`: the tag must equal the workspace version
and `CHANGELOG.md` must have a `## [X.Y.Z]` section, whose body becomes the release notes
(a pre-release tag such as `v0.1.0-rc.1` falls back to `## [Unreleased]`). The stylesheet
is built once and embedded into every binary. Binaries are built natively (Linux x86_64 and
aarch64 on Ubuntu 22.04 runners for a glibc 2.35 baseline, macOS both architectures on
Apple silicon, Windows x86_64), archived with the licenses, docs and deployment examples,
and published with `SHA256SUMS`. The image `ghcr.io/gitrbit/rbit` (amd64, arm64) packages
those same Linux binaries through the Dockerfile's `BINARY=prebuilt` stage instead of
compiling under emulation; a plain `docker build .` still compiles from source. CI checks
licenses and advisories with `cargo deny` (`deny.toml`); each ignored advisory carries its
reason. rbit is licensed MIT OR Apache-2.0.

---

### 9.1 Release checks and version privacy

The footer also follows ordinary htmx navigation without polling. HTML responses carry
versionless `Rbit-Update-Available` metadata from the same cached service snapshot.
UTC-nanosecond revisions advance only when the background worker observes a result
(or the service starts), and the initial footer carries its revision too. The browser
orders snapshots with `BigInt`, including prefetched navigation responses and restored
history, so an old cached page cannot erase or resurrect a newer notice. A newer clear
removes it; a newer available snapshot from a preload can reveal it. This adds no upstream
request to page rendering.

`UpdateService` owns a single cancellable background task. It reads only the fixed
`https://api.github.com/repos/GitRbit/rbit/releases/latest` endpoint, with a constant
versionless User-Agent, no credentials, cookies, instance identity or running-version
parameter. The client honors deployment proxy settings, requires HTTPS, refuses redirects,
and limits requests to a five-second connect timeout, ten seconds total and a 64 KiB body.
Tags are validated as SemVer and release links are constructed under the fixed upstream
repository. Drafts and prereleases are excluded. A stable release supersedes the same
version's prerelease; an older stable release does not supersede a newer local prerelease.
Build metadata does not affect precedence.

Successful checks are cached in memory for 24 hours. Failures back off from 15 minutes
to at most 24 hours, honoring bounded `Retry-After` and GitHub reset hints. The last
successful notice survives transient failures; restarting forgets the cache and performs
one new check. Page rendering only reads a mutex-protected snapshot and never contacts
upstream. The footer shows a generic release-notes link; administrators also see check
status. No release is installed or downloaded automatically. Tests inject local HTTP
fixtures into the private worker constructor and disable live checks in application
test harnesses.

`[updates] check_for_updates = false` disables the task and its HTTP client independently
of `report_version`. Both options default to true and are deployment configuration:
TOML, `RBIT_UPDATES__CHECK_FOR_UPDATES`, `RBIT_UPDATES__REPORT_VERSION`, or the administrator
configuration editor. Saved changes require restart. With reporting disabled, the
global policy overrides the live footer branding flag, `/api/v1/version` answers 404,
the pre-authentication SSH banner omits its version suffix, and provider, import/API/
download and webhook User-Agents omit the running version (including webhook delivery
records). Update checks never send it even when reporting is enabled. Administrator-only
installed-version diagnostics, local CLI output and backup manifests remain accurate.
Opaque asset content hashes and API/Prometheus protocol versions are not release versions
and remain unchanged. Historical delivery records retain their original headers until
normal retention removes them.

## 10. Roadmap

| Phase | Scope |
|---|---|
| **1 — Foundation** ✓ | Accounts, sessions, repository create/browse, code/tree/blob/commits views, smart-HTTP clone & push, README rendering, syntax highlighting, PWA shell, light/dark theme. |
| **2 — Collaboration** ✓ | Commit diffs, branches/tags pages, personal access tokens, organisations with roles, stars, language breakdown, last commit per entry, user/repository settings, administration panel, runtime instance settings, suspension. |
| **M1 — Safe to self-host** ✓ | Rate limiting, trusted proxies, session management, TOTP two-factor, audit log, email (SMTP) with password reset and verification, SSH server with user and deploy keys, background jobs and maintenance, metrics and health checks, backups, Docker/systemd/CI, end-to-end and property tests. |
| **M2 wave A — A real forge** ✓ | Collaborators, teams and organisation base permission, repository transfer with old-name redirects, REST API `/api/v1` with token scopes and OpenAPI, signed webhooks with a persistent delivery queue, README relative links, archives, blame, compare, releases with attached files, forks, pull mirrors, the theme gallery. |
| **M2 wave B — Workflow** ✓ | Issues with labels, milestones, assignees, references and search; in-app notifications; pull requests across forks with line comments, reviews, mergeability and merge/squash/rebase; branch protection enforced over HTTP and SSH; REST endpoints and `issues`/`issue_comment`/`pull_request`/`pull_request_review` webhooks ([design/milestone-2b.md](design/milestone-2b.md)). |
| **M3 — Ready to open-source** (in progress) | Done: MIT OR Apache-2.0, community files and release packaging, `rbit_` tokens; commit statuses and required checks; REST API for collaborators, teams, releases and forks, `member` and `status` webhooks; email notifications; editing files in the browser; Git LFS; external sign-in with OpenID Connect and GitHub; importing from GitHub and Gitea/Forgejo, and push mirrors ([design/milestone-3.md](design/milestone-3.md)). In progress: wave B stretch items (W). |
| Next | A gitoxide transport behind a switch, a runner protocol on top of commit statuses, email digests. |
| Later — Platform | Actions/CI runner protocol, package registry, full-text code search (tantivy). |
| 5 — Scale | Read replicas for repos, object storage for LFS, horizontal web tier. |
