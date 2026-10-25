<p align="center">
  <img src="docs/logo.avif" width="280" height="281" alt="rbit logo — an orange frog mascot in a gear">
</p>

# rbit

[![CI](https://github.com/GitRbit/rbit/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/GitRbit/rbit/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-informational)](#license)
[![Latest release](https://img.shields.io/github/v/release/GitRbit/rbit?include_prereleases&sort=semver)](https://github.com/GitRbit/rbit/releases)

**Blazing fast, self-hosted Git hosting — written in Rust, shipped as one binary, with no external services.**

rbit is a Git forge that gives you repositories, issues, pull requests, reviews and CI checks on your
own server. It is built around three things: **speed**, **security** and **performance**. No Postgres,
no Redis, no JavaScript runtime — download it, run it, push.

**[Quick start](#quick-start)** · [Why rbit](#why-rbit) · [Features](#features) · [Install](#install) ·
[First steps](#first-steps) · [Configuration](#configuration) · [Command line](#command-line) ·
[Development](#development) · [Community](#community)

<p align="center">
  <img src="docs/rbit_preview1.avif" width="880" alt="rbit preview 1">
</p>
<p align="center">
  <img src="docs/rbit_preview2.avif" width="880" alt="rbit preview 2">
</p>

## Quick start

```bash
docker run -d --name rbit -p 3000:3000 -v rbit-data:/data ghcr.io/gitrbit/rbit:latest
```

Retrieve the one-time setup token with `docker logs rbit`, then open
<http://localhost:3000> and sign up with it — **the first account becomes the site administrator**.
Create a repository (or import one from GitHub or Gitea), then push:

```bash
git remote add rbit http://localhost:3000/<you>/<repo>.git
git push rbit main
```

No Docker? Download a [release binary](#release-binaries) and run `./rbit serve`, or build
[from source](#from-source). For a production server, follow [docs/deployment.md](docs/deployment.md).

## Why rbit

### Speed

- **Rust from request to response**: Axum, compiled Askama templates (no runtime template parsing), and
  gitoxide reading repositories in-process.
- **Server-rendered, SPA-fast**: htmx swaps only the part of the page that changes — SPA-like navigation
  without a JavaScript framework, and pages still work with JavaScript disabled.
- **Fewer asset requests**: initial styles arrive with the HTML, and browser modules ship as one bundle.
  External scripts and styles have content hashes and immutable caching, with Brotli compression in release builds.

### Security

- **Secure by default**: Argon2id passwords, two-factor authentication with passkeys or TOTP, rate limiting,
  hashed session and token secrets, strict CSP, origin-checked forms, an audit log.
- **Least privilege everywhere**: scoped, expiring, revocable tokens; branch protection enforced over HTTPS,
  SSH and merges; an SSH server that speaks Git and nothing else — no shell, no port forwarding.
- **Safe outbound requests**: webhooks, mirrors, imports and sign-in reach only public addresses unless you
  allow otherwise, with DNS pinned to the address that was checked.

### Performance

- **One process, zero services**: SQLite + redb + bare Git repositories — relational data, hot key/value
  data, and Git's own storage format, all in one data directory.
- **Bounded work by design**: paged listings, capped workers and budgeted caches, so a huge repository or a
  20,000-line diff doesn't take the rest of the server down with it.
- **Built lean**: fat LTO, a single codegen unit and mimalloc — small enough for a small server, fast enough
  for a team.

## Features

The footer and administrator dashboard show a notice when a newer stable rbit release
is available. A background check contacts the public GitHub release feed without sending
your running version, instance address, users or repository data. Successful results are
cached in memory for 24 hours; failures retry with backoff and never delay page requests.
Disable checks with `RBIT_UPDATES__CHECK_FOR_UPDATES=false`. Independently, set
`RBIT_UPDATES__REPORT_VERSION=false` to hide the public version API/footer/SSH version and
omit running-version numbers from outgoing integration User-Agents. Administrators retain
local version diagnostics. Both options are also in `[updates]` and the administrator
configuration editor; changes require a restart. Update notices link to release notes;
they do not install or download an update automatically.

**Git and code**

- **Git over HTTPS and SSH**: smart HTTP (protocol v2) and a built-in SSH server with user and deploy keys.
- **Git LFS**: batch API, streamed transfers, file locking and `git-lfs-authenticate` over SSH; objects are stored
  once and shared by forks; the browser shows the stored file, not the pointer.
- **Repository essentials**: releases with attached files, `.zip`/`.tar.gz` archives, blame, compare, forks,
  pull and push mirrors, README relative links and images.
- **Repository documents**: sticky document tabs, a searchable heading outline, and sidebar shortcuts
  for READMEs, licenses, contributing guides, codes of conduct and security policies.
- **Mermaid diagrams**: fenced `mermaid` blocks render in READMEs and other Markdown views,
  with an expandable source and a readable fallback for invalid diagrams.
- **Import from GitHub, GitHub Enterprise and Gitea/Forgejo**: history, LFS objects, issues and pull requests
  with their original numbers, labels, milestones, comments and releases; the token is never stored and
  imported history sends no notifications or webhooks.
- **Edit in the browser**: create, edit, rename, delete and upload files as one commit, directly or on a new
  branch with a pull request; branch protection applies.
- **Organisations, stars, language statistics, commit diffs**, an administration panel with runtime settings.
- **Contribution heatmaps** on user and organisation profiles and repository Activity tabs, with year
  selection, daily counts, monthly activity and streaks. Phone layouts show quarters without sideways
  scrolling. Counts use default-branch commits and UTC author dates, and include only repositories the
  viewer can access. User graphs match the account email; identical commits across repositories count
  once. Scans exceeding 100,000 commits per repository are marked as partial.

**Collaboration**

- **Issues**: labels, milestones, assignees, `#12` / `@name` references, `fixes #12` in commits, search with
  GitHub-style filters, and a notification inbox — by email too if you want it, threaded per issue or
  collected into a daily or weekly digest, with one-click unsubscribe.
- **Pull requests**: within a repository or across forks, line comments with resolvable conversations,
  reviews and review requests, mergeability checks, a base-branch picker for existing pull requests,
  and merging by merge commit, squash or rebase.
- **Branch protection**: no force pushes or deletion, required pull requests, approvals and status checks,
  restricted pushers — enforced over HTTPS and SSH and for merges.
- **Access**: collaborators and organisation teams with read/write/admin permissions, repository transfer
  with redirects from old names.
- **Archived repositories**: administrators archive a finished project from its settings (or the API's
  `archived` field) to make it read-only. It can still be read, cloned, forked and starred, but pushes over
  HTTPS and SSH, merges, browser edits, issues, pull requests, comments, reviews, labels, milestones,
  releases, commit statuses and LFS uploads and locks are refused, and its pull and push mirrors pause.
  Lists mark archived repositories and Explore can hide or show only them; unarchiving restores everything.

Change your username in **Settings → Account** using your current password. Your profile
links, repository URLs and Git HTTP/SSH remotes continue to work under old account names
until someone claims a name again. Update shared links and Git remotes after renaming;
use your new username or unchanged email to sign in. Sessions, tokens, keys and repository
permissions keep their account identity.

**Automation and integrations**

- **REST API and webhooks**: a versioned REST API (`/api/v1`, OpenAPI 3.1: repositories, issues, pull requests,
  collaborators, teams, releases with file uploads, forks, statuses, …) and HMAC-signed, GitHub-shaped webhooks
  (push, issues, pull requests, reviews, releases, collaborators, statuses, …) with a persistent, retrying
  delivery queue.
- **Commit statuses**: a GitHub-compatible status API for external CI, with checks shown on pull requests and
  commits.
- **Actions** (opt-in): GitHub-style workflows under `.rbit/workflows` (`.github/workflows` work too) run on
  **rbit-runner**, a separate program, in Docker/Podman containers or on the host — with built-in checkout,
  cache and artifacts, sealed secrets, fork-safe pull request runs, live logs and commit statuses. See
  [docs/actions.md](docs/actions.md).
- **External sign-in**: OpenID Connect providers and GitHub, with PKCE and verified ID tokens; link providers to
  existing accounts or let them register new ones.

**Running it**

- **Operable**: background maintenance, Prometheus metrics, health checks, consistent backups, Docker and systemd.
- **Mobile first & installable** as a progressive web app.
- **Themes**: built-in light and dark plus ~500 tinted-theming schemes, remembered per account.

See [docs/deployment.md](docs/deployment.md) for running it in production,
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design, [ROADMAP.md](ROADMAP.md) for
what comes next, and [docs/HANDOFF.md](docs/HANDOFF.md) for the project's current state.

## Install

rbit needs **Git** on the server at run time (the transports, `git gc` and backups use the
`git` executable). Everything else is inside the binary: templates, styles, scripts and the
database migrations.

### Release binaries

Every [release](https://github.com/GitRbit/rbit/releases) has archives for Linux (x86_64 and
aarch64, glibc 2.35 or newer), macOS (Intel and Apple silicon) and Windows (x86_64, `.zip`), plus a
`SHA256SUMS` file. Each archive holds the `rbit` binary and the `rbit-runner` binary (the Actions
runner, which you install on build machines instead), the licenses, this README, the changelog,
the third-party notices, the example configurations and the systemd units.

```bash
version=0.1.5                       # the release you want
target=x86_64-unknown-linux-gnu     # or aarch64-unknown-linux-gnu, aarch64-apple-darwin, ...
curl -LO "https://github.com/GitRbit/rbit/releases/download/v$version/rbit-$version-$target.tar.gz"
curl -LO "https://github.com/GitRbit/rbit/releases/download/v$version/SHA256SUMS"
sha256sum --check --ignore-missing SHA256SUMS   # macOS: shasum -a 256 --check --ignore-missing
tar -xzf "rbit-$version-$target.tar.gz"
./rbit-$version-$target/rbit serve  # http://localhost:3000, data in ./data
```

For a real server (a system user, TLS in front, systemd, backups) follow
[docs/deployment.md](docs/deployment.md).

### Container image

Multi-architecture images (`linux/amd64`, `linux/arm64`) are published to the GitHub
Container Registry for every release, tagged with the version (`0.1.5`, `0.1`) and `latest`:

```bash
docker run -d --name rbit -p 3000:3000 -v rbit-data:/data \
  -e RBIT_SERVER__PUBLIC_URL=https://git.example.com \
  ghcr.io/gitrbit/rbit:latest
```

The image runs as an unprivileged user, keeps everything under `/data`, and has a health
check on `/healthz`. Add `-p 2222:2222 -e RBIT_SSH__ENABLED=true` for Git over SSH.

### From source

Requirements: Rust 1.94+, Git, and Node.js (build-time only, for Tailwind CSS).

```bash
npm ci
npm run build        # vendors htmx and builds the stylesheet
cargo run            # serves http://localhost:3000, data in ./data
```

`cargo build --release` produces `target/release/rbit`; `docker build -t rbit .` builds the
image from source.

## First steps

The first account you create becomes the site administrator. While there is no account,
the server logs a one-time setup token at start-up that the sign-up form asks for (or create
the administrator with `rbit user create <name> <email> --admin`).

Clone and push over HTTPS with your username and password:

```bash
git clone http://localhost:3000/<owner>/<repo>.git
```

Instead of your account password you can use a personal access token (`rbit_…`) as the Git
password — required once two-factor authentication is enabled. Tokens are scoped (`read`
can clone and fetch, `write` can also push, `admin` can also change repository settings,
webhooks and delete through the API), can expire, and can be revoked individually. Tokens
issued by earlier releases start with `rit_…` and keep working.

With `[ssh] enabled = true`, add a public key under **Settings → SSH keys** and use the SSH
URL from the repository's **Clone** menu.

[Git LFS](https://git-lfs.com) needs no setup: `git lfs track` and push as usual, over
HTTPS or SSH, with the same credentials.

The same tokens authenticate the REST API:

```bash
curl -H "Authorization: Bearer rbit_…" http://localhost:3000/api/v1/user
```

`GET /api/v1/openapi.json` describes every endpoint.

## Configuration

Administrators can personalize a self-hosted instance in **Site administration → Settings**:
name, logo, favicon, app icon, primary button colors, site description, welcome copy, footer,
support/terms/privacy links, attribution, version visibility and welcome-page features.
Registration, organization creation and default repository visibility are also editable there.
These settings are validated, audited, stored in SQLite and apply immediately. Use HTTPS image
URLs or local `/paths` served by your reverse proxy; no asset rebuild is required.

**Site administration → Configuration** organizes deployment settings into labeled forms for
SMTP, SSH, security, SSO, integrations and feature limits. Each section has help text, toggles,
dropdowns and numeric limits. Save individual sections, then restart the server. Identity providers
have add/remove cards; add the providers first, then enter credentials and save. No JSON editing
or JavaScript is required.

Credential inputs are always empty: leave them blank to preserve existing secrets, enter a
replacement, or explicitly select Clear. Validation errors retain ordinary input, while new
credentials must be entered again. Protect the instance database and its backups, which contain
these credentials.
Storage relocation requires an offline move and a file/environment change.

Precedence is defaults → TOML → saved administrator deployment settings → legacy `RUSTIT_*` →
`RBIT_*`. Live branding and policy overrides take precedence over their deployment defaults.
Remove saved deployment overrides in the configuration screen to return to file/environment
configuration on restart. For recovery, start with `RBIT_IGNORE_ADMIN_CONFIG=1`, correct or remove
the saved overrides, then remove that environment variable and restart.


Settings come from built-in defaults, then an optional TOML file (`--config rbit.toml`),
then `RBIT_*` environment variables (nested keys use `__`, e.g.
`RBIT_SERVER__PUBLIC_URL`). Every key is optional. A fully commented production example
lives in [deploy/rbit.example.toml](deploy/rbit.example.toml).

| Section | Purpose | Key settings |
|---|---|---|
| `[server]` | HTTP listener and identity | `bind`, `public_url`, `site_name`, `open_registration` (default for the runtime setting) |
| `[storage]` | Data location | `data_dir` (SQLite, redb, repositories, release files, LFS objects) |
| `[security]` | Sessions and abuse protection | `session_lifetime_days`, `session_max_age_days`, `min_password_length`, `trusted_proxies` (CIDRs allowed to set `X-Forwarded-*`), `rate_limit`, `setup_token`, `hsts` / `hsts_include_subdomains` (with an `https://` public URL), `external_images` (false keeps viewers from loading images on other sites) |
| `[repository]` | Git behaviour | `default_branch`, `git_binary`, `max_render_bytes`, Git transport limits (`max_git_processes`, `max_git_requests_per_ip`, `git_idle_timeout_seconds`, `git_max_seconds`, `max_push_bytes`), archive download limits (`max_archives_per_ip`, `archive_idle_timeout_seconds`, `archive_max_seconds`) |
| `[mail]` | Outgoing email | `transport` = `disabled` \| `log` \| `smtp`; `from_address`, `from_name`, `smtp_host`, `smtp_port`, `smtp_security` (`starttls` \| `tls` \| `none`), `smtp_username`, `smtp_password`, `timeout_seconds` |
| `[ssh]` | Built-in SSH server | `enabled`, `bind`, `public_host`, `public_port`, `host_key_path`, connection limits and timeouts |
| `[operations]` | Maintenance and metrics | `metrics`, `metrics_token`, `warm_caches_on_push`, `maintenance_interval_minutes`, `job_concurrency`, insights cache limits |
| `[api]` | REST API | `enabled`, `default_page_size`, `max_page_size` |
| `[webhooks]` | Outgoing deliveries | `enabled`, `allow_private_addresses`, `timeout_seconds`, `max_attempts`, `concurrency`, `delivery_retention_days` |
| `[releases]` | Release files (web and API uploads) | `max_asset_bytes`, `max_assets_per_release`, `upload_timeout_seconds` |
| `[mirrors]` | Pull mirrors | `enabled`, `default_interval_minutes`, `min_interval_minutes`, `fetch_timeout_seconds`, `allow_private_addresses` |
| `[push_mirrors]` | Push mirrors | `enabled`, `interval_minutes`, `debounce_seconds`, `push_timeout_seconds`, `allow_private_addresses`, `allow_local_urls` |
| `[imports]` | Importing from GitHub and Gitea/Forgejo | `enabled`, `max_concurrent`, `git_timeout_seconds`, `api_timeout_seconds`, `rate_limit_max_wait_seconds`, `author_mapping` (`none` \| `email` \| `email_or_username`), `allow_private_addresses` |
| `[issues]` | Issues and notifications | `scan_commit_limit`, `notifications`, `notification_retention_days` |
| `[pulls]` | Pull requests | `merge_timeout_seconds`, `max_concurrent_merges`, `mergeability_concurrency`, `max_commits`, `noreply_domain` |
| `[notifications]` | Email notifications on top of the inbox (need a `[mail]` transport and a verified address) | `email`, `default_email` (`off` \| `participating` \| `all`), `digest_hour` (0–23 UTC, when daily and weekly digests are sent) |
| `[editor]` | Editing and uploading files in the browser | `max_upload_bytes`, `max_upload_files`, `upload_timeout_seconds` |
| `[lfs]` | Git Large File Storage | `enabled`, `max_object_bytes`, `token_lifetime_seconds` |
| `[auth]` | External sign-in; register `{public_url}/login/{id}/callback` at the provider | `timeout_seconds`, `allow_private_addresses`, `[[auth.providers]]` (`kind` = `oidc` \| `github`, `id`, `name`, `issuer` / `base_url`, `client_id`, `client_secret`, `scopes`, `allow_registration`, `trust_email`, `trust_mfa`) |

Site name, registration, who may create organisations, the default repository
visibility and the default email digest hour can also be changed at runtime by
administrators under **Site administration → Settings**.

## Command line

| Command | Purpose |
|---|---|
| `rbit serve` | Run the web (and SSH, if enabled) server — the default |
| `rbit migrate` | Apply database migrations and exit |
| `rbit backup --output DIR` | Write a consistent backup (database snapshot, repository mirrors, release files, LFS objects, manifest) |
| `rbit user create <name> <email> [--admin]` | Create an account (password read from stdin) |

Every command accepts `-c, --config <FILE>` (or `RBIT_CONFIG`); `rbit --version` prints the version.
Log verbosity follows `RUST_LOG` (e.g. `RUST_LOG=debug`), and `NO_COLOR` turns off coloured logs.

## Development

[CONTRIBUTING.md](CONTRIBUTING.md) explains the setup, the live development server
(`npm run dev`), the quality gate and the rules for commits and pull requests. In short:

```bash
npm run dev                            # rebuild and restart on every change, data in ./data-live
cargo fmt --all --check                # rustfmt.toml pins the style
cargo clippy --workspace --all-targets -- -D warnings   # pedantic lints, workspace-wide
cargo test --workspace                 # unit, integration, property and end-to-end tests
```

The end-to-end suite (`crates/rbit-e2e`) boots real servers on ephemeral ports and drives
them with an HTTP client and the `git` CLI; the SSH tests also need the OpenSSH client.

The orange frog is rbit's logo and mascot. The original artwork lives in
[`scripts/icons/logo.png`](scripts/icons/logo.png), outside the embedded assets so the
binary does not carry it. After replacing it, run `npm run icons` to regenerate
the browser, home-screen, and installable-app icons, and commit the generated PNGs.

## Workspace

| Crate | Responsibility |
|---|---|
| `rbit-core` | Domain types, validated names, permissions, configuration |
| `rbit-storage` | SQLite (sqlx) and redb persistence |
| `rbit-auth` | Password hashing, session and token secrets, TOTP, passkeys (WebAuthn), OpenID Connect token checks, access policy |
| `rbit-git` | Repository storage, read model, diffs, blame, compare, archives, forks, mirrors, languages, maintenance (gitoxide) |
| `rbit-transport` | Git smart protocol: stateless (HTTP) and interactive (SSH) modes |
| `rbit-services` | Application use-cases, domain events, background jobs, webhook delivery, imports |
| `rbit-mail` | Email rendering and delivery (SMTP, log) |
| `rbit-ssh` | Built-in SSH server for Git over SSH |
| `rbit-web` | Routes, REST API, templates, themes, assets, metrics |
| `rbit-server` | The `rbit` binary: CLI, configuration, backups |
| `rbit-e2e` | End-to-end and Git compatibility tests (test-only) |

## Community

- **Questions and ideas:** [discussions](https://github.com/GitRbit/rbit/discussions) and
  [feature requests](https://github.com/GitRbit/rbit/issues/new?template=feature_request.yml);
  [ROADMAP.md](ROADMAP.md) says what is planned and what is out of scope.
- **Bugs:** [open an issue](https://github.com/GitRbit/rbit/issues/new?template=bug_report.yml).
- **Contributing:** [CONTRIBUTING.md](CONTRIBUTING.md). Everyone taking part follows the
  [Code of Conduct](CODE_OF_CONDUCT.md).
- **Security:** please report vulnerabilities privately, as described in
  [SECURITY.md](SECURITY.md) — never in a public issue.

## License

rbit is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option. Bundled third-party material (htmx, the tinted-theming schemes, ...) keeps
its own license; see [THIRD-PARTY.md](THIRD-PARTY.md).

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in rbit by you, as defined in the Apache-2.0 license, shall be dual licensed as
above, without any additional terms or conditions.
