# Rustit — Handoff

**As of 2026-10-16, `main` @ `5d19ba5`.** Everything below is what the next person (or agent
team) needs to pick this up without re-reading the history.

## 1. Where things stand

| Area | Status |
|---|---|
| Foundation (accounts, repositories, browsing, smart HTTP, PWA shell) | ✅ Done |
| Phase 2 (commit diffs, branches/tags, tokens, orgs, stars, languages, settings, admin panel) | ✅ Done |
| **Milestone 1 — safe to self-host** (rate limiting, 2FA, sessions, audit log, email recovery, SSH, jobs, metrics, backups, deployment, CI, e2e tests) | ✅ Done, merged |
| Milestone 2 — a real forge (collaboration features) | ⏭️ Next — see §5 |

Quality gate on `main`, exactly as CI runs it:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # 388 tests, 0 failing, 0 ignored
npm ci && npm run build         # vendored htmx + Tailwind CSS
```

Verified by hand on a running server: database upgrade through all 8 migrations, SSH
key added in the UI then `ssh -T`/clone/push/re-clone with OpenSSH, password-reset email
(log transport), security settings, audit log, Clone menu SSH URL, `/healthz`, `/readyz`,
`/metrics`. The PWA service worker was verified in real Edge (the in-app browser pane
cannot register service workers — ignore failures there).

## 2. Open items (do these first)

1. **Unmerged branch** `claude/serene-elbakyan-73e4d3` holds one commit, `b43876b`
   "Test that the service worker is served versioned and site-scoped" (a regression
   test, green). Rebase onto `main` and fast-forward if wanted.
2. **Never executed:** the `Dockerfile`, `.github/workflows/ci.yml` (Linux + Windows) and
   `deploy/rustit.service`. Build the image, push a branch to GitHub to run CI, and try the
   unit on a Linux host before calling deployment "supported".
3. **One-time sign-out:** the session store moved to `sessions_v2`; upgrading an existing
   instance signs everyone out once. Mention it in release notes.
4. Nothing is pushed anywhere; there is no remote configured yet.

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
```

Create accounts from the CLI with `rustit user create <name> <email> [--admin]` (password
on stdin). Production setup: [deployment.md](deployment.md) and
[`deploy/rustit.example.toml`](../deploy/rustit.example.toml).

## 4. How this codebase is worked on

**Read first:** [ARCHITECTURE.md](ARCHITECTURE.md) (design, schema, security model,
design system), then [design/milestone-1.md](design/milestone-1.md) (the most recent
contract — its engineering and commit rules are the house rules).

**Standards (non-negotiable)**
- Layering: `web → services → (storage, git, auth, mail) → core`. Handlers are thin;
  business rules live in services; SQL lives in stores; templates contain no logic.
- Self-describing code, `//!` module docs, validated domain types, clippy **pedantic**
  clean, `unsafe` forbidden, no `unwrap` outside tests, `cargo fmt` (see `rustfmt.toml`).
- Cross-cutting reactions go through the `EventBus` (`DomainEvent`), not direct calls.
- Security defaults: secrets stored only as digests, 404 for things a viewer may not know
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

## 5. Next: Milestone 2 — a real forge

Goal: the collaboration layer that makes Rustit competitive with Gitea/Forgejo. Suggested
as two waves because pull requests reuse the issue timeline/comment infrastructure.

**Wave A (independent, parallel)**

| Workstream | Scope |
|---|---|
| **Access** | Collaborators on personal repositories (read/write/admin); teams inside organisations with per-repository permissions; extend the pure `repo_access` policy and keep it exhaustively tested; repository transfer between owners. |
| **API & webhooks** | Versioned REST API (`/api/v1`) authenticated by access tokens (scopes), OpenAPI document, pagination conventions; webhooks per repository/org with HMAC-signed deliveries, retries with backoff and a delivery log — driven by `DomainEvent`s (add `PushEvent` details: ref, before/after). |
| **Repository essentials** | README relative links and images; `zip`/`tar.gz` archives; releases (notes, attached files — needs an upload/storage design); blame view; branch compare; forks; pull mirrors. |

**Wave B (after A merges)**

| Workstream | Scope |
|---|---|
| **Issues** | Issues per repository: Markdown bodies and comments, labels, milestones, assignees, open/closed state, references (`#12`, commit messages `fixes #12`), a shared timeline model reusable by PRs; search and filters; notifications hook via events. |
| **Pull requests** | Compare branches (incl. forks), PR timeline on the issue model, review comments on diff lines, approvals/changes requested, merge strategies (merge commit, squash, rebase) executed safely on a temporary worktree, conflict detection, required checks placeholder. |
| **Branch protection** | Protected branch rules (no force-push, required reviews, restrict pushers) enforced in both HTTP and SSH receive paths (pre-receive check), UI under repository settings. |

Before launching each wave: write `docs/design/milestone-2.md` (contracts, file
ownership, reserved migrations `0009+`), pre-wire stubs on `main`, then follow §4.

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
