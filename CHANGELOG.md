# Changelog

All notable changes to Rustit are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Milestone 1 — safe to self-host

The first release meant to run on the internet. It builds on the foundation (accounts,
repositories, browsing, smart HTTP, installable PWA) and the second phase (commit diffs,
branches and tags, access tokens, organisations, stars, language statistics, settings,
administration panel).

### Upgrade notes

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

### Added

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

### Security

- Session tokens are stored only as SHA-256 digests; a leaked database never yields usable
  cookies.
- Password-reset tokens (one hour) and email-verification tokens (two days) are single-use
  and stored as digests; the reset form answers identically whether or not an address
  exists.
