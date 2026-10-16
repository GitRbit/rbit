# Rustit — Architecture & Design

Rustit is a self-hosted Git forge written in Rust. It competes with Gitea, Forgejo and
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
Rustit/
├── Cargo.toml                 # workspace: shared deps, lints, release profile
├── rustfmt.toml               # formatting standard (checked in CI)
├── package.json               # Tailwind v4 CLI, htmx, icon rendering (build-time only)
├── Dockerfile, deploy/        # container image, systemd unit, example production config
├── .github/workflows/ci.yml   # fmt, clippy -D warnings, tests (Linux + Windows), builds
├── docs/                      # ARCHITECTURE.md, deployment.md, design/ (per-phase contracts)
└── crates/
    ├── rustit-core/           # Domain model and configuration. No I/O, no async.
    ├── rustit-storage/        # SQLite (relational) + redb (key/value) persistence
    ├── rustit-auth/           # Passwords, session/token secrets, TOTP, permission rules
    ├── rustit-git/            # Repository read model, diffs, languages, maintenance (gitoxide)
    ├── rustit-transport/      # Git smart protocol: stateless (HTTP) and interactive (SSH)
    ├── rustit-services/       # Application use-cases, domain events, background jobs
    ├── rustit-mail/           # Email rendering (Askama) and delivery (SMTP, log)
    ├── rustit-ssh/            # Built-in SSH server (russh) for Git over SSH
    ├── rustit-web/            # Axum router, handlers, Askama templates, assets, metrics
    ├── rustit-server/         # The `rustit` binary: CLI, config, wiring, backups, shutdown
    └── rustit-e2e/            # End-to-end and Git compatibility tests (test-only)
```

### Dependency graph

```
                    rustit-server (bin)
                   ┌────────┴─────────┐
              rustit-web          rustit-ssh
                   └────────┬─────────┘
             rustit-services ─────── rustit-transport
     ┌──────────┬───────┼─────────┬────────────┐
rustit-storage rustit-auth rustit-git rustit-mail
     └──────────┴───────┬─────────┴────────────┘
                    rustit-core
```

Each crate exposes a small, documented public API from `lib.rs`; internals live in
sub-modules grouped by concept (e.g. `storage::sqlite::repositories::users`).

### Crate responsibilities

**`rustit-core`** — Validated value types (`Username`, `RepoName`, `Email`, `BranchName`),
domain entities (`User`, `Repository`, `Visibility`), the `RepoAccess` permission lattice,
and the shared `Config` model. Pure Rust, trivially unit-testable.

**`rustit-storage`**
- `sqlite/` — `sqlx` pool (WAL, foreign keys, busy timeout), embedded migrations, and one
  *store* per aggregate (`UserStore`, `RepositoryStore`). Stores map rows ↔ domain types.
- `kv/` — a `redb` database with typed table definitions. Used for sessions and derived,
  rebuildable data (caches). Everything in redb can be lost without losing user data.

**`rustit-auth`** — `PasswordHasher` (Argon2id, OWASP parameters), `SessionToken` and
`AccessTokenSecret` (256-bit random, only SHA-256 digests are persisted), TOTP (RFC 6238,
own implementation verified against the RFC test vectors) with recovery codes, and the
`repo_access` policy (who may read, write or administer a repository).

**`rustit-git`** — `RepositoryStorage` (where bare repos live on disk, creation, deletion)
and `RepositoryReader` (refs, trees, blobs, commit history) built on **gitoxide** (`gix`).
Repos are always opened with *isolated* options: no system/global config, no hooks
executed on read.

**`rustit-transport`** — The Git smart protocol. `ProcessTransport` runs
`git upload-pack/receive-pack` with a scrubbed environment in two modes: *stateless*
(`--stateless-rpc`, for HTTP `info/refs` and POSTs) and *interactive* (one bidirectional
conversation with exit status, for SSH). Both stream; pack data is never buffered. A
pure-Rust gitoxide transport could replace it without touching callers.

**`rustit-ssh`** — The built-in SSH server (russh): public-key authentication against user
SSH keys and repository deploy keys, strictly parsed `git-upload-pack` / `git-receive-pack`
exec requests, the same authorisation as HTTP, connection limits, throttled failures, and
no shells, PTYs or forwarding. The Ed25519 host key is generated on first start.

**`rustit-mail`** — A `Mailer` trait with SMTP (lettre over rustls), log and disabled
transports; every message is rendered from Askama templates with text and HTML parts.

**`rustit-services`** — Use-cases with business rules, one service per area, all built from
a shared `ServiceContext` (config, database, key/value store, repository storage):
`AccountService` (sign-up, sign-in, sessions, suspension), `RepositoryService` (create,
membership-aware access, listings, explore search), `OrgService` (organisations, members,
roles), `AccessTokenService`, `ProfileService`, `RepoSettingsService`, `StarService`,
`InsightsService` (languages, last commit per entry, cached), `InstanceService` (runtime
settings), `AdminService` (statistics, moderation), `ThrottleService` (rate limits),
`TwoFactorService`, `AuditService`, `MailService`, `RecoveryService` (password reset, email
verification), `SshKeyService` and `JobService` (background work). Services communicate
through an in-process `EventBus` of `DomainEvent`s (`RepositoryPushed` with the pusher and
the ref updates, repository created/deleted/transferred/forked, `ReleasePublished`,
`UserSignedUp`, `PasswordChanged`, `EmailChanged`), so producers never depend on reactions
such as cache warming, session revocation, verification email or webhooks. Both Git
transports report pushes through `PushService`, which diffs ref snapshots taken before and
after `receive-pack` (it runs without hooks). This is the only
layer that combines storage, git and auth. It is HTTP-agnostic so a future REST/GraphQL
API, CLI admin commands and background jobs all reuse it.

**`rustit-web`** — Axum 0.8 application:
```
rustit-web/src/
├── lib.rs              # build_router()
├── state.rs            # AppState (cheaply cloneable Arc)
├── error.rs            # WebError → status page / HTMX-friendly response
├── assets.rs           # embedded static files with content-hash cache busting
├── client_ip.rs        # ClientIp (trusted-proxy aware), Https detection
├── metrics.rs          # request metrics (Prometheus)
├── extract.rs          # Viewer, SignedIn, SiteAdmin, Page extractors
├── middleware.rs       # sessions, rate limiting, security headers, CSRF origin check
├── routes/             # one module per area; each feature exposes its own routes()
├── views/              # view models + Askama template structs
└── templates/          # *.html (Askama), organised by area
```

**`rustit-server`** — `clap` CLI (`serve`, `migrate`, `backup`, `user create`), layered
config (`figment`: defaults → TOML file → `RUSTIT_*` env), tracing and metrics setup, the
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
├── rustit.db            # SQLite (+ -wal, -shm)
├── rustit.redb          # redb key/value store
└── repositories/
    └── <owner_id>/<repo_id>.git   # bare repositories, addressed by immutable IDs
```

Repositories are addressed by numeric IDs, not names, so renaming a user or repository
is a single SQL update — no filesystem moves, no path-traversal surface.

### 3.3 SQLite schema

```
accounts           id, kind(user|organization), name UNIQUE NOCASE, display_name, bio, created_at, updated_at
user_logins        account_id → accounts, email UNIQUE NOCASE, password_hash, is_admin, suspended_at
account_profiles   account_id → accounts, website, location                       (users)
org_profiles       account_id → accounts, description, website                    (organisations)
org_members        org_id → accounts, user_id → accounts, role(owner|member)
repositories       id, owner_id → accounts, name NOCASE, description, visibility(public|private),
                   default_branch, created_at, updated_at, pushed_at, UNIQUE(owner_id, name)
stars              account_id → accounts, repo_id → repositories, created_at
access_tokens      id, account_id → accounts, name, token_digest BLOB UNIQUE, hint, scope(read|write),
                   created_at, last_used_at, expires_at
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
```

Migrations live in `crates/rustit-storage/migrations/` and are embedded in the binary
(`0001_initial` … `0008_ssh_keys`). Secrets (passwords, sessions, tokens, recovery codes,
email links) are only ever stored hashed.

Users and organisations share one namespace (`accounts`), exactly like GitHub URLs
(`/rustit` can be either). Timestamps are Unix seconds (`INTEGER`).

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

---

## 4. Request lifecycle

```
TCP → hyper → tower layers:
  TraceLayer → metrics::track (route template, status, latency)
→ Router
  ├── /assets/*, /sw.js, /manifest.webmanifest   immutable, content-hashed, embedded
  ├── /healthz, /readyz, /metrics                operations (metrics optional, token-gated)
  ├── /{owner}/{repo}.git/…                       smart HTTP (Basic auth or token; throttled; streaming)
  └── HTML routes
        Compression → Timeout → body limit (1 MiB) → SecurityHeaders → CsrfOriginCheck
        → rate_limit (unsafe requests, per client address) → session → error pages
        → extractors → handler → service → view model → Askama

SSH (separate listener, rustit-ssh): public-key auth → exec git-upload-pack/receive-pack
```

**HTMX conventions**
- `<body hx-boost="true">` turns every link and form into an AJAX swap with history support,
  giving SPA-like navigation with server-rendered HTML.
- Fragments (e.g. branch switcher list, file-tree expansion) are returned when the
  `HX-Request` header is present; full pages otherwise. The same handler serves both.
- `htmx.config.allowEval = false` and no inline scripts, so CSP can forbid `unsafe-inline`
  and `unsafe-eval` for scripts.

---

## 5. Security model

| Concern | Measure |
|---|---|
| Passwords | Argon2id (m = 19 MiB, t = 2, p = 1 — OWASP 2024), hashed on the blocking pool. |
| Sessions | 32 random bytes from the OS CSPRNG, base64url in an `HttpOnly; SameSite=Lax` cookie (`Secure` behind HTTPS). Only SHA-256 digest stored. Sliding expiry. |
| Access tokens | `rit_` + 256 random bits; only the SHA-256 digest is stored, plus a 4-character hint. Used as the Git HTTP password; scope (`read`/`write`) caps the owner's access, so a read token can clone but never push. Optional expiry; `last_used_at` is recorded at most hourly. Max 50 per user. |
| CSRF | Unsafe methods require `Sec-Fetch-Site: same-origin` or a matching `Origin` header. Combined with `SameSite=Lax` cookies. |
| XSS | Askama auto-escapes. Markdown is rendered with `pulldown-cmark` then sanitised with `ammonia`. Strict CSP: `default-src 'self'`. |
| Path traversal | `RepoName`/`Username` reject anything outside `[A-Za-z0-9._-]`, leading dots and reserved names. On-disk paths use numeric IDs only. |
| Git config injection | Repos opened with isolated gix options; transport processes run with a scrubbed environment and `GIT_CONFIG_NOSYSTEM=1`. |
| Enumeration | Private repositories return **404**, never 403, to unauthorised viewers. |
| Headers | `X-Content-Type-Options`, `Referrer-Policy`, `frame-ancestors 'none'`, `Permissions-Policy`. |
| Two-factor | TOTP (RFC 6238, ±1 step, each code accepted once), confirmed at enrolment; 10 single-use recovery codes stored hashed; disabling requires password + code. Enrolled accounts must use access tokens for Git over HTTP. |
| Brute force | `ThrottleService` budgets per scope (sign-in, sign-up, 2FA, Git auth, SSH auth, email requests) keyed by account and by client address (larger shared budget); failures cost extra, success resets; 429 with `Retry-After`. Form posts are limited per address. |
| Client address | `X-Forwarded-For` / `Forwarded` / `X-Forwarded-Proto` are honoured only from `[security] trusted_proxies`. |
| Sessions | Listed per account; revoke one or all others; all other sessions end when the password changes. |
| Recovery | Reset and verification links are single-use, expire (1 h / 2 days), stored as digests, invalidated by address changes; reset requests never reveal whether an account exists. |
| SSH | Public keys only; strictly parsed exec commands; no shell, PTY, agent or port forwarding; deploy keys scoped to one repository and read-only unless allowed. |
| Audit | Sign-ins, 2FA changes, session revocations, token changes, password changes and every administrator action are recorded in `audit_events` (`/admin/audit`). |

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
| `--canvas` / `--surface` | #ffffff / #ffffff | #000000 / #0a0a0a |
| `--subtle` / `--hover` | #fafafa / #f2f2f2 | #111111 / #1a1a1a |
| `--line` / `--line-strong` | #ebebeb / #d4d4d4 | #1f1f1f / #333333 |
| `--ink` / `--ink-muted` / `--ink-faint` | #0a0a0a / #666666 / #8f8f8f | #ededed / #a1a1a1 / #6e6e6e |
| `--inverted` (primary buttons) | #0a0a0a | #ededed |

Every token is defined once with CSS `light-dark()`, so components never need `dark:`
variants. The theme follows `prefers-color-scheme`; a toggle pins a choice (`data-theme`
on `<html>`, persisted in `localStorage`). `scrollbar-gutter: stable` keeps page width
identical whether or not a page scrolls.

**Components** (`styles/app.css`): `.box`, `.box-header`, `.row`, `.btn` (`-primary`,
`-ghost`, `-danger`, `-icon`), `.input`, `.label`, `.hint`, `.badge`, `.tab`, `.menu`,
`.code-view`, `.diff-view`, `.prose-rustit`. Icons and shared fragments are Askama macros in
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
- Planned: redb caches for last-commit-per-path and language statistics, invalidated by
  push events.

---

## 8. Extensibility

- **Trait seams** at every external boundary: `GitTransport`, store traits in services,
  `PasswordHasher`. Swapping an implementation never changes callers.
- **Event bus (planned)** — `services` emits domain events (`RepositoryPushed`,
  `UserSignedUp`) on a `tokio::broadcast` channel; webhooks, CI triggers, search indexing
  and cache invalidation subscribe independently.
- **Feature crates** — new areas (issues, pull requests, CI, packages) arrive as new
  crates (`rustit-issues`, …) contributing services and a `Router` that `rustit-web` merges.

---

## 9. Roadmap

| Phase | Scope |
|---|---|
| **1 — Foundation** ✓ | Accounts, sessions, repository create/browse, code/tree/blob/commits views, smart-HTTP clone & push, README rendering, syntax highlighting, PWA shell, light/dark theme. |
| **2 — Collaboration** ✓ | Commit diffs, branches/tags pages, personal access tokens, organisations with roles, stars, language breakdown, last commit per entry, user/repository settings, administration panel, runtime instance settings, suspension. |
| **M1 — Safe to self-host** ✓ | Rate limiting, trusted proxies, session management, TOTP two-factor, audit log, email (SMTP) with password reset and verification, SSH server with user and deploy keys, background jobs and maintenance, metrics and health checks, backups, Docker/systemd/CI, end-to-end and property tests. |
| M2 — A real forge | Collaborators and teams, issues, pull requests with review and merge strategies, branch protection, webhooks and a REST API, releases, archives, blame, forks and mirrors. |
| 3 — Workflow | Issues, labels, milestones, pull requests with merge strategies, code review. |
| 4 — Platform | Webhooks, REST API (+ OpenAPI), actions/CI runner protocol, package registry, full-text code search (tantivy). |
| 5 — Scale | Read replicas for repos, object storage for LFS, horizontal web tier. |
