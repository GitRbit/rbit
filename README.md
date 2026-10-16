# Rustit

A fast, self-hosted Git platform written in Rust — one binary, no external services.

- **Axum + Askama + htmx**: compiled server-side templates, SPA-like navigation without a JS framework.
- **SQLite + redb + bare Git repositories**: relational data, hot key/value data, and Git's own storage format.
- **Secure by default**: Argon2id passwords, hashed session tokens, strict CSP, origin-checked forms.
- **Mobile first & installable** as a progressive web app.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design.

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
password. Tokens are scoped (`read` can clone and fetch, `write` can also push), can expire,
and can be revoked individually.

## Configuration

Settings come from built-in defaults, then an optional TOML file (`--config rustit.toml`),
then `RUSTIT_*` environment variables (nested keys use `__`):

```toml
[server]
bind = "0.0.0.0:3000"
public_url = "https://git.example.com"
site_name = "Rustit"
open_registration = true

[storage]
data_dir = "data"

[security]
session_lifetime_days = 30
min_password_length = 10

[repository]
default_branch = "main"
git_binary = "git"
```

```bash
RUSTIT_SERVER__PUBLIC_URL=https://git.example.com cargo run --release
```

## Command line

| Command | Purpose |
|---|---|
| `rustit serve` | Run the web server (default) |
| `rustit migrate` | Apply database migrations and exit |
| `rustit user create <name> <email> [--admin]` | Create an account (password read from stdin) |

## Development

```bash
npm run watch:css                      # rebuild CSS on template changes
cargo test --workspace                 # unit + integration tests
cargo clippy --workspace --all-targets # pedantic lints are enabled workspace-wide
```

## Workspace

| Crate | Responsibility |
|---|---|
| `rustit-core` | Domain types, validated names, permissions, configuration |
| `rustit-storage` | SQLite (sqlx) and redb persistence |
| `rustit-auth` | Password hashing, session tokens, access policy |
| `rustit-git` | Repository storage and read model (gitoxide) |
| `rustit-transport` | Git smart-HTTP protocol |
| `rustit-services` | Application use-cases |
| `rustit-web` | Routes, templates, assets |
| `rustit-server` | The `rustit` binary |
