# Plugin core handoff

## Package validation foundations — 2026-10-31

`rbit-plugin-api` now has bounded versioned package metadata for native, Wasm and
data adapters, portable payload paths, SHA256/size declarations and requested resource
ceilings. Requests grant no authority. `rbit-plugin-packages` validates complete
canonical uncompressed USTAR archives without extracting or executing anything.
It caps metadata before parsing, streams payload hashing through a fixed heap buffer
and enforces exact membership, sizes, hashes, padding and termination. Its README
defines the intentionally narrow packaging profile accepted by the validator.
Both changes received independent security review. Existing dependency versions and
migrations remain unchanged; the only lockfile addition is the local package crate.

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
variation that the two-percent gate remains unresolved. A fresh-process paired
experiment is in progress. All 120 restart-only samples were valid; these are not
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

The next package milestone is immutable staging and persistent installation metadata,
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
