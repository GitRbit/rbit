# Milestone 3 — Ready to open-source

The working contract for the milestone that takes rbit from "done for its author" to "public
project people can adopt". House rules from [milestone-2.md](milestone-2.md#rules-all-workstreams),
[milestone-2b.md](milestone-2b.md) and [../HANDOFF.md](../HANDOFF.md) §4 apply unchanged unless
this document says otherwise.

Nine workstreams, each on its **own branch in its own git worktree**, run in three waves of
three (concurrent `cargo` builds starve the machine beyond that):

| Wave | | Workstream | Migrations |
|---|---|---|---|
| 1 | **H** | Open-source hygiene and release packaging | — |
| 1 | **S** | Commit statuses and required checks | `0033` |
| 1 | **A** | REST API continuation: collaborators, teams, releases, forks | `0026`–`0031` (if needed) |
| 2 | **N** | Email notifications | `0034` |
| 2 | **E** | Editing files in the browser | — (`0035` spare) |
| 2 | **W** | Wave B stretch items | `0023`, `0025` |
| 3 | **L** | Git LFS | `0036` |
| 3 | **O** | External sign-in (OpenID Connect, GitHub OAuth) | `0037` |
| 3 | **M** | Importing from GitHub/Gitea, push mirrors | `0038`, `0039` |

Never create a migration outside your range. Files are `NNNN_<topic>.sql`, `STRICT` tables,
Unix-second timestamps, foreign keys with explicit `ON DELETE`. `crates/rbit-storage/tests/upgrade.rs`
must keep passing: extend it with a check that your tables exist and start empty.

**Decisions already made by the project owner:** the license is **MIT OR Apache-2.0** (was
AGPL); the public home is **github.com/rbit/rbit** (container image `ghcr.io/rbit/rbit`);
vulnerability reports go through **GitHub private security advisories** (no email published);
personal access tokens switch to the **`rbit_`** prefix while `rit_` tokens keep working.

**Out of scope for every workstream:** a package registry, code search, wikis, project boards,
reactions, CODEOWNERS, tag protection, signature verification, i18n, Postgres, object storage,
an Actions runner (S builds the status API that external CI posts to, not a runner).

## Rules for shared files (no pre-wired stubs this milestone)

The integrator did not pre-wire stubs. Instead, shared registration points are edited by
**appending one marked block at the end of the existing list**, so rebases conflict only as
"keep both sides":

- `crates/rbit-web/src/routes/mod.rs` — your `mod x;` lines (keep alphabetical order) and one
  `.merge(x::routes())` group after the wave B group, under a comment `// Milestone 3: <name> (<letter>).`
- `crates/rbit-web/src/routes/api/mod.rs` — new child modules and their table entries, plus an
  `openapi/<area>.json` fragment (the existing equality test must pass).
- `crates/rbit-services/src/lib.rs` — `pub mod`, `pub use`, a struct field group under a
  `// Milestone 3: … (<letter>).` comment, construction, `spawn_background`/`shutdown` lines.
- `crates/rbit-core/src/config/mod.rs` — a new submodule per section and one field at the end of `Config`.
- `crates/rbit-storage` — store modules under `src/sqlite/<area>.rs` plus their accessor on `Database`.
- `crates/rbit-services/src/events.rs` — new `DomainEvent` variants appended at the end of the enum.
- Templates: navigation entries (settings sidebar, repository tabs) are one line each, appended.

**Documentation is integrated by the integrator.** Do **not** edit `README.md`, `CHANGELOG.md`,
`docs/ARCHITECTURE.md`, `docs/HANDOFF.md`, `docs/deployment.md` or `deploy/rbit.example.toml`
(exception: H owns `README.md`). Instead write `docs/design/milestone-3/<letter>.md` with:
an "as built" summary, the CHANGELOG entry, the README feature line and configuration-table row,
the commented `rbit.example.toml` section, the ARCHITECTURE section text, and any integration
requests (edits you need in files you don't own).

**Quality gate per commit** (run one after another, never in parallel):
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`. Limit parallelism with `CARGO_BUILD_JOBS=8` — two other agents are
building at the same time. Commits are atomic, imperative subject ≤ 72 chars, body explains
what and why, ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never push,
never touch `main`, never merge.

**UI:** monochrome minimal (see HANDOFF §4 Design). Reuse `.box`, `.row`, `.btn`, `.input`,
`.badge`, `.tab`; colour only for status. Strict CSP: no inline `style=` or scripts. Check 375 px.

---

## H — Open-source hygiene and release packaging

- `LICENSE-MIT`, `LICENSE-APACHE` (full texts; copyright "The rbit contributors"); set
  `license = "MIT OR Apache-2.0"` in the workspace and `package.json`; every crate inherits it.
- `THIRD-PARTY.md` (or `NOTICE`): vendored htmx, fonts, the tinted-theming schemes (their
  existing `LICENSE.txt` stays), the logo. A `cargo deny` config (`deny.toml`) that allows the
  licenses actually in the tree and a CI job that runs `cargo deny check licenses advisories`.
- `CONTRIBUTING.md` (the quality gate, layering and commit rules from HANDOFF §4, written for
  humans; how to run the dev server and the tests), `SECURITY.md` (GitHub private advisories,
  supported versions = latest release), `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1, contact
  via the maintainers through GitHub), `.github/ISSUE_TEMPLATE/{bug_report,feature_request}.yml`
  + `config.yml`, `.github/pull_request_template.md`, `ROADMAP.md` (public: what's done, what's
  next — derived from HANDOFF §5 and this document's out-of-scope list).
- `.github/workflows/release.yml`: on `v*` tags, build release binaries for
  `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` (or musl, whichever builds cleanly
  with SQLite), `x86_64-apple-darwin`/`aarch64-apple-darwin` and `x86_64-pc-windows-msvc`,
  including the CSS build, upload archives + SHA-256 sums to a GitHub release whose notes come
  from the matching CHANGELOG section; build and push a multi-arch image to `ghcr.io/rbit/rbit`.
  Validate the workflow YAML (e.g. `actionlint` if available, otherwise careful review).
- Dockerfile: OCI labels (`org.opencontainers.image.source`, licenses).
- Token prefix: new personal access tokens are `rbit_…`; `rit_…` tokens still parse and
  authenticate (the digest is over the whole secret, so nothing in storage changes). Update the
  token UI/help text, API docs, tests, README.
- Sweep the remaining `rustit` / `Rustit` / `Rbit` (capitalised) names. **Keep** deliberate
  compatibility shims (the legacy `rustit.db`/`rustit.redb` file names, legacy env vars or
  cookie names if any) and say so in a comment. Fix the webhook user agent to `rbit-Hookshot/…`
  → `rbit-hookshot/…` only if nothing else depends on the case (GitHub's is `GitHub-Hookshot`;
  choose and document).
- `.claude/launch.json`: no absolute personal paths (use a relative, gitignored data dir).
- README: license section, badges (CI, license), install from release binaries and the image,
  link the new community files. H owns `README.md` this milestone; others send README text
  through their notes file.

## S — Commit statuses and required checks

GitHub-compatible commit statuses so any external CI can report results.

- Migration `0033`: `commit_statuses` (repo, sha, state `pending|success|failure|error`,
  context, description, target_url, creator, timestamps; latest per (repo, sha, context) wins),
  and `branch_protections.require_status_checks` + `branch_protection_required_checks (rule_id, context)`.
- API: `POST /repos/{owner}/{repo}/statuses/{sha}` (write scope), `GET /repos/{owner}/{repo}/commits/{ref}/statuses`,
  `GET /repos/{owner}/{repo}/commits/{ref}/status` (combined), shaped like GitHub's.
- `status` webhook event. A domain event for status changes.
- Pull requests: the merge box's `ChecksStatus` comes from the head commit's statuses; with
  `require_status_checks`, merging needs every required context to be `success`. The PR page
  lists the checks (state, context, description, link). Commit lists and the commit page show a
  combined status mark. Protection settings UI gains the flag and a list of required contexts.
- Mergeability re-evaluates when a status arrives (reuse the existing mergeability worker/events).

## A — REST API continuation

Endpoints, OpenAPI fragments and tests for what wave B deferred, following the existing API
conventions exactly (pagination, errors, scopes, 404 for invisible things):

- Collaborators: list, get permission, add/update (`PUT`), remove.
- Teams (organisations): list/get/create/update/delete teams, members, repositories with permission.
- Releases: list, get, by tag, latest, create, update, delete; assets list/get/upload/download/delete
  (upload streams under `[releases]` limits; reuse `ReleaseService`).
- Forks: list forks, create a fork.
- `member`, `release` and `fork` webhook events if the domain events exist and the payload is
  GitHub-shaped (check `webhooks/payloads.rs`); otherwise list them as follow-ups.
- Use migrations `0026`–`0031` only if a schema change is genuinely needed.

## N — Email notifications

On top of the in-app inbox (`NotificationService`) and `MailService`:

- Migration `0034`: per-user notification email preference (`off`, `participating` = default for
  verified addresses, `all`), per-repository mute is optional; an outbox/dedup key so a
  notification is mailed at most once.
- Only verified email addresses receive mail; never mail the actor about their own action;
  respect repository visibility at send time (same rule as the inbox).
- Messages for: mention, assignment, review requested, review submitted, comment on a
  participating thread, issue/PR closed/merged/reopened. Plain-text + HTML, `List-Unsubscribe`
  (one-click link that turns emails off, signed token), `Message-ID`/`In-Reply-To` threading per
  issue, `Reply-To` not set (no incoming mail).
- Settings → Notifications page; config `[notifications]` or extend `[issues]` (document).
- Delivery through the existing mail queue and rate limits; tests with the log transport.

## E — Editing files in the browser

- Create, edit, rename, delete and upload files (multi-file upload, size limit from config) from
  the repository UI, committing to the current branch **or** a new branch with an "open a pull
  request" follow-up. Commit message + optional description; author is the signed-in user with
  their primary verified email (or the `noreply_domain` address).
- Implementation never checks files out: build trees with git plumbing in a scratch area (the
  same approach as `rbit-git/src/merge.rs`), then one `update-ref` compare-and-swap against the
  commit the editor started from (conflict → a clear "the branch moved, review and retry" page).
- Branch protection applies (`check_push`/`PushActor`): if the user may not push, offer the
  new-branch + PR route. The commit goes through the same post-push path as a real push
  (`RepositoryPushed`, webhooks, `fixes #n`, mergeability), so nothing downstream can tell.
- Editor: a plain `<textarea>` (no JS framework), with a Markdown preview for `.md` files if W's
  preview endpoint exists — otherwise leave a hook. "Edit" / "Delete" buttons on the blob page,
  "Add file" on the tree page, only for writers.
- Archived and mirror repositories are read-only.

## W — Wave B stretch items

- Resolving review threads (migration `0023`): resolve/unresolve a line-comment thread (writers
  and the thread author), collapsed when resolved, timeline event; optional protection flag
  "require resolved conversations" is **out of scope**.
- Base-branch picker on the PR page (the `…/pull/{n}/base` route and `BaseChanged` event exist).
- Cross-repository dashboards `/issues` and `/pulls` (names reserved): created by me, assigned,
  mentioned, review requested; filters reuse the issue query syntax; only visible repositories.
- Markdown preview tab for issue/PR bodies and comments: a `POST /markdown/preview` endpoint
  (same renderer and sanitiser as everywhere else; Origin-checked; throttled), htmx tab, no inline JS.
- Redirects for renamed **accounts** (migration `0025`, SQLite triggers like `repo_redirects`):
  `/{old}` and `/{old}/{repo}` (web, Git HTTP and SSH) redirect or resolve to the new name until
  the old name is taken.

## L — Git LFS

- Server side of the Git LFS API: batch API (`POST /{owner}/{repo}.git/info/lfs/objects/batch`),
  basic transfer adapter (upload/download/verify) with streaming bodies, locks API
  (create/list/verify/delete). Auth: HTTP basic with password or token (same rules as smart
  HTTP), and over SSH via `git-lfs-authenticate` returning a short-lived signed header.
- Migration `0036`: `lfs_objects (oid, size, created_at)`, `lfs_repo_objects (repo, oid)`
  (dedup across a fork network allowed only where readable), `lfs_locks`.
- Storage under `<data_dir>/lfs/<aa>/<bb>/<oid>`; SHA-256 verified on upload; size limits and an
  `[lfs]` config section (enabled, max object size); included in `rbit backup`; orphan clean-up
  in maintenance.
- Web: the blob page renders the LFS object (not the pointer) when present, raw/download serves it.
- Tests with the real `git lfs` client if installed (skip otherwise) plus protocol-level tests.

## O — External sign-in

- Generic **OpenID Connect** providers (discovery, authorization code + PKCE, state/nonce,
  ID token validation) and **GitHub OAuth** (not OIDC), configured as a list:
  `[[auth.providers]] kind = "oidc"|"github", id, name, issuer, client_id, client_secret, scopes,
  allow_registration, ...` (document; secrets also via env).
- Migration `0037`: `external_identities (account, provider_id, subject, email, created_at)`.
- Sign-in page buttons; first sign-in either links to an existing account (only after the user
  signs in to it — never auto-link by email unless the provider's email is verified **and** an
  admin setting allows it) or registers a new one if registration is allowed. Settings →
  Security lists linked identities with unlink (refuse unlinking the last way to sign in).
- 2FA: an external sign-in satisfies the password step; the account's own 2FA still applies
  unless config says the provider's MFA is trusted (default: still applies).
- Audit log entries; rate limits; tests against an in-process fake OIDC provider (JWKs, signed tokens).

## M — Importing and push mirrors

- **Import** (migration `0038` for import jobs/state if needed): from GitHub and Gitea/Forgejo
  via their REST APIs with a user-supplied token: the Git repository (with LFS objects only if
  L has landed — otherwise skip and note), labels, milestones, issues, pull requests (as closed/
  merged history with comments; refs fetched into `refs/pull/n/head`), releases (with assets).
  Authors map to local accounts by verified email or are shown as "originally by @x on GitHub".
  Runs as a background job with progress on a page; respects `allow_private_addresses`.
- **Push mirrors** (migration `0039`): per-repository remote URL + credentials (stored like
  webhook secrets: write-only), push on every push (debounced) and periodically, last result
  shown in settings; force-push mirror semantics; uses the `git` binary.
- Settings UI under the repository's Mirroring settings; import from the "New repository" page.
