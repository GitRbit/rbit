# rbit — Handoff

## Current project status — 2026-10-25

The public, open-source upstream is [GitRbit/rbit](https://github.com/GitRbit/rbit),
licensed under MIT OR Apache-2.0. The workspace version is `0.1.5`, and
[CHANGELOG.md](../CHANGELOG.md) records prepared releases through `0.1.5`.

Version 0.1.5 follows the separately prepared 0.1.4 release and includes security
hardening, rendering and database performance improvements, archived repositories and
notification email digests. The Actions branch remains outside this release.

Upgrading from 0.1.4 applies migrations `0044`–`0047`. Take a complete pre-upgrade backup,
then keep the generated/configured secret key with every post-upgrade database backup;
restoring sealed credentials requires the original key. See the 0.1.5 upgrade notes in
[CHANGELOG.md](../CHANGELOG.md#015---2026-10-25) and the deployment backup instructions.

This commit prepares the next version; it does not publish a tag or release artifacts.
Publish only after all prerequisite feature PRs and this version change reach main and
the final main commit passes CI.

Use [README.md](../README.md#features) for available features and
[ROADMAP.md](../ROADMAP.md) for current priorities and scope. Source builds require
Rust 1.94+ and Node.js 24.

Follow [CONTRIBUTING.md](../CONTRIBUTING.md#preparing-a-release) for the release gate.
Require a green CI run for the final commit, review the release notes, and verify platform
archives and container packaging for each release. GitHub repository and GHCR package
visibility are separate; verify anonymous downloads and container pulls.

[Repository configuration](repository-configuration.md) records the live GitHub audit,
required settings, repeatable checks and the pending private conduct-reporting address.

Commit attribution names human contributors only; do not add AI tool coauthor trailers.

## Historical milestone snapshot

Everything below records the earlier workstream handoff as of 2026-10-17. Its progress
counts, commit ids, publication instructions and unverified items are historical, not
current release guidance. Use the linked roadmap and contributor guide above for current work.

**As of 2026-10-17, `main` @ `5607485` plus the milestone 3 documentation commits on top.
Milestone 2 is complete; milestone 3 ("ready to open-source",
[design/milestone-3.md](design/milestone-3.md)) is in progress with 7 of its 9 workstreams
merged.** Everything below is what the next person (or agent team) needs to pick this up
without re-reading the history.

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
| **Milestone 3 — ready to open-source** | 🚧 In progress: 7 of 9 workstreams merged |
| ↳ H — open-source hygiene (MIT OR Apache-2.0, community files, `cargo deny`, release workflow and ghcr.io image, `rbit_` tokens) | ✅ Merged; CI and the release workflow have never run (§2) |
| ↳ S — commit statuses and required checks (`0033`) | ✅ Merged |
| ↳ A — REST API: collaborators, teams, releases (streamed uploads), forks; `member` webhook | ✅ Merged (no migration) |
| ↳ N — email notifications (`0034`, `server_secrets`) | ✅ Merged |
| ↳ E — editing files in the browser | ✅ Merged (no migration; `0035` spare) |
| ↳ L — Git LFS (`0036`, backup format 3) | ✅ Merged |
| ↳ O — external sign-in: OpenID Connect and GitHub (`0037`) | ✅ Merged |
| ↳ W — wave B stretch items (`0023`, `0025`) | 🚧 In progress |
| ↳ M — importing from GitHub/Gitea, push mirrors (`0038`, `0039`) | 🚧 In progress |
| Frontend speed wave — virtual blocks, compact highlighting, deferred counts, partial pages, new preloader ([design/frontend-virtualization.md](design/frontend-virtualization.md); fixture and probe in `scripts/perf/`) | ✅ Done on branch `perf/virtualization` (not yet on `main`) |

Quality gate on `main`, exactly as CI runs it:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # 1043 tests, 0 failing, 0 ignored
cargo deny --locked check licenses advisories   # CI's deny job
npm ci && npm run build         # vendored htmx + Tailwind CSS
```

Verified by hand: a copy of a real milestone 1 database (migrations `0001`–`0008`, with
users, an organisation, repositories, an SSH key and audit entries) migrated with
`rbit migrate` through `0016` — integrity and foreign-key checks clean, organisation
base permission `write`, all triggers present — and the server then served the
repositories, the organisation, releases, archives and the API from it.
`crates/rbit-storage/tests/upgrade.rs` repeats that upgrade on every test run, now through
`0037`, and checks that the tables of wave B and milestone 3 exist and start empty. Wave B is covered end to
end by `crates/rbit-e2e/tests/e2e/{timeline,issues,pulls,protection}.rs` (real `git` over
HTTP); the SSH protection scenarios are in `crates/rbit-ssh/tests/protection.rs`. Earlier
(milestone 1): SSH key added in the UI then `ssh -T`/clone/push/re-clone with OpenSSH,
password-reset email (log transport), security settings, audit log, `/healthz`,
`/readyz`, `/metrics`; the Docker image and the systemd unit were run (see `5635638`).
The PWA service worker was verified in real Edge (the in-app browser pane cannot register
service workers — ignore failures there).

Milestone 3 is covered end to end by `crates/rbit-e2e/tests/e2e/{api_workflow,statuses,
notification_email,edits,lfs,external_auth,tokens}.rs`: the API scenario streams a 1.5 MiB
release file; notification emails are read back from the log transport; Git LFS runs the
real `git lfs` client over HTTP and SSH (skipped without it or OpenSSH); external sign-in
runs against an in-process fake OIDC provider (signed ID tokens, JWKs) and a fake GitHub.
By hand: the status marks and checks at 375 px (S), the release workflow with `actionlint`
and `docker build --check` for both Dockerfile variants (H). Nothing of milestone 3 has run
on GitHub yet.

## 2. Open items (do these first)

1. **CI has never run on GitHub:** nothing is pushed anywhere and there is no remote
   configured yet. Push a branch to `github.com/GitRbit/rbit` to run
   `.github/workflows/ci.yml` (Linux + Windows, and the new `cargo deny` job).
2. **The release workflow has never run either.** Push a `v0.1.0-rc.1` tag first (a
   pre-release: its notes come from `## [Unreleased]`, it does not move `latest`) to
   exercise the whole pipeline — binaries for five targets, `SHA256SUMS`, the GitHub
   release, the ghcr.io push — before the real `v0.1.0`. For `v0.1.0`, turn
   `## [Unreleased]` in [CHANGELOG.md](../CHANGELOG.md) into `## [0.1.0] - <date>` (keep an
   empty `## [Unreleased]` above it): the release job refuses a tag without its section.
   The workspace version is already `0.1.0`.
3. **GitHub repository settings** (owner, after the first push): enable *Private
   vulnerability reporting* (Settings → Code security) — `SECURITY.md` and the issue
   template link to it; enable *Discussions* (linked from the issue chooser and README) or
   drop those links; after the first image push make the `rbit` package on ghcr.io public
   and link it to the repository (new packages are private); add the `bug` and
   `enhancement` labels if the repository lacks GitHub's defaults (the issue forms apply
   them).
4. **Owner to confirm the logo's license:** `THIRD-PARTY.md` says the logo and icons are
   distributed under the project's MIT OR Apache-2.0 terms. If the mascot should be
   reserved instead (trademark-style "all rights reserved" for the artwork), change that
   row and the README.
5. **Finish milestone 3:** merge W (wave B stretch items: resolving review threads with
   `0023`, the base-branch picker, `/issues` and `/pulls` dashboards, the Markdown preview,
   account-rename redirects with `0025`) and M (import from GitHub/Gitea, push mirrors:
   `0038`, `0039`), then integrate their `docs/design/milestone-3/{W,M}.md` notes like the
   other seven (CHANGELOG, README, example config, ARCHITECTURE, this file, ROADMAP).
   Hooks waiting for them: E's `markdown_preview_url()` in
   `crates/rbit-web/src/routes/edits.rs` returns `None` until `POST /markdown/preview`
   exists (the editor posts the field `content`); M's importer should bring LFS objects in
   through `LfsService::store`.
6. **Not yet recorded as verified by hand:** upgrading a real milestone 2 database through
   `0037` with `rbit migrate` (the upgrade test covers a synthetic milestone 1 one); a real
   OIDC provider (Keycloak, Entra ID, Google) and real GitHub OAuth (only fakes are tested);
   notification emails through a real SMTP server and mail client (threading, one-click
   unsubscribe). (The milestone 2 scenario — protected `main`, a "Fixes #1" pull request,
   review, squash merge, notifications and webhooks — exists as
   `crates/rbit-e2e/tests/e2e/workflow.rs`.)
7. **Follow-ups the milestone 3 workstreams left** (none blocks a release):
   - *Archived repositories* do not exist yet. When they do, extend
     `EditService::read_only_reason` and `RepoPage::can_edit_on` (E) and refuse LFS uploads
     where mirrors are refused (`lfs::Authorized::require_write` and the SSH push
     authorisation `git-lfs-authenticate upload` reuses) (L).
   - Webhooks: `member` covers collaborators only; team grants and memberships (GitHub's
     `membership` / `team` events) are not reported (A).
   - Statuses: no "strict" (branch up to date) checks; `require_status_checks` needs at
     least one context; "Required" badges are shown to writers only; `mergeable_state:
     blocked` is still never produced (S).
   - REST API: releases do not store `target_commitish`; an API upload cut off by the
     timeout leaves its `.part` until the start-up sweep (A).
   - Email notifications: no per-repository mute; repeatable actions dedupe per second;
     renaming a repository starts new mail threads (N).
   - Editing: no rename for binaries (delete and upload instead); no "create the first
     file" in an empty repository (`EditWorkspace` needs a parent-less commit); mirrors are
     covered by service tests only (E).
   - Git LFS: the pure-SSH `git-lfs-transfer` protocol is not implemented (clients fall
     back); the token key is per process (a restart makes clients re-authenticate); the
     orphan grace period (24 h) is a constant; uploads have no time limit; locks ignore
     `ref`; a crash between placing an object and writing its row leaves a file nothing
     sweeps; images committed to Git (not LFS) still show "Binary file not shown" (L).
   - External sign-in: no "set a password" form for passwordless accounts (a reset link
     sets one; without mail only an administrator can), and the change-email,
     change-password and remove-2FA forms ask for a password such accounts lack — they
     could explain it using `ExternalAuthService::methods(user).has_password`; the
     provider's display name is not copied; pending registrations live in memory (a
     restart in the 15-minute window means signing in again); no "Sign up with …" buttons
     on `/signup`; the JWKS refetch on an unknown `kid` is not rate limited (only a token
     from the provider's own token endpoint can trigger it) (O).
   - Packaging: Linux binaries are glibc only (musl static builds could be added as extra
     targets); token hints are shown without a prefix (`…a1b2`); `RUSTSEC-2023-0071` (RSA
     Marvin attack) is ignored in `deny.toml` because rbit never uses an RSA private key —
     revisit if it ever signs or decrypts with RSA (H).
8. **Follow-ups from the frontend speed wave** (none blocks a release):
   - Measure scrolling in a real Chrome/Edge window with `scripts/perf/scroll-probe.js`:
     the in-app browser pane does not run animation frames while hidden, so the wave was
     verified with static numbers (sizes, element counts, forced layout per step, exact
     block heights) only.
   - After posting a pull request line comment the boosted redirect loses `#anchor`
     (pre-existing): the XHR never sees `Location`; answer htmx posts with `HX-Location`
     carrying the fragment.
   - A line-comment link still renders the whole files page and `hx-select`s one file; a
     fragment route would make it cheap.
   - The first view of a large highlighted file is still CPU-bound (~2.4 s for 20,000
     lines in a debug build); the cache makes later views fast. Blame keeps a few slow
     layout steps on files with many tiny hunks.
   - Horizontal scroll width follows only the blocks laid out so far (D sets a minimum
     per diff file; file and blame views can still widen while scrolling).
   - Not done, by design until measurements call for them: one streamed response for many
     slots, evicting far-off blocks, `ETag`s from inputs, compression dictionaries.

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
- Security defaults: secrets stored only as digests (webhook secrets and the
  server-generated keys in `server_secrets` are the documented exceptions — HMAC needs the
  raw value), 404 for things a viewer may not know
  exist, strict CSP (no inline `style=`/scripts — use SVG attributes for proportional
  widths), Origin-checked forms, throttle anything that verifies a secret.
- **Design: monochrome minimal** — the user rejected an earlier colourful design. Reuse
  `.box`, `.row`, `.btn` (`-primary`/`-ghost`/`-danger`), `.input`, `.badge`, `.tab`;
  colour only for status, diffs, syntax and the language bar. Mobile-first, check 375 px.
- **Commits are atomic**: each builds, is formatted, lint-clean and tested on its own;
  imperative subject ≤ 72 chars, body explains what and why. AI tool coauthor trailers
  are not permitted. Use rebase or squash for new integrations on `main`.
- Dependencies must pass `cargo deny check licenses advisories`; new licenses are added to
  `deny.toml` deliberately.

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

Milestone 3 varied this: no pre-wired stubs. Shared registration points are edited by
**appending one marked block** (`// Milestone 3: <name> (<letter>).`) at the end of each
list, so rebases conflict only as "keep both sides"; and workstreams do not edit the
shared documents — each writes `docs/design/milestone-3/<letter>.md` (as built, deviations,
and prepared text for CHANGELOG, README, example config, ARCHITECTURE and HANDOFF), which
the integrator merges. Both worked; the notes files are also the best record of why
something is the way it is.

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

## 5. Next steps

Milestone 2 is done (wave A: [design/milestone-2.md](design/milestone-2.md), wave B:
[design/milestone-2b.md](design/milestone-2b.md)). Milestone 3
([design/milestone-3.md](design/milestone-3.md), as-built notes in
[design/milestone-3/](design/milestone-3/)) has delivered:

| Workstream | Delivered |
|---|---|
| **H — Open-source hygiene** | MIT OR Apache-2.0, `THIRD-PARTY.md`, `deny.toml` + CI job, community files and `ROADMAP.md`, the release workflow (five native targets, `SHA256SUMS`, notes from the changelog, a multi-arch `ghcr.io/gitrbit/rbit` image from the release binaries), OCI labels, `rbit_` tokens with `rit_` still accepted, the rename sweep. |
| **S — Commit statuses** | GitHub-compatible status API, required status checks in branch protection (a bypassable protection requirement), checks in the merge box and on commits, the `status` webhook (`0033`). |
| **A — REST API continuation** | Collaborators, teams, releases with streamed file uploads and downloads (`.streaming()` routes), forks; the `member` webhook; team grants refuse other owners' repositories. |
| **N — Email notifications** | Preferences (off / participating / all), threading headers, signed one-click unsubscribe (the one CSRF exemption), per-address throttle, `server_secrets` (`0034`). |
| **E — Editing in the browser** | Create, edit, rename, delete and upload as one commit via a scratch repository and the merge's `update-ref` lease; push gate as for pushes; noreply author; LFS pointers not editable. |
| **L — Git LFS** | Batch API, basic transfers, locks, `git-lfs-authenticate`, content-addressed storage shared by fork networks where readable, signed short-lived tokens, blob/raw pages, backups (format 3), orphan clean-up (`0036`). |
| **O — External sign-in** | OpenID Connect and GitHub, PKCE, sealed flow cookie, full ID token checks, linking rules, register step, passwordless accounts, Settings → Security sign-in methods (`0037`). |

Then, in order:

1. Merge W and M and integrate their notes (§2 item 5).
2. Run CI on GitHub, then the `v0.1.0-rc.1` tag, fix what the first real runs find, apply
   the repository settings (§2 items 1–4), and tag `v0.1.0`.
3. Candidates for milestone 4, roughly by value — write a contract like
   `design/milestone-3.md` first:
   - **gitoxide transport** for `receive-pack` behind a config switch (see §6);
   - a **runner protocol** on top of commit statuses (rbit still would not run builds);
   - **archived repositories** (read-only everywhere: pushes, edits, LFS uploads, §2 item 7);
   - `membership` / `team` webhooks, strict (up-to-date) status checks, email digests,
     `git-lfs-transfer` over SSH — the follow-ups of §2 item 7 by demand.

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

Milestone 3:

- Licensed **MIT OR Apache-2.0** (was AGPL). The public home is github.com/GitRbit/rbit, the
  image `ghcr.io/gitrbit/rbit`; vulnerability reports go through GitHub private security
  advisories (no email published).
- Tokens are `rbit_…`; `rit_…` is accepted forever (it costs nothing: the digest covers
  only the random part). The webhook user agent stays `rbit-Hookshot/…` and the header
  names `X-Rbit-*`. Legacy `rustit.*` file names, `RUSTIT_*` variables and `rustit_*`
  cookies stay as commented shims; applied migrations are never edited, not even comments
  (sqlx checks their checksums).
- Release images package the release binaries (`BINARY=prebuilt`), never a second build
  under QEMU; Linux binaries are glibc (2.35 baseline), built on native runners.
- Commit statuses: the latest status per (repository, commit, context) wins; only
  *required* contexts block, as a protection requirement an administrator may bypass where
  the rule allows it; statuses never affect pushes.
- API scopes: deleting releases and release files needs `write` (writers' actions), all
  collaborator endpoints (reads too) and team management need `admin`.
- Web edits are authored **with the user's noreply address, like merges** (rbit never
  writes a real address into a commit — the contract's "verified email" was overridden for
  privacy). They never check files out (scratch repository + scratch index +
  `commit-tree`), reach the branch through the merge's `update-ref` compare-and-swap, are
  decided by the push gate (`check_push`) and reported through `PushService` like pushes.
  Git LFS pointer files cannot be edited in the browser.
- Email notifications get their own `[notifications]` section and their own throttle
  scope (a busy thread must never use up a user's password-reset budget); emails are
  claimed in an outbox before sending, so each (recipient, thread, activity) is mailed at
  most once.
- `server_secrets` is the store for keys rbit generates itself (stored raw, like webhook
  secrets, because HMAC needs them); the one CSRF exemption is the RFC 8058 one-click
  unsubscribe `POST`.
- Git LFS: objects are stored once (content-addressed) and **linked per repository**. A
  repository may use the objects linked to it, and those of its fork network where the
  requester can read them (then linked to it lazily); anything else must be uploaded again
  before it is linked, so knowing an object id never grants content. Transfer tokens are
  stateless, signed with a per-process key and re-check access on every use.
- External sign-in: identities are keyed by the provider's stable subject. A first
  sign-in links only (a) from a link started in Settings → Security in the same session,
  or (b) by email when the provider has `trust_email` and **both** addresses are verified;
  otherwise it registers (if `allow_registration` allows) or is refused. The account's own
  2FA still applies unless `trust_mfa` and the provider reports MFA. Accounts registered
  through a provider have no password until a reset link sets one; the last way to sign in
  cannot be unlinked. The flow state is a signed cookie, nothing is stored server-side.
