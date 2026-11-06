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
| Actions design | Review-only themed overview/run/job fixtures cover approval boundaries, mobile navigation and bounded long logs; production and browser acceptance remain pending |
| Native containment experiments | Windows root/nested Job fixtures and isolated Linux group-anchor/pidfd scenarios pass; Linux lost-anchor and setsid cases retain quarantine. Neither experiment is a production containment claim |

The kernel now coordinates password admission and its first statically owned background workers. Full module lifecycle and the remaining feature ports are still pending. Existing services remain the behavior authority during the transition.

## Latest published checkpoint

`cd2198a4` builds on `fcdd2f95`, which adds repository discovery, collaboration label/milestone catalogs and
issue read/create applications through static ports. Existing SQL and event ordering
remain the compatibility authority. Issue creation still spans separate durable
transactions; its partial-commit limitation is covered rather than presented as
crash-atomic behavior. These ports do not retire their complete feature groups.

The feature-free `rbit-identity-models` package separates model consumers from
globally unified credential/session application features. The boundary check now
examines the full enabled application graph, in addition to isolated model graphs.
Credential inputs gain eager zeroizing ownership and redacted form diagnostics.
The new offline credential-inventory tool reports bounded aggregate metadata from
an explicit stopped copy; no live user database has been inspected and no new
password-cost rejection policy is activated.

Actions job logs retain original step limits while sharing one bounded indexed
decode across requested windows. A default-off real-page shared-shell experiment
retains the original production renderer. Explore dynamic labels now defer
formatting until rendering. New Rust and feature-equivalence validation remains
required for this integrated batch.

- Main CI passed at `ec57e0ac`: [run 42188842556](https://github.com/GitRbit/rbit/actions/runs/42188842556).
  The full [run 42182650192](https://github.com/GitRbit/rbit/actions/runs/42182650192)
  also passed at `7326e563`. Both precede the latest domain-port batch.
- Local Node checks pass: 378 tests, five platform skips. Source policy reports
  1,330 authored files and no violations. Formatting, strict workspace/all-target
  Clippy and cumulative structural checks pass; the password-boundary audit covers
  140 files. The later UTF-8 snapshot preflight passes focused domain/tool tests
  and strict Clippy, including closed UTF-16 little- and big-endian refusal fixtures.
- The latest broad local run passes all 110 E2E cases, 208 service unit tests and
  355 service integration tests. It reaches web tests with 445 passed and one failed
  new fragment assertion, corrected to preserve the escaped site name in its title
  while excluding the full-document subtitle from results. A fresh web run is pending.
  Hosted `fcdd2f95` has the same sole test failure: 2,384 Linux and 2,369 Windows
  tests passed. Its release build passed; its contract download assumption and lint
  failures are corrected locally. A new complete hosted/doctest pass remains required.
- Opt-in owned Windows Cargo completion passes a real lingering-child fixture,
  kernel/IDE checks, checker build/handoff/execution and strict workspace Clippy.
  It remains off by default. No automatic generation rotation or hard storage
  bound is claimed; the protected shared namespace remains above the soft limit.
- The [matched release footer comparison](measurements/footer-small-pages.md)
  retains four valid runs and 1,800 measured responses against pinned main.
  Both revisions have sub-500-microsecond medians for the six small page cases.
  Tail differences remain investigation items; later ports and operational workloads
  require fresh evidence.
- The separate [visual review run](https://github.com/GitRbit/rbit/actions/runs/42204656483)
  passed namespace prerequisites and browser launch, then failed the command-line
  verification protocol call. The pinned Playwright defaults omit its required
  automation switch; the new checkpoint supplies it explicitly while retaining
  actual command-line and namespace/seccomp proof. No images or browser acceptance
  are claimed before a successful hosted follow-up.

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

The [cache investigation](../build-cache-retention.md) records retained artifact growth and a whole-lane retention proposal. Automatic rotation is deferred until its cold-build cost, transient space requirements and ownership gates are measured. The [Actions prototype](../prototypes/actions/README.md) remains a review artifact: source/HTTP tests pass, while browser, accessibility and production integration remain open.
