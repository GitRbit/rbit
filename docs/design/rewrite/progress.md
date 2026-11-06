# Rewrite implementation progress

The rewrite is active on `feature/ground-up-rewrite`, starting from GitHub main `39f932a00b7aa35b324e06a9ff2343a57d4bed76`. Changes are kept in atomic commits. The [delivery plan](../ground-up-rewrite-plan.md) remains the completion contract; the foundation work below does not mean the platform rewrite is complete.

Public development and review continue in [draft PR #53](https://github.com/GitRbit/rbit/pull/53), opened by ChaseTheCode against main. It supersedes [PR #47](https://github.com/GitRbit/rbit/pull/47), which is closed in its favor. The [update on issue #46](https://github.com/GitRbit/rbit/issues/46#issuecomment-6673779981) explains how the modularity and localization direction fits into this broader rewrite; the issue remains open.

## Implemented foundations

| Area | Current implementation |
| --- | --- |
| Baseline and parity | Immutable Git-object inventory, schema fingerprints, 55 feature groups, required security contracts and guarded acceptance records |
| Shared contracts | `rbit-contracts` owns IDs, timestamps and naming contracts; existing core module/root paths, constructors and serialized behavior remain compatible |
| Identity models and credentials | Account models have a dependency-light, models-only layer; credentials and bounded password work have explicit separate features and legacy aliases |
| Session contracts and applications | Feature-owned records/projections retain storage/service aliases and wire bytes; static session ports own application policy, while counted admission and tracked credential finalizers support explicit drain |
| Account applications | Registration, provisioning and ordered password authentication now delegate to feature-owned applications through a static repository port; existing SQL transactions retain race authority |
| Repository access | Dependency-free permission lattice and pure grant policy; existing core/auth paths and entity adapter remain compatible |
| Kernel admission | Immediate bounded blocking admission and non-cloneable reservations; actual worker lifetime owns capacity across request cancellation |
| Kernel lifecycle | Explicit cooperative ownership and real drain for recovery email intake and session cleanup, with truthful per-worker health states |
| Localization | Bounded locale resolution, typed compiled messages, opt-in expansion/RTL diagnostics and plain-text interpolation; static English labels borrow catalog text |
| Structural checks | Physical-file ceilings, legacy size/dependency ratchets, immutable migrations and an AST checker for cumulative type definitions and implementations in replacement crates |
| Developer storage | Nested worktree lifecycle, one shared Cargo lane for compatible builds, both output-directory overrides, exact executable copies, cache reporting and a configurable 16 GiB soft watermark for inactive namespaces |
| Build integration | Shared development and performance launchers, editor JSON output, isolated runtime data/HTTP ports, verified process-tree ownership and managed-development SSH disabled by default |
| CI foundation | Linux/Windows rewrite contracts plus a stable, fail-closed aggregate and readable revision/platform/result summary |
| Git and SSH ownership | Controlled exchanges reap direct children and join pumps; admitted HTTP advertisements survive waiter cancellation, and SSH shutdown drains its tracked session and Git owners |
| Actions design | Production run/overview and dispatch presentation use existing native routes and typed labels; themed fixtures cover mobile review and long logs. Production browser/accessibility acceptance remains pending |
| Native containment experiments | Windows root/nested Job fixtures and isolated Linux group-anchor/pidfd scenarios pass; Linux lost-anchor and setsid cases retain quarantine. Neither experiment is a production containment claim |

The kernel now coordinates password admission and its first statically owned background workers. Full module lifecycle and the remaining feature ports are still pending. Existing services remain the behavior authority during the transition.

## Current validation checkpoint

The published shared-output guard (`1263cc38`) and owned Windows default
(`b58fa8c6`) prevent admission based only on Cargo source timestamps. The
[source-cache evidence](measurements/shared-source-cache.md) records 22 native
worktree scenarios, including backdated source switches, build-script inputs,
feature variants, legacy-writer interference, failed compilation and warm reuse.
An unproven command does not approve local compiler output. Unix owned completion
is still pending; the active namespace remains protected above the soft watermark.

After integration with the accessible-theme update, 512 Node tests pass with seven
platform skips; formatting and the source ratchet pass (1,479 authored files, zero
violations). The integrated helper source matches the native-tested source exactly.
These checks do not establish complete application validation.

The pending feature batch passes 625 service tests using the new guard. Its full
workspace run stops at the import/mirror E2E case: 109 E2E cases pass and one fails
because the returned import page is not successful. A diagnostic rerun is required
before assigning a cause. The latest completed hosted test run at `84150e1e` also
fails four highlighting-style parity assertions on both Linux and Windows; those
assertions require resolved keyword-weight inheritance after the theme update.
The correction now passes all 15 highlighting Rust tests; production highlighting
is unchanged, and its source, tests and CSS oracle are each below 400 lines.
The focused import/mirror rerun also passes, so its earlier failure is intermittent.

Both focused Cargo commands exit successfully, but their helper invocations reject
source-proof admission: Cargo's global-home cache activity changes the parent
directory watched for missing config files. A captured highlighting observation
isolates the changed absence checks to the two global Cargo config filenames;
the candidate source files remain unchanged. This is a conservative false-positive
path, not trusted cache reuse. Keep the guard fail-closed until a target-specific
mutation observation preserves config create/delete detection without treating
unrelated cache writes as config changes. Neither rerun is reported as a successful
helper/provenance check, and the candidate still has no full green validation claim.

The integrated CI cache policy allows dependency saves within each PR merge-ref
scope, retaining read-only permissions, existing action pins, and explicit exclusion
of workspace crates, tools and failures. Policy and aggregate checks pass as part of
515 Node tests (seven platform skips); 1,482 authored files pass the source ratchet.
Hosted cache hit rates, archive sizes and time savings remain unmeasured.

Earlier footer measurements retain their actual executable hashes and responses,
but their cached local dependencies were not qualified by the new source guard.
Fresh guarded release builds and repeated paired measurements remain required for
performance acceptance, alongside all workload, feature and release gates below.

## Earlier validation checkpoints

`c4c84e314eb486d5e912389e930edb8a774bbc08` adds production run-page presentation,
thread text/state applications and label/milestone attachment applications. Static
borrowed adapters retain the original SQL, no-op, authorization, throttle and event
boundaries. The new run page uses existing data and routes, typed labels, responsive
sections and the existing polling interval. Unsent review drafts survive only a
matching same-route poll into freshly authorized forms; permission removal wins.
Hidden inputs and server authority are never restored from the previous document.

The latest fully green hosted checkpoint is `68e304b3`:
[CI run 42211302304](https://github.com/GitRbit/rbit/actions/runs/42211302304).
At `c4c84e3`, [CI run 42221344674](https://github.com/GitRbit/rbit/actions/runs/42221344674)
passes Linux tests/contracts, Clippy, frontend, advisories/licenses and runner image.
Two Windows observation tests fail because their temporary root uses an 8.3 alias;
the new fixture canonicalization correction retains production containment checks.
The Windows test and release jobs also pass; the overall run fails solely on the two
Windows observation fixtures. Native ownership and prototype visual runs pass.

- Integrated local checks at `c4c84e3`: 409 Node tests pass, five platform skips;
  1,372 authored files and no source-policy violations. Formatting, strict workspace/
  all-target Clippy and cumulative structure checks pass; password-boundary coverage
  is 144 files.
- The broad Rust run passes 208 service unit and 377 service integration tests,
  446 web unit tests, Actions HTTP evaluation, collaboration policy and localization
  cases. Three new HTML assertions initially expected named rather than Askama's
  numeric escaped entities. After correction, all ten Actions page HTTP cases pass;
  the other web integration cases already passed in the broad run. Fresh full latest
  candidate, feature-specific, doctest and release gates remain required.
- The default-off Windows observer completed an actual helper build, native empty-Job
  proof, input-graph observation and independent exact executable handoff. The legacy
  lane remains protected and above its soft watermark. No managed promotion, deletion,
  automatic rotation or hard storage bound is established.
- [Matched release footer evidence](measurements/footer-small-pages.md) retains four
  runs and 1,800 measured responses against pinned main. Both revisions have
  sub-500-microsecond medians for six small anonymous/authenticated pages. Tail
  differences remain investigation items; this does not validate later ports,
  production Actions, large workloads, concurrency, CPU/RSS or browser rendering.
- The [isolated prototype visual run](https://github.com/GitRbit/rbit/actions/runs/42221344697)
  passes sandbox proof, 29 captures and interactions. Its preceding interaction
  failure selected a completed job while expecting a live control; the corrected
  fixture selects the running job. Captures retain keyboard/geometry evidence.
  Prototype screenshots do not establish production-page browser acceptance.

## Next integration acceptance

A source-only integration adds assignee attachment and human comment applications, production Actions
Overview/dispatch presentation, isolated inventory-tool CI compilation and opt-in
fresh-output build measurement. The latter reserves one candidate per namespace,
requires explicit observed free-space headroom, protects evidence from namespace
pruning and remains ineligible for promotion/deletion. Fresh output alone does not
prove cold compiler caches or satisfy the rotation cost gate.

The main-branch speed audit found eager store-handle clones before rejected catalog
and thread operations and new per-job owned Actions label buffers. Concrete factory
applications restore the original store-construction points; borrowed English label
formatters preserve plain-text escaping and diagnostic catalog output. Construction/
ordering tests complement SQL counters, which cannot detect pool-handle clones.

The expanded integration passes the complete local Windows workspace suite: 2,506
passed, zero failed and two ignored across 103 result sets, including doctests.
Strict workspace/all-target Clippy passes. The final JavaScript suite passes 467
cases with six platform skips; source policy checks 1,467 authored files with no
violations. Formatting and whitespace checks pass. A fresh asset build reproduces
the committed generated assets.

The structural checker passes all 28 tests. Its actual component mode checks the
selected legacy replacements as well as strict replacement crates: ThreadServices
79 lines, ReadView 40, ThreadReader 34 and ThreadWriter 204. Password-boundary
coverage is 153 files. This does not claim all legacy types already meet the limit.

This batch also owns bounded timeline reads and explicit thread composition, human
and Actions comment applications, thread locking/system closure, production Job
presentation and typed locale response boundaries. Removed diff columns release
resize observers and theme-picker scroll bursts share one pending animation frame.
An early full run exposed workspace-control environment leakage into application
children, a sole-administrator fixture mistake and missing Job locale negotiation;
all three are corrected and the full suite rerun successfully. Application unknown-
field validation remains strict.

SQL migration 0060 and alternate epoch snapshot reads are dormant foundations, not
active credential fencing or cross-store recovery. An older executable rejects the
new migration version. Before any local preview upgrade, retain a complete stopped
data/configuration/key snapshot; an older binary alone is not a rollback plan.

The contained production browser harness now reuses a verified existing Linux test
binary and public assets. Its actual hosted execution and screenshot review remain
pending. Prototype capture-origin correction retains keyboard and geometry checks.
Fresh hosted gates, release build, latest paired performance measurements and full
migration/rollback acceptance remain open. The current preview still runs c4c84e3.
These ports do not retire their complete feature groups or establish speed gains.

## Historical validation checkpoints

The records below describe their named revisions, not the latest candidate.

- The model/transport checkpoint `04ce49f1` passes the full Windows workspace test command, including doctests, and strict workspace/all-target Clippy after a test assertion correction. Later account application integration has its own focused results below; those do not imply a new full-workspace or hosted pass.
- Account applications: models-only (seven tests), credentials-only (nine tests) and the combined identity/core/auth/storage/services test command pass. This includes 535 service and 249 storage tests. Strict all-target Clippy, formatting, source/type boundaries and advisories/licenses pass. All eight application-port tests also pass after fixture corrections. Existing SQL bootstrap races, events, audit, session and migration acceptance remain authoritative.
- Session model/token extraction: models-only (10), credentials-only (12), token-only (12) and default (43) identity tests pass. Combined auth/core/storage/services tests pass, including 535 service and 250 storage tests, with strict all-target Clippy. Independent normal-graph checks and CI compilation keep tokens free of password/runtime dependencies and models free of crypto/runtime dependencies. Application/enforcement activation is now integrated at `802d8845`; the newer focused results below do not replace a full-workspace or hosted checkpoint.
- Session application/enforcement at `802d8845`: the combined focused command passed 67 identity, 43 auth, 151 core, 530 services and 250 storage tests. Sessions-only passed 34 tests without Argon2. These cover feature-owned session policy and ownership from admission before the first await through detached credential finalization and shutdown drain. They do not prove durable cross-store crash recovery or a new full-workspace/hosted pass.
- Native development proof: three contract tests, strict all-target Clippy and all eight Windows scenarios pass. [Recorded evidence and dependency audit](../../../tools/process-tree-probe/validation.md) distinguish real abrupt-parent/assignment cases from simulated wait/query/resume faults. Integration changes no existing registry version/checksum and passes advisories/licenses. Isolated Linux [run 42169158625](https://github.com/GitRbit/rbit/actions/runs/42169158625) and the newer [run 42174878951](https://github.com/GitRbit/rbit/actions/runs/42174878951) passed package tests, strict Clippy and all five native scenarios. [Retained Linux evidence](../../../tools/process-tree-probe/linux-validation.md) records known-fixture pidfd/pipe completion and explicit lost-anchor/setsid quarantine, not general descendant containment.
- Model feature matrix: six models-only, eight credentials-only and 30 default identity tests passed, along with 151 core and 42 auth tests and focused strict Clippy. The read-only dependency check confirms that core's 24-package normal graph excludes Argon2, Tokio, kernel and zeroize. Ten shared-contract tests preserve timestamp, naming, error and serialization behavior.
- Transport/SSH: 42 transport and 27 Windows SSH tests passed with focused strict Clippy. The Unix-only live-child shutdown case also passed in the hosted Linux test job at `04ce49f1`. These results prove selected direct-child and asynchronous-owner contracts, not arbitrary OS descendant containment or successful cleanup after a child wait I/O error.
- Representative rendering: seven tests compare Explore and login candidates through actual style/CSP, partial, timing and cache middleware, including ETag/304 and private-response safeguards. These are test-only candidates; production rendering is unchanged, and no speedup or complete browser acceptance is claimed.
- Contracts/core: 150 tests and strict Clippy passed after ID extraction; the 148 core tests also passed with the patched Markdown parser.
- Identity/auth: 24 identity tests and 42 auth/compatibility tests passed; strict Clippy passed for both crates and all targets. The password-attempt API covers single-slot login/reuse checks and eagerly zeroizing inputs, with a fixed published Argon2id compatibility vector. Production password consumers now use this owner.
- Access/auth/core: all 196 tests and focused strict all-target Clippy passed. Four access tests include 3,072 resolved-fact combinations, lattice thresholds and monotonic grants; the existing entity adapter matrix remains independent.
- Services: all 532 tests passed after lifecycle integration (205 unit and 327 integration), including the activated password contracts and recovery's single-start, disabled-start and stopped-intake cases.
- Kernel: all 17 tests and strict Clippy passed: ten admission cases and seven lifecycle cases covering single start, cancellation/deadline waits, concurrent drain, panic health, owner drop and destruction of worker resources.
- Server: all 15 tests passed, including bind-failure store release, cooperative purger release, host stop, real restart and backup behavior. One standalone helper is ignored; its parent executes seven isolated startup/configuration cases.
- Localization: 13 tests, including real Askama escaping and borrowed/static output checks, and strict Clippy passed.
- Structural checker: 22 Windows tests and strict Clippy passed. Eight replacement/tool crates pass cumulative type checks, including the native proof types. The password-boundary audit covers 118 service/server files after account activation and passes. Source policy covers prototype code/styles and development tooling, reporting 1,175 authored files and no violations; installed tooling dependencies are excluded. Unix-only link fixtures are separate from the Windows result.
- JavaScript: the integrated Windows suite completed 301 cases (299 passed, two Unix-only cases skipped), including sandbox/contained-image policy, owned-container cleanup, Docker-client error/close drain, Actions prototype contracts and isolated identity graphs. Compiler/observer/provenance/report failures retain the real shared lease. Actual browser execution is pending; Node contract tests are not visual evidence. Earlier Linux helper runs use a disposable native Git fixture because Linux Git cannot resolve Windows worktree metadata.
- Web/server: the broad run passed 423 web unit tests, 134 web integration tests and one Actions evaluation test, plus 12 server tests. One server helper remains ignored as a standalone test; its parent executes seven isolated startup/configuration child cases. A subsequent test-only expansion passed all six password-protocol cases, bringing the web integration inventory to 135. These verify real cookie, PAT, Git and LFS behavior under Busy and Closed admission.
- Rendering probe: all six semantic/CLI tests and strict Clippy passed at eight distinct contexts. The [runtime experiment](measurements/render-runtime-count8.md) records 18 equal-output samples. The [compiler screening](measurements/render-check-counts.md) retains all 72 valid timings at 1/8/32/96 contexts; its memory observations are rejected because proxies can share compiler arguments. A subsequent real smoke check distinguished proxy and actual compiler with the corrected observer; it is not a repeated comparative memory benchmark. Real-page and real-web acceptance remain open.
- Dependency security: `pulldown-cmark` minimum and lockfile raised to 0.13.6 for RUSTSEC-2026-0433. Advisories and licenses pass, with no new advisory exception or unrelated external upgrade.
- Real worktree check: a second source checkout passed all ten kernel tests using the same compatibility namespace and was removed cleanly. No checkout-local compiler tree was created.
- Hosted CI passed in full at `7a72a38301c10e30e7d081e2c3f58be80e38147a`: [run 42154805809](https://github.com/GitRbit/rbit/actions/runs/42154805809). That run includes the new `CI required` aggregate, Linux/Windows rewrite contracts and tests, Clippy, frontend, runner image, advisories/licenses and release build. It validates password-service activation and the access/lifecycle checkpoint; later model/transport results do not imply a hosted pass for the new revision.
- At `04ce49f1`, [run 42163780694](https://github.com/GitRbit/rbit/actions/runs/42163780694) passed both platform rewrite-contract jobs and the Linux test job (2,243 passed, one skipped), including the live-child SSH shutdown case. Hosted Linux Clippy found a nested-condition lint in that Unix-only test; the condition is corrected without changing its behavior. A fresh full hosted result is required.
- At `f82aaf09`, the [Linux test job](https://github.com/GitRbit/rbit/actions/runs/42174879076/job/127750932586) passed 2,285 tests with one skipped, including the inherited SQL work-count, access-recheck and SQLite planner-stability regressions. The overall workflow was superseded and cancelled before Windows tests and release completed. The full green hosted result above remains specifically the older `7a72a383` checkpoint.
- Footer timing correction is integrated: eight Rust timing tests pass, covering the separate SSR `view` interval, display rounding, 200/304 responses and concurrent request isolation. Two response-shape tests and three real HTTP Explore localization/privacy tests pass after removing its full header-map clone. Strict web/all-target Clippy and formatting pass; the Node suite passes 329 tests with four platform skips. Matching release comparisons remain pending; these functional checks are not a speed result.

## Validated production slice

Explore uses typed feature labels and one shared Askama results component for its full page and direct HTMX fragment. The existing public-only listing service retains search, sort, archive and paging authority. Normal navigation, boosted requests and history restoration retain full-page responses. Diagnostic negotiation is scoped to Explore so other routes do not receive partially localized document/cache behavior.

The passing HTTP regressions cover full/fragment consistency, language and ETag dimensions, private repository exclusion even for an owner session, malicious text and ambiguous request headers. No runtime speed percentage is claimed from static inspection or test duration.

Password work is a second production slice. Startup validates `[security] max_password_jobs` (default 2, range 1–64), administrator saves preserve atomic validation, and the setting requires restart. Account, profile, administration, recovery and MFA services share one owner; offline account creation uses an explicit short-lived owner. Request rate-limit configuration cannot disable this resource bound.

Busy and Closed return generic retryable 503 responses without credential challenges or password-failure surcharges. Admission precedes one-use proof consumption. Real HTTP regressions preserve reset and MFA proofs and sessions across overload, then retry the same operation successfully. PAT-backed Git/LFS/API and health requests remain usable. Generic errors retain the HTMX history barrier when replacing a secret-bearing page. The [identity contract](../identity-admission.md) records remaining input-bound and operational measurement obligations. This slice does not retire the complete identity feature group.

Session application and enforcement are now a further production slice. Identity owns session creation/resolution, expiry, listing/revocation policy and self-service password-change attribution through static ports; common session reads do not create tracked tasks. Guards and all three credential mutations reserve counted ownership before their first await, and already-admitted mutations can transfer that ownership into the existing detached commit/revocation/audit/event finalizer after admission closes. Shutdown waits for these finalizers and their captured resources to be destroyed. This is in-process ownership and drain, not atomic durability across SQLite and redb: durable revocation intent, cross-store crash recovery and restart reconciliation remain open.

## Work still required

1. Continue from the passing foundation checks. Process-tree ownership has independent review and Windows/Linux fault-test coverage; uncertain compiler/runtime ownership remains protected.
2. Measure the activated password-work slice under constrained memory/CPU before treating its initial default as a deployment recommendation; its hosted compatibility checks now pass.
3. Continue the complete identity port and resolve its remaining input-bound policy. Preserve the proof, failure and session contracts already verified by this slice.
   Complete access policy ports and OS process containment. SSH tracks its session/Git descendants, but legacy arbitrary-drop paths, exceptional wait failures and OS process-tree cleanup remain explicit obligations.
4. Establish fresh release/runtime benchmarks against the pinned main. The [rendering experiment](../rendering-compilation.md) investigates inherited Askama shell duplication without presuming a measured cause or improvement.
5. Continue repository/access/identity ports, durable required events, collaboration, Actions engine/UI, complete localization, release hardening, and migration/rollback rehearsals through the plan's gates.

Automatic cache maintenance evicts inactive compatibility namespaces after successful builds/tests while retaining the current namespace. The watermark is soft: a protected namespace can exceed it and reports its overage. Zero disables automatic maintenance; manual zero-budget pruning still empties an inactive cache. Crashed or uncertain build leases require process inspection before recovery. Account locale persistence, translations beyond English, global UI/email localization, runtime performance acceptance and full platform feature parity remain open.

The [cache investigation](../build-cache-retention.md) records retained artifact growth and a whole-lane retention proposal. Automatic rotation is deferred until its cold-build cost, transient space requirements and ownership gates are measured. The [Actions prototype](../prototypes/actions/README.md) remains a review artifact. Its isolated browser checks pass; production run/overview are integrated, while production Job presentation is integrated locally and production browser and accessibility acceptance remain open.
