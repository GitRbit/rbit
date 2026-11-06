# Rewrite implementation progress

The rewrite is active on `feature/ground-up-rewrite`, starting from GitHub main `39f932a00b7aa35b324e06a9ff2343a57d4bed76`. Changes are kept in atomic commits. The [delivery plan](../ground-up-rewrite-plan.md) remains the completion contract; the foundation work below does not mean the platform rewrite is complete.

## Implemented foundations

| Area | Current implementation |
| --- | --- |
| Baseline and parity | Immutable Git-object inventory, schema fingerprints, 55 feature groups, required security contracts and guarded acceptance records |
| Shared contracts | `rbit-contracts` owns account/repository IDs; existing core module/root paths, constructors and serialized integer behavior remain compatible |
| Kernel admission | Immediate bounded blocking admission and non-cloneable reservations; actual worker lifetime owns capacity across request cancellation |
| Localization | Bounded locale resolution, typed compiled messages, opt-in expansion/RTL diagnostics and plain-text interpolation; static English labels borrow catalog text |
| Structural checks | Physical-file ceilings, legacy size/dependency ratchets, immutable migrations and an AST checker for cumulative type definitions and implementations in replacement crates |
| Developer storage | Nested worktree lifecycle, one shared Cargo lane for compatible builds, both output-directory overrides, exact executable copies, cache reporting and a configurable 16 GiB soft watermark for inactive namespaces |
| Build integration | Shared development and performance launchers, editor JSON output, isolated runtime data/HTTP ports, verified process-tree ownership and managed-development SSH disabled by default |
| CI foundation | Dedicated Linux and Windows rewrite-contracts jobs supplement the existing platform checks and release safeguards |

The current kernel is a work-admission foundation. Full module lifecycle, identity-budget wiring and the remaining feature ports are still pending. Existing services remain the behavior authority during the transition.

## Validation checkpoint

- Contracts/core: 150 tests and strict Clippy passed after ID extraction; the 148 core tests also passed with the patched Markdown parser.
- Kernel admission: all ten cancellation, saturation, panic, closure, reservation and queued-abort tests and strict Clippy passed.
- Localization: 13 tests, including real Askama escaping and borrowed/static output checks, and strict Clippy passed.
- Structural checker: 16 tests and strict Clippy passed; runtime checks of all four replacement/tool crates, including reservations, reported no size violations.
- JavaScript: the final full Windows suite passed 231 tests, with one Unix-only case skipped. The final helper/performance suite passes 50 tests on Windows and 49 on Linux, with only opposite-platform cases skipped. Linux runs use a disposable native Git fixture because Linux Git cannot resolve Windows worktree metadata.
- Web: all targets passed type checking. All 415 unit, 129 integration and one Actions evaluation test passed with the patched Markdown parser. Strict Clippy passed for all core/web targets with the patched dependency graph. The combined core/web result is 693 passing tests.
- Dependency security: `pulldown-cmark` minimum and lockfile raised to 0.13.6 for RUSTSEC-2026-0433. Advisories and licenses pass, with no new advisory exception or unrelated external upgrade.
- Real worktree check: a second source checkout passed all ten kernel tests using the same compatibility namespace and was removed cleanly. No checkout-local compiler tree was created.
- CI configuration is committed; no hosted CI result is claimed for unpublished local commits.

## Validated production slice

Explore uses typed feature labels and one shared Askama results component for its full page and direct HTMX fragment. The existing public-only listing service retains search, sort, archive and paging authority. Normal navigation, boosted requests and history restoration retain full-page responses. Diagnostic negotiation is scoped to Explore so other routes do not receive partially localized document/cache behavior.

The passing HTTP regressions cover full/fragment consistency, language and ETag dimensions, private repository exclusion even for an owner session, malicious text and ambiguous request headers. No runtime speed percentage is claimed from static inspection or test duration.

## Work still required

1. Continue from the passing foundation checks. Process-tree ownership has independent review and Windows/Linux fault-test coverage; uncertain compiler/runtime ownership remains protected.
2. Integrate and validate the prepared identity credential extraction and bounded password-attempt API. Existing services still use the legacy primitive until their constructors and call sites migrate.
3. Wire service password admission using the [identity contract](../identity-admission.md), reserving capacity before consuming one-use proofs and preserving uniform known/unknown-login behavior.
4. Establish fresh release/runtime benchmarks against the pinned main. The [rendering experiment](../rendering-compilation.md) investigates inherited Askama shell duplication without presuming a measured cause or improvement.
5. Continue repository/access/identity ports, durable required events, collaboration, Actions engine/UI, complete localization, release hardening, and migration/rollback rehearsals through the plan's gates.

Automatic cache maintenance evicts inactive compatibility namespaces after successful builds/tests while retaining the current namespace. The watermark is soft: a protected namespace can exceed it and reports its overage. Zero disables automatic maintenance; manual zero-budget pruning still empties an inactive cache. Crashed or uncertain build leases require process inspection before recovery. Account locale persistence, translations beyond English, global UI/email localization, runtime performance acceptance and full platform feature parity remain open.
