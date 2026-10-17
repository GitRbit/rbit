<p align="center">
  <img src="crates/rbit-web/assets/logo.png" width="280" alt="rbit logo — an orange frog mascot in a gear">
</p>

# rbit

[![CI](https://github.com/rbit/rbit/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/rbit/rbit/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-informational)](#license)
[![Latest release](https://img.shields.io/github/v/release/rbit/rbit?include_prereleases&sort=semver)](https://github.com/rbit/rbit/releases)

A fast, self-hosted Git platform written in Rust — one binary, no external services.

- **Axum + Askama + htmx**: compiled server-side templates, SPA-like navigation without a JS framework.
- **SQLite + redb + bare Git repositories**: relational data, hot key/value data, and Git's own storage format.
- **Git over HTTPS and SSH**: smart HTTP (protocol v2) and a built-in SSH server with user and deploy keys.
- **Git LFS**: batch API, streamed transfers, file locking and `git-lfs-authenticate` over SSH; objects are stored
  once and shared by forks; the browser shows the stored file, not the pointer.
- **Secure by default**: Argon2id passwords, two-factor authentication with passkeys or TOTP, rate limiting, hashed session
  and token secrets, strict CSP, origin-checked forms, an audit log.
- **External sign-in**: OpenID Connect providers and GitHub, with PKCE and verified ID tokens; link providers to
  existing accounts or let them register new ones.
- **Organisations, stars, language statistics, commit diffs**, an administration panel with runtime settings.
- **Collaboration**: collaborators and organisation teams with read/write/admin permissions, repository transfer
  with redirects from old names.
- **Repository essentials**: releases with attached files, `.zip`/`.tar.gz` archives, blame, compare, forks,
  pull mirrors, README relative links and images.
- **Edit in the browser**: create, edit, rename, delete and upload files as one commit, directly or on a new
  branch with a pull request; branch protection applies.
- **Issues**: labels, milestones, assignees, `#12` / `@name` references, `fixes #12` in commits, search with
  GitHub-style filters, and a notification inbox — by email too if you want it, threaded per issue, with
  one-click unsubscribe.
- **Pull requests**: within a repository or across forks, line comments, reviews and review requests,
  mergeability checks, and merging by merge commit, squash or rebase.
- **Branch protection**: no force pushes or deletion, required pull requests, approvals and status checks,
  restricted pushers — enforced over HTTPS and SSH and for merges.
- **Commit statuses**: a GitHub-compatible status API for external CI, with checks shown on pull requests and
  commits.
- **Automation**: a versioned REST API (`/api/v1`, OpenAPI 3.1: repositories, issues, pull requests,
  collaborators, teams, releases with file uploads, forks, statuses, …) and HMAC-signed, GitHub-shaped webhooks
  (push, issues, pull requests, reviews, releases, collaborators, statuses, …) with a persistent, retrying
  delivery queue.
- **Themes**: built-in light and dark plus ~500 tinted-theming schemes, remembered per account.
- **Operable**: background maintenance, Prometheus metrics, health checks, consistent backups, Docker and systemd.
- **Mobile first & installable** as a progressive web app.

See [docs/deployment.md](docs/deployment.md) for running it in production,
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design, [ROADMAP.md](ROADMAP.md) for
what comes next, and [docs/HANDOFF.md](docs/HANDOFF.md) for the project's current state.

## Install

rbit needs **Git** on the server at run time (the transports, `git gc` and backups use the
`git` executable). Everything else is inside the binary: templates, styles, scripts and the
database migrations.

### Release binaries

Every [release](https://github.com/rbit/rbit/releases) has archives for Linux (x86_64 and
aarch64, glibc 2.35 or newer), macOS (Intel and Apple silicon) and Windows (x86_64), plus a
`SHA256SUMS` file. Each archive holds the `rbit` binary, the licenses, this README, the
changelog, the example configuration and the systemd unit.

```bash
version=0.1.0                       # the release you want
target=x86_64-unknown-linux-gnu     # or aarch64-unknown-linux-gnu, aarch64-apple-darwin, ...
curl -LO "https://github.com/rbit/rbit/releases/download/v$version/rbit-$version-$target.tar.gz"
curl -LO "https://github.com/rbit/rbit/releases/download/v$version/SHA256SUMS"
sha256sum --check --ignore-missing SHA256SUMS   # macOS: shasum -a 256 --check --ignore-missing
tar -xzf "rbit-$version-$target.tar.gz"
./rbit-$version-$target/rbit serve  # http://localhost:3000, data in ./data
```

For a real server (a system user, TLS in front, systemd, backups) follow
[docs/deployment.md](docs/deployment.md).

### Container image

Multi-architecture images (`linux/amd64`, `linux/arm64`) are published to the GitHub
Container Registry for every release, tagged with the version (`0.1.0`, `0.1`) and `latest`:

```bash
docker run -d --name rbit -p 3000:3000 -v rbit-data:/data \
  -e RBIT_SERVER__PUBLIC_URL=https://git.example.com \
  ghcr.io/rbit/rbit:latest
```

The image runs as an unprivileged user, keeps everything under `/data`, and has a health
check on `/healthz`. Add `-p 2222:2222 -e RBIT_SSH__ENABLED=true` for Git over SSH.

### From source

Requirements: Rust 1.90+, Git, and Node.js (build-time only, for Tailwind CSS).

```bash
npm ci
npm run build        # vendors htmx and builds the stylesheet
cargo run            # serves http://localhost:3000, data in ./data
```

`cargo build --release` produces `target/release/rbit`; `docker build -t rbit .` builds the
image from source.

## First steps

The first account you create becomes the site administrator.

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

Settings come from built-in defaults, then an optional TOML file (`--config rbit.toml`),
then `RBIT_*` environment variables (nested keys use `__`, e.g.
`RBIT_SERVER__PUBLIC_URL`). Every key is optional. A fully commented production example
lives in [deploy/rbit.example.toml](deploy/rbit.example.toml).

| Section | Purpose | Key settings |
|---|---|---|
| `[server]` | HTTP listener and identity | `bind`, `public_url`, `site_name`, `open_registration` (default for the runtime setting) |
| `[storage]` | Data location | `data_dir` (SQLite, redb, repositories, release files, LFS objects) |
| `[security]` | Sessions and abuse protection | `session_lifetime_days`, `min_password_length`, `trusted_proxies` (CIDRs allowed to set `X-Forwarded-*`), `rate_limit` |
| `[repository]` | Git behaviour | `default_branch`, `git_binary`, `max_render_bytes` |
| `[mail]` | Outgoing email | `transport` = `disabled` \| `log` \| `smtp`; `from_address`, `smtp_host`, `smtp_port`, `smtp_security` (`starttls` \| `tls` \| `none`), `smtp_username`, `smtp_password` |
| `[ssh]` | Built-in SSH server | `enabled`, `bind`, `public_host`, `public_port`, `host_key_path`, connection limits and timeouts |
| `[operations]` | Maintenance and metrics | `metrics`, `metrics_token`, `warm_caches_on_push`, `maintenance_interval_minutes`, `job_concurrency`, insights cache limits |
| `[api]` | REST API | `enabled`, `default_page_size`, `max_page_size` |
| `[webhooks]` | Outgoing deliveries | `enabled`, `allow_private_addresses`, `timeout_seconds`, `max_attempts`, `concurrency`, `delivery_retention_days` |
| `[releases]` | Release files (web and API uploads) | `max_asset_bytes`, `max_assets_per_release`, `upload_timeout_seconds` |
| `[mirrors]` | Pull mirrors | `enabled`, `default_interval_minutes`, `min_interval_minutes`, `fetch_timeout_seconds`, `allow_private_addresses` |
| `[issues]` | Issues and notifications | `scan_commit_limit`, `notifications`, `notification_retention_days` |
| `[pulls]` | Pull requests | `merge_timeout_seconds`, `max_concurrent_merges`, `mergeability_concurrency`, `max_commits`, `noreply_domain` |
| `[notifications]` | Email notifications on top of the inbox (need a `[mail]` transport and a verified address) | `email`, `default_email` (`off` \| `participating` \| `all`) |
| `[editor]` | Editing and uploading files in the browser | `max_upload_bytes`, `max_upload_files`, `upload_timeout_seconds` |
| `[lfs]` | Git Large File Storage | `enabled`, `max_object_bytes`, `token_lifetime_seconds` |
| `[auth]` | External sign-in; register `{public_url}/login/{id}/callback` at the provider | `timeout_seconds`, `allow_private_addresses`, `[[auth.providers]]` (`kind` = `oidc` \| `github`, `id`, `name`, `issuer` / `base_url`, `client_id`, `client_secret`, `scopes`, `allow_registration`, `trust_email`, `trust_mfa`) |

Site name, registration, who may create organisations and the default repository
visibility can also be changed at runtime by administrators under **Site administration →
Settings**.

## Command line

| Command | Purpose |
|---|---|
| `rbit serve` | Run the web (and SSH, if enabled) server — the default |
| `rbit migrate` | Apply database migrations and exit |
| `rbit backup --output DIR` | Write a consistent backup (database snapshot, repository mirrors, release files, LFS objects, manifest) |
| `rbit user create <name> <email> [--admin]` | Create an account (password read from stdin) |

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
[`crates/rbit-web/assets/logo.png`](crates/rbit-web/assets/logo.png) and is shared by
the README and landing page. After replacing it, run `npm run icons` to regenerate
the browser, home-screen, and installable-app icons, and commit the generated PNGs.

## Workspace

| Crate | Responsibility |
|---|---|
| `rbit-core` | Domain types, validated names, permissions, configuration |
| `rbit-storage` | SQLite (sqlx) and redb persistence |
| `rbit-auth` | Password hashing, session and token secrets, TOTP, passkeys (WebAuthn), OpenID Connect token checks, access policy |
| `rbit-git` | Repository storage, read model, diffs, blame, compare, archives, forks, mirrors, languages, maintenance (gitoxide) |
| `rbit-transport` | Git smart protocol: stateless (HTTP) and interactive (SSH) modes |
| `rbit-services` | Application use-cases, domain events, background jobs, webhook delivery |
| `rbit-mail` | Email rendering and delivery (SMTP, log) |
| `rbit-ssh` | Built-in SSH server for Git over SSH |
| `rbit-web` | Routes, REST API, templates, themes, assets, metrics |
| `rbit-server` | The `rbit` binary: CLI, configuration, backups |
| `rbit-e2e` | End-to-end and Git compatibility tests (test-only) |

## Community

- **Questions and ideas:** [discussions](https://github.com/rbit/rbit/discussions) and
  [feature requests](https://github.com/rbit/rbit/issues/new?template=feature_request.yml);
  [ROADMAP.md](ROADMAP.md) says what is planned and what is out of scope.
- **Bugs:** [open an issue](https://github.com/rbit/rbit/issues/new?template=bug_report.yml).
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
