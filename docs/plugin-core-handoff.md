# Plugin core handoff

## Bounded renewal HTTP and CLI adapters — 2026-11-01

The dedicated `PUT`/`GET /api/v1/admin/plugin-policy-renewals/{operation_id}` routes
and `rbit plugin policy renew --request FILE [--operation-id ID]` /
`renewal-operation ID` commands expose the bounded backend transition explicitly.
PUT sends strict raw SDK JSON, capped at 1 KiB before deserialization; responses
remain capped at 64 KiB. Admin-PAT scope and live active administrator checks precede
body polling, with the existing body/command deadlines and FULL transaction checks.
The seven exact policy/package recovery patterns bypass optional hooks while all
host authentication/transport guards remain active; unrelated API routes stay hooked.

The CLI retains and prints the known key before PUT, uses fixed instance-relative
paths, and never auto-prepares, approves, retries or follows server URLs/redirects.
It validates source fingerprint/seal identity, distinct destination identity and
Applied's Open destination gate. Historical receipts remain original evidence even
when that destination has retired. GET returns receipt-only metadata and a separate
gate URL. `retention_full` 409 retains a sealed gate and null receipt, creates no
receipt/key reservation and exits nonzero. Lost responses/404 never prove rollback;
recover the same key and inspect current status on the same instance.

At most two renewals retain two retired plus one current epoch. No review,
selection, grant or enabled intent carries forward. Execution remains unavailable
for native, Wasm and data packages; renewal creates neither broker authority nor a
sandbox/worker. Export/pruning, broader recovery, managed runtime barriers and the
remaining service/Web extraction work are still required. PR #47 stays open and
the modularization goal remains active.

All 12 policy HTTP tests passed in default and core-only profiles, covering
pre-body authority and headers, strict/exact/chunked 1 KiB bounds, cooperative
deadlines, replay/stale/retention outcomes and receipt recovery after an admitted
waiter drops. The complete Web suite passed 647 tests with four expected ignores,
including actual-router deny/stalled-hook recovery. The complete server suite
passed 46 tests, including fixed-destination CLI recovery, malformed-response
refusal, backup and restart. All four dependency guards passed again. Functional
tests used unchanged Rust 1.95. Strict Rust 1.99 workspace/all-target Clippy,
formatting and whitespace checks passed with warnings denied. These
results do not establish a new full-workspace or hosted-CI pass. Backend results
in the next section belong to the preceding batch.

## Bounded backend policy renewal — 2026-11-01

The SDK renewal comparisons, migration `0065`, epoch-aware storage and exclusive
Services maintenance owner now support an explicit sealed-epoch transition. The
caller supplies a known operation key and the observed fingerprint/seal operation;
current administrator authorization, exact CAS and retained graph validation share
the FULL transaction. Host-generated epoch entropy and descriptor-version-2
fingerprints establish a fresh empty policy epoch. Migration preserves version-1
fingerprints and original canonical audit bytes rather than recomputing old identity.

Retention is fixed at three epochs (two retired plus one current) and two immutable
renewal receipts. A third retirement refuses recovery and leaves the gate sealed;
no pruning, export or unlimited recovery is implemented. Ordinary policy keys,
renewal keys and rejected-capacity seal keys remain reserved across retained epochs.
Historical replay reports evidence without restoring old heads, and stale unrecorded
fingerprints cannot exploit a new `Absent` state. Reviews, selections, grants and
enabled intent never carry forward. Archive-store capacity is separate and unchanged.

The Services maintenance owner is exclusive with ordinary policy mutations, survives
waiter loss and remains tracked through commit/shutdown drain. Backup format 5 now
validates the frozen legacy five-table `0064` decoder or all seven current tables,
including retired/current epochs, renewal links and bounded histories. Historical
schema detection never ignores a partial epoch graph because its marker is absent.
Retained archives remain part of the backup; epoch duplication does not duplicate
shared digest objects. Restore changes no executable authority.

At this backend checkpoint, maintenance HTTP/CLI adapters were not yet exposed;
ordinary policy commands did not rotate epochs or query renewal receipts. The later
adapter section above records the dedicated transport increment. Every package remains
execution unavailable, broker grants remain empty, and future managed admission
must fence retired generations and host calls before any runtime integration.
Export/pruning, remaining recovery, native worker/sandbox adapters and first-party
service/Web separation remain unfinished. PR #47 and the broader goal stay active.

Focused local checks passed 38 SDK tests, 14 policy storage tests, two epoch codec
tests, 19 upgrade tests and four policy service tests in each of the default and
core-only profiles. The complete server suite passed 42 tests, including renewed
backup/restore, malformed inventory scalar rejection and restart. The populated
legacy-policy cases use the actual SQLx migrator for both Open and already-Sealed
version-1 graphs, preserving canonical audit bytes and selected reviews. The complete
workspace suite passed 2,500 tests, zero failures and eight expected ignores across
74 test summaries. Strict Rust 1.99 workspace/all-target lint, formatting,
whitespace and all four dependency guards passed. After documentation markup and
a test-only string-assignment lint fix, all 14 policy storage tests passed again.
These local results do not certify hosted/native Windows behavior. Exact-head CI
remains required after publication. Earlier sections retain the validation
provenance of preceding stages.

## Bounded policy administration adapters — 2026-11-01

Admin-PAT HTTP routes and `rbit plugin policy` commands now expose explicit
preparation, exact installed-package inspection, review, selection, clearing and
known-key receipt recovery. JSON commands and responses are bounded to 64 KiB;
mutating handlers enforce body and command deadlines. Live administrator checks
precede body polling, and the owned FULL transaction repeats authority and CAS.
Exact policy and package-operation routes bypass optional hooks while retaining
host guards. Other administrator API routes remain subject to hooks.

The CLI prints its operation key before mutation, uses fixed instance-relative
destinations and never retries or follows returned URLs. It checks exact receipt
identity and decision summaries before success. Historical receipts never imply a
new approval. A committed capacity seal is preserved as structured non-success;
a missing receipt or lost response never establishes rollback. Prepare has no
receipt and recovers through gate inspection. Every view remains execution
unavailable; no worker, enabled intent or broker grant is created.

Focused HTTP tests passed all six cases, including real-router waiter loss behind
an independent SQLite writer, native review/select/deny/historical replay across
fresh Services composition, and clear/stale-clear tombstone behavior. Full adapter
validation passed: the complete Web suite passed 641 tests with four expected
ignores, and the complete server suite passed 40 tests. Strict Rust 1.99
workspace/all-target Clippy, formatting, whitespace and dependency guards passed.
After extracting two long test scenarios into helpers, all six HTTP policy cases
and the hook-recovery integration scenario passed again. Functional tests used
unchanged Rust 1.95. Hosted head `751aa01` passed all 11 CI jobs, including
2,481 Linux tests, 2,462 native Windows tests, release, core-only and both optional
profiles. The three Windows package-upload guards passed on their native platform.
That evidence covers the adapter head, before the renewal backend above. The
broader modularization goal and PR #47 remain active.

## Durable review and candidate policy — 2026-11-01

Policy storage is committed as `a38cf3e`, owned Services as `808e363` and backup
validation as `40063c9`. The batch adds migration `0064`, an owned Services facade
and backup validation. Administrative initialization creates one persistent epoch and
host-owned fingerprint. Exact installation references, current administrator
rights and revision expectations are checked in the same FULL write transaction.
Review history and operation receipts are immutable; cleared candidates retain
revision tombstones. Replay acknowledges historical results without restoring old
heads. At capacity or terminal revision, a valid rejected command commits only a
sticky seal and bounded provenance. Ordinary APIs cannot reopen it.

This stage grants no executable authority. Every candidate remains unavailable,
nonempty broker grants are rejected, and embedded Actions/i18n desired state and
presentation remain separate. Native intent explicitly acknowledges full service
OS-user trust; Wasm intent requires a real sandbox and never permits native fallback.
A future managed adapter must fence existing generations and host calls before
publishing revocation; this metadata stage does not prove that runtime behavior.

Local checks passed: 35 SDK tests, nine policy storage tests, 19 upgrade tests,
three policy service tests in both default and core-only builds, and 35 server
tests including backup and restart. Strict Rust 1.99 workspace/all-target lint and
all four dependency guards passed. The full local workspace passed 2,476 tests,
zero failures and eight ignored tests across 74 test summaries. Five ignores are
manual checks and three require native Windows. Backup
coverage preserves selected reviews, epochs and an already sealed snapshot, and
rejects incomplete schemas or malformed singleton state. Corruption regression
coverage includes empty receipt IDs skipped by pagination and NUL-hidden oversized
scalars; byte bounds precede row materialization.

The preceding `c9469e7` hosted run passed 10 of 11 jobs: all 2,444 Linux tests,
release smoke checks, strict lint, advisories and all three feature profiles. Its Linux swap release reclaimed 8 GiB; that runner
also had a larger disk than the earlier failing runner, so this is no identical-
resource causality proof. Its Windows compile failure was an incorrect enum path
in a new fixture, corrected in `a94b403`. The portable fixture now typechecks on all
hosts; its three Windows behavior tests remain skipped elsewhere and require
native CI. Validation of the next pushed head remains required.

Policy HTTP/CLI adapters, deliberate maintenance/epoch renewal, full recovery,
managed native execution, enforced broker grants, first-party service/Web
separation, optional Wasm and final performance gates remain unfinished. PR #47
stays open and unmerged, and the modularization goal remains active.

## Administrator package transport and setup — 2026-11-01

Published package head: `c9469e7`. Admin-scoped PAT endpoints now stream canonical
archives through owned bounded work and inspect known operation keys. Authentication
and live administrator checks precede body polling. Absolute deadlines, cooperative
frame yielding and waiter cancellation wake stalled readers. Installation grants neither trust nor execution authority; runtime selection
remains unavailable.

The CLI prints its recovery key before upload, streams file bytes, rejects redirects
and retries, and caps responses at 64 KiB with exact-key checks before success. Pending
durability exits 8. Local Unix `package-store provision/check` resolves deployment
storage without opening SQLite/redb, creates only missing private synchronized roots,
and refuses unsafe existing paths. Trusted ownership/stable ancestors remain host
prerequisites. Windows HTTP uploads fail 503 before body reads until supported ACL
verification exists; GET inspection and lower-level trusted-host primitives remain.

The final local Web suite passed 635 tests with one manual benchmark ignored; the
final server suite passed 32 tests across binary, backup and real restart suites.
Strict Rust 1.99 workspace/all-target Clippy, formatting, whitespace and all four
dependency guards passed. A built-binary provision/check test preserved an invalid
SQLite sentinel and created no key or redb file. These package checks supplement the
older full-workspace evidence below; they are not a new full-workspace total.

[Exact-head CI](https://github.com/GitRbit/rbit/actions/runs/41280718047) remains required.
The preceding `72841fd` Linux run passed 2,419/2,421 tests; two established fixtures
failed with explicit disk-full errors. CI now retains its temporary 8 GiB compilation
swap through `nextest --no-run`, then releases only that swap before the unchanged
runtime test command. Hosted resource validation and native Windows tests remain
pending on the published head.

The goal remains active: complete recovery, publisher review/grants, managed native
workers, first-party service/Web separation, optional Wasm and final performance/release
gates are still required. PR #47 remains open and unmerged. Earlier dated sections
retain their validation provenance.

## Durable package records and backup integration — 2026-11-01

The host coordinator now retains validated archives without activation or trust
approval. The fixed private package root is provisioned by the host. Admission is
bounded and owned through blocking I/O and journal settlement; waiter cancellation
cannot release capacity early or abort a possible commit. Administrator authority
is refreshed after staging and again before publication. Shutdown closes admission
and drains owners before core stores close.

Migration `0063` adds immutable digest objects, full-version pins and revision-fenced
operation records. Its separate lazy FULL-synchronous writer leaves ordinary request
connections at NORMAL. Idle/lifetime reapers are disabled with zero minimum
connections, so an unused ledger opens no connection or maintenance timer. Database
shutdown closes both pools. Windows matching-existing publication still needs an
exact prior durable installation record or remains awaiting durability.

Backup format 5 reads its own SQLite snapshot, validates every retained object and
installation pin, and verifies exact copied size/SHA256/EOF before completion.
Missing/corrupt objects fail without a final manifest. Pending operations assert no
durable object and their private files are omitted. Objects remain append-only;
full reconciliation and garbage collection require further coordination.

See [package contracts and remaining boundaries](plugin-packages.md). The full
local workspace at `4384603` passed 2,423 tests, zero failures and five ignored manual benchmarks
across 74 suites. Strict Rust 1.99 workspace/all-target Clippy, formatting and all
four core/UI dependency profiles pass. Focused evidence includes nine core-only
coordinator tests, eleven backup integration tests, two backup helpers, seven ledger
tests and nineteen existing upgrade tests. No dependency version/checksum changes;
only existing package edges were added. Final-head hosted CI remains required.

The preceding `0f1a632` Linux CI passed 2,393 tests, including all eleven real Unix
store tests. Its only reported failure was newer diagnostic-assertion lint rules;
`0caf4c1` fixes those, and the complete local 1.99 lint now passes. That earlier CI
result does not certify the installation coordinator/backup increment or Windows.

The broader goal remains active: complete package
recovery, publisher policy/grants, independently installed native workers, first-party
service/Web ports, the Wasm adapter and unresolved performance/release gates remain
required. PR #47 stays open and unmerged.

## Package validation foundations — 2026-10-31

`rbit-plugin-api` now has bounded versioned package metadata for native, Wasm and
data adapters, portable payload paths, SHA256/size declarations and requested resource
ceilings. Requests grant no authority. `rbit-plugin-packages` validates complete
canonical uncompressed USTAR archives without extracting or executing anything.
It caps metadata before parsing, streams payload hashing through a fixed heap buffer
and enforces exact membership, sizes, hashes, padding and termination. Its README
defines the intentionally narrow packaging profile accepted by the validator.
Both validation changes received independent security review. At those commits,
dependency versions and migrations remained unchanged; the only lockfile addition
was the local package crate. Staging dependency additions are recorded below.

Focused validation passes 24 API tests and 12 archive tests, strict all-target Clippy
for both crates, formatting and whitespace checks. All 219 frontend/tooling tests,
the generated asset build and shared UI/core dependency guards pass; generated assets
remain unchanged. CI includes the archive suite in the isolated contract job.
Require all 11 jobs on the final pushed head; earlier-head checks do not certify these
new contracts. The full workspace result below belongs to the preceding UI extraction.

The paired release HTTP experiment compares exact archived `0075cec` and `3b9f263`
sources. Both binaries completed with production fat LTO and one codegen unit,
identical generated assets and recorded source/binary hashes. Initial warmed p95
changes were below one percent, but same-binary controls exposed enough process
variation that the two-percent gate remains unresolved. The completed 60-block fresh-process paired experiment
leaves the repository page inconclusive at the two-percent bound; the report records
all four page intervals and same-binary controls in
[the performance report](../scripts/perf/UI-EXTRACTION.md). All 120 restart-only samples were valid; these are not
disk-cold measurements. Scripts and raw evidence live under ignored
`target/perf/ui-extraction/`.

Standalone [parser fuzzing](../fuzz/README.md) now exercises arbitrary bytes and
structured native/Wasm/data envelopes with fragmented and interrupted readers. The
recorded AddressSanitizer campaigns completed 6,875,188 raw and 350,166 structured
executions without findings. Their input and time bounds are documented; they do
not certify installation or maximum-size packages. Tooling dependencies remain in
a separate workspace.

The shared presentation head `3b9f263` passed all 11 CI jobs in
[run 41272483318](https://github.com/GitRbit/rbit/actions/runs/41272483318),
including Linux 2,360 tests, Windows 2,352 and the artifact-page regression scenario.
The newer package and tooling commits require their own final-head CI.

Host-owned staging now validates, hashes and writes the same private inode,
synchronizes its contents and publishes a digest-addressed object without replacement.
Existing objects are rehashed; static error receipts distinguish visibility, durability
and cleanup. Unix staging directories are explicitly 0700 and files 0600. Windows
uses a narrowly scoped `atomicwrites` 0.4.4 dependency for lossless-path, write-through
publication; existing-object namespace durability remains unconfirmed. No installation
ledger, trust grant or live intent is written. The package suite passes 23 tests
(including 11 real-filesystem staging cases), strict all-target Clippy on macOS and
Windows GNU cross-target compilation/lint. Native Linux/Windows execution and the
license/advisory check remain exact-head CI requirements. Existing dependency versions
and the tempfile random-source edge are preserved; the new dependency adds only
its older rustix/linux-raw-sys lock entries.

The next package milestone is persistent installation metadata and backup references,
followed by bounded worker framing/supervision and an independently built native sample
installed on a running core-only server. First-party Actions/i18n still need narrow
host-service ports and independent Web extraction; the Wasm adapter and release gates
remain required. The broader goal is active and PR #47 stays open and unmerged.

## Shared presentation foundation — 2026-10-31

The next extraction introduces `rbit-ui` below Web. It owns passive components,
bounded listing controls, URL construction/redirect validation, code-row layout,
the single locale context and the captured-locale compiled Askama wrapper. Web keeps
compatibility exports; native Actions uses the shared wrapper. Constructors that
resolve users/repositories, full chrome/templates, HTTP security, assets, CSRF keys
and plugin admission stay host-owned. No `AppState` or Services capability was exported.

CI now checks the UI's resolved normal dependency graph with all features and all
targets selected. It rejects HTTP frameworks/clients, storage, host composition,
optional implementations and executable/sandbox adapters, including indirect edges.
Message validation scans the shared crate and recognizes its helpers while preserving
schema and HTML-escaping checks. No third-party package version changed.

The standalone UI suite passes 19 tests. Locale coverage includes nested/concurrent
and cancelled scopes, detached template rendering, writer limits, runtime values and
error restoration. A Web integration test exercises both helper paths against one
captured locale. Frontend/tooling passes 219 tests; assets are unchanged, all three
language packs reproduce, and all 1,908 messages/1,913 static references validate.
The full workspace run passed 2,366 tests with five manual benchmarks ignored;
strict workspace/all-target Clippy and formatting also passed.

Baseline `0075cec` is preserved as an exact Git source archive with SHA-256 provenance
under gitignored performance artifacts. A paired release HTTP comparison remains
required; this code move is not evidence that the two-percent core-page latency gate
has passed. Require all 11 CI jobs on the exact pushed head before certification.

The next architectural prerequisite is passive page/repository chrome and shared shell
templates. Keep host constructors, visibility projection, enhancement allowlisting,
cached CSS/CSP handling and render-time footer timing in Web. Then extract Actions
views and factories behind a narrow host adapter; moving or publicizing `AppState`
would preserve the dependency knot. Managed executable packages and the sandboxed
adapter remain required later stages in the original architecture plan. The broader
modularization goal remains active; leave PR #47 open and unmerged.

## Resumed — 2026-10-31

The user resumed work from PR #47 at `9fd65b9`, with a team reviewing architecture,
validation and security. The checkout tracks `plugin-core`; `origin/main` is already
an ancestor. Author and committer remain `ChaseTheCode`; the repository-local HTTPS
credential helper uses the authenticated GitRbit account for pushes.

The next increment adds a shared publication-time owner index for native page/form
selection. Immutable snapshots retain weak callback ownership, one shared `Arc`, exact
generation admission and synchronous closed-scope filtering. Empty/single-owner lookup
skips hashing; other lookups inspect only the requested owner's bounded batch. The
native SDK's entry array is now a read-only `entries()` slice to preserve index pairing.
Multi-owner, scope, replacement, shifted-index and stale-authority regressions cover
the boundary. [Route documentation](plugin-routes.md) records the reproducible lookup
benchmark, measured small-registry overhead and larger-registry improvement.

The installed Rust 1.95 toolchain also exposed existing strict Clippy diagnostics.
A separate mechanical cleanup preserves duration values, fixes documentation markup
and exact-value test assertions, and simplifies an equivalent SSH data match.
No dependency, migration, production security limit or retry policy changes.

Full Cargo validation exposed a pre-existing artifact-test diagnostic subscriber
conflicting with the SQL query counter's global subscriber in the shared Web test
binary. Request-local diagnostic dispatch fixes that conflict while retaining the
counter assertions and the native deadline; nextest's separate test processes had
hidden it. An explicit observed-query assertion in the artifact scenario now catches
the conflict in an isolated process too. Keep this test-fixture fix in its own commit.

Local validation passed 2,359 workspace tests with five manual benchmarks ignored,
strict workspace/all-target Clippy, formatting, 213 frontend tests, all language-pack
and message checks, generated asset parity, and all three dependency profile guards.
The strengthened artifact scenario also passed alone after the full suite. The idle
release lookup benchmark passed in both comparison orders; it is not an HTTP latency
gate. Require all 11 CI jobs on the exact pushed head before certifying this increment;
use the live PR checks for the current result.
Starting-head run `41268349947` at `9fd65b9` has now passed Linux (2,349 tests) and
Windows (2,341 tests), with four manual benchmarks skipped on each platform. The
previously failing Windows artifact-page scenario passed in 0.654 seconds. These
results certify only the starting head; its remaining profiles/release checks and
the resumed increment's exact-head checks are separate requirements.
The previous stopping point below is historical evidence, not certification of the
resumed changes. Leave PR #47 open and unmerged.

## Stopping point — 2026-10-31

Work is paused at the user's request. Implementation is committed and pushed to
[PR #47](https://github.com/GitRbit/rbit/pull/47), `plugin core: native Actions and i18n routes`.
The PR stays open and unmerged. The implementation commit is
`866edc36bdd21a697ad5eb0463d6fc64e9285f21`; the following handoff commit changes documentation only.
Final CI validation remains incomplete. Do not treat this handoff as merge approval.

Use the existing `plugin-core` PR branch, preserve atomic commits, and attribute both
author and committer to `ChaseTheCode <60942013+ChaseTheCode@users.noreply.github.com>`.
The authorized PR account is GitRbit. Do not add tool branding or coauthor trailers.
The development checkout uses local branch `plugin-i18n`; push with
`git push origin HEAD:plugin-core`. Do not reset the separate original checkout's
eight unrelated build/configuration edits.

## Delivered in this phase

- Actions run, workflow, job, step-log, summary, configuration, environment and runner
  pages and forms use the native plugin route registries and compiled Askama documents.
  Canonical repository URLs start at `/{owner}/{repo}/-/plugins/rbit.actions/`;
  global URLs start at `/-/plugins/rbit.actions/`.
- Routes and form authority belong to the actual active Actions scope and generation.
  Disable withdraws them, re-enable issues fresh registrations and proofs, and shutdown
  drains owned work. Weak host captures avoid retaining a retired host.
- The host enforces session/repository authorization, Origin checks, CSRF proofs,
  declared query/form schemas, admission, output and time limits. Domain services
  reauthorize resource IDs; designated read-only deployment reviewers remain supported.
- Full documents preserve existing chrome, themes, unread indicators, active navigation
  and captured localization. Runner token success documents are delivered once through
  the typed 200 response path; validation uses 422 and redirects use constrained 303.
- Metadata, log and summary previews are bounded and paginated. Selected variable
  editors retain the full 48 KiB value; secret inputs retain 64 KiB support. Dispatch
  retains 25 inputs of 1 KiB each. Complete current-task summaries have bounded HTML
  rendering with an escaped-source fallback and indexed neighboring steps.
- Legacy GET bookmarks redirect; legacy mutation execution is removed. Existing badge,
  raw-log, archive, REST and runner transports retain specialized security and lifecycle
  guards. No database migration, executable installer, sandbox or machine-code unload
  capability was added.

See [Actions route contracts](actions-plugin-routes.md),
[runtime pages](plugin-routes.md), [native mutations](plugin-mutations.md), and
[the architecture plan](design/plugin-architecture.md).

## Validation evidence

The full local workspace passed **2,355 tests**, zero failures, with four manual
benchmarks ignored across 67 suites, including the real runner end-to-end scenario.
Strict workspace all-target Clippy and formatting passed. Core-only Services lint
and the unavailable-Actions route test passed. Frontend passed **213 tests**;
1,908 messages and 1,909 static references validate, all three language packs
reproduce, and dependency guards/generated assets pass.

The final three-file follow-up passed its local artifact scenario, all **20** generic
plugin-page integration tests, and strict all-target web Clippy. It adds safe debug
logging of the plugin owner, literal route and error variant, plus a targeted test
subscriber and a Windows CI concurrency bound. It changes no production deadline,
assertion, retry policy or browser behavior.

Disposable browser checks at 375 by 812 verified runs, run details, job log expansion,
settings, environment and runner pages without horizontal overflow or console errors.
A complete 48 KiB escaped variable was saved through the native form and reopened
intact. Runtime disable returned 404 for the withdrawn page; re-enable restored stored
history and navigation. The temporary server, storage, example, tab and viewport
override were all removed.

### CI that still needs completion

[Run 41267040899](https://github.com/GitRbit/rbit/actions/runs/41267040899) targets
implementation commit `866edc3`. At stopping time, **5 of 11 jobs passed**:
frontend, contracts/lifecycle, licenses/advisories, formatting/Clippy and runner image.
Linux, Windows, release, core-only, Actions-only and i18n-only jobs were still running;
no failure had been reported on this run. Contracts passed **145 tests**, with three
manual benchmarks ignored, including both compile-fail doctests. Frontend passed 213.
The documentation handoff commit will trigger a newer run; require all 11 checks on
the exact current PR head before declaring this phase complete.

The preceding `e335f4f` run passed 10 of 11 jobs: Linux 2,349 tests, core-only 28,
Actions-only 24, i18n-only 25, contracts, frontend, lint, release and packaging checks.
Windows passed 2,340 tests but failed one artifact-page test: its first native run-page
GET returned 503 instead of 200. Neighboring Git-heavy tests showed substantial
contention. A one-second callback timeout is a likely explanation, not a proven cause:
that run did not record the underlying error variant. The follow-up keeps the
one-second production deadline and all assertions, adds diagnostics, and uses two
Windows test processes while retaining Linux's default CPU-based parallelism.
Retries remain disabled. This Windows outcome must be verified, not assumed fixed.

## Resume checklist

1. Check the current PR head and its latest CI run. Do not substitute an earlier
   implementation or documentation run for exact-head validation.
2. Inspect all 11 jobs and their logs. Confirm Windows passes the artifact scenario and
   the complete suite; if it fails, use the recorded page-error variant to diagnose it.
   Keep production security limits and assertions intact.
3. Record actual Linux/Windows, contract and optional-profile test counts. Avoid using
   the preceding run's numbers as certification of the new head.
4. Fix actual issues in atomic commits under the attribution above, push the same PR,
   and repeat affected local checks and exact-head CI as needed. Browser revalidation
   is needed only if behavior or UI changes.
5. Update the PR and handoff, verify clean worktree and matching local/remote/PR heads,
   and audit commit attribution. Leave the PR open and unmerged unless separately
   authorized. Mark the phase complete only after all required validation passes.
