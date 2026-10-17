# Milestone 1 — Safe to self-host

The working contract for hardening rbit for real use. Five workstreams are built in
parallel, each on its **own branch in its own git worktree**, each owning a disjoint set of
files. Shared registration points (services, stores, routes, views, crates, config
sections, CLI commands, navigation entries) are pre-wired on `main` as stubs.

## Workstreams

### S — Security
- **Rate limiting** (`ThrottleService`, contract below): sign-in, sign-up, 2FA codes, Git
  HTTP Basic auth, plus the per-IP `middleware::rate_limit` layer (already wired). Failed
  attempts cost extra; success resets. In-memory is fine (single binary). 429 responses
  carry `Retry-After` (`WebError::TooManyRequests` exists).
- **Client address**: `client_ip::ClientIp` honours `X-Forwarded-For`/`Forwarded` only from
  `[security] trusted_proxies` (CIDR list). Secure cookies when the trusted proxy says
  `X-Forwarded-Proto: https`.
- **Sessions**: account → sessions index in redb; `/settings/security` lists active sessions
  (created, last seen, client, current marker) with revoke-one and sign-out-everywhere.
  Revoke all other sessions on `DomainEvent::PasswordChanged`.
- **Two-factor (TOTP, RFC 6238)**: enrol with QR code (inline SVG) + manual secret, confirm
  with a code, 10 single-use recovery codes (shown once, stored hashed), disable with
  password + code. Sign-in second step for enrolled users (short-lived pending state).
  Replay protection (a code works once). With 2FA enabled, Git over HTTP refuses the account
  password and explains that a personal access token is required.
- **Audit log**: `audit_events` table; record sign-ins (success/failure), 2FA changes,
  session revocations, token creation/revocation and every administrator action. `/admin/audit`
  with filtering and pagination.

### E — Email & recovery
- `rbit-mail` crate: a `Mailer` trait with SMTP (lettre, rustls) and log transports;
  messages rendered from Askama templates (text + HTML).
- `[mail]` config: SMTP host/port/security (starttls, tls, none)/credentials, from address.
- Password reset (`/password/forgot`, `/password/reset/{token}`): one-hour single-use
  tokens stored as digests, identical response whether or not the address exists,
  throttled with `ThrottleScope::EmailRequest`. A completed reset publishes
  `PasswordChanged`.
- Email verification: `user_logins.email_verified_at`; verification mail on
  `UserSignedUp` and on `EmailChanged` (plus a notice to the previous address); resend from
  `/settings/account`; status shown in settings and in the admin user list.
- Admin: "Send password reset" and "Set password" actions for a user.
- With `transport = "disabled"`, every flow explains that email is unavailable.

### H — SSH
- `rbit-ssh` crate (russh): public-key auth only; Ed25519 host key generated on first
  start under the data directory; `git-upload-pack` / `git-receive-pack` exec requests
  (`'owner/repo.git'`, strictly parsed); bidirectional streaming to the git process
  (non-stateless mode — extend `rbit-transport`); the same authorisation as HTTP;
  `RepositoryPushed` on successful pushes; suspended users refused; `ThrottleScope::SshAuth`.
- SSH keys (`/settings/keys`) and deploy keys (`/{owner}/{repo}/settings/keys`, read-only or
  read-write): OpenSSH format, Ed25519/ECDSA/RSA ≥ 2048/security keys, SHA256 fingerprints,
  duplicates rejected, last use recorded.
- `[ssh]` config: public host/port for clone URLs; SSH clone URL in the Clone menu and the
  empty-repository instructions.

### O — Operations
- `JobService`: on `RepositoryPushed`, warm the language and last-commit caches for the
  default branch; periodic `git gc --auto` per repository; prune the insights cache;
  bounded concurrency; graceful shutdown.
- Metrics (`metrics::track`, wired): request counts by method/route/status, latency
  histograms, Git operations, job runs. `/metrics` in Prometheus format behind
  `[operations] metrics` and an optional bearer token. `/healthz` checks SQLite and redb
  (503 on failure).
- `rbit backup --output DIR`: SQLite `VACUUM INTO`, repository copies, a manifest; restore
  documented.
- Deployment: multi-stage `Dockerfile` (non-root, `git` installed, healthcheck),
  `.dockerignore`, hardened systemd unit, example config, `docs/deployment.md` (reverse
  proxy for Caddy/nginx, TLS, backups, upgrades).
- CI: `.github/workflows/ci.yml` — fmt check, clippy `-D warnings`, tests (with git), CSS
  build, release build.
- PWA: PNG icons (192, 512, maskable) generated from `icon.svg`, referenced by the manifest
  and `apple-touch-icon`.

### T — End-to-end and compatibility tests
- `rbit-e2e`: boot the full application on `127.0.0.1:0` with a temporary data directory;
  drive it with an HTTP client (cookies, `Origin` header) and the real `git` CLI.
- Flows: sign-up/first admin, sign-in/out, create/browse/rename/delete repositories,
  HTTP clone and push, private repositories invisible (web 404, Git 401/404), token scopes,
  organisation access, runtime registration toggle, suspension, stars.
- Property tests: name validation, pkt-line, diff hunk invariants, Markdown sanitisation,
  URL encoding.
- Bugs found in other areas are reported with a failing test marked `#[ignore = "…"]`.

## Contracts (stable APIs other workstreams rely on)

```rust
// rbit_services::throttle — S implements, E/H call.
pub enum ThrottleScope { SignIn, SignUp, TwoFactor, EmailRequest, GitAuth, SshAuth }
impl ThrottleService {
    pub fn check(&self, scope: ThrottleScope, key: &str) -> Result<(), ServiceError>; // RateLimited
    pub fn record_failure(&self, scope: ThrottleScope, key: &str);
    pub fn reset(&self, scope: ThrottleScope, key: &str);
}

// rbit_services::events — subscribe with `services.events.subscribe()`.
pub enum DomainEvent {
    RepositoryPushed { repo: RepoId },                      // HTTP today; SSH adds
    UserSignedUp { user: AccountId },
    PasswordChanged { user: AccountId },
    EmailChanged { user: AccountId, previous: String },
}

// Background subscribers start in `Services::spawn_background()`:
// `JobService::spawn_background` (O) and `RecoveryService::spawn_background` (E).
```

## Migrations (numbers reserved)

| File | Workstream |
|---|---|
| `0006_security.sql` | S — TOTP secrets, recovery codes, audit events |
| `0007_email.sql` | E — `email_verified_at`, email tokens |
| `0008_ssh_keys.sql` | H — SSH keys, deploy keys |

## File ownership

Stubs marked ★ already exist.

**S** `rbit-core/src/config/security.rs`, `rbit-auth/src/**` (new `totp.rs`, …),
`rbit-storage/src/kv/{sessions.rs,mod.rs}`, `rbit-storage/src/sqlite/{two_factor.rs★,audit.rs★,accounts.rs}`,
`rbit-services/src/{throttle.rs★,two_factor.rs★,audit.rs★,accounts.rs,admin.rs,tokens.rs}`,
`rbit-web/src/{client_ip.rs★,middleware.rs,extract.rs}`,
`rbit-web/src/routes/{security.rs★,audit.rs★,auth.rs,git_http.rs}`, `rbit-web/src/views/{security.rs★,auth.rs}`,
`templates/auth/**`, `templates/settings/security.html`, `templates/admin/audit.html`.

**E** `rbit-core/src/config/mail.rs`, `rbit-mail/**`★, `rbit-storage/src/sqlite/{recovery.rs★,profiles.rs}`,
`rbit-services/src/{mail.rs★,recovery.rs★,profiles.rs}`,
`rbit-web/src/routes/{recovery.rs★,settings.rs,admin.rs}`, `rbit-web/src/views/{recovery.rs★,settings.rs,admin.rs}`,
`templates/recovery/**`, `templates/settings/{account.html,profile.html}`, `templates/admin/{users.html,new_user.html}` (and new admin templates it adds).

**H** `rbit-core/src/config/ssh.rs`, `rbit-ssh/**`★, `rbit-transport/**`,
`rbit-storage/src/sqlite/ssh_keys.rs★`, `rbit-services/src/ssh_keys.rs★`,
`rbit-web/src/routes/{ssh_keys.rs★,repo.rs}`, `rbit-web/src/views/{ssh_keys.rs★,repo.rs}`,
`templates/settings/keys.html`, `templates/repo/{subnav.html,empty.html,settings.html,deploy_keys.html}`,
`rbit-server/src/ssh.rs★`.

**O** `rbit-core/src/config/operations.rs`, `rbit-git/src/{lib.rs,maintenance.rs}`,
`rbit-storage/src/kv/insights.rs`, `rbit-services/src/{jobs.rs★,repositories.rs,insights.rs}`,
`rbit-web/src/{metrics.rs★,routes/ops.rs★,routes/pwa.rs}`, `templates/base.html`,
`rbit-server/src/{backup.rs★,telemetry.rs}`, `Dockerfile`, `.dockerignore`, `deploy/**`,
`docs/deployment.md`, `.github/**`, `scripts/**`, `package.json`, `crates/rbit-web/assets/*.png`.

**T** `rbit-e2e/**`★ and new `tests/properties.rs` files in any crate (new files only).

**Integrator (not a workstream):** `rbit-services/src/{lib.rs,events.rs,error.rs}`,
`rbit-storage/src/{lib.rs,sqlite/mod.rs,sqlite/rows.rs}`, `rbit-web/src/{lib.rs,error.rs,routes/mod.rs,views/mod.rs}`,
`rbit-server/src/{main.rs,cli.rs,serve.rs}`, `rbit-core/src/config/mod.rs`, `styles/app.css`,
`templates/macros.html`, `docs/ARCHITECTURE.md`, `README.md`.

**Cargo.toml (workspace):** add dependencies only inside your workstream's labelled block.
A crate's own `Cargo.toml` belongs to its owner (shared crates: append only the lines you need).

Need a change in a file you don't own? Describe the exact change in your final report.

## Commit rules

- Work on your branch; commit early and often. **Each commit is atomic**: one coherent change
  that builds, is formatted and passes tests on its own (`cargo fmt --all`,
  `cargo clippy --workspace --all-targets`, `cargo test -p <crates you touched>`).
- Message format: imperative subject ≤ 72 characters, blank line, a body explaining *what and
  why* (wrapped at 72), ending with the trailer
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Typical sequence: migration + store → service + tests → routes/views/templates → docs.
- Never rewrite `main`, never merge other branches, never commit generated `app.css`,
  `target/` or `data/`.

## Engineering rules

Same as phase 2 (`docs/design/phase-2.md`): self-describing code with `//!` docs, validated
domain types, thin handlers → services → stores, clippy pedantic clean, no `unsafe`, no
`unwrap` outside tests, `'static` SQL, monochrome minimal design, strict CSP (no inline
styles/scripts), mobile-first, 422 for validation errors, 303 after success, 404 for things
the viewer may not know exist. Never add a `macros.html` inside a template subfolder.
