# rbit — Handoff

**As of 2026-10-17, `main` @ `17ecacf` plus the wave B integration commits on top
(Protected badge, documentation). Milestone 2 is complete.** Everything below is what
the next person (or agent team) needs to pick this up without re-reading the history.

## 1. Where things stand

| Area | Status |
|---|---|
| Foundation (accounts, repositories, browsing, smart HTTP, PWA shell) | ✅ Done |
| Phase 2 (commit diffs, branches/tags, tokens, orgs, stars, languages, settings, admin panel) | ✅ Done |
| **Milestone 1 — safe to self-host** (rate limiting, 2FA, sessions, audit log, email recovery, SSH, jobs, metrics, backups, deployment, CI, e2e tests) | ✅ Done, merged |
| Frontend wave (landing page and dashboard, live explore search, theme gallery with ~500 schemes, stored preferences) | ✅ Done, merged |
| **Milestone 2 wave A** — access (collaborators, teams, base permission, transfer, old-name redirects), REST API `/api/v1` + signed webhooks, repository essentials (README links, archives, blame, compare, releases with files, forks, pull mirrors) | ✅ Done, merged and integrated |
| **Milestone 2 wave B** — shared issue timeline, issues (labels, milestones, assignees, references, search, in-app notifications), pull requests (fork network, reviews, line comments, mergeability, merge/squash/rebase), branch protection (push gate over HTTP and SSH, merge rules), REST endpoints and webhooks | ✅ Done, merged and integrated |
| Alongside wave B (the user's work) — rename to **rbit**, passkeys (WebAuthn, migration `0032`), diff file tree, modal theme picker, live dev server (`npm run dev`) | ✅ Done, on `main` |
| Next milestone | ⏭️ Not started — see §5 |

Quality gate on `main`, exactly as CI runs it:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # 874 tests, 0 failing, 0 ignored
npm ci && npm run build         # vendored htmx + Tailwind CSS
```

Verified by hand: a copy of a real milestone 1 database (migrations `0001`–`0008`, with
users, an organisation, repositories, an SSH key and audit entries) migrated with
`rbit migrate` through `0016` — integrity and foreign-key checks clean, organisation
base permission `write`, all triggers present — and the server then served the
repositories, the organisation, releases, archives and the API from it.
`crates/rbit-storage/tests/upgrade.rs` repeats that upgrade on every test run, now through
`0032`, and checks that wave B's tables exist and start empty. Wave B is covered end to
end by `crates/rbit-e2e/tests/e2e/{timeline,issues,pulls,protection}.rs` (real `git` over
HTTP); the SSH protection scenarios are in `crates/rbit-ssh/tests/protection.rs`. Earlier
(milestone 1): SSH key added in the UI then `ssh -T`/clone/push/re-clone with OpenSSH,
password-reset email (log transport), security settings, audit log, `/healthz`,
`/readyz`, `/metrics`; the Docker image and the systemd unit were run (see `5635638`).
The PWA service worker was verified in real Edge (the in-app browser pane cannot register
service workers — ignore failures there).

## 2. Open items (do these first)

1. **CI has never run on GitHub:** nothing is pushed anywhere and there is no remote
   configured yet. Push a branch to run `.github/workflows/ci.yml` (Linux + Windows).
2. **Release notes:** [CHANGELOG.md](../CHANGELOG.md) has the wave B upgrade notes
   (migrations `0017`–`0024`, `refs/pull/*` reserved, the force-push side effect,
   `<data_dir>/tmp` excluded from backups, `[issues]`/`[pulls]`), the rename and passkeys
   notes, wave A's, and milestone 1's one-time sign-out (`sessions_v2`).
3. **Not yet recorded as verified by hand:** upgrading a real wave A database through
   `0024` with `rbit migrate` (the upgrade test covers a synthetic milestone 1 one), and the
   cross-workstream scenario the contract sketches as `tests/e2e/workflow.rs` (protected
   `main` with one approval, a fork PR whose body says "Fixes #1", squash merge, the issue
   closed via the PR, a notification and both webhooks). Its parts are tested separately
   in `pulls.rs`, `issues.rs` and `protection.rs`.
4. **Wave B stretch items not built:** resolving review threads (migration `0023` stays
   spare), a base-branch picker on the PR page (the `…/pull/{n}/base` route and the
   `BaseChanged` event exist), cross-repository dashboards (`/issues`, `/pulls`; the names
   are reserved), a Markdown preview tab, and redirects for renamed *accounts* (only
   repositories redirect).
5. API endpoints for collaborators, teams, releases and forks are still missing (moved out
   of wave B; migrations `0026`–`0031` were reserved for them).

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
(environment variables override `rbit.toml`):

```bash
RBIT_MAIL__TRANSPORT=log             # print emails (reset/verify links) to the log
RBIT_SSH__ENABLED=true RBIT_SSH__PUBLIC_HOST=localhost RBIT_SSH__PUBLIC_PORT=2222
RBIT_OPERATIONS__METRICS=true        # expose /metrics
RBIT_WEBHOOKS__ALLOW_PRIVATE_ADDRESSES=true   # deliver webhooks to a local listener
RBIT_MIRRORS__ALLOW_PRIVATE_ADDRESSES=true    # mirror from a local Git server
```

Create accounts from the CLI with `rbit user create <name> <email> [--admin]` (password
on stdin). Production setup: [deployment.md](deployment.md) and
[`deploy/rbit.example.toml`](../deploy/rbit.example.toml).

## 4. How this codebase is worked on

**Read first:** [ARCHITECTURE.md](ARCHITECTURE.md) (design, schema, routes, events, API
and webhooks, the push gate and merges, security model, design system), then
[design/milestone-2.md](design/milestone-2.md) (wave A — its rules section holds the house
rules) and [design/milestone-2b.md](design/milestone-2b.md) (wave B, including its "P0 as
built" and "T0 as built" sections — the model for refining a contract while agents run).

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
   **Branch every agent from the integrator's current base** (the commit with the
   stubs and contract), never from an older `main`: an agent started from a stale base
   rebuilds what the stubs provide and conflicts everywhere.
3. As each finishes: rebase its branch onto `main` *inside its worktree*, resolve, verify
   (fmt, clippy `-D warnings`, full tests), fast-forward `main`, remove the worktree.
4. Apply the agents' integration requests yourself as separate small commits.
5. When shared code is refined while workstreams run (wave B's P0 and T0), record "as
   built" notes in the contract so every agent builds on the same facts.

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
- A running dev server locks `target/debug/rbit.exe`; stop it before `cargo build`.
  `npm run dev` (`scripts/dev.mjs`) avoids that: it builds into `target/live` and serves
  `./data-live`. On Windows never run two dev servers on the same target or data
  directory (the executable and the SQLite/redb files are locked).
- Run the quality gates **one after another**, not in parallel: concurrent `cargo`
  builds and the e2e suite starve the machine and make the live server unresponsive.
- Renames: rebasing a branch across a directory rename (`rustit-*` → `rbit-*`) needs
  `git -c merge.directoryRenames=true rebase …`; even then, **new subfolders** a branch
  created inside a renamed directory are not moved automatically — look for files left
  under the old path after the rebase and move them by hand.

## 5. Next milestone (suggested)

Milestone 2 is done: wave A ([design/milestone-2.md](design/milestone-2.md)) and wave B
([design/milestone-2b.md](design/milestone-2b.md)) are merged and integrated.

| Wave B step | Delivered |
|---|---|
| **P0 + T0** (integrator) | Stubs, events, the push-gate glue in both receive paths (SSH receive-pack in two steps); the shared thread core: one number sequence per repository for issues and PRs, comments, the timeline (`issue_events`), `issue_permissions`, `TimelineService`, generic routes and timeline views (`0017`). |
| **B — Branch protection** | Rules per pattern, the in-process push gate over HTTP and SSH (all-or-nothing refusals in git's report format, `receive.denyNonFastForwards`, `refs/pull/*` reserved), merge rules for PRs, settings UI, the Protected badge, REST endpoints (`0024`). |
| **I — Issues** | Labels, milestones, assignees, references and closing keywords, FTS5 search and filters, in-app notifications, `issues`/`issue_comment` webhooks, REST endpoints (`0018`–`0020`). |
| **P — Pull requests** | Fork networks and overlay compares, `refs/pull/{n}/head`, line comments and reviews, the mergeability worker, merge/squash/rebase via `merge-tree` + `commit-tree` and an `update-ref` lease, `pull_request`/`pull_request_review` webhooks, REST endpoints (`0021`, `0022`). |

Candidates for milestone 3, roughly by value — write a contract like
`design/milestone-2b.md` first and pre-wire it as in §4:

1. **Actions / CI integration**: a commit status API (a runner protocol later) feeding the
   existing required-checks placeholder (`ChecksStatus` in `rbit_core::pull`, shown as "No
   status checks" in the merge box), plus a "require checks" protection flag.
2. **API continuation**: collaborators, teams, releases and forks (migrations
   `0026`–`0031` were reserved for this).
3. **Email notifications** on top of the in-app inbox (the `MailService` seam exists;
   per-user settings, digests).
4. **gitoxide transport** for `receive-pack` behind a config switch (see §6).
5. The wave B stretch items of §2.

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
- Issues and pull requests are one table with one number sequence and one timeline; pull
  requests add a row keyed by the issue id. Only `sqlite/issues.rs` writes the core tables.
- Branch protection is enforced in-process (no `pre-receive` executable): git runs without
  hooks, and the tests boot the server in-process.
- Merges never check files out (`merge-tree --write-tree` + `commit-tree` in a scratch
  repository) and reach the base branch through one `update-ref` compare-and-swap, not
  `receive-pack`; protection for merges is `check_merge`.
