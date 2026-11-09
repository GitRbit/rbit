# Branch protection retirement acceptance

Acceptance evidence for the `collaboration.protection` ledger row: protected refs, stale
approvals and heads, and the merge gates shared by HTTP, SSH, the web editor and pull
request merges. The decisions and rule management now live in `rbit-access`.

## What moved

| Decision | Before | After |
| --- | --- | --- |
| Branch patterns, glob matching, case-alias matching on Windows/macOS, the reserved `refs/pull/` and namespace-case refusals | `rbit-core/src/protection.rs` | `rbit_access::protection` (`pattern.rs`) |
| Rule settings and their validation, rule limits, the audit summary | `rbit-core`, `rbit-services` | `rbit_access::protection` (`rules.rs`) |
| Push decision (refusal priority, force-push protection, admin bypass and its reporting), merge requirements, branch deletion, `is_protected` | `rbit-core/src/protection.rs` | `rbit_access::protection` (`decision.rs`) |
| Rule management: administrators only, pattern and settings validation, required contexts, allowed users (write access) and teams (owning organisation only), list limits, store outcome messages | `rbit-services/src/protection.rs` | `rbit_access::protection::RuleManagement` |
| Loading rules and the pusher's teams for push, merge, deletion and editor checks | `rbit-services/src/protection.rs` | `rbit_access::protection::ProtectionGate` |

`rbit-access` stays free of internal dependencies: rule identifiers and contexts are
generic (`ProtectionRule<U, T, C>`, `Pusher<U, T>`, `MergeRules<C>`), push commands are read
through a `RefChange` trait, and `rbit-core` fixes them to `AccountId`, `TeamId` and
`StatusContext` with type aliases at the old paths (`rbit_core::protection::ProtectionRule`
and friends), implements `RefChange` for `RefUpdate`, and keeps the stored
`BranchProtection` record and the transport-facing `PushActor`. `RuleInput` and
`RuleChange` moved from `rbit-storage`, which re-exports them; `MAX_REQUIRED_CONTEXTS`
moved from `rbit_core::status`, which re-exports it.

Ports: `RuleRepository` (rules, records, the pusher's teams, writes) implemented on
`BranchProtectionStore` in `rbit-storage/src/access_protection.rs`, whose SQL keeps the rule
limit, pattern uniqueness and organisation-only teams in the write transaction; and
`RuleDirectory` (allowed-user and team lookups) implemented in the service, because write
access is decided by the repository access policy (`access.policy`, still in `rbit-auth`).

`ProtectionService` stays the composition adapter with an unchanged public API: archived
repositories are refused before any rule is read, rule changes are audited, bypasses that
changed a push's outcome are audited as `BranchProtectionBypassed`, and refusals are traced.

## Legacy code removed

- The 600-line decision engine in `rbit-core/src/protection.rs`; the file now holds only
  aliases, the `RefChange` impl, `PushActor` and `BranchProtection`.
- The service's `require_admin`, `validate`, `writer_called`, `distinct`,
  `required_contexts`, `pattern_taken`, `team_outside`, `pusher` and `rules`, and its direct
  `decide_push`/`merge_rules`/`may_delete` calls.
- `RuleInput` and `RuleChange` definitions in `rbit-storage`.
- The decision tests moved verbatim into `rbit-access` (patterns, the 32×6×3 flag matrix,
  strict combination, required checks, deletion, the Windows/macOS alias cases), over
  generic identifiers.

## Boundaries that fail if legacy paths return

`scripts/rewrite/protection-retirement.mjs`, run by `scripts/rewrite/check.mjs` on its own
line:

- distinctive rule and decision items (`BranchPattern`, `RuleSettings`, `RuleError`,
  `ProtectionId`, `RuleChange`, `RuleInput`, `PushVerdict`, `PushDecision`, `decide_push`,
  `merge_rules`) may be defined only in `rbit-access`, in any crate;
- the legacy protection files may not redefine the generically named items (`Refusal`,
  `Pusher`, `MergeRules`, `ProtectionRule`, `reasons`, `may_delete`, …) except as aliases
  of the `rbit-access` ones;
- the service adapter may not define the moved decisions or call the decision functions
  directly.

Against the pre-slice sources it reports three violations; on this tree, none.
`scripts/tests/protection-retirement.test.mjs` covers each rule and the whole tree.

## Results

One Linux run of the repository's own jobs at `71a9a16e` (the code is identical at this
record's commit), in quick mode: formatting and workspace-wide all-target Clippy with
`-D warnings`, licenses and advisories, the front-end build, the rewrite contracts, and
nextest for the changed packages (`rbit-access`, `rbit-core`, `rbit-storage`,
`rbit-services`). The workspace-wide test run is left to the integrated batch.

| Job | Result |
| --- | --- |
| Format and all-target Clippy (whole workspace) | pass |
| Licenses and advisories, front-end build | pass |
| Nextest, changed packages | 1,400 run, 1,400 passed: `rbit-access` 32, `rbit-core` 136, `rbit-storage` 452, `rbit-services` unit 264 and integration 516 |
| `node scripts/rewrite/check.mjs` and the script tests | 0 violations; 331 Node tests pass |
| `scripts/rewrite/check-types.mjs` | `rbit-access` strict report clean; the gate fails only on the base's `identity_access_tokens.rs` self-type resolution, fixed separately in the stack and not touched here |

Protection-relevant groups in that run, all passing: services `protection` 5, `pulls` 16
(merge gates, stale approvals and heads, head-branch deletion), `edits` 6 (editor branch
refusal), `archives` 6 (archived push refusal), `statuses` 3, `repo_settings` 21,
`permission_matrix` 6, `access` 7 (collaborators); storage `protection` 5, `collaborators`
7. The moved engine tests run in `rbit-access`: patterns, the full flag/actor/update
matrix, strict combination, every ref of a push, merge requirements, required checks and
deletion. The two Windows/macOS case-alias tests compile and run only on those systems;
they were not run for this slice. `rbit-ssh` protection tests and `rbit-web` were outside
the changed-package set and were not run; neither crate changed.

## Parity argument

- Routes, transports, templates and localization are untouched. Public service signatures
  are unchanged; the moved types are reachable at their old paths through re-exports and
  aliases, so callers (pull merging, editor, SSH and HTTP receive paths, statuses, web
  views and storage) compile unchanged.
- Every refusal reason and validation message moved verbatim. The engine is the same code
  with identifiers made generic; its exhaustive tests moved unchanged apart from the
  identifier types.
- Check order is preserved: archive refusal first; administrator check, pattern, settings,
  contexts, then users before the "only organisations can allow teams" refusal and team
  lookups; teams are loaded only when a rule names teams; deploy keys never match allowed
  lists or administrator bypass.
- `may_delete` and the editor check build the same command shapes as before (an existing
  ref deleted or updated).

## Migration, rollback and performance

No migration, schema or wire format changed. Rollback is the previous binary against the
same data. Request paths run the same queries in the same order. No release measurement
was taken and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 4 (protection engine, service delegation, boundary, collaborator slice) plus this record |
| Files changed by code commits | 30 |
| Lines added / removed (all) | 2,479 / 1,300 |
| Production Rust (`src`, non-test) | 18 files, +1,618 / −1,299 (the engine moved; about 400 of the removed lines are its tests, re-added under `rbit-access`) |
| Tests | 7 files, +807 (moved decision tests, new management, gate and collaborator tests, two Node guards' tests) |
| Checker and policy scripts | two new Node guards (31 and 16 lines), each on its own line in `check.mjs` |

Tricky parts:

- `rbit-access` may not depend on `rbit-core`, yet the engine's types are used across
  storage, services, web, SSH and transport. Generic identifiers plus aliases at the old
  paths kept every caller unchanged; struct literals and enum variants work through the
  aliases.
- Generic decision functions lose inference where a caller passes an empty slice; only
  tests did, including two Windows/macOS-only tests that Linux builds never compile.
- Allowed-user validation needs the repository access policy, which is still in
  `rbit-auth` and the service layer, so the directory port is implemented in the service
  rather than in storage.
- `rbit-core/src/status.rs` sits at its size ceiling, so `FromStr for StatusContext` lives
  in the protection compatibility module.

Estimate: a row whose decisions are already pure but live in a legacy crate costs about
one focused day plus verification, mostly in keeping the old paths compiling. Rows that
still mix decisions with I/O in one service file (like collaborators here) are about half
a day each.
