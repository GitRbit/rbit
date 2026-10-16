# Rustit

A fast, self-hosted Git platform written in Rust — one binary, no external services.

- **Axum + Askama + htmx**: compiled server-side templates, SPA-like navigation without a JS framework.
- **SQLite + redb + bare Git repositories**: relational data, hot key/value data, and Git's own storage format.
- **Git over HTTPS and SSH**: smart HTTP (protocol v2) and a built-in SSH server with user and deploy keys.
- **Secure by default**: Argon2id passwords, TOTP two-factor authentication, rate limiting, hashed session
  and token secrets, strict CSP, origin-checked forms, an audit log.
- **Organisations, stars, language statistics, commit diffs**, an administration panel with runtime settings.
- **Operable**: background maintenance, Prometheus metrics, health checks, consistent backups, Docker and systemd.
- **Mobile first & installable** as a progressive web app.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design,
[docs/HANDOFF.md](docs/HANDOFF.md) for the current state and next steps, and
[docs/deployment.md](docs/deployment.md) for running it in production.

## Quick start

Requirements: Rust 1.90+, Git, and Node.js (build-time only, for Tailwind CSS).

```bash
npm install
npm run build        # vendors htmx and builds the stylesheet
cargo run            # serves http://localhost:3000
```

The first account you create becomes the site administrator.

Clone and push over HTTPS with your username and password:

```bash
git clone http://localhost:3000/<owner>/<repo>.git
```

Instead of your account password you can use a personal access token (`rit_…`) as the Git
password — required once two-factor authentication is enabled. Tokens are scoped (`read`
can clone and fetch, `write` can also push), can expire, and can be revoked individually.

With `[ssh] enabled = true`, add a public key under **Settings → SSH keys** and use the SSH
URL from the repository's **Clone** menu.

## Configuration

Settings come from built-in defaults, then an optional TOML file (`--config rustit.toml`),
then `RUSTIT_*` environment variables (nested keys use `__`, e.g.
`RUSTIT_SERVER__PUBLIC_URL`). Every key is optional. A fully commented production example
lives in [deploy/rustit.example.toml](deploy/rustit.example.toml).

| Section | Purpose | Key settings |
|---|---|---|
| `[server]` | HTTP listener and identity | `bind`, `public_url`, `site_name`, `open_registration` (default for the runtime setting) |
| `[storage]` | Data location | `data_dir` (SQLite, redb, repositories) |
| `[security]` | Sessions and abuse protection | `session_lifetime_days`, `min_password_length`, `trusted_proxies` (CIDRs allowed to set `X-Forwarded-*`), `rate_limit` |
| `[repository]` | Git behaviour | `default_branch`, `git_binary`, `max_render_bytes` |
| `[mail]` | Outgoing email | `transport` = `disabled` \| `log` \| `smtp`; `from_address`, `smtp_host`, `smtp_port`, `smtp_security` (`starttls` \| `tls` \| `none`), `smtp_username`, `smtp_password` |
| `[ssh]` | Built-in SSH server | `enabled`, `bind`, `public_host`, `public_port`, `host_key_path`, connection limits and timeouts |
| `[operations]` | Maintenance and metrics | `metrics`, `metrics_token`, `warm_caches_on_push`, `maintenance_interval_minutes`, `job_concurrency`, insights cache limits |

Site name, registration, who may create organisations and the default repository
visibility can also be changed at runtime by administrators under **Site administration →
Settings**.

## Command line

| Command | Purpose |
|---|---|
| `rustit serve` | Run the web (and SSH, if enabled) server — the default |
| `rustit migrate` | Apply database migrations and exit |
| `rustit backup --output DIR` | Write a consistent backup (database snapshot, repository mirrors, manifest) |
| `rustit user create <name> <email> [--admin]` | Create an account (password read from stdin) |

## Development

```bash
npm run watch:css                      # rebuild CSS on template changes
cargo test --workspace                 # unit, integration, property and end-to-end tests
cargo clippy --workspace --all-targets # pedantic lints are enabled workspace-wide
cargo fmt --all                        # rustfmt.toml pins the style
```

The end-to-end suite (`crates/rustit-e2e`) boots real servers on ephemeral ports and drives
them with an HTTP client and the `git` CLI; the SSH tests also need the OpenSSH client.

## Workspace

| Crate | Responsibility |
|---|---|
| `rustit-core` | Domain types, validated names, permissions, configuration |
| `rustit-storage` | SQLite (sqlx) and redb persistence |
| `rustit-auth` | Password hashing, session and token secrets, TOTP, access policy |
| `rustit-git` | Repository storage, read model, diffs, languages, maintenance (gitoxide) |
| `rustit-transport` | Git smart protocol: stateless (HTTP) and interactive (SSH) modes |
| `rustit-services` | Application use-cases, domain events, background jobs |
| `rustit-mail` | Email rendering and delivery (SMTP, log) |
| `rustit-ssh` | Built-in SSH server for Git over SSH |
| `rustit-web` | Routes, templates, assets, metrics |
| `rustit-server` | The `rustit` binary: CLI, configuration, backups |
| `rustit-e2e` | End-to-end and Git compatibility tests (test-only) |
