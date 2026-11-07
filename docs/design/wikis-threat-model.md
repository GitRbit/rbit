# Repository wiki threat model

W0 contract for `rbit-wikis`, derived from the
[repository wiki plan](repository-wikis-plan.md). Status: design only. No wiki crate,
route, schema, store, cache or test described here is implemented, and every control
below is a requirement for a later slice, not a property of the code. Existing
chokepoints are cited by path; where the plan assumes one that does not exist, the gap
is stated rather than papered over. The write protocol is decided in the
[authority and commit ADR](wikis-authority-commit.md), formats and cache keys in the
[storage format](wikis-storage-format.md), limits in the [budgets](wikis-budgets.md)
and HTTP status codes in the [navigation contract](wikis-navigation-contract.md).

## Assets

- Page bodies, titles, paths, aliases, link graph and navigation tree of private wikis,
  including their existence, counts and modification times.
- Revision history: authors, timestamps, diffs and tombstoned or restored content.
- Media bytes and manifests, their MIME/disposition metadata and quota accounting.
- Authority: sessions, personal access tokens, workflow task tokens, repository grants,
  archive/suspension/deletion state, and the SQLite visible-revision pointers.
- The repository's code refs, hooks configuration and Actions trigger surface.
- Same-origin application integrity (scripts, forms, htmx targets, element ids).
- Server availability: parser, diff, search, Git and upload capacity shared with
  ordinary repository reads, clone and Actions.
- Backups and their key material; derived redb caches (integrity, not confidentiality
  of record, but they may hold private rendered bodies).

## Actors

| Actor | Capability assumed |
| --- | --- |
| Anonymous | Any request to any URL; public read baseline only |
| Read-only viewer | Current repository read; no wiki mutation |
| Writer | Current repository write; may author pages and media |
| Admin | Repository or site admin; settings, archive, transfer, delete |
| Revoked/suspended user | Holds a cookie, token or open tab issued before revocation |
| Stale session | Valid session whose grants changed (team removal, transfer) mid-flow |
| Scoped token | `TokenScope::Read`/`Write`, or a workflow task token, possibly leaked |
| Malicious page author | Writer who crafts Markdown, titles, paths, links and aliases |
| Malicious uploader | Writer who crafts media bytes, names and declared types |
| Hostile import source | Remote server, DNS and redirects controlled by an attacker |
| Operator | Host shell, config and backup access; trusted but error-prone |

A writer is trusted to change the wiki, not to run script in other viewers' sessions,
read other repositories, reach the host network or affect code refs and Actions.

## Trust boundaries

1. Browser/API to web handlers: every wiki read and write crosses the repository
   authorization boundary (`crates/rbit-services/src/repositories.rs`, `RepoContext`
   via `RepositoryService::open`/`open_page`, used by `open_pinned` in
   `crates/rbit-web/src/routes/repo.rs`), which delegates to the pure lattice in
   `crates/rbit-access/src/policy.rs` through `crates/rbit-auth/src/policy.rs`.
2. Authored content to HTML: the single Markdown boundary
   `crates/rbit-web/src/render/markdown.rs` (pulldown-cmark, then ammonia last, with
   `user-content-` id prefixing) and Askama autoescaping.
3. Wiki store to code repository: Git objects/refs, hooks and push-event derivation
   (`crates/rbit-git/src/push.rs`, `crates/rbit-services/src/actions/triggers.rs`).
4. Server to network: no wiki-initiated egress except an explicit, separately gated
   import using `GuardedResolver` (`crates/rbit-services/src/webhooks/delivery.rs`) and
   `crates/rbit-core/src/network.rs`.
5. Authoritative SQLite/Git to derived redb cache and HTTP caches
   (`crates/rbit-web/src/middleware/cache.rs`).
6. Live data to backup/restore and to older binaries
   (`crates/rbit-server/src/backup.rs`, `crates/rbit-storage/src/sqlite/schema.rs`).

## Threats and required controls

Each entry: threat; required control; chokepoint to reuse; acceptance test and slice.
"Gap" marks a chokepoint the plan presumes but the code does not provide.

### Information disclosure

**I1 Private content via search, snippets, counts, titles.** Search filters before it
ranks, counts or snippets; result totals reflect only readable pages; no per-row
permission SQL. Reuse the `readable_by!`/`readable_among` predicate in
`crates/rbit-storage/src/sqlite/repositories.rs` and the FTS escaping of
`fts_expression` in `crates/rbit-storage/src/sqlite/issue_search.rs`. Gap: no
cross-repository wiki index exists; scope search to one already-authorized repository
until a filtered index is designed. Test (W3): private-wiki fixture returns zero
results, zero count and identical timing class for anonymous, outsider and revoked
viewers; snippet text never appears in any denied response body.

**I2 Link previews, broken-link markers and alias redirects.** Resolving `[[Page]]` or
an alias must not reveal whether a page exists in a repository the viewer cannot read;
a reclaimed alias never redirects to unrelated or private content. Reuse the
moved-repository pattern in `crates/rbit-web/src/middleware/errors.rs`
(`moved_repository_location` redirects only when the viewer may read the target) and
`RepositoryService::moved_to`. Cross-repository links render as plain links, never
previews. Test (W3): alias created, page deleted, alias reclaimed by a new private
page; old URL yields not-found for a viewer without access and the new page for one
with access; link states are identical for missing and unreadable targets.

**I3 Caches, 304 and ETag after ACL change.** `html_caching` in
`crates/rbit-web/src/middleware/cache.rs` derives a weak ETag from the already
authorized, rendered body, so a denied viewer receives a non-200 and no validator.
Wiki handlers must not answer `If-None-Match` before authorization (the Actions run
badge path in `crates/rbit-web/src/routes/actions/runs.rs` handles validators itself;
do not copy that shape without an authorization-first review). `cache_pinned` permits
year-long caching only for anonymously readable repositories; a visibility flip must
not leave a pinned private fragment valid. The rendered-document cache in
`crates/rbit-web/src/render/document_cache.rs` is content-addressed and viewer-free;
this is safe only while rendering is a pure function of source and base. Wiki link
resolution depends on catalog state, so cached wiki bodies hold no resolved link
state: they are keyed by wiki ID, body OID and render-policy version, links resolve per
request after authorization, and any later cache of resolved links adds catalog
revision and head OID (storage format, redb cache tables). Test (W1, W4): revoke
team grant, then replay prior ETag, preload, HX and full requests; every one is
denied; flip public to private and confirm no pinned fragment is reused.

**I4 Media and history.** Media download, revision view, diff and restore preview
authorize against current repository read before reading bytes, including range
requests. Gap: there is no media store; the release-asset path
(`crates/rbit-web/src/routes/api/releases.rs`) is the nearest precedent; the planned
store is `<data_dir>/wiki-media/<repo_id>/<wiki_id>/` (storage format). Test (W2
history, W3 media): an unreadable revision or media content ID yields the same
not-found as a nonexistent one, before any Git or object-store read.

**I5 Secrets in URLs, logs and cache keys.** No token, session or signed capability in
wiki URLs or cache keys; page paths are user content and may be confidential titles.
Note `csrf_origin_check` in `crates/rbit-web/src/middleware/security.rs` logs the
request path at warn level: private page titles can reach logs through it. Test (W2):
log capture over the full fixture contains no token material; a W0 decision is
required on whether wiki paths are logged at all.

### Elevation and stale authority

**E1 ACL change, archive, transfer, delete during a mutation.** Recheck permission,
archive and deletion state inside the SQLite commit transaction, after Git object
creation and before the ref CAS (ADR step 2). Reuse `RepoContext::ensure_writable`
(archive after access, preserving error order) and `ProtectionService::check_push`
ordering in `crates/rbit-services/src/protection.rs`. Gap: today's checks run at
request time; no existing path rechecks authority inside a commit transaction. Test
(W2, W4): remove write between form load and commit; archive, transfer and delete
races; each yields one deterministic refusal and no visible revision.

**E2 Revoked session, suspended account, stale tab.** Reuse session resolution in
`crates/rbit-web/src/middleware/auth.rs` over `crates/rbit-storage/src/kv/sessions.rs`
(renewal cannot resurrect a revoked session) and suspended-token refusal in
`crates/rbit-web/src/routes/api/auth.rs`. The credential-fencing tables in
`crates/rbit-storage/src/sqlite/credential_revocations.rs` are inactive and must not be
cited as enforcement. Test (W2): suspend the author mid-edit; the submit fails.

**E3 Token scope.** API reads require `TokenScope::Read`; page create, edit, restore,
rename, delete, undelete and media writes require `TokenScope::Write` (the navigation
contract's "repo write"); any wiki-level setting requires `TokenScope::Admin`
(`crates/rbit-core/src/access_token.rs`, enforced by `Caller::require` in
`crates/rbit-web/src/routes/api/auth.rs`, which also refuses workflow tasks in
`require_user`). Workflow task tokens (`crates/rbit-auth/src/task_token.rs`) get no
wiki write unless a later design grants it explicitly. Gap: any wiki Git remote needs
its own scope mapping in the push gate. Test (W4): scope matrix per endpoint; a Read
token cannot upload, a task token cannot edit.

**E4 Independent public switch.** Wiki visibility derives from repository visibility
only. Test (W1): no settings field or API can make a private repository's wiki
anonymously readable.

### Tampering and injection

**T1 Stored/reflected XSS through Markdown.** Render pages, previews and diffs of
rendered output only through `render_markdown` in
`crates/rbit-web/src/render/markdown.rs`; preview reuses the bounded route shape of
`crates/rbit-web/src/routes/markdown_preview.rs`. Enabling new parser options (wiki
links, math, raw embeds) requires a policy review; ammonia stays the last step. Mermaid
remains confined to the sandboxed frame served by `crates/rbit-web/src/assets.rs`.
Test (W2): a hostile-Markdown corpus including wiki-link payloads produces
byte-identical sanitized output in view, preview, history and HTMX fragments.

**T2 Titles, paths, snippets, diff metadata, errors.** Autoescape via Askama; never
pass a localized string or user field through a trusted-HTML wrapper. Search snippets
are escaped text with server-inserted highlight markup, not sanitized HTML. Test (W2,
W3): payloads in title, path, alias, commit message, snippet and validation error.

**T3 Uploads: SVG/HTML, MIME sniffing.** Reuse the raw-file stance in `raw`
(`crates/rbit-web/src/routes/repo.rs`): `sandbox; default-src 'none'` CSP and `nosniff`
on every media response; non-image types as `attachment` with an RFC 5987 filename as
in `crates/rbit-web/src/routes/api/releases.rs`. Server decides type from an allowlist
and content checks, never from the client header. SVG, HTML, XML, JavaScript and
unknown types are rejected at upload; the allowlist is in the budgets (media). Global
headers come from
`security_headers` in `crates/rbit-web/src/middleware/security.rs`. Test (W3):
HTML/SVG/polyglot uploads opened directly execute nothing; mislabelled types refused.

**T4 Unicode, traversal, reserved names, symlinks.** Define one canonical page-path
form. Reuse `FilePath` in `crates/rbit-core/src/file_edit.rs` (no `.`/`..`, `.git`
spellings, control characters, backslash, length caps). Gap: it applies no Unicode
normalization, no Windows device names (`CON`, `NUL`, `COM1`), no bidi/zero-width or
confusable policy, and the workspace has no normalization dependency; case-alias
handling exists only for branch protection (`crates/rbit-core/src/protection.rs`).
Wiki trees written by rbit contain no symlinks or gitlinks; any ingress rejects them.
Test (W3): NFC/NFD pairs, case aliases, `%2e%2e`, overlong and reserved names,
symlink entries in imported trees.

**T5 Git namespace contamination.** Wiki commits must never run hooks, appear as code
refs, trigger Actions or flow to mirrors and forks. Reuse `hardened` Git invocation
(`core.hooksPath` null) in `crates/rbit-git/src/merge.rs` for every Git command on a
wiki repository. Gap: push
events are derived by diffing all refs (`RefSnapshot` in `crates/rbit-git/src/push.rs`,
which already admits racing pushes reporting each other's updates), and
`evaluate_push` in `crates/rbit-services/src/actions/triggers.rs` applies no ref-prefix
filter. A wiki namespace inside the code repository could therefore reach webhooks and
Actions. Forks (`crates/rbit-git/src/fork.rs`, `clone --local`) hard-link the parent's
object files. Control (decided in the ADR): each wiki is a separate bare repository at
`<data_dir>/wikis/<repo_id>/<wiki_id>.git` with the single ref `refs/rbit/wiki/head`,
which no transport, fork, mirror or Actions path opens. A reserved, push-refused,
unadvertised namespace inside the code repository plus a trigger filter is a rejected
alternative. Test (W4): concurrent code push and wiki edit create no run, webhook or
code ref; mirrors (`crates/rbit-git/src/mirror.rs` heads/tags refspecs) and forks
contain no wiki refs or objects.

**T6 Concurrent overwrite.** Ref CAS plus SQLite CAS on expected revision; a different
target is a conflict, never last-write-wins. Test (W2): N writers, one winner.

### Request forgery and network

**R1 CSRF.** All browser writes are POST through `csrf_origin_check`
(`crates/rbit-web/src/middleware/security.rs`) and form-post `rate_limit`; no GET
mutation, including restore and alias claim. Preload exclusion (`is_unsafe_to_preload`
in `crates/rbit-web/src/middleware/cache.rs`) must cover any wiki GET with side effects
(there should be none). Test (W2): cross-site POST refused; preload of every wiki GET
changes no state.

**R2 SSRF.** No automatic server fetch of remote images, embeds, link previews or
unfurls. The browser may load remote images when `external_images`
(`crates/rbit-core/src/config/security.rs`) sets `img-src https:`; that is a
viewer-privacy matter, not server egress. Explicit import reuses `GuardedResolver`,
`is_internal_address`, pinned resolution (`ResolvedHost` in
`crates/rbit-git/src/remote.rs`) and the bounded client in
`crates/rbit-services/src/imports/client.rs`. Test (W3, W4): a network-denied harness
shows zero outbound connections while rendering hostile pages; import to loopback,
rebinding DNS and redirect targets is refused.

### Denial of service

**D1 Parser and render.** Bound source bytes before parsing (the preview route sets a
body limit; the issue `MAX_TEXT_BYTES` cap reexported by `crates/rbit-core/src/issue.rs`
is the precedent), nesting, link and heading counts, and the rendered-cache byte
budget. Gap: `render_markdown` has no wall-clock or output-size bound of its own; W0
must set page-size budgets that keep worst-case render within a measured ceiling.

**D2 Diff and history.** Reuse `DIFF_DEADLINE` and display limits in
`crates/rbit-git/src/diff.rs`; paginate history; cap restore and compare to two
revisions. **D3 Search.** Bounded query terms, result pages and index work per
mutation; reads are not rate limited today (`rate_limit` throttles posts only), so
search needs admission. **D4 Uploads.** Per-request body limit, per-wiki quota enforced
atomically in storage (as the audit did for release-file counts), cancellation cleanup
owned as for release uploads. **D5 Catalog and links.** Page, alias, link and
navigation-depth caps; rename never rewrites every page. Test (W2-W4): limit-sized and
over-limit adversarial fixtures at concurrency 64 keep ordinary repository reads
serviceable, per `docs/design/rewrite/security-contracts.md`.

### Lifecycle, backup and downgrade

**L1 Delete and cleanup.** Cleanup intent survives parent-row removal; storage keyed by
repository ID and the AUTOINCREMENT wiki incarnation ID, so neither a reused name nor a
reused repository ID inherits a wiki. Test (W4): delete, recreate with the same name
(and, where SQLite reuses it, the same repository ID), confirm empty wiki and no alias
carryover.

**L2 Backup completeness and consistency.** `crates/rbit-server/src/backup.rs` snapshots
SQLite, then `clone --mirror`s repositories, attachments and LFS listed in that
snapshot. Gap: wiki repositories and wiki media are not covered (the rejected in-repo
namespace would also have been copied unpinned against the SQLite snapshot). W4 must
add both, with the visible pointer taken from the snapshot. Test (W4): backup
during editing restores to a catalog whose every pointer resolves.

**L3 Restore and downgrade abuse.** Restore validates content, alias and media
integrity and recovers pending intents before writes reopen. `migrate` in
`crates/rbit-storage/src/sqlite/schema.rs` is the only version fence; whether an older
binary refuses newer applied migrations must be tested, not assumed. An older binary
must not serve wiki-bearing data with wiki authorization absent. Test (W4): restore
drill and old-binary start against new data fail safely.

## Out of scope

- Real-time collaborative editing, comments on pages, page-level ACLs narrower or wider
  than the repository, and anonymous editing.
- Server-side rendering of math, diagrams or embeds beyond the existing Mermaid frame.
- Provider wiki import and a user-facing wiki Git remote until separately gated (W4).
- Host compromise, malicious operators, and browser or OS vulnerabilities.
- Defending content a viewer is authorized to read from being copied by that viewer.

## Residual risks

- Content-addressed render caching leaks only through timing that a reader could
  already observe; accepted while lookups follow authorization.
- Browsers fetching author-chosen remote images disclose viewer IP and timing to third
  parties when `external_images` is enabled.
- Confusable and bidi titles can mislead readers even with normalization.
- Writers can fill quota and history; the bound is per-wiki, not per-author.
- No cross-store atomic commit: crash windows rely on the unproved intent protocol.
- Private page titles may appear in operator logs until the logging decision in I5 is
  made and enforced.
