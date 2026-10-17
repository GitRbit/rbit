# Rbit — Architecture & Design

Rbit is a self-hosted Git forge written in Rust. It competes with Gitea, Forgejo and
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
Rbit/
├── Cargo.toml                 # workspace: shared deps, lints, release profile
├── rustfmt.toml               # formatting standard (checked in CI)
├── package.json               # Tailwind v4 CLI, htmx, icon rendering (build-time only)
├── Dockerfile, deploy/        # container image, systemd unit, example production config
├── .github/workflows/ci.yml   # fmt, clippy -D warnings, tests (Linux + Windows), builds
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
reach, shared by webhooks and mirrors), and the shared `Config` model. Pure Rust, trivially
unit-testable.

**`rbit-storage`**
- `sqlite/` — `sqlx` pool (WAL, foreign keys, busy timeout), embedded migrations, and one
  *store* per aggregate (`UserStore`, `RepositoryStore`). Stores map rows ↔ domain types.
- `kv/` — a `redb` database with typed table definitions. Used for sessions and derived,
  rebuildable data (caches). Everything in redb can be lost without losing user data.

**`rbit-auth`** — `PasswordHasher` (Argon2id, OWASP parameters), `SessionToken` and
`AccessTokenSecret` (256-bit random, only SHA-256 digests are persisted), TOTP (RFC 6238,
own implementation verified against the RFC test vectors) with recovery codes, a minimal
WebAuthn relying party for passkeys (`webauthn`: no attestation, ES256/EdDSA/RS256, a small
CBOR/COSE reader; origin, RP ID, challenge, user presence, signature and counter checked on
every ceremony), and the pure `repo_access(viewer, repo, &Grants)` policy (who may read, write or administer a
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
config, no hooks executed on read; every `git` process gets a scrubbed environment.

**`rbit-transport`** — The Git smart protocol. `ProcessTransport` runs
`git upload-pack/receive-pack` with a scrubbed environment in two modes: *stateless*
(`--stateless-rpc`, for HTTP `info/refs` and POSTs) and *interactive* (one bidirectional
conversation with exit status, for SSH). Both stream; pack data is never buffered. A
pure-Rust gitoxide transport could replace it without touching callers.

**`rbit-ssh`** — The built-in SSH server (russh): public-key authentication against user
SSH keys and repository deploy keys, strictly parsed `git-upload-pack` / `git-receive-pack`
exec requests, the same authorisation as HTTP, connection limits, throttled failures, and
no shells, PTYs or forwarding. The Ed25519 host key is generated on first start.

**`rbit-mail`** — A `Mailer` trait with SMTP (lettre over rustls), log and disabled
transports; every message is rendered from Askama templates with text and HTML parts.

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
(scheduled fetches). `Services::spawn_background()` starts the webhook, release and mirror
tasks; `Services::shutdown()` stops them (and the job queue) within about ten seconds.

Services communicate through an in-process `EventBus` of `DomainEvent`s, so producers never
depend on reactions such as cache warming, session revocation, verification email or
webhooks. Subscribers ignore variants they do not handle, so adding one breaks nothing.

| Event | Fields | Published by |
|---|---|---|
| `RepositoryPushed` | `repo`, `pusher: Option<AccountId>` (`None` for deploy keys and mirror fetches), `updates: Vec<RefUpdate>` (`name`, `before`, `after`; ordered by name) | `PushService::complete` (HTTP, SSH, mirror fetches, tags created for releases) |
| `RepositoryCreated` | `repo`, `actor` | `RepositoryService::create_in` (not forks) |
| `RepositoryDeleted` | `repo`, `owner`, `full_name` (as it was), `actor` | repository settings, site administration |
| `RepositoryTransferred` | `repo`, `from`, `to`, `actor` | `TransferService` |
| `RepositoryForked` | `repo`, `parent`, `actor` | `ForkService` |
| `ReleasePublished` | `repo`, `release`, `actor` | `ReleaseService` (created as non-draft, or a draft published) |
| `UserSignedUp`, `PasswordChanged`, `EmailChanged` | `user` (`previous` address for email) | `AccountService`, `AdminService`, `ProfileService`, `RecoveryService` |

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
└── attachments/
    ├── <repo_id>/<asset_id>       # files attached to releases, named by id only
    └── tmp/<random>.part          # uploads in progress
```

Repositories are addressed by numeric IDs, not names, so renaming a user or repository
is a single SQL update — no filesystem moves, no path-traversal surface. A transfer moves
the directory to the new owner's folder (`rbit_git::relocate`, a same-filesystem
rename) before the owner changes in SQL, and moves it back if the database step fails.

Release files never use a user-supplied name on disk and do not depend on the owner, so
transfers move nothing. An upload streams into `tmp/<random>.part` while its SHA-256 and
size are computed and is abandoned the moment it exceeds `[releases] max_asset_bytes`;
only a complete file gets a `release_assets` row, after which it is synced and renamed
into place (the row is removed again if the rename fails). Deleting a file removes the row
first, then the bytes, so a crash leaves at worst an orphaned file, never a row without
bytes. `ReleaseService` removes `attachments/<repo_id>` when a repository is deleted and
sweeps `.part` files older than a day at start-up. Downloads are always
`application/octet-stream` with `Content-Disposition: attachment`, `nosniff` and an
`ETag` from the digest.

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
two_factor         account_id → accounts, secret, created_at, confirmed_at, last_used_step (replay guard)
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

**Migrations** live in `crates/rbit-storage/migrations/` and are embedded in the binary:

| | Milestone 1 | | Milestone 2 wave A |
|---|---|---|---|
| `0001` | `initial` (accounts, logins, members, repositories) | `0009` | `access` (collaborators, teams, base permission, revoke trigger) |
| `0002` | `access_tokens` | `0010` | `preferences` (`user_preferences`) |
| `0003` | `organizations` (`org_profiles`) | `0011` | `webhooks` (hooks, delivery queue) |
| `0004` | `administration` (instance settings, suspension) | `0012` | `token_scopes` (rebuilds `access_tokens` for `admin`, keeping every id) |
| `0005` | `profiles_and_stars` | `0013` | `releases` (releases, assets) |
| `0006` | `security` (2FA, recovery codes, audit) | `0014` | `forks` |
| `0007` | `email` (email tokens, verified address) | `0015` | `mirrors` |
| `0008` | `ssh_keys` (user and deploy keys) | `0016` | `repo_redirects` (table and triggers) |
| | | `0032` | `passkeys` (WebAuthn credentials as a second factor) |

`crates/rbit-storage/tests/upgrade.rs` builds a database at the milestone 1 schema
(`0001`–`0008`) with representative rows and migrates it to the current schema, asserting
that every row survives and the new defaults and triggers apply to old rows. Wave B
reserves `0017`–`0031` ([design/milestone-2b.md](design/milestone-2b.md)); `0032` was taken
by passkeys while that range is unfilled (sqlx applies missing lower versions in order).

Secrets (passwords, sessions, tokens, recovery codes, email links) are only ever stored
hashed — with one documented exception: **webhook secrets** are stored as given, because
signing a delivery with HMAC needs the raw value (see §5).

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
└── manifest.json                  # written last; its presence marks a complete backup
```

The manifest records `format` (currently **2**; 1 had no `attachments/`), the Rbit
version, the time, and counts (accounts, repositories, attachments) plus the ids of
repositories and attachments that disappeared between the snapshot and the copy. Release
files are written once and never changed, so a plain copy is consistent. redb is not
backed up: it holds only sessions and rebuildable caches.

---

## 4. Request lifecycle

```
TCP → hyper → tower layers:
  TraceLayer → metrics::track (route template, status, latency)
→ Router
  ├── /assets/*, /sw.js, /manifest.webmanifest   immutable, content-hashed, embedded
  ├── /healthz, /readyz, /metrics                operations (metrics optional, token-gated)
  ├── /{owner}/{repo}.git/…                       smart HTTP (Basic auth or token; throttled; streaming)
  ├── /api/v1/…                                   REST API (own layers, below; no cookies, no CSRF)
  ├── /{owner}/{repo}/archive/{rev}.zip|.tar.gz   archives (session + security headers; streamed, no timeout)
  ├── /{owner}/{repo}/releases/{id}/assets        uploads (session + CSRF + headers; upload_timeout_seconds,
  │   /{owner}/{repo}/releases/download/…           body limit max_asset_bytes + 1 MiB) and downloads
  └── HTML routes
        Compression → Timeout (30 s) → body limit (1 MiB) → SecurityHeaders → CsrfOriginCheck
        → rate_limit (unsafe requests, per client address) → session → error pages
        → extractors → handler → service → view model → Askama

SSH (separate listener, rbit-ssh): public-key auth → exec git-upload-pack/receive-pack
```

**Old names redirect.** When a page answers 404 and its first two path segments name a
repository's *old* location (`repo_redirects`) that the viewer may read, the error-page
layer answers `301` (`308` for anything but `GET`/`HEAD`, so forms keep their method) to the
same path under the current name; redirects are not cacheable, because a new repository
may take the old name at any time and then wins. Git over HTTP and SSH serve the
repository directly under an old name (`RepositoryService::open_following_renames`).

**Pull mirrors refuse pushes** in both transports (`403` over HTTP, an error message over
SSH), checked after access so only people who could otherwise push learn it is a mirror.

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

- **Authentication** — `Authorization: Bearer rit_…` (or GitHub's `token rit_…`). Cookies
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
- **OpenAPI** — `openapi.json` is a hand-written OpenAPI 3.1 document embedded in the
  binary. The router is built from one route table (`api::table()`), and a test asserts that
  the documented paths and methods equal the registered ones.

### 4.3 Webhooks

Repository hooks fire for that repository; organisation hooks for every repository of the
organisation (and are the only ones to see deletions). Repository administrators and
organisation owners manage them in the UI and the API; at most 20 per repository or
organisation; changes are audited.

- **Fields** — `url` (`http`/`https`, ≤ 2048 bytes, no credentials in it; internal address
  literals and `localhost` names are rejected when saved), optional `secret` (write-only), `events` (`push`,
  `repository`, `release`, `fork`; a `ping` is sent on creation), `active`. Bodies are
  always `application/json`.
- **Payloads** follow GitHub's shapes so existing receivers work: `push` (one delivery per
  changed ref: `ref`, `before`/`after` with 40 zeros for creation and deletion, `created`,
  `deleted`, up to 20 `commits` plus `total_commits`, `head_commit`, `pusher`, `compare`),
  `repository` (`created`, `deleted`, `transferred`), `release` (`published`), `fork`,
  `ping` (`zen`, `hook_id`, `hook`); all with `sender`, and with `repository` (and
  `organization` for organisation-owned repositories) where one applies.
- **Headers** — `User-Agent: Rbit-Hookshot/<version>`, `X-Rbit-Event`,
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
- **SSRF** — unless `allow_private_addresses`, every address a target resolves to must pass
  `rbit_core::network::is_internal_address`: loopback, private (RFC 1918), shared/CGNAT
  (`100.64.0.0/10`), link-local, unique-local (`fc00::/7`), multicast, unspecified,
  broadcast, reserved, documentation and benchmarking ranges are refused, and IPv6 forms
  embedding an IPv4 address (mapped, compatible, NAT64, 6to4) are judged by that address.
  The check runs inside the HTTP client's DNS resolver at connection time, so DNS rebinding
  cannot slip past it; address literals are checked before sending; proxy environment
  variables are ignored. Pull mirrors use the same classifier right before each fetch.

---

## 5. Security model

| Concern | Measure |
|---|---|
| Passwords | Argon2id (m = 19 MiB, t = 2, p = 1 — OWASP 2024), hashed on the blocking pool. |
| Sessions | 32 random bytes from the OS CSPRNG, base64url in an `HttpOnly; SameSite=Lax` cookie (`Secure` behind HTTPS). Only SHA-256 digest stored. Sliding expiry. |
| Access tokens | `rit_` + 256 random bits; only the SHA-256 digest is stored, plus a 4-character hint. Used as the Git HTTP password and as the API bearer token; scope (`read`/`write`/`admin`) caps the owner's access, so a read token can clone but never push, and only an admin token changes settings, webhooks or deletes. Optional expiry; `last_used_at` is recorded at most hourly. Max 50 per user. |
| Repository access | One pure policy (`repo_access`) decides every check — pages, API, Git over HTTP and SSH — from the viewer's grants (organisation role and base permission, teams, collaborator entry); SQL listings use one readable-by predicate tested against it. Leaving an organisation revokes its team memberships and collaborator entries in the same transaction (trigger). |
| Webhook secrets | **The one exception to "secrets only as digests":** HMAC signing needs the raw value, so the secret is stored as given. It is write-only — never shown, returned by the API, logged or copied into the delivery log (the UI only says "configured"). |
| Outbound requests (SSRF) | Webhook deliveries and mirror fetches refuse internal addresses (`network::is_internal_address`) unless `allow_private_addresses`; webhooks check inside the DNS resolver at connection time (no DNS rebinding), follow no redirects and ignore proxy variables; mirrors check right before each fetch, refuse redirects, and run `git` without prompts or credential helpers. See §4.3. |
| Release files | Stored by numeric id only; downloads are `application/octet-stream` + `attachment` + `nosniff`, so an uploaded HTML file can never render on the site's origin. The size limit is enforced while the upload streams; names are validated (no `/`, `\`, control characters or leading dots). |
| CSRF | Unsafe methods require `Sec-Fetch-Site: same-origin` or a matching `Origin` header. Combined with `SameSite=Lax` cookies. |
| XSS | Askama auto-escapes. Markdown is rendered with `pulldown-cmark` then sanitised with `ammonia`. Strict CSP: `default-src 'self'`. |
| Path traversal | `RepoName`/`Username` reject anything outside `[A-Za-z0-9._-]`, leading dots and reserved names. On-disk paths use numeric IDs only. |
| Git config injection | Repos opened with isolated gix options; transport processes run with a scrubbed environment and `GIT_CONFIG_NOSYSTEM=1`. |
| Enumeration | Private repositories return **404**, never 403, to unauthorised viewers — in pages and in the API. Old-name redirects are only issued to viewers who may read the repository. |
| Headers | `X-Content-Type-Options`, `Referrer-Policy`, `frame-ancestors 'none'`, `Permissions-Policy`. |
| Two-factor | Passkeys / security keys (WebAuthn, bound to `server.public_url`; single-use challenges, signature counters must advance) and/or TOTP (RFC 6238, ±1 step, each code accepted once, confirmed at enrolment). 10 single-use recovery codes stored hashed, shared by all factors and removed with the last one. Removing a factor or regenerating codes requires the password plus a code or passkey. Enrolled accounts must use access tokens for Git over HTTP. |
| Brute force | `ThrottleService` budgets per scope (sign-in, sign-up, 2FA, Git auth, SSH auth, email requests) keyed by account and by client address (larger shared budget); failures cost extra, success resets; 429 with `Retry-After`. Form posts are limited per address. |
| Client address | `X-Forwarded-For` / `Forwarded` / `X-Forwarded-Proto` are honoured only from `[security] trusted_proxies`. |
| Sessions | Listed per account; revoke one or all others; all other sessions end when the password changes. |
| Recovery | Reset and verification links are single-use, expire (1 h / 2 days), stored as digests, invalidated by address changes; reset requests never reveal whether an account exists. |
| SSH | Public keys only; strictly parsed exec commands; no shell, PTY, agent or port forwarding; deploy keys scoped to one repository and read-only unless allowed. |
| Audit | Sign-ins, 2FA changes, session revocations, token changes, password changes and every administrator action are recorded in `audit_events` (`/admin/audit`), as are repository access changes (category "Repository access": collaborators added/changed/removed, teams created/changed/deleted, transfers, webhooks created/changed/deleted, an organisation's member permission changed). |

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
Mocha), falling back to Rbit Light/Dark; the palette menu beside it picks any theme.

**Components** (`styles/app.css`): `.box`, `.box-header`, `.row`, `.btn` (`-primary`,
`-ghost`, `-danger`, `-icon`), `.input`, `.label`, `.hint`, `.badge`, `.tab`, `.menu`,
`.code-lines`, `.diff-view`, `.prose-rbit`. Icons and shared fragments are Askama macros in
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
- Navigation: `hx-boost` plus the htmx preload extension (`assets/nav.js` marks eligible
  links; 100 ms hover intent, touchstart on touch screens). Within a repository only
  `<main>` swaps and the tabs update out of band. `middleware::preload_guard` answers
  preloads of side-effecting or heavy paths with `204`; `middleware::html_caching` makes
  HTML `private, no-cache` with a weak `ETag` (revisits cost a `304`), preloads briefly
  fresh, varying on `HX-Request` and a per-page-load navigation epoch so nothing preloaded
  before a form post is shown after it. Secret-showing pages carry `hx-history="false"`.
- Large files and diffs: one block per line with `content-visibility: auto`, line numbers
  drawn by CSS, per-file diffs laid out only near the viewport.
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
  cache warming, session revocation, verification email, release-file clean-up and
  webhooks subscribe independently, and CI triggers or search indexing can join the same way.
- **REST API** — new resources add rows to `api::table()` and paths to `openapi.json`; the
  route/document test keeps them in step.
- **Feature crates** — new areas (issues, pull requests, CI, packages) arrive as new
  crates (`rbit-issues`, …) contributing services and a `Router` that `rbit-web` merges.

---

## 9. Roadmap

| Phase | Scope |
|---|---|
| **1 — Foundation** ✓ | Accounts, sessions, repository create/browse, code/tree/blob/commits views, smart-HTTP clone & push, README rendering, syntax highlighting, PWA shell, light/dark theme. |
| **2 — Collaboration** ✓ | Commit diffs, branches/tags pages, personal access tokens, organisations with roles, stars, language breakdown, last commit per entry, user/repository settings, administration panel, runtime instance settings, suspension. |
| **M1 — Safe to self-host** ✓ | Rate limiting, trusted proxies, session management, TOTP two-factor, audit log, email (SMTP) with password reset and verification, SSH server with user and deploy keys, background jobs and maintenance, metrics and health checks, backups, Docker/systemd/CI, end-to-end and property tests. |
| **M2 wave A — A real forge** ✓ | Collaborators, teams and organisation base permission, repository transfer with old-name redirects, REST API `/api/v1` with token scopes and OpenAPI, signed webhooks with a persistent delivery queue, README relative links, archives, blame, compare, releases with attached files, forks, pull mirrors, the theme gallery. |
| M2 wave B | Issues, pull requests with review and merge strategies, branch protection, API endpoints for wave A's resources ([design/milestone-2b.md](design/milestone-2b.md)). |
| 3 — Workflow | Issues, labels, milestones, pull requests with merge strategies, code review. |
| 4 — Platform | Webhooks, REST API (+ OpenAPI), actions/CI runner protocol, package registry, full-text code search (tantivy). |
| 5 — Scale | Read replicas for repos, object storage for LFS, horizontal web tier. |
