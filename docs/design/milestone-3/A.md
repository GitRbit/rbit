# Milestone 3 — A: REST API continuation (as built)

Workstream A of [milestone 3](../milestone-3.md): the REST API endpoints wave B deferred —
collaborators, teams, releases (with streamed file uploads) and forks — plus the `member`
webhook. No migration was needed (`0026`–`0031` stay unused).

## As built

Every area is its own child module of `crates/rbit-web/src/routes/api/` with its own
`OpenAPI` fragment (`openapi/{collaborators,teams,releases,forks}.json`), appended to
`api::table()` and `FRAGMENTS` under `// Milestone 3: REST API continuation (A).`; the
existing route/document equality test covers them. Handlers stay thin: every rule lives in
a service, and the services gained what the API needed (with tests).

| Area | Endpoints | Scope and rights |
|---|---|---|
| Collaborators | `GET /repos/{o}/{r}/collaborators`, `GET\|PUT\|DELETE …/collaborators/{username}`, `GET …/collaborators/{username}/permission` | `admin` scope and repository administrator (403 if the repository is visible, 404 otherwise); a collaborator may `DELETE` themselves |
| Teams | `GET\|POST /orgs/{org}/teams`, `GET\|PATCH\|DELETE …/teams/{team_slug}`, `GET …/members`, `GET\|PUT\|DELETE …/memberships/{username}`, `GET …/repos`, `GET\|PUT\|DELETE …/repos/{owner}/{repo}` | `read` to read (organisation members and site administrators; 404 for everyone else); `admin` scope and organisation owner to manage (403 for members); members may leave |
| Releases | `GET\|POST /repos/{o}/{r}/releases`, `GET …/releases/latest`, `GET …/releases/tags/{tag}`, `GET\|PATCH\|DELETE …/releases/{release_id}`, `GET\|POST …/releases/{release_id}/assets`, `GET\|DELETE …/releases/assets/{asset_id}` | `read` for `GET`; `write` scope and write access for everything else; drafts exist only for writers |
| Forks | `GET\|POST /repos/{o}/{r}/forks` | `read` to list (only forks the caller may read); `write` scope to fork (anyone who can read the parent) |

Shapes follow GitHub's: collaborators carry `permissions` (`admin`, `maintain`, `push`,
`triage`, `pull`) and `role_name`; `PUT` of a collaborator answers `201` with the
collaborator when added and `204` when an existing collaborator's permission changed; the
permission endpoint answers `{permission, role_name, user}` (`none` when the user cannot
read). Teams have `slug` (= name), `members_url`, `repositories_url`, counts and their
`organization`; membership `PUT` is idempotent and answers `200 {url, role, state}`.
Releases are the same object webhooks send, now with GitHub's API links (`url`,
`assets_url`, `upload_url` as `…/assets{?name,label}`, and each file's `url`). Creating a
release whose tag does not exist creates it at `target_commitish` (default: the default
branch), reported like a push. `POST …/forks` answers `202` with the full repository
object including `parent`.

**Streaming routes.** `ApiRoute` gained a `streaming` flag (`route(…).streaming()`).
`api::routes()` partitions the table: ordinary routes get the existing layers (1 MiB body
limit, 30 s timeout, gzip, `nosniff`, `no-store`); streaming routes get only `nosniff` and
`no-store` and are merged *after* those layers, so the same table and the same equality
test cover them. Two routes stream:

- `POST …/releases/{release_id}/assets?name=` — the raw request body is the file
  (GitHub's `upload_url` protocol; `Content-Type` is recorded, `label` ignored). It streams
  through `ReleaseService::upload_asset` into `<attachments_dir>/tmp/*.part`, hashed as it
  goes. 413 at once when `Content-Length` exceeds `[releases] max_asset_bytes`, and as soon
  as more than that has arrived otherwise (bytes are counted on the way in, so "too large"
  is told apart from other refusals); 408 after `[releases] upload_timeout_seconds`; 409
  for a taken file name; 422 for a bad or missing name or too many files.
- `GET …/releases/assets/{asset_id}` — JSON by default; with
  `Accept: application/octet-stream` the file itself, streamed uncompressed with
  `Content-Disposition: attachment`, `Content-Length`, an `ETag` (SHA-256) and
  `Content-Security-Policy: sandbox`, counted as a download. Works with a token for
  private repositories (the web `browser_download_url` needs a session).

**Service additions** (all tested in `rbit-services/tests/`):

- `CollaboratorService::set` (idempotent add-or-change) and `access_of` (a user's access
  from every grant, through `viewer_grants` + `repo_access`, the policy every check uses).
  Collaborator changes publish `DomainEvent::CollaboratorChanged` (new, appended).
- `TeamService::ensure_member` (idempotent add). `find_repository` now refuses
  `other-owner/name` instead of silently mapping it to the organisation's repository of the
  same name (a fix the API path needed; the web form behaves the same for plain names).
- `ReleaseService::page`, `people`, `asset`, `open_asset_by_id`;
  `ReleaseStore::find_asset_by_id`.
- `ForkService::page` (with a total); `ForkStore::list_readable` takes an offset.

**Webhooks.** `release` (`published`) and `fork` already existed with GitHub shapes; the
release object gained the API links above. New: **`member`** (`WebhookEvent::Member`,
subscribable like any other event, label "Collaborators"): `action` `added` / `edited` /
`removed`, `member`, `changes.permission` (`to` when added; `from` and `to` plus GitHub's
older `old_permission.from` when edited), `repository`, `sender`, `organization`. No event
when a permission is "changed" to the same value. No email addresses.

**Tests.** Per area in-process API tests (success, 403/404, validation; shared fixture in
`api/test_support.rs`), service tests, storage tests, a payload unit test and a delivery
test for `member`, and the e2e scenario `crates/rbit-e2e/tests/e2e/api_workflow.rs`
(release + 1.5 MiB streamed upload + download, collaborator who then pushes, fork via the
API, removal refusing the next push).

## Deviations

1. **Scopes for deletions.** Deleting a release or a release file needs the `write` scope,
   not `admin`: in rbit those are writers' actions (`ReleaseService`), like deleting a label
   or milestone, which already use `write`. `admin` is used where the action needs
   administrator rights: all collaborator endpoints (including the `GET`s, whose data is
   administrators-only, exactly like branch protection) and team management.
2. **Collaborators** lists direct collaborators only (GitHub's `affiliation=direct`);
   people are added directly (no invitations), so `PUT` answers `201` with the
   collaborator rather than an invitation. Permissions are rbit's three; `pull`/`push` are
   accepted, `triage`/`maintain` are 422. In `permissions`, `maintain` = `admin` and
   `triage` = `push`.
3. **Teams** are addressed by name only (no legacy `/teams/{id}` routes), have no parent
   (`parent: null`), no maintainers (`role` is always `member`), `privacy: "closed"` and
   GitHub's legacy `permission: "pull"`.
4. **Releases**: no `target_commitish` in responses (rbit does not store it); `latest`
   prefers full releases but falls back to a pre-release (GitHub ignores pre-releases);
   file `label` is accepted and ignored, files cannot be renamed (no `PATCH` of an asset);
   a second release for a tag and a taken file name are 409 (rbit's convention) where
   GitHub answers 422; the file download streams directly (200) instead of GitHub's 302 to
   a signed storage URL.
5. **Forks** answer `202 Accepted` like GitHub although the fork is complete when the
   response is sent; there is no `sort` (newest first only) and `default_branch_only` is
   ignored (every branch and tag is copied).
6. An upload cut off by the timeout leaves its `.part` file until the start-up sweep
   (the web upload route behaves the same).
7. `member` covers collaborators only; team grants and team membership changes are not
   reported (GitHub's separate `membership`/`team` events — follow-up).
8. Commit gates: every commit passed `cargo fmt --all --check` and
   `cargo clippy --workspace --all-targets -- -D warnings`; the intermediate area commits
   (collaborators, teams, releases) ran the tests of the crates they touch (rbit-web, the
   touched rbit-services/rbit-storage test binaries); `cargo test --workspace` ran in full
   on the forks commit and on the final state.

## CHANGELOG entry

Under `## [Unreleased]`, `### Milestone 3`, `#### Added`:

```markdown
- **REST API: collaborators, teams, releases and forks.**
  `GET /repos/{owner}/{repo}/collaborators`, `GET|PUT|DELETE …/collaborators/{username}`
  and `…/collaborators/{username}/permission`; organisation teams under
  `/orgs/{org}/teams` with members (`…/memberships/{username}`) and repository grants
  (`…/repos/{owner}/{repo}`); releases (`…/releases`, `/latest`, `/tags/{tag}`,
  `/{id}`) with attached files — uploaded through the release's `upload_url`
  (`POST …/releases/{id}/assets?name=`, the raw body is the file, streamed under the
  `[releases]` limits) and downloaded with `Accept: application/octet-stream`; forks
  (`GET|POST …/forks`). Shaped like GitHub's and documented in the OpenAPI document.
- **`member` webhook** when a collaborator is added, changes permission or is removed.
```

`#### Changed`:

```markdown
- Release objects in webhook payloads (and the API) carry GitHub's API links: `url`,
  `assets_url`, `upload_url` and each file's `url`.
- Granting a team `owner/name` of a different owner is refused instead of granting the
  organisation's repository of that name.
```

`#### Upgrade notes`: none needed beyond "the `member` webhook event is new; existing hooks
are not subscribed to it until you tick it". No migrations.

## README

Feature line (replace the Automation bullet's list):

```markdown
- **Automation**: a versioned REST API (`/api/v1`, OpenAPI 3.1: repositories, issues, pull requests,
  collaborators, teams, releases with file uploads, forks, …) and HMAC-signed, GitHub-shaped webhooks
  (push, issues, pull requests, reviews, releases, collaborators, …) with a persistent, retrying delivery queue.
```

Configuration table: no new section; the `[releases]` row could read
"Release files (web and API uploads)".

## `deploy/rbit.example.toml`

No new keys. Suggested comment change in `[releases]`:

```toml
[releases]
# Files attached to releases live in <data_dir>/attachments (included in backups).
# Largest attached file, in bytes (512 MiB), for web and API uploads
# (POST /api/v1/repos/{owner}/{repo}/releases/{id}/assets). Raise the reverse proxy's
# body limit to match.
max_asset_bytes = 536870912
max_assets_per_release = 50
# Upper bound for one upload (web or API); uploads are exempt from the 30-second page and
# API timeouts.
upload_timeout_seconds = 3600
```

## ARCHITECTURE text

In §4.2 after "Endpoints (wave B)":

```markdown
- **Endpoints (milestone 3)** — `read` for `GET`, `write` for release and fork changes,
  `admin` for collaborators (reads included: the data is administrators-only) and team
  management:
  - *Collaborators* — `GET /repos/{owner}/{repo}/collaborators` (direct collaborators),
    `GET|PUT|DELETE …/collaborators/{username}` (`PUT` `{permission}` → 201 added / 204
    changed; a collaborator may remove themselves), `GET …/collaborators/{username}/permission`
    (the effective access from every grant, `none` included).
  - *Teams* — `GET|POST /orgs/{org}/teams`, `GET|PATCH|DELETE /orgs/{org}/teams/{team_slug}`
    (slug = name), `GET …/members`, `GET|PUT|DELETE …/memberships/{username}` (idempotent
    `PUT`), `GET …/repos`, `GET|PUT|DELETE …/repos/{owner}/{repo}` (`{permission}`; only the
    organisation's own repositories). Visible to organisation members and site
    administrators only.
  - *Releases* — `GET|POST …/releases`, `GET …/releases/latest`, `GET …/releases/tags/{tag}`,
    `GET|PATCH|DELETE …/releases/{release_id}`, `GET|POST …/releases/{release_id}/assets`
    (`POST ?name=` with the file as the raw body, GitHub's `upload_url`),
    `GET|DELETE …/releases/assets/{asset_id}` (`Accept: application/octet-stream` streams
    the file). Drafts exist only for writers.
  - *Forks* — `GET|POST …/forks` (`{organization, name}` → 202 with the repository and its
    `parent`).
- **Streaming routes** — a table entry marked `.streaming()` (the release file upload and
  download) is served without the 1 MiB body limit, the 30 s timeout and compression:
  `api::routes()` merges those routes after the shared layers. Their handlers bound
  themselves: `[releases] max_asset_bytes` (413, at once from `Content-Length`, otherwise
  as the bytes arrive) and `upload_timeout_seconds` (408).
```

And change the OpenAPI bullet's fragment list to
`api/openapi/{issues,notifications,pulls,protection,collaborators,teams,releases,forks}.json`.

In §4.3 Webhooks: add `member` to the event list in **Fields**, and to **Payloads**:
"`member` (`added`, `edited` with `changes.permission.from/to` and `old_permission.from`,
`removed`; `member`)"; the `release` object now includes `url`, `assets_url`,
`upload_url` (`{?name,label}`) and each asset's `url`. In §2's event table, add the row
`` | `CollaboratorChanged` | `repo`, `user`, `actor`, `change` | `CollaboratorService` (added, permission changed, removed) | ``.

In the roadmap table (§9), drop "API endpoints for collaborators, teams, releases and
forks" from **Next**. In HANDOFF, §2 item 5 and §5 item 2 are done.

## Integration requests

1. **Merging with S** (commit statuses), which touches the same registration points:
   - `rbit-core/src/webhook.rs`: both add a `WebhookEvent` variant before `Ping` and grow
     `SUBSCRIBABLE` (A: `Member`; S: `Status`) — keep both, the array becomes `[Self; 10]`,
     and keep both `as_str`/`label`/`description` arms.
   - `rbit-services/src/webhooks/payloads.rs` `action_of`: keep both new arms.
   - `rbit-services/src/events.rs`: both append a `DomainEvent` variant — keep both; A also
     adds the `CollaboratorChange` enum after the `DomainEvent` enum.
   - `rbit-services/src/recovery.rs` `handle`: the match over `DomainEvent` is exhaustive;
     A added `| DomainEvent::CollaboratorChanged { .. }` to the ignored arm, S's variant
     needs the same (any other exhaustive match S adds needs A's variant).
   - `rbit-web/src/routes/api/openapi.json` `HookEvent` enum: A appended `"member"`, S
     appends `"status"` — keep both.
   - `rbit-web/src/routes/api/mod.rs`: A changed `ApiRoute` (a `streaming` field and
     method), `route()`, `routes()` (partition + second router) and the module docs, and
     appended its child modules (`collaborators`, `forks`, `releases`, `teams`,
     `#[cfg(test)] test_support`), table lines and `FRAGMENTS` entries under a
     `// Milestone 3: REST API continuation (A).` comment. S's additions are separate
     appends — keep both and bump the `FRAGMENTS` array length to the sum.
2. **H (token prefix)**: the API docs in this workstream don't mention the token prefix;
   nothing to change. `openapi.json` `info.description` still says `rit_` — H's sweep.
3. The documentation texts above (CHANGELOG, README, ARCHITECTURE, example config,
   HANDOFF) for the integrator to apply.
