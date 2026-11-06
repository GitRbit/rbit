# Rewrite implementation progress

The rewrite is active on `feature/ground-up-rewrite`, starting from GitHub main `39f932a00b7aa35b324e06a9ff2343a57d4bed76`. Changes are kept in atomic commits. The [delivery plan](../ground-up-rewrite-plan.md) remains the completion contract; the foundation work below does not mean the platform rewrite is complete.

Public development and review continue in [draft PR #53](https://github.com/GitRbit/rbit/pull/53), opened by ChaseTheCode against main. It supersedes [PR #47](https://github.com/GitRbit/rbit/pull/47), which is closed in its favor. The [update on issue #46](https://github.com/GitRbit/rbit/issues/46#issuecomment-6673779981) explains how the modularity and localization direction fits into this broader rewrite; the issue remains open.

## Implemented foundations

| Area | Current implementation |
| --- | --- |
| Baseline and parity | Immutable Git-object inventory, schema fingerprints, 55 feature groups, required security contracts and guarded acceptance records |
| Shared contracts | `rbit-contracts` owns account/repository IDs; existing core module/root paths, constructors and serialized integer behavior remain compatible |
| Identity credentials | Extracted credential primitives preserve legacy auth types; explicit shared password attempts retain capacity through queued/running work |
| Kernel admission | Immediate bounded blocking admission and non-cloneable reservations; actual worker lifetime owns capacity across request cancellation |
| Localization | Bounded locale resolution, typed compiled messages, opt-in expansion/RTL diagnostics and plain-text interpolation; static English labels borrow catalog text |
| Structural checks | Physical-file ceilings, legacy size/dependency ratchets, immutable migrations and an AST checker for cumulative type definitions and implementations in replacement crates |
| Developer storage | Nested worktree lifecycle, one shared Cargo lane for compatible builds, both output-directory overrides, exact executable copies, cache reporting and a configurable 16 GiB soft watermark for inactive namespaces |
| Build integration | Shared development and performance launchers, editor JSON output, isolated runtime data/HTTP ports, verified process-tree ownership and managed-development SSH disabled by default |
| CI foundation | Dedicated Linux and Windows rewrite-contracts jobs supplement the existing platform checks and release safeguards |

The current kernel is a work-admission foundation. Password consumers now share one validated instance owner; full module lifecycle and the remaining feature ports are still pending. Existing services remain the behavior authority during the transition.

## Validation checkpoint

- The activated integration tree passes `cargo fmt --all -- --check`, strict workspace/all-target Clippy, advisory/license checks, source policy, cumulative type ceilings and the service/server password-boundary audit. Full workspace tests and release/runtime gates still require the candidate's hosted and operational evidence.
- Contracts/core: 150 tests and strict Clippy passed after ID extraction; the 148 core tests also passed with the patched Markdown parser.
- Identity/auth: 24 identity tests and 42 auth/compatibility tests passed; strict Clippy passed for both crates and all targets. The password-attempt API covers single-slot login/reuse checks and eagerly zeroizing inputs, with a fixed published Argon2id compatibility vector. Production password consumers now use this owner.
- Services: all 530 tests passed after activation. They cover shared saturation, closure, cancellation-safe ownership, proof preservation, bootstrap races, password replacement compare-and-swap and session revocation. Explicit injected capacity must match validated configuration.
- Kernel admission: all ten cancellation, saturation, panic, closure, reservation and queued-abort tests and strict Clippy passed.
- Localization: 13 tests, including real Askama escaping and borrowed/static output checks, and strict Clippy passed.
- Structural checker: 22 Windows tests and strict Clippy passed. The password-boundary audit reports 114 service/server source files and no legacy-facade violations. The replacement/tool crate type checks report no size violations. The source policy reports 1,046 authored files and no violations; Unix-only link fixtures are separate from the Windows result.
- JavaScript: the final integrated Windows suite completed 264 cases (263 passed, one Unix-only case skipped), including compiler-observer startup/shutdown, process identity, report validity, controlled compiler flags and helper hooks. Combined compiler/observer/provenance/report failures retain the real shared lease, verified without starting a compiler. Root-alias helper regressions also passed with a declared Windows junction as the temporary root. Earlier Linux helper runs use a disposable native Git fixture because Linux Git cannot resolve Windows worktree metadata.
- Web/server: the broad run passed 423 web unit tests, 134 web integration tests and one Actions evaluation test, plus 12 server tests. One server helper remains ignored as a standalone test; its parent executes seven isolated startup/configuration child cases. A subsequent test-only expansion passed all six password-protocol cases, bringing the web integration inventory to 135. These verify real cookie, PAT, Git and LFS behavior under Busy and Closed admission.
- Rendering probe: all six semantic/CLI tests and strict Clippy passed for inherited, segmented and block composition at eight distinct contexts. The [balanced runtime experiment](measurements/render-runtime-count8.md) records 18 samples with equal HTML output; synthetic render-loop medians are not production HTTP performance evidence. The compiler experiment now supports balanced 1/8/32/96-context samples, explicit build settings, package-scoped cleaning and owned-process memory observations; measurements remain pending.
- Dependency security: `pulldown-cmark` minimum and lockfile raised to 0.13.6 for RUSTSEC-2026-0433. Advisories and licenses pass, with no new advisory exception or unrelated external upgrade.
- Real worktree check: a second source checkout passed all ten kernel tests using the same compatibility namespace and was removed cleanly. No checkout-local compiler tree was created.
- Hosted CI passed in full at `e8e5dba098d5185258232b497da0d478a9d96e0a`: [run 42141554197](https://github.com/GitRbit/rbit/actions/runs/42141554197). That run includes Linux/Windows rewrite contracts and tests, Clippy, frontend, runner image, advisories/licenses and release build. It predates password-service activation; later local results do not imply a hosted pass for the new revision.

## Validated production slice

Explore uses typed feature labels and one shared Askama results component for its full page and direct HTMX fragment. The existing public-only listing service retains search, sort, archive and paging authority. Normal navigation, boosted requests and history restoration retain full-page responses. Diagnostic negotiation is scoped to Explore so other routes do not receive partially localized document/cache behavior.

The passing HTTP regressions cover full/fragment consistency, language and ETag dimensions, private repository exclusion even for an owner session, malicious text and ambiguous request headers. No runtime speed percentage is claimed from static inspection or test duration.

Password work is a second production slice. Startup validates `[security] max_password_jobs` (default 2, range 1–64), administrator saves preserve atomic validation, and the setting requires restart. Account, profile, administration, recovery and MFA services share one owner; offline account creation uses an explicit short-lived owner. Request rate-limit configuration cannot disable this resource bound.

Busy and Closed return generic retryable 503 responses without credential challenges or password-failure surcharges. Admission precedes one-use proof consumption. Real HTTP regressions preserve reset and MFA proofs and sessions across overload, then retry the same operation successfully. PAT-backed Git/LFS/API and health requests remain usable. Generic errors retain the HTMX history barrier when replacing a secret-bearing page. The [identity contract](../identity-admission.md) records remaining input-bound and operational measurement obligations. This slice does not retire the complete identity feature group.

## Work still required

1. Continue from the passing foundation checks. Process-tree ownership has independent review and Windows/Linux fault-test coverage; uncertain compiler/runtime ownership remains protected.
2. Validate the activated password-work slice in hosted CI, and measure constrained-memory/CPU behavior before treating its initial default as a deployment recommendation.
3. Continue the complete identity port and resolve its remaining input-bound policy. Preserve the proof, failure and session contracts already verified by this slice.
4. Establish fresh release/runtime benchmarks against the pinned main. The [rendering experiment](../rendering-compilation.md) investigates inherited Askama shell duplication without presuming a measured cause or improvement.
5. Continue repository/access/identity ports, durable required events, collaboration, Actions engine/UI, complete localization, release hardening, and migration/rollback rehearsals through the plan's gates.

Automatic cache maintenance evicts inactive compatibility namespaces after successful builds/tests while retaining the current namespace. The watermark is soft: a protected namespace can exceed it and reports its overage. Zero disables automatic maintenance; manual zero-budget pruning still empties an inactive cache. Crashed or uncertain build leases require process inspection before recovery. Account locale persistence, translations beyond English, global UI/email localization, runtime performance acceptance and full platform feature parity remain open.
