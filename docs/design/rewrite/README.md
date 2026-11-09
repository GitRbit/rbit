# Rewrite baseline and acceptance tracking

The baseline is GitHub main `39f932a00b7aa35b324e06a9ff2343a57d4bed76`. The rewrite branch begins there, independently of other pull requests. Changes use atomic commits with focused validation. The [rewrite plan](../ground-up-rewrite-plan.md) defines the complete scope.

`main-inventory.json` is generated from immutable Git objects, so dirty checkouts and moving refs cannot contaminate it. It records source/schema fingerprints, existing internal dependencies, file-size ceilings, literal route declarations, OpenAPI operations and available test sources. Regenerate it only for an explicit reviewed baseline update:

```text
node scripts/rewrite/inventory.mjs docs/design/rewrite/main-inventory.json 39f932a00b7aa35b324e06a9ff2343a57d4bed76
```

Literal route declarations do not resolve dynamically constructed routes, conditional feature registration or HTTP method chains. OpenAPI operations complement that inventory; runtime route and protocol acceptance tests remain necessary. Source counts include tests/comments/blanks. Generated JSON is data and is exempt from authored-source line limits.

`feature-ledger.json` maps observable behaviors to owning replacement modules and baseline evidence. `baseline-retained` means existing code remains available and replacement is pending. A source or documentation reference in the `test` field is evidence to inspect, not a newly executed passing test. Ported entries must record replacement paths and concrete acceptance results before changing status. The initial catalog groups related capabilities; expand it when route/protocol inspection finds distinct acceptance obligations.

`security-contracts.md` defines required threat and regression boundaries. Legacy implementations remain while features are ported. Do not remove a feature merely because the first replacement slice does not implement it yet.

Newly requested product scope lives in `plannedAdditions`, without pretending it was
present in the pinned baseline. Its plan, target crate and delivery phases are
required. `planned` and `in-progress` entries may record partial preparation/results;
`implemented` additionally requires complete implementation, actual target-crate
source, acceptance-result files and a `phaseEvidence` file for every delivery phase.
The checker validates these references and states; it does not establish that the
referenced tests actually passed or cover the complete feature. Inspect those results
and the runtime behavior before accepting completion. A wiki plan or W0 contract
cannot satisfy the W1–W5 product and operational requirements.

The policy command rejects new internal dependency edges, new authored files over 400 physical lines, growth beyond baseline ceilings for existing oversized files, and edits/removal of historical SQL migrations. Cumulative logical-type checking is a separate AST gate for strict replacement crates. Public interfaces require crate/module responsibility and invariant documentation.

The same checker also audits every crate's Rust source, tests included, for the retired
`Passwords` facade. It checks parsed source tokens, namespace aliases and auth/identity
wildcard imports, including cfg-gated and authored macro tokens; comments and strings
are allowed, and macro expansion is not claimed. The
[identity retirement](identity-retirement.md) lists the other boundaries that keep
retired identity paths from returning. See the
[checker scope](../../../tools/rewrite-check/README.md#legacy-password-scheduling-boundary).

Runtime performance acceptance requires new release measurements from the pinned main and candidate with matching fixtures/toolchains/assets. Source inventories, test durations and old noisy debug measurements are not substitutes for performance results. Build through the [shared worktree helper](../../development-worktrees.md); do not create per-worktree compiler directories.
