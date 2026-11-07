# Repository wiki budgets

Status: W0 design. Nothing here is implemented or measured. This document defines the
finite page, media, search, cache and work bounds that the
[repository wiki plan](repository-wikis-plan.md) requires before W1, and the protocol
that turns proposed defaults into numeric acceptance budgets. It promises no latency.
Write ordering is in the [authority and commit ADR](wikis-authority-commit.md), table
and cache-key formats in the [storage format](wikis-storage-format.md), HTTP behavior
in the [navigation contract](wikis-navigation-contract.md) and attacker goals in the
[threat model](wikis-threat-model.md).

## How to read the tables

Each bound has a provenance tag:

- **D** derived from an existing rbit limit, cited by path and value. Aligning with it
  keeps wiki behavior consistent with the repository paths it sits beside.
- **P** proposed default pending measurement. The number is a starting point for the
  fixtures below and must be confirmed, lowered or justified before rollout.
- **R** policy bound fixed by correctness or security, not by performance. Measurement
  may lower its default but cannot raise its ceiling without a design change.

"Ceiling" is the largest value an operator may configure. Configurable bounds live in a
new `[wikis]` section of `rbit.toml`, following the existing convention
(`crates/rbit-core/src/config/`: one struct per section, `serde(default,
deny_unknown_fields)`, `RBIT_*` environment overrides). As with
`security.max_password_jobs` (1..=64, rejected at composition in
`crates/rbit-core/src/config/security.rs`), a value of zero or above the ceiling is a
startup configuration error, never a silent clamp. Bounds marked "no" are code
constants with tests at the boundary and boundary-plus-one. The `[wikis]` section holds
the instance switch `enabled` and every bound marked "yes" below; exact key names are
fixed with the W1 configuration struct. This document is the only owner of that
section; the other wiki documents refer to it.

Every rejection below happens before the expensive work it names. Validation errors
re-render the no-JavaScript form (or the HTMX fragment) with a localized, announced
field message and preserve the user's input; they never create Git objects, intents,
cache entries or index rows. "Busy" means admission refusal under the
[kernel blocking-work contract](kernel-contracts.md) (immediate for blocking slots,
after the bounded wait for the per-wiki writer lock). Every Busy is the existing
overload response: `503` with `Retry-After` and `Cache-Control: no-store`
(`WebError::Unavailable` and `unavailable_response` in `crates/rbit-web/src/error.rs`),
the status the repository Git path uses when `max_git_processes` is full. A refused
form POST keeps the user's text in its body. Busy is never reported as a conflict.

## Existing limits this design aligns with

| Existing limit | Value | Source |
| --- | --- | --- |
| Inline render/edit size, `[repository] max_render_bytes` | 1 MiB | `crates/rbit-core/src/config/mod.rs` |
| Page router form body, `MAX_FORM_BYTES` | 1 MiB | `crates/rbit-web/src/routes/mod.rs` |
| Page router timeout, `PAGE_TIMEOUT` | 30 s | `crates/rbit-web/src/routes/mod.rs` |
| API JSON body, `MAX_BODY_BYTES` | 1 MiB | `crates/rbit-web/src/routes/api/mod.rs` |
| Editor body: text bytes x6 encoding factor + 256 KiB form overhead | derived | `crates/rbit-web/src/routes/edits.rs` |
| Upload: `[editor] max_upload_bytes` / `max_upload_files` / timeout | 50 MiB / 100 / 600 s | `crates/rbit-core/src/config/editor.rs` |
| Release asset: `max_asset_bytes`, +1 MiB `UPLOAD_OVERHEAD_BYTES` body slack | 512 MiB | `crates/rbit-core/src/config/releases.rs`, `crates/rbit-web/src/routes/releases.rs` |
| LFS object `max_object_bytes` | 2 GiB | `crates/rbit-core/src/config/lfs.rs` |
| Attachment download headers: octet-stream, `attachment`, `nosniff`, `sandbox` CSP | policy | `crates/rbit-web/src/routes/releases.rs` |
| Issue/PR title `MAX_TITLE_CHARS`; body `MAX_TEXT_BYTES` | 256 chars; 64 KiB | `crates/rbit-collaboration/src/threads/text.rs` |
| Repository name `MAX_LEN` | 100 | `crates/rbit-repositories/src/names.rs` |
| Diff `DiffLimits` default: files / bytes per side / total lines | 300 / 512 KiB / 20,000 | `crates/rbit-git/src/changes.rs` |
| Diff fallback `DIFF_DEADLINE`; `CONTEXT_LINES` | 500 ms; 3 | `crates/rbit-git/src/diff.rs` |
| Highlight input / line length | 512 KiB / 5,000 | `crates/rbit-web/src/render/highlight.rs` |
| Highlight cache bytes / jobs | 32 MiB / 4 | `crates/rbit-web/src/render/highlight_cache.rs` |
| Markdown `DocumentCache`: bytes / small-input bypass / entry <= 1/4 cache | 8 MiB / 512 B | `crates/rbit-web/src/render/document_cache.rs` |
| Blame cache entries / bytes / jobs / input | 128 / 32 MiB / 4 / 1 MiB | `crates/rbit-services/src/blame.rs` |
| List page size `LIST_PAGE_SIZE`; ref picker | 100; 50 | `crates/rbit-web/src/views/pagination.rs`, `docs/performance-listings.md` |
| Commit history page `COMMITS_PER_PAGE` | 30 | `crates/rbit-web/src/routes/repo.rs` |
| Explore page / search text `MAX_SEARCH_LEN` (truncated) | 30 / 100 | `crates/rbit-repositories/src/discovery/application.rs` |
| Timeline page `MAX_TIMELINE_PAGE` | 200 | `crates/rbit-collaboration/src/timeline.rs` |
| redb insights cache max entries / age | 50,000 / 30 days | `crates/rbit-core/src/config/operations.rs` |
| Background `job_concurrency`; maintenance interval | 2; 6 h | `crates/rbit-core/src/config/operations.rs` |
| Directory metadata: slots / workers / slot bytes / reuse | 32 / 4 / 512 KiB / 30 s | `docs/performance-listings.md` |
| SQLite pool connections | parallelism clamped 2..=16 | `crates/rbit-storage/src/sqlite/mod.rs` |
| Password `BlockingBudget` `max_password_jobs` | 2 (1..=64) | `crates/rbit-core/src/config/security.rs` |

Two gaps matter. The shared Markdown renderer (`crates/rbit-web/src/render/markdown.rs`)
has no explicit output-size or nesting bound and runs synchronously on the request
task; READMEs are bounded only by `max_render_bytes` at the call site. No repository
file path length/depth constant and no per-repository attachment quota were found.
The wiki bounds below must not be satisfied by a second, weaker renderer.

## Page identity and content

| Bound | Default | Ceiling | Config | Enforcement point | Outcome | Tag |
| --- | --- | --- | --- | --- | --- | --- |
| Title length | 256 chars | 256 | no | Domain title type, before slug derivation or Git work | Field error | D (`MAX_TITLE_CHARS`) |
| Path segment length | 100 bytes UTF-8 after normalization | 100 | no | Domain path type, before alias/catalog lookup | Field error | D (`RepoName::MAX_LEN`) |
| Path total length | 512 bytes | 512 | no | Same | Field error | R |
| Path depth | 8 segments | 16 | yes | Same | Field error | P |
| Page body | 512 KiB after CRLF normalization | `[repository] max_render_bytes` (1 MiB default) | yes | Route `DefaultBodyLimit` = body x6 + 256 KiB (editor formula), then domain byte count before Git blob creation | `413` from body limit; field error from domain | D (`DiffLimits.max_file_bytes`) |
| Rendered HTML | 4 x body bytes, at most 4 MiB | 8 MiB | no | Counting writer around the shared renderer, before sanitized output enters any cache | "Too large to display" with source/history links; page remains editable | P |
| Block nesting (lists, quotes) | 32 | 64 | no | Bounded pre-parse scan at save and preview, before full render | Field error on save; same notice on read of legacy content | P |
| Links per page | 1,000 | 5,000 | yes | Link extraction during validation, before intent creation | Field error naming the count | P |
| Footnotes and headings in TOC | 500 / 200 | same | no | Render/TOC builder; excess not listed | TOC truncated with notice | P |
| Pages per wiki (live) | 10,000 | 100,000 | yes | Count recheck in the step-2 SQLite intent transaction | Create refused, quota message | P |
| Aliases per page / per wiki | 16 / 20,000 | 64 / 200,000 | yes | Same transaction as rename | Rename refused, oldest alias not silently dropped | P |
| Revision retention | all | n/a | no | Restore creates a new revision; history is never rewritten | none | R |
| History page size | 30 | 100 | no (`per_page` query clamped) | SQLite page query, before any Git read | Paged links | D (`COMMITS_PER_PAGE`, `LIST_PAGE_SIZE`) |
| Diff input per side | 512 KiB | 512 KiB (an operator-raised body cap yields summary-only diffs above it) | no | `DiffLimits.max_file_bytes` before line split | Summary with both revisions | D |
| Diff output lines | 20,000 | 20,000 | no | `diff_bytes_bounded`, `DIFF_DEADLINE` 500 ms fallback | Omitted-body notice, distinct from "identical" | D |

The page body default is below `max_render_bytes` so that every accepted revision is
also diffable under the default `DiffLimits` and highlightable as source. Unbounded
revision count is deliberate: growth is bounded per write by the body cap and the
existing form-post throttle, and per wiki by the storage question in "Open questions".

## Navigation and search

| Bound | Default | Ceiling | Config | Enforcement point | Outcome | Tag |
| --- | --- | --- | --- | --- | --- | --- |
| Sidebar children per level | 100 | 100 | no | Selected-level SQLite query after authorization | "More" link to paged listing | D (`LIST_PAGE_SIZE`) |
| Sidebar rendered nodes | ancestors (<= depth) + 100 siblings + 100 children | ~216 | no | Navigation builder; never loads the whole tree | Same | P |
| Page listing page size | 100 | 100 | no | SQLite page query | Paged links | D |
| Search query length | 100 chars, truncated | 100 | no | Search request parsing before tokenizing | Truncated silently, as Explore does | D (`MAX_SEARCH_LEN`) |
| Search terms | 8 | 16 | no | Same | Extra terms ignored with notice | P |
| Result page size | 30 | 100 | no | SQLite index query, filtered by repository access before titles/snippets/counts | Paged links | D (`EXPLORE_PAGE_SIZE`) |
| Reachable results (offset cap) | 1,000 | 1,000 | no | Query offset clamp | "Refine your search" notice | P |
| Snippet length | 240 chars, 1 per hit | 480 | no | Snippet extraction from stored index text, then autoescape | Ellipsized | P |
| Count shown | exact up to 1,000, then "1,000+" | same | no | `LIMIT 1001` count query | Bounded count label | P |
| Index lag | 0 visible revisions (index revision finalized with page pointer) | 0 in W1-W3 | no | Step-4 finalize transaction | n/a | R |

Index lag is zero by construction in the ADR's commit protocol. A later deferred
indexer must first define a finite lag (pending intents and age), validate every hit
against the visible revision before display and show staleness honestly; this
document does not budget a worker that does not exist.

## Media

| Bound | Default | Ceiling | Config | Enforcement point | Outcome | Tag |
| --- | --- | --- | --- | --- | --- | --- |
| File size | 10 MiB | `[editor] max_upload_bytes` (50 MiB default) | yes | Upload route `DefaultBodyLimit` = size + 1 MiB (release slack), then streaming byte count before the content ID is published | `413`, partial scratch file removed | P (ceiling D) |
| Upload timeout | `[editor] upload_timeout_seconds` | same | inherited | Upload router outside the 30 s page timeout, as the editor/release uploads are | `408` | D |
| Files per upload | 10 | `max_upload_files` (100) | yes | Multipart field count | Field error | P (ceiling D) |
| Media count per wiki | 1,000 | 10,000 | yes | `staged` row insert (committed plus staged rows), rechecked in the step-2 intent transaction | Quota message | P |
| Media bytes per wiki | 1 GiB | 64 GiB | yes | Same two points, summed with the incoming size before the file is renamed into place | Quota message with current use | P |
| Media bytes per repository | equals per-wiki (one wiki per repository) | same | no | Same | Same | R |
| MIME allowlist (inline) | PNG, JPEG, GIF, WebP | fixed | no | Server-side sniff of the leading bytes (as LFS sniffs 8,000 bytes) agreeing with the declared type | Unsupported-type error | R |
| MIME allowlist (download only) | PDF, plain text | operator may remove only | yes, subtractive | Same | Served with the release attachment headers | R |
| Scriptable types | SVG, HTML, XML, JS, unknown | never inline | no | Rejected at upload in W3 | Unsupported-type error | R |

Inline images are served with `nosniff`, a fixed sniffed content type and a sandboxing
CSP; everything else reuses the release download headers. No remote media is fetched.
Media bytes are stored per wiki under `<data_dir>/wiki-media/<repo_id>/<wiki_id>/`
([storage format](wikis-storage-format.md)); media is not deduplicated across wikis.

## Caches

Wiki pages share one repository ACL, so derived entries need no per-viewer or
per-locale key. The storage format owns the key formats: rendered bodies are keyed by
wiki ID, body OID and render-policy version and hold no resolved link state;
navigation slices are keyed by wiki ID, head OID, navigation-policy version and
parent. No repository access epoch is used. No such counter exists, and no cached
value depends on the viewer's access, because authorization runs before every lookup,
including 304 validation. HTTP dimensions (full/fragment, locale) belong to the
navigation contract. Invalidation is by key change, not by TTL.

| Tier | Entries / bytes | Ceiling | Config | TTL and invalidation | Tag |
| --- | --- | --- | --- | --- | --- |
| In-memory rendered body | 8 MiB, entry <= 1/4, bypass below 512 B | 64 MiB | yes | LRU; a new body or render policy changes the key | D (`DocumentCache`) |
| In-memory navigation slices | 256 entries, 4 MiB | 32 MiB | yes | Keyed by wiki head OID; any finalized commit changes it | P |
| redb derived render/nav | 50,000 entries, 30 days, 256 MiB | 1,000,000 entries | yes | Versioned keys; pruned by existing maintenance; corrupt entry is a miss | D (insights limits); bytes P |
| HTTP validators | n/a | n/a | no | Existing ETag/304 middleware (`private, no-cache`); `cache_pinned` only for anonymously readable repositories | D (`rbit-web/src/middleware/cache.rs`) |

Whether wikis get their own `DocumentCache` instance (so wiki traffic cannot evict
READMEs) or a shared one is decided by the measurement below, not by preference.

## Work, admission and lifecycle

| Bound | Default | Ceiling | Config | Enforcement point | Outcome | Tag |
| --- | --- | --- | --- | --- | --- | --- |
| Render/diff blocking slots | 4 | 64 | yes | Dedicated `BlockingBudget`, reserved before parse/diff | Busy | D (highlight/blame `MAX_JOBS`) |
| Write blocking slots (blob/tree/commit creation, ref CAS) | 2 | 64 | yes | Separate `BlockingBudget`, reserved after authorization and validation | Busy; no intent created | D (`job_concurrency`, `max_password_jobs`) |
| Per-wiki writer-lock wait | 2 s | 10 s | yes | Step-1 lock acquisition, before any tree or commit is built | Busy (`503`) | P |
| Active mutation intents per wiki | 1 | 1 | no | Writer lock, plus the partial unique index in the step-2 transaction | Busy (`503`); a stale expected revision is a separate `409` | R |
| Active mutation intents per instance | 64 | 1,024 | yes | Same transaction | Busy (`503`) | P |
| Reconciliation batch | 32 intents per pass | 256 | yes | Startup pass (bounded, before wiki writes reopen); a later maintenance step may reuse it (ADR) | Remainder resolved on access; health reports pending count | P |
| Page request deadline | 30 s | 30 s | no | Existing page router timeout | `408` | D (`PAGE_TIMEOUT`) |
| Non-page wiki form bodies | 1 MiB | 1 MiB | no | Page router body limit | `413` | D (`MAX_FORM_BYTES`) |
| Wiki API JSON bodies | 1 MiB | 1 MiB | no | API router body limit | `413` | D (`MAX_BODY_BYTES`) |
| Work when disabled | 0 tasks, 0 budgets, 0 cache instances, 0 redb opens, 0 maintenance steps, routes `404` | 0 | `[wikis] enabled` | Composition root does not construct the feature | n/a | R |
| Work when enabled and idle | 0 per-page tasks; only bounded reconciliation | same | no | Composition | n/a | R |

Render and write admission are separate pools, as the kernel contract requires for
different workload classes. Search runs as bounded SQLite queries on the existing
pool and gets no extra pool; its bound is the query, result and offset caps above.

## Measurement protocol

Budgets become acceptance budgets only through recorded measurements, stored like the
existing records in [`rewrite/measurements/`](rewrite/measurements/): a Markdown report
plus sanitized JSON with exact source revisions, executable hashes, build profile and
every raw sample, with no machine paths or credentials.

Fixtures are created through ordinary authenticated flows, never SQLite/redb inserts:

- Wikis of 1, 100 and 10,000 pages; bodies at 2 KiB, 32 KiB and the body ceiling.
- A 1,000-revision page; paired revisions at the diff input and output limits.
- Adversarial limit-sized content: maximum nesting, links, aliases, title/path lengths,
  mixed RTL/combining Unicode, and one-past-limit inputs for every bound.
- Media sets of 0, 100 and limit-count files at the median and maximum sizes.
- Public, private and archived repositories; anonymous and authorized viewers.

Run release builds with identical flags, assets, data, configuration, locale and
authorization state on an idle machine, using the
[paired HTTP harness](../../scripts/perf/README.md#paired-http-measurements): at least
two warmup and six measured rounds, alternating order. Measure page read, history,
diff, search, navigation, edit commit, media upload/download and preview at
concurrency 1, 4, 16 and 64; separately cold (fresh process, empty caches and redb
tier) and warm; separately full page, HTMX fragment and 304.

Record p50/p95/p99 and error counts by class (designed Busy/limit refusals are counted
apart from failures); SQL statements, prepares and pool acquisitions per request;
allocations and bytes; cache hits, misses, evictions and RSS; parser/render and diff
work; HTTP payload bytes; browser interaction and layout behavior; build time, peak
build RSS, binary size and dependency footprint; disk growth per revision and media.

Conversion rules:

1. Statement/acquisition counts become exact budgets in a router work-count test in
   the style of `crates/rbit-web/tests/work_counts.rs`; increases need review.
2. A **P** default is accepted only if its limit-size fixture at concurrency 64 shows
   no undesigned errors, RSS within baseline plus configured cache ceilings, and cache
   bytes never above their caps. Otherwise lower the default and re-run.
3. Latency budgets, if any are set, are taken from the measured distributions with
   the observed run-to-run spread stated; none is stated before measurement.
4. The disabled build is checked for zero added tasks, budgets and redb opens.

## Regression rule

Measure existing repository paths in three matched configurations: pinned main, the
candidate with wikis disabled, and the candidate with wikis enabled and idle. Paths:
repository home and README, tree, blob, commit history, diff, blame; anonymous HTTP
clone/fetch and authenticated push; Actions run and job pages. Work-count budgets
must be identical in all three. A latency or RSS difference beyond the measured spread
blocks adoption unless a reviewed tradeoff with evidence is recorded alongside it.
This rule follows [original speed retention](rewrite/original-speed-retention.md).

## Open questions

- Should wikis share the README `DocumentCache` or own an instance? Shared reuse versus
  eviction isolation is the measured tradeoff.
- Is a counting writer around `render_markdown` sufficient for the output cap, or must
  the shared renderer gain a cooperative output bound that READMEs would also use?
- Pre-parse nesting scan versus parser-event depth counting: which adds less work to
  the existing render path, and do READMEs also adopt it?
- Should a per-repository attachment quota spanning releases and wiki media exist?
  None exists today; this design does not invent one for releases.
- Should a per-wiki history byte quota exist, given unbounded revision retention?
- Body default 512 KiB versus `max_render_bytes` 1 MiB: confirm with editing and diff
  measurements rather than issue-sized assumptions.
- Default value of `[wikis] enabled`.
