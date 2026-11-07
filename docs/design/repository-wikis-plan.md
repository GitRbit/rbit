# Repository wiki platform

Added to the comprehensive rewrite scope on 2026-11-06 at the user's request.
Status: new feature in W0 source preparation. Pure contracts/policy are prepared in
`rbit-wikis`; workspace registration and Rust/AST acceptance are pending. No runtime
wiki backend, routes, persistence or UI are implemented. See [the W0 contract
record](wiki-domain-contracts.md) for exact limits and remaining production gaps. Pinned main has no supported wiki feature: the older milestone-3
plan excludes it and its import design shows wiki import as unsupported. This addition
supersedes that scope exclusion for the rewrite, without rewriting historical evidence.

## Product and architecture

Introduce `rbit-wikis` as the cohesive owner of repository documentation: page identity,
validated titles/paths, revisions, rename/alias/link policy, navigation/search contracts,
mutation applications, media references and feature events. Keep Rust, Axum, Askama,
HTMX, Tailwind CSS v4, SQLite, redb and the existing Git implementation. There is no
new service process, runtime, renderer, per-page actor or crate per model.

Pure models and policies have no runtime, database, Git-process, HTTP or template
dependency. The directed domain graph is `rbit-wikis` to `rbit-contracts`, pure access
policy and repository public contracts; never repositories to the wiki implementation.
Use only the required model-only surfaces so full-server feature union cannot import
crypto/runtime into pure wiki contracts. Static ports include `WikiCatalog`,
`WikiRevisionStore`, `WikiMediaStore`, `WikiSearch` and `WikiRepositoryAccess` at real
I/O/authorization boundaries, with borrowed adapters and concrete dispatch on hot paths.
No universal context, generic plugin registry, boxed per-row futures or DTO copy tree.

SQLite adapters remain in storage; native Git operations remain in Git infrastructure;
web/presentation owns Askama components, localized views, HTTP negotiation and HTMX.
Composition connects these adapters to wiki commands and repository lifecycle effects.
Any new adapter-to-feature edge must pass the dependency/feature-union check before
implementation; the wiki domain never imports concrete storage/Git/web/core adapters.
No legacy core-to-wiki dependency is necessary for a new feature. Kernel owns bounded
startup, cancellation/drain, admissions and truthful health, not wiki authorization.
Disabled wikis create no worker and expose no accidental write/upload surface.

Suggested modules describe responsibilities rather than divide an oversized facade:
`pages`, `revisions`, `links`, `navigation`, `search`, `media`, `applications` and `ports`.
Use small reader/writer components with explicit configuration subsets. Every authored
file and cumulative logical type, including distributed implementations/tests, remains
at most 400 lines and enters the existing AST gate from its first production slice.

## Pages and a platform-native interface

Build a modern, polished documentation workspace matching the rest of rbit: consistent
repository header/tabs, typography, spacing, colors/themes, navigation and empty states;
a readable document canvas with page tree/sidebar, breadcrumbs, table of contents,
search and clear edit/history controls. Avoid a detached generic wiki skin.

Desktop navigation may remain visible; narrow screens use an accessible drawer and
single-column content. Keep long titles, code blocks, tables, nested navigation and
large diffs usable without whole-page horizontal overflow. Keyboard navigation,
visible focus, landmark/headings, accessible control names, non-color status cues,
reduced motion and announced validation/loading states are acceptance requirements.

The complete read/create/edit/preview/history/diff/restore/rename/delete/navigation
flows work without JavaScript through ordinary forms, links and redirects. HTMX adds
bounded in-place updates with preserved history/back/scroll/focus behavior; it is an
enhancement. Editor preview is optional/on-demand and must share the production safe
content policy, never a second renderer with weaker sanitization. Preserve exact
full-page/fragment negotiation, CSP, styles and conditional-cache semantics.

Localize all chrome, validation, errors, confirmation flows, accessibility text,
notifications, dates and pluralization from the beginning. Exercise English, a longer
translation and RTL layouts. User-authored page content is not silently translated.
Machine page IDs, revision IDs, API error codes and event names remain locale-independent.

A stable page ID survives title/slug changes. Provide versioned page contents and
metadata, author/time/history, revision comparison and restore-as-new-revision rather
than destructive history rewriting. Page deletion/tombstone and restoration semantics
must be explicit. Rename preserves authorized old links and detects namespace clashes;
reclaimed aliases cannot redirect to unrelated private content. Resolve links against
stable identities/aliases, expose broken links safely and avoid rewriting every page
on each rename. Define Unicode normalization, case sensitivity, percent decoding,
reserved names, relative links and anchors once, with adversarial compatibility tests.

Search and navigation respect the current repository viewer. Paginate results and
history, filter before revealing titles/snippets/counts and batch selected-page metadata.
Provide bounded title/content search and useful empty/no-results states; no per-row
permission SQL or eagerly rendering an entire wiki. Define media uploads/downloads,
quotas, allowed MIME types, references, retention and deletion separately from page text.
Media bytes use a bounded attachment/object-storage adapter with immutable content
identifiers and SQLite ownership; Git manifests reference them without requiring every
large upload in page text or redb. Validate actual reuse/GC/backup contracts before
selecting that adapter. A user-facing wiki Git remote/import is a separately gated ingress: storing revisions
in Git does not silently authorize an unvalidated push or enable currently unsupported
provider wiki imports. Any exposed ingress must use the same authority/CAS pipeline.

## Data authority and concurrency

Use stable repository ID for wiki ownership and disk layout, so owner transfer or
repository rename does not duplicate history or require page-path rewrites. Keep page
content/revisions and immutable revision-addressed media manifests in a dedicated Git
namespace/store isolated from repository code refs, hooks and Actions triggers.
Untrusted wiki content cannot create workflow executions or arbitrary Git hooks.

SQLite is authoritative for wiki catalog/page IDs, visible revision pointers, aliases,
media ownership, mutation intents and search-index revision. Git is authoritative for
immutable content/history bytes. redb may hold bounded, versioned, rebuildable rendered
content/navigation caches; it is never the only copy of a page, ACL or pending mutation.
A missing/corrupt derived cache must rebuild safely, without changing visible state.

Define one concrete commit protocol before adding writes; a candidate to validate is:

1. Authenticate/authorize and validate bounded input, expected page/wiki revision and
   repository state; create immutable Git objects without publishing a visible head.
2. Under a short SQLite write transaction, recheck authoritative expected revision,
   permissions/archive/deletion state and absence of another active wiki mutation;
   persist a fixed intent naming expected/target Git OIDs and affected stable IDs.
3. Update the managed Git ref with compare-and-swap. A retry recognizes its exact
   already-applied target; a different target is a conflict, not last-write-wins.
4. Finalize that same intent with SQLite CAS, updating visible revision/catalog/link
   projections and the index revision together. Readers use only finalized pointers.
   Record required downstream facts transactionally where delivery must survive crash.

This is a design candidate, not a claim of cross-store atomic commit. Prove each crash
window, cancellation and lost acknowledgment before adopting it. Bound active intents
and reconciliation; keep old content readable while pending or return an explicit
safe unavailable state if integrity is unproved. Do not hold SQL transactions across
Git/network work. Account for objects/refs/intent failure and orphan collection; never
silently publish mismatched catalog and content or overwrite a concurrent edit.

A stale edit/restore/rename/delete returns a conflict with useful compare/reload UI;
never force-push over another user's accepted edit. Concurrent aliases/media changes
must use the same authoritative versioning rather than read-then-write races. Search
indexing may be deferred only with visible revision validation, bounded recovery and
an honest staleness contract. Do not claim an outbox/recovery worker exists before it
is implemented, owned, drained and tested.

## Access, cache and untrusted content

Read access inherits the repository's current read policy; mutations inherit current
write/admin policy as appropriate. Private wikis cannot become publicly readable through
an independent switch. Preserve organization/team/collaborator, suspension, archive,
MFA/IP/throttle and scoped-token rules at the applicable existing chokepoints. Recheck
state at mutation commit; changed permissions or newly archived/deleted repositories
must defeat stale write authority. Hiding a UI control is not authorization.

Authorize before conditional 304/range responses, Git object/media access, history,
search, link previews or caches. Bound cache bytes/entries/work, with keys covering
repository/page/revision, locale and render-policy version plus applicable viewer/access
and full/fragment dimensions. Never share a private response through a public cache or
return a formerly private rendered body after an ACL change. Preserve Vary/ETag/history
semantics and avoid metrics that mix footer view time with total request time.

Use the existing reviewed Markdown/safe-content boundary; do not enable every parser
extension because this is called a wiki. Raw HTML, unsafe URLs, scriptable media,
DOM attribute injection and dangerous image/link schemes must be rejected or sanitized
by a documented shared policy. Autoescape titles, paths, search snippets, errors and
diff metadata. Apply CSP and safe attachment disposition/MIME/no-sniff rules; SVG/HTML
uploads cannot execute as same-origin application content. Test stored/reflected XSS,
malformed Unicode, path traversal, symlinks, device/reserved paths and oversized inputs.

No automatic server-side fetch of remote page images, embeds or arbitrary links.
Explicit remote import, if implemented, inherits existing outbound SSRF/DNS/redirect/
credential policies, finite byte/time limits and protected local-network restrictions.
CSRF, ordinary session/token authentication and explicit API scopes apply to writes.
Do not put secrets in URLs, content logs, cache keys or public fixtures.

## Lifecycle, backup and deletion

Repository rename/transfer preserves wiki identity, versions, links and media while
immediately applying the destination's current permissions. Do not retain stale team
access or cached private content. Repository archive blocks wiki content mutation,
restore, rename and media writes, with the same permission-before-archive error order;
read/history/export remain available to authorized viewers.

Coordinate repository/wiki deletion explicitly. A cleanup intent must survive removal
of the parent row until Git/media cleanup is proved; cascades must not erase the only
recovery evidence. Stop new writes, drain owned work, unpublish access, remove storage
under the repository lifecycle owner and retry bounded cleanup idempotently. Document
partial failure/reconciliation instead of adding a hidden detached cleanup task.

Backups include consistent SQLite catalog/intents plus reachable wiki Git objects and
media, required keys/config and format versions; redb derived cache may be rebuilt.
Pin referenced objects against GC during backup. Restore validates referenced content,
alias/media integrity and pending-intent recovery before writes reopen. Rehearse crash,
backup during editing, transfer/delete, interrupted restore and version downgrade. An
old binary that cannot understand wiki state must fail safely or use a tested restore;
do not promise rollback against incompatible writes.

## Phased delivery and acceptance

| Slice | Coherent production outcome | Required acceptance |
| --- | --- | --- |
| W0 Contracts and budgets | Static domain/ports, threat model, authority/commit ADR, schema/format and UI navigation contract | Dependency/feature-union and 400-line gates; defined finite page/media/search/cache/work budgets; no reverse feature cycle |
| W1 Read and platform UI | Authorized repository tab, page/navigation read, localized responsive Askama views and no-JS navigation | Private/public/archived/disabled fixtures; auth-before-304/media/history; mobile/keyboard/RTL; HTML/HTMX/history parity; no startup per-page tasks |
| W2 Versioned editing | Create/edit/preview/history/diff/restore with CAS and recoverable publication | Concurrent writer one-winner/stale conflict, every Git/SQL/commit/cancel crash window, restart/replay, exact escaping/safe preview, original read fast paths |
| W3 Complete documentation flows | Rename/aliases/link integrity, bounded search, navigation management and media | Alias reclaim/privacy, Unicode/path adversaries, selected-page work budgets, quotas/MIME/XSS/SSRF, no per-row queries; complete no-JS forms |
| W4 Lifecycle and operational parity | Transfer/rename/archive/delete, consistent backup/restore and upgrade, explicit API/optional Git/import ingress | ACL races and stale cache denial, retained cleanup intent, disk/SQL failure recovery, restore drill, protocol/error/scope contracts, no code-ref/Actions contamination |
| W5 Acceptance and dogfooding | Complete polished wiki used through ordinary rbit workflows | Browser visual/accessibility/localization gates, paired performance/footprint evidence, public-repository dogfooding and final feature ledger evidence |

Measure representative one-page, 100-page and 10,000-page wikis, varied text/media sizes,
large history/diffs and adversarial limit-sized content. At concurrency 1/4/16/64,
record p50/p95/p99/errors, request SQL/acquisitions, allocations/bytes, cache hits/RSS,
parser/render work, HTTP payload and browser interaction/layout behavior. Compare
matched default builds/data/locale/authorization/cache states; separate cold/warm and
full/fragment/304. Set numeric acceptance budgets from these measurements before
rollout; no unmeasured latency promise. Include build time/peak RSS/binary/dependency
footprint, bounded cache/disk growth and zero idle worker cost when disabled. A feature
port cannot regress existing repository read/clone/Actions paths without an explicit
reviewed tradeoff and evidence.

## Required local dogfooding

The local rbit instance must host public rbit main history and published rewrite commits
as ordinary repositories. Populate and update them through normal authenticated Git,
import and API/UI flows with ordinary users, ACLs, tokens and audit behavior. Exercise
browse/clone/fetch/push, issues/pulls, Actions and the new wiki when available. Main
reference and rewrite measurement fixtures remain separately labeled; dogfooding does
not change the rewrite's pinned 39f baseline or establish benchmark equivalence.

Use the existing scheduled pull-mirror workflow for the explicitly approved public
GitHub upstream `GitRbit/rbit`; it requires no upstream PAT for public content. Its
supported public upstream heads/tags expose main and published rewrite branches for
selection and validation. This plan does not invent a per-ref selector absent from
the mirror UI or require copying workstation refs. Verify the public upstream and
reported source OIDs; never mirror all local workstation refs/private worktree commits
or send local credentials/unintended namespaces upstream.

A pull mirror's Git contents are read-only locally. Test authenticated push using a
separate ordinary writable repository rather than bypassing that restriction. Local
issues and future wiki workflows use their own actual repository/feature policy; do
not assume that mirroring code grants a write or enables an unimplemented wiki.
Do not bypass authorization by inserting repositories/users/pages directly into
SQLite/redb or fabricating success through private APIs. Keep credentials out of
content and reports. Public fixture names, branches and pages use ordinary project
terminology and contain no private local paths or internal tooling branding. Record
tested public source OIDs, authorized operations and results without publishing
secret/session material. Failure must remain visible rather than silently rewriting
data to make the local platform appear healthy.
