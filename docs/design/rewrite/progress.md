# Rewrite implementation progress

The rewrite is active on `feature/ground-up-rewrite`, starting from GitHub main `39f932a00b7aa35b324e06a9ff2343a57d4bed76`. Changes are kept in atomic commits. The [delivery plan](../ground-up-rewrite-plan.md) remains the completion contract; the foundation work below does not mean the platform rewrite is complete.

Public development and review continue in [draft PR #53](https://github.com/GitRbit/rbit/pull/53), opened by ChaseTheCode against main. It supersedes [PR #47](https://github.com/GitRbit/rbit/pull/47), which is closed in its favor. The [update on issue #46](https://github.com/GitRbit/rbit/issues/46#issuecomment-6673779981) explains how the modularity and localization direction fits into this broader rewrite; the issue remains open.

## Implemented foundations

| Area | Current implementation |
| --- | --- |
| Baseline and parity | Immutable Git-object inventory, schema fingerprints, 55 feature groups, required security contracts and guarded acceptance records |
| Shared contracts | `rbit-contracts` owns IDs, timestamps and naming contracts; existing core module/root paths, constructors and serialized behavior remain compatible |
| Identity models and credentials | Account models have a dependency-light, models-only layer; credentials and bounded password work have explicit separate features and legacy aliases |
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

The kernel now coordinates password admission and its first statically owned background workers. Full module lifecycle and the remaining feature ports are still pending. Existing services remain the behavior authority during the transition.

## Validation checkpoint

- The model/transport checkpoint `04ce49f1` passes the full Windows workspace test command, including doctests, and strict workspace/all-target Clippy after a test assertion correction. Later account application integration has its own focused results below; those do not imply a new full-workspace or hosted pass.
- Account applications: models-only (seven tests), credentials-only (nine tests) and the combined identity/core/auth/storage/services test command pass. This includes 535 service and 249 storage tests. Strict all-target Clippy, formatting, source/type boundaries and advisories/licenses pass. All eight application-port tests also pass after fixture corrections. Existing SQL bootstrap races, events, audit, session and migration acceptance remain authoritative.
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
- Structural checker: 22 Windows tests and strict Clippy passed. Seven replacement/tool crates pass cumulative type checks, including the account application types. The password-boundary audit covers 118 service/server files after account activation and passes. Source policy now covers prototype code and styles, reporting 1,134 authored files and no violations; Unix-only link fixtures are separate from the Windows result.
- JavaScript: the integrated Windows suite completed 287 cases (286 passed, one Unix-only case skipped), including Actions prototype route/markup/cascade contracts, prototype source ceilings, isolated identity feature graphs, compiler-observer startup/shutdown, actual executable identity, report validity, controlled compiler flags and CI aggregation. Combined compiler/observer/provenance/report failures retain the real shared lease, verified without starting a compiler. Root-alias helper regressions also passed with a declared Windows junction as the temporary root. Earlier Linux helper runs use a disposable native Git fixture because Linux Git cannot resolve Windows worktree metadata.
- Web/server: the broad run passed 423 web unit tests, 134 web integration tests and one Actions evaluation test, plus 12 server tests. One server helper remains ignored as a standalone test; its parent executes seven isolated startup/configuration child cases. A subsequent test-only expansion passed all six password-protocol cases, bringing the web integration inventory to 135. These verify real cookie, PAT, Git and LFS behavior under Busy and Closed admission.
- Rendering probe: all six semantic/CLI tests and strict Clippy passed at eight distinct contexts. The [runtime experiment](measurements/render-runtime-count8.md) records 18 equal-output samples. The [compiler screening](measurements/render-check-counts.md) retains all 72 valid timings at 1/8/32/96 contexts; its memory observations are rejected because proxies can share compiler arguments. A subsequent real smoke check distinguished proxy and actual compiler with the corrected observer; it is not a repeated comparative memory benchmark. Real-page and real-web acceptance remain open.
- Dependency security: `pulldown-cmark` minimum and lockfile raised to 0.13.6 for RUSTSEC-2026-0433. Advisories and licenses pass, with no new advisory exception or unrelated external upgrade.
- Real worktree check: a second source checkout passed all ten kernel tests using the same compatibility namespace and was removed cleanly. No checkout-local compiler tree was created.
- Hosted CI passed in full at `7a72a38301c10e30e7d081e2c3f58be80e38147a`: [run 42154805809](https://github.com/GitRbit/rbit/actions/runs/42154805809). That run includes the new `CI required` aggregate, Linux/Windows rewrite contracts and tests, Clippy, frontend, runner image, advisories/licenses and release build. It validates password-service activation and the access/lifecycle checkpoint; later model/transport results do not imply a hosted pass for the new revision.
- At `04ce49f1`, [run 42163780694](https://github.com/GitRbit/rbit/actions/runs/42163780694) passed both platform rewrite-contract jobs and the Linux test job (2,243 passed, one skipped), including the live-child SSH shutdown case. Hosted Linux Clippy found a nested-condition lint in that Unix-only test; the condition is corrected without changing its behavior. A fresh full hosted result is required.

## Validated production slice

Explore uses typed feature labels and one shared Askama results component for its full page and direct HTMX fragment. The existing public-only listing service retains search, sort, archive and paging authority. Normal navigation, boosted requests and history restoration retain full-page responses. Diagnostic negotiation is scoped to Explore so other routes do not receive partially localized document/cache behavior.

The passing HTTP regressions cover full/fragment consistency, language and ETag dimensions, private repository exclusion even for an owner session, malicious text and ambiguous request headers. No runtime speed percentage is claimed from static inspection or test duration.

Password work is a second production slice. Startup validates `[security] max_password_jobs` (default 2, range 1–64), administrator saves preserve atomic validation, and the setting requires restart. Account, profile, administration, recovery and MFA services share one owner; offline account creation uses an explicit short-lived owner. Request rate-limit configuration cannot disable this resource bound.

Busy and Closed return generic retryable 503 responses without credential challenges or password-failure surcharges. Admission precedes one-use proof consumption. Real HTTP regressions preserve reset and MFA proofs and sessions across overload, then retry the same operation successfully. PAT-backed Git/LFS/API and health requests remain usable. Generic errors retain the HTMX history barrier when replacing a secret-bearing page. The [identity contract](../identity-admission.md) records remaining input-bound and operational measurement obligations. This slice does not retire the complete identity feature group.

## Work still required

1. Continue from the passing foundation checks. Process-tree ownership has independent review and Windows/Linux fault-test coverage; uncertain compiler/runtime ownership remains protected.
2. Measure the activated password-work slice under constrained memory/CPU before treating its initial default as a deployment recommendation; its hosted compatibility checks now pass.
3. Continue the complete identity port and resolve its remaining input-bound policy. Preserve the proof, failure and session contracts already verified by this slice.
   Complete access policy ports and OS process containment. SSH tracks its session/Git descendants, but legacy arbitrary-drop paths, exceptional wait failures and OS process-tree cleanup remain explicit obligations.
4. Establish fresh release/runtime benchmarks against the pinned main. The [rendering experiment](../rendering-compilation.md) investigates inherited Askama shell duplication without presuming a measured cause or improvement.
5. Continue repository/access/identity ports, durable required events, collaboration, Actions engine/UI, complete localization, release hardening, and migration/rollback rehearsals through the plan's gates.

Automatic cache maintenance evicts inactive compatibility namespaces after successful builds/tests while retaining the current namespace. The watermark is soft: a protected namespace can exceed it and reports its overage. Zero disables automatic maintenance; manual zero-budget pruning still empties an inactive cache. Crashed or uncertain build leases require process inspection before recovery. Account locale persistence, translations beyond English, global UI/email localization, runtime performance acceptance and full platform feature parity remain open.

The [cache investigation](../build-cache-retention.md) records retained artifact growth and a whole-lane retention proposal. Automatic rotation is deferred until its cold-build cost, transient space requirements and ownership gates are measured. The [Actions prototype](../prototypes/actions/README.md) remains a review artifact: source/HTTP tests pass, while browser, accessibility and production integration remain open.
