# Milestone 3, workstream S — Commit statuses and required checks (as built)

GitHub-compatible commit statuses, so any external CI can report results, and required
status checks on protected branches. Contract: [../milestone-3.md](../milestone-3.md#s--commit-statuses-and-required-checks).

## As built

**Core** (`rbit_core::status`, new): `StatusState` (`pending|success|failure|error`),
`StatusContext` (trimmed, 1–255 characters, no control characters, case-sensitive),
`StatusReport::parse` (state; context defaults to `default`; description ≤ 140 characters,
one line; `target_url` an absolute `http`/`https` URL without credentials, ≤ 2048 bytes —
the `WebhookUrl` rules), `parse_sha` (40 or 64 hex digits, lower-cased), `CommitStatus`,
and the pure decisions: `combined_state` (GitHub's: `failure` if any latest status is
`failure`/`error`, `pending` if none or any `pending`, else `success`), `checks_status`
(→ `pull::ChecksStatus`, `NotConfigured` when nothing reported) and `required_checks`
(→ `RequiredChecks { required, passing, failing }`). Limits: `MAX_STATUSES_PER_CONTEXT`
= 1000 per (repository, commit, context), `MAX_REQUIRED_CONTEXTS` = 50 per rule.

`rbit_core::protection`: `RuleSettings.require_status_checks`, `ProtectionRule.required_contexts`,
and `MergeRules.{require_status_checks, required_contexts}` (union of the matching rules
that set the flag, sorted, deduplicated). Status checks never affect the push gate.

`rbit_core::pull`: `Requirement::RequiredChecks(RequiredChecks)` (a *protection*
requirement, so an administrator whose rules allow bypass may skip it, audited as before)
and `Requirement::Checks(ChecksStatus)` is now informational — always met, listed only
when the head has statuses ("All checks have passed", "Some checks were not successful",
…). `MergeFacts.required_checks` is new. Statuses nobody requires never block, as on
GitHub. The placeholder text "No status checks — not supported yet" is gone.

**Storage**: migration `0033_commit_statuses.sql` — `commit_statuses` (every report is a
row; the highest id per (repo, sha, context) is current; index `(repo_id, sha, context,
id)`; `CASCADE` with the repository, creator `SET NULL`; CHECKs on sha shape, state,
lengths), `branch_protections.require_status_checks` (default 0) and
`branch_protection_required_checks (rule_id CASCADE, context, PK)`.
`CommitStatusStore` (`Database::commit_statuses()`): `create` (insert under the cap in one
statement), `find`, `list` (newest first), `count`, `latest`, `latest_of` (many commits,
one `json_each` query). `BranchProtectionStore` reads/writes the contexts (one more query
per list). `PullStore::open_with_head_sha` added. `tests/upgrade.rs` has a separate test
for the new tables (start empty, defaults, CHECKs, cascade).

**Services**: `StatusService` (`Services::statuses`): `create` (write access, full sha that
exists in the repository — for fork PRs the head is in the base repository as
`refs/pull/{n}/head`; publishes `DomainEvent::CommitStatus { repo, status, sha, actor }`),
`resolve`, `list`, `combined`, `combined_for`, `combined_of`, `creators`.
`statuses::hooks::{status, payload}`: the JSON object and the GitHub-shaped `status`
payload (status fields at the top level, `commit` with message/author/committer/parents,
`branches` whose tip it is with `protected`, `repository`, `sender`, `organization`; no
`action`). `ProtectionService` validates contexts (1–50 distinct when the flag is set;
dropped without it, like allowed pushers); the audit summary says "status checks".
`PullService::merge_box` reads the head's latest statuses in the base repository
(`MergeBox.statuses`); `PullService::handle_status` (background subscriber on
`CommitStatus`) re-evaluates open pull requests whose head is that commit: stale or
unknown mergeability is marked unknown and the mergeability worker is woken, so the merge
button is ready when the checks pass (the checks themselves are always read live).

**Webhooks**: `WebhookEvent::Status` (`status`, "Statuses"), subscribable;
`WebhookService::handle_event` routes `CommitStatus` to `statuses::hooks::payload`.

**REST API** (`api/statuses.rs`, `openapi/statuses.json`):
`POST /repos/{owner}/{repo}/statuses/{sha}` (write scope + write access; 201 with
`Location`; 422 naming `state`, `target_url`, `description`, `context` or `sha`),
`GET /repos/{owner}/{repo}/commits/{ref}/statuses` (newest first, paginated, `X-Total-Count`)
and `GET /repos/{owner}/{repo}/commits/{ref}/status` (`state`, `statuses` = latest per
context (paginated), `sha`, `total_count`, `repository`, `commit_url`, `url`). Hidden
repositories are 404; unknown references 404. The branch protection object gains
`require_status_checks` and `required_status_checks`.

**Web**: the merge box lists the checks (state icon, context, description, "Required"
badge, "Details" link to `target_url` with `rel="nofollow noopener noreferrer"`), required
contexts without a report as "Expected"; required checks still running show as pending
(warning), failed ones as failures. The commit page shows a combined mark next to the
title and the checks under the header (`#checks`); the commit list shows the combined
mark per commit (one query per page). Branch protection form: "Require status checks to
pass" + a textarea of required checks (one per line); list badge "Status checks".
Templates `statuses/{mark,checks}.html`, view module `views/statuses.rs`. Status colours
only on the state marks.

**Checked by hand** at 375 px on a seeded instance: the merge box checks list, the
commit page checks and the commit list mark have no horizontal overflow; descriptions
truncate, "Details" stays on the row. `npm run build` produced no CSS change (all
classes already existed).

**Tests**: core unit tests (`status`, `protection`, `pull`), storage
`tests/statuses.rs` + `tests/protection.rs` + `tests/upgrade.rs`, services
`tests/statuses.rs` (permissions, validation, reading, event, `status` payload) +
`tests/protection.rs` + `tests/pulls.rs` (required checks gate, fork head, re-evaluation),
web unit tests (`api::statuses`, `views::statuses` incl. template render, `views::protection`),
e2e `tests/e2e/statuses.rs`.

## Deviations

- **Routing of the two `GET` endpoints.** `/commits/{*reference}` (a reference may contain
  `/`) cannot coexist with `/commits/{ref}/status` in axum's router. The wildcard's handler
  is now `statuses::commit_or_statuses`, which serves references ending in `/statuses` or
  `/status` and hands everything else to `repos::commit`. The API table gained
  `route_in_wildcard` entries (documented, not registered) so the OpenAPI equality test
  still checks them. A branch whose name ends in `/status` or `/statuses` can only be
  addressed by commit id there (documented in the fragment).
- **"Mergeability re-evaluates when a status arrives"**: statuses do not change the
  trial merge, and the merge box reads checks live, so the re-evaluation refreshes stale
  mergeability and wakes the worker rather than storing a checks state.
- **`require_status_checks` needs at least one context** (GitHub allows an empty list,
  which only means "up to date"; strict/up-to-date checks are not implemented).
- No extra throttle on status creation: the API's per-account `ApiRequest` budget applies
  and the per-context cap bounds storage.
- Required checks are only shown as "Required" to viewers with write access (the merge
  rules are computed for writers only, as approvals already were).
- `pulls::json`'s `mergeable_state: blocked` is still never produced (P left `blocked:
  false`); unchanged here.

## CHANGELOG entry

```markdown
### Milestone 3 — commit statuses and required checks

#### Added
- Commit statuses, compatible with GitHub's commit status API: external CI reports
  `pending`, `success`, `failure` or `error` per check (`context`) on a commit with
  `POST /api/v1/repos/{owner}/{repo}/statuses/{sha}` (a `write` token); read them with
  `GET …/commits/{ref}/statuses` and the combined status with `GET …/commits/{ref}/status`.
- Branch protection can require status checks: list the required contexts, and pull
  requests into matching branches merge only when each one's latest status on the head is
  `success` (administrators may bypass where the rule allows it).
- The pull request merge box lists the head's checks with links to their details; commit
  lists and the commit page show a combined status mark.
- A `status` webhook event (GitHub's payload shape).

#### Upgrade notes
- Migration `0033_commit_statuses` runs at start-up: two new tables and a
  `require_status_checks` column (off) on branch protection rules. Existing rules are
  unchanged.
- Pull requests no longer show "No status checks — not supported yet"; statuses nobody
  requires are shown but never block merging.
```

## README

Feature line (append to the Pull requests / Branch protection bullets, or a new one):

> - **Commit statuses**: GitHub-compatible status API for external CI, required status
>   checks on protected branches, check lists on pull requests and commits, `status` webhooks.

Configuration table: no new section (nothing configurable).

## `deploy/rbit.example.toml`

Nothing new (no configuration).

## ARCHITECTURE text

- §3.3 schema: `commit_statuses(id, repo_id → repositories CASCADE, sha, state CHECK IN
  ('pending','success','failure','error'), context, description, target_url, creator_id →
  accounts SET NULL, created_at, updated_at)`, index `(repo_id, sha, context, id)`; latest
  per (repo, sha, context) = highest id; ≤ 1000 per triple. `branch_protections.
  require_status_checks`; `branch_protection_required_checks(rule_id CASCADE, context, PK)`.
- §3.7 merges: requirements gain "required checks" (protection requirement, bypassable)
  and an informational "checks" line from the head's combined status (read in the base
  repository).
- §4.2 API, endpoints (milestone 3): *Commit statuses* — `POST /statuses/{sha}` (write),
  `GET /commits/{ref}/statuses`, `GET /commits/{ref}/status`; served below the commits
  wildcard by `statuses::commit_or_statuses`. Fragment list gains `statuses`.
- §4.3 webhooks: event `status` (no `action`; `id`, `sha`, `name`, `state`, `context`,
  `description`, `target_url`, `commit`, `branches`, timestamps, `repository`, `sender`).
  Payloads include the commit author's name and email inside `commit.commit.author`, as
  `push` payloads already do.
- Events: `DomainEvent::CommitStatus { repo, status, sha, actor }` (webhooks; the pull
  request service re-evaluates open pull requests with that head).

## Integration requests (files I touched that I do not own)

All edits are small and additive; on conflict keep both sides.

- `crates/rbit-web/src/routes/api/mod.rs`: `mod statuses;`, `FRAGMENTS` gains
  `("statuses", …)` (array length +1), `routes.extend(statuses::routes())` under a
  milestone 3 comment, **the commits wildcard route's handler changed to
  `statuses::commit_or_statuses`**, `ApiRoute.handler` is now an `Option` with
  `route_in_wildcard()` for documented-only entries, and `routes()` registers only entries
  with a handler. If A adds routes, keep both.
- `crates/rbit-web/src/routes/api/openapi.json`: `status` added to the `HookEvent` and
  delivery `event` enums (A may add `member`/`release`/`fork`: keep both).
- `crates/rbit-web/src/routes/api/openapi/protection.json`, `api/protection.rs`,
  `routes/protection.rs`, `views/protection.rs`, `templates/protection/form.html`:
  the required checks fields.
- `crates/rbit-core/src/webhook.rs`: `Status` appended (SUBSCRIBABLE is now 9).
  `crates/rbit-web/src/routes/webhooks.rs`: the form input lists events by hand, so it
  gained `event_status` (any new `WebhookEvent` from another workstream needs the same).
- `crates/rbit-services/src/{events.rs, lib.rs, webhooks.rs, recovery.rs, webhooks/payloads.rs}`:
  the event variant, service field, dispatch arm, exhaustive match arm, `action_of`.
- `crates/rbit-services/src/pulls/{merging.rs, background.rs}`, `crates/rbit-core/src/pull.rs`,
  `crates/rbit-storage/src/sqlite/pulls.rs`, `crates/rbit-web/src/views/pulls.rs`,
  `templates/pulls/merge_box.html`: checks in the merge box.
- `crates/rbit-web/src/views/{repo.rs, commit.rs}`, `routes/{repo.rs, history.rs}`,
  `templates/repo/{commits.html, commit.html}`: the status marks.
- `crates/rbit-e2e/tests/e2e/main.rs`: `mod statuses;`.
- Docs above (README, CHANGELOG, ARCHITECTURE) for the integrator.
