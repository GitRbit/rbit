# Changelog

All notable changes to Rustit are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Milestone 2 — wave A

The collaboration layer, first half: other people can work on a repository with exactly
the access they need, Rustit can be automated through a REST API and signed webhooks, and
repositories gain downloads, releases, blame, comparison, forks and mirrors. Issues, pull
requests and branch protection follow in wave B.

#### Upgrade notes

- Database migrations `0009_access` through `0016_repo_redirects` run automatically at
  start-up (or explicitly with `rustit migrate`, which now also works while the server is
  running). Take a backup first; migrations only move forward. They were tested against a
  milestone 1 database: every row is kept. `0012_token_scopes` rebuilds the
  `access_tokens` table to allow the new `admin` scope, copying every token with its id,
  so existing tokens keep working unchanged.
- **Nobody loses access.** Organisation members keep write access to every repository of
  their organisation: the new organisation base permission defaults to `write`, the
  milestone 1 behaviour. Owners can lower it to `read` or `none` under the organisation's
  settings and grant access through teams instead.
- **Nobody is signed out** by this upgrade.
- **Backups use format 2.** `rustit backup` now also copies release files into
  `attachments/` and writes `"format": 2` to `manifest.json`. When restoring, copy
  `attachments/` along with `rustit.db` and `repositories/` (see
  [docs/deployment.md](docs/deployment.md#backups)). Backups in format 1 restore as before.
- New data directory: `<data_dir>/attachments` holds release files (uploads in progress
  in `attachments/tmp`). It is created on demand; back it up with the rest of the data
  directory (the backup command does).
- New configuration sections `[api]`, `[webhooks]`, `[releases]` and `[mirrors]`; every key
  is optional and documented in [deploy/rustit.example.toml](deploy/rustit.example.toml).
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
  the metric `rustit_webhook_attempts_total{outcome}`.
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
  Explore, and the Rustit version in the footer.

#### Changed

- Members who create a repository in an organisation become its administrators (as a
  collaborator entry, which leaving the organisation removes). Repositories created
  before the upgrade keep their milestone 1 access.
- Repository deletion and transfer are recorded with the person who did it; changing an
  organisation's base permission is audited.
- `rustit user create` and `rustit migrate` no longer need the server to be stopped.
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
  at start-up (or explicitly with `rustit migrate`). Take a backup first; migrations only
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
- **Backups**: `rustit backup --output DIR` writes a consistent SQLite snapshot, a mirror of
  every repository and a manifest while the server keeps running. Restore is documented.
- **Deployment**: a multi-stage `Dockerfile` (non-root, `git` included, health check), a
  hardened systemd unit, a commented production configuration
  (`deploy/rustit.example.toml`) and [docs/deployment.md](docs/deployment.md) covering
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
