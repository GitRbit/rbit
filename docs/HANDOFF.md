# Rustit — Handoff

**As of 2026-10-16, `main` @ `208ee4f` (plus the documentation and upgrade-test commits
on top).** Everything below is what the next person (or agent team) needs to pick this up
without re-reading the history.

## 1. Where things stand

| Area | Status |
|---|---|
| Foundation (accounts, repositories, browsing, smart HTTP, PWA shell) | ✅ Done |
| Phase 2 (commit diffs, branches/tags, tokens, orgs, stars, languages, settings, admin panel) | ✅ Done |
| **Milestone 1 — safe to self-host** (rate limiting, 2FA, sessions, audit log, email recovery, SSH, jobs, metrics, backups, deployment, CI, e2e tests) | ✅ Done, merged |
| Frontend wave (landing page and dashboard, live explore search, theme gallery with ~500 schemes, stored preferences) | ✅ Done, merged |
| **Milestone 2 wave A** — access (collaborators, teams, base permission, transfer, old-name redirects), REST API `/api/v1` + signed webhooks, repository essentials (README links, archives, blame, compare, releases with files, forks, pull mirrors) | ✅ Done, merged and integrated |
| Milestone 2 wave B — issues, pull requests, branch protection | ⏭️ Next — contract in [design/milestone-2b.md](design/milestone-2b.md), see §5 |

Quality gate on `main`, exactly as CI runs it:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # 618 tests, 0 failing, 0 ignored
npm ci && npm run build         # vendored htmx + Tailwind CSS
```

Verified by hand: a copy of a real milestone 1 database (migrations `0001`–`0008`, with
users, an organisation, repositories, an SSH key and audit entries) migrated with
`rustit migrate` through `0016` — integrity and foreign-key checks clean, organisation
base permission `write`, all triggers present — and the server then served the
repositories, the organisation, releases, archives and the API from it.
`crates/rustit-storage/tests/upgrade.rs` repeats that upgrade on every test run. Earlier
(milestone 1): SSH key added in the UI then `ssh -T`/clone/push/re-clone with OpenSSH,
password-reset email (log transport), security settings, audit log, `/healthz`,
`/readyz`, `/metrics`; the Docker image and the systemd unit were run (see `5635638`).
The PWA service worker was verified in real Edge (the in-app browser pane cannot register
service workers — ignore failures there).

## 2. Open items (do these first)

1. **CI has never run on GitHub:** nothing is pushed anywhere and there is no remote
   configured yet. Push a branch to run `.github/workflows/ci.yml` (Linux + Windows).
2. **Release notes:** [CHANGELOG.md](../CHANGELOG.md) has the wave A upgrade notes
   (migrations `0009`–`0016`, backup format 2, the new outbound requests, proxy body
   limits for uploads, the newly reserved names `themes` and `teams`) and still carries
   milestone 1's one-time sign-out (`sessions_v2`) for instances upgrading from before it.
3. Wave A left API endpoints for collaborators, teams, releases and forks out on purpose;
   wave B moved them to a later wave (see the contract's out-of-scope list).

Known, accepted limitations (documented, not bugs): tags that point at trees/blobs are
not listed; URL revisions accept names and ids but not expressions like `v1.0~1`;
anonymous `/admin` redirects to sign-in rather than 404; the smart-HTTP and SSH
transports shell out to the `git` binary (a native gitoxide transport is possible but a
large, separate project — see the discussion summary in §6).

## 3. Running it

```bash
npm install && npm run build
cargo run                              # http://localhost:3000, data in ./data (gitignored)
```

The first account created becomes the site administrator. Useful development switches
(environment variables override `rustit.toml`):

```bash
RUSTIT_MAIL__TRANSPORT=log             # print emails (reset/verify links) to the log
RUSTIT_SSH__ENABLED=true RUSTIT_SSH__PUBLIC_HOST=localhost RUSTIT_SSH__PUBLIC_PORT=2222
RUSTIT_OPERATIONS__METRICS=true        # expose /metrics
RUSTIT_WEBHOOKS__ALLOW_PRIVATE_ADDRESSES=true   # deliver webhooks to a local listener
RUSTIT_MIRRORS__ALLOW_PRIVATE_ADDRESSES=true    # mirror from a local Git server
```

Create accounts from the CLI with `rustit user create <name> <email> [--admin]` (password
on stdin). Production setup: [deployment.md](deployment.md) and
[`deploy/rustit.example.toml`](../deploy/rustit.example.toml).

## 4. How this codebase is worked on

**Read first:** [ARCHITECTURE.md](ARCHITECTURE.md) (design, schema, routes, events, API
and webhooks, security model, design system), then [design/milestone-2.md](design/milestone-2.md)
(wave A — its rules section holds the house rules) and
[design/milestone-2b.md](design/milestone-2b.md) (wave B, the next contract).

**Standards (non-negotiable)**
- Layering: `web → services → (storage, git, auth, mail) → core`. Handlers are thin;
  business rules live in services; SQL lives in stores; templates contain no logic.
- Self-describing code, `//!` module docs, validated domain types, clippy **pedantic**
  clean, `unsafe` forbidden, no `unwrap` outside tests, `cargo fmt` (see `rustfmt.toml`).
- Cross-cutting reactions go through the `EventBus` (`DomainEvent`), not direct calls.
- Security defaults: secrets stored only as digests (webhook secrets are the one
  documented exception — HMAC needs the raw value), 404 for things a viewer may not know
  exist, strict CSP (no inline `style=`/scripts — use SVG attributes for proportional
  widths), Origin-checked forms, throttle anything that verifies a secret.
- **Design: monochrome minimal** — the user rejected an earlier colourful design. Reuse
  `.box`, `.row`, `.btn` (`-primary`/`-ghost`/`-danger`), `.input`, `.badge`, `.tab`;
  colour only for status, diffs, syntax and the language bar. Mobile-first, check 375 px.
- **Commits are atomic**: each builds, is formatted, lint-clean and tested on its own;
  imperative subject ≤ 72 chars, body explains what and why, trailer
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. History on `main` is linear.

**The multi-agent workflow that worked (use it again)**
1. On `main`, the integrator pre-wires every shared registration point as inert stubs
   (service fields, store accessors, `routes()` merges, view modules, config sections,
   nav entries, CLI commands, migration numbers, per-workstream `Cargo.toml` blocks) and
   writes `docs/design/<milestone>.md` with scope, contracts, file ownership and rules.
   Commit it.
2. Launch one agent per workstream in its **own git worktree** (they land under
   `.claude/worktrees/`, which is gitignored). Each owns a disjoint file set and commits
   atomically on its branch; requests for files it doesn't own go in its report.
3. As each finishes: rebase its branch onto `main` *inside its worktree*, resolve, verify
   (fmt, clippy `-D warnings`, full tests), fast-forward `main`, remove the worktree.
4. Apply the agents' integration requests yourself as separate small commits.

**Gotchas learned the hard way**
- Rebases conflict almost only in `Cargo.lock` (take `main`'s side, run `cargo metadata`)
  and in `Cargo.toml` dependency lists (keep both sides). Git can silently auto-merge
  two `[dev-dependencies]` headers into one manifest (a duplicate-key build error):
  rebase with an `--exec` step that de-duplicates TOML sections and amends, so every
  commit stays valid.
- Askama 0.16: every template that calls macros needs `{% import "macros.html" as ui %}`;
  pass string literals to `ui::icon`; never create a `macros.html` inside a subfolder
  (imports resolve relative to the template first).
- sqlx 0.9 rejects non-`'static` SQL: use literals or `concat!` macros; for variable-length
  `IN` lists bind a JSON array and use `json_each(?)`.
- Windows host: Python writes CRLF unless `newline="\n"`; `.gitattributes` normalises on
  commit. Long Python heredocs can break the Bash tool — write scripts to the scratchpad.
- Tests that build fixtures use the real `git` CLI; SSH tests need the OpenSSH client and
  skip without it. Argon2 is slow in debug builds — sign in sparingly in e2e tests.
- A running dev server locks `target/debug/rustit.exe`; stop it before `cargo build`.

## 5. Next: Milestone 2 wave B — issues, pull requests, branch protection

Goal: the collaboration layer that makes Rustit competitive with Gitea/Forgejo, in two
waves because pull requests reuse the issue timeline and comments.

**Wave A is done** ([design/milestone-2.md](design/milestone-2.md)), merged and integrated:

| Workstream | Delivered |
|---|---|
| **Access** | `Permission`/`BasePermission`, collaborators, teams, organisation base permission (default `write`), the exhaustive `repo_access(viewer, repo, &Grants)` policy and one readable-by SQL predicate, repository transfer; integration: old-name redirects (`0016`), a Leave button for collaborators, audited base-permission changes. |
| **API & webhooks** | `/api/v1` (token scopes incl. `admin`, pagination, JSON errors, throttling, OpenAPI from one route table), repository and organisation webhooks with HMAC signatures, a SQLite delivery queue with backoff, a delivery log and redelivery, the shared internal-address classifier (`rustit_core::network`). |
| **Repository essentials** | Markdown relative links and images, archives, blame, compare, releases with uploads and downloads, forks, pull mirrors (pushes refused over HTTP and SSH); integration: release files in `rustit backup` (format 2). |

Migrations `0009`–`0016` are on `main` and covered by the upgrade test
(`crates/rustit-storage/tests/upgrade.rs`); [ARCHITECTURE.md](ARCHITECTURE.md) documents the
schema, routes, events, API and webhooks.

**Wave B** ([design/milestone-2b.md](design/milestone-2b.md)) — read its sequencing first:

| Step | Who | Scope |
|---|---|---|
| **P0** | integrator, on `main` | Pre-wire stubs, events, the push gate in both receive paths (HTTP + SSH), config, navigation, icons, CSS |
| **T0** | integrator, on `main` | The shared issue/timeline core (migration `0017`, store, service, generic routes, timeline views); frozen once I and P start |
| **B** | agent, after P0 | Branch protection: rules per pattern, a pre-receive gate in HTTP **and** SSH, merge requirements for P, settings UI, REST endpoints |
| **I** | agent, after T0 | Issues: labels, milestones, assignees, references (`#12`, `fixes #12`), search, in-app notifications, webhooks and REST endpoints |
| **P** | agent, after T0 | Pull requests: cross-fork compare, `refs/pull/N/head`, line comments, reviews, merge strategies on a temporary worktree, webhooks and REST endpoints |

Wave B reserves migrations `0017`–`0031`. Launch the agents as in §4.

## 6. Decisions on record

- Git objects stay in bare repositories on disk (not redb): native format keeps `git gc`,
  `fsck`, backups and mirroring working and is faster. redb holds only sessions and
  rebuildable caches; SQLite holds relational data.
- The `git` binary is a runtime dependency for transport, `gc` and backups. Replacing it
  with gitoxide is feasible for `receive-pack` first; `upload-pack` lacks server-side
  negotiation and delta compression in gitoxide today. Treat it as its own phase behind
  a config switch with compatibility testing.
- Runtime instance settings (site name, registration, org creation, default visibility)
  override config defaults and are edited in `/admin/settings`.
- Webhook secrets are stored as given (HMAC needs them) and are write-only — the one
  exception to "secrets only as digests".
- Release files live on disk under numeric ids (`attachments/<repo>/<asset>`), not in
  SQLite or redb, and are always downloaded as attachments.
- Old repository names are kept by SQLite triggers (`repo_redirects`), so every code path
  that renames, transfers, creates, forks or mirrors stays correct without remembering to.
