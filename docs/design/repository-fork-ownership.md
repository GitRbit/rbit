# Fork application ownership

`rbit-repositories::forks` now owns fork input and the production creation, parent,
listing/flags and private-ancestor workflows. `rbit-services::forks::ForkInput` and
`MAX_LISTED_FORKS` remain exact compatibility reexports. No manifest, lock, schema,
route, serialized response, Git algorithm or SQL body changes are required.

Creation keeps read access before owner resolution before name validation. The
existing owner authority remains shared with repository creation; no second lookup
or fresh actor policy is introduced. Empty owner/name retain actor/source defaults.
Visibility, description and branch are copied exactly as before, including archived
sources being forkable. The metadata handle is constructed only after validation;
role wrappers move existing concrete store handles without cloning them.

The application owns metadata → native copy → relationship recording order. Concrete
copy retains the original source lease while cloning and releases it before recording
the relationship. A copy/record failure logs the original error, attempts disk cleanup,
logs its raw Git cleanup error without replacing the primary failure, then removes SQL
metadata. SQL rollback failure still takes precedence over the original copy/record
error. Successful rollback returns that original failure. The source-pushed mark is
best-effort and never suppresses successful publication. Only RepositoryForked is
published, never RepositoryCreated; no intermediate creation service is substituted.

`ForkQueries` retains public-parent one-query return and private-parent open/NotFound
suppression. Other open errors propagate. Listing uses existing SQL visibility with
the same optional viewer/admin facts, raw limit/offset, default100 and sequential list
then count. No per-row permission query or cache is added. Private ancestry keeps its
original HashSet and maximum64 parent lookups: missing/deleted parents end the walk;
private ancestors, cycles and exhaustion return true. The separate existing SQL fork
network depth10 is unchanged.

ForkService owns explicit configuration/database/disk/event/owner-service handles.
Feature ports receive borrowed views, exact existing DTOs and infrastructure effects;
there is no new ServiceContext or copied DTO tree. Pure application diagnostics omit
payloads and impose no port Debug bounds. New legacy adapters and ForkService itself
are registered in the cumulative400-line component ledger.

## Preserved gaps and prepared acceptance

This is an ownership extraction, not new transaction or background-task guarantees.
Cancelling foreground work after SQL creation can still leave a metadata/partial-disk
gap; there is no new retained owned worker or startup reconciliation. SQL/disk changes
remain separate operations. The source lease still ends before recording the edge,
and the existing primary/compensation failure behavior can leave repair work. These
limits require a separately reviewed lifecycle design; do not infer crash safety or
native descendant containment from the new ports. Accepted RepoContext/User pairing
remains trusted composition rather than a newly minted authorization receipt.

Nine pure fixtures cover validation/store construction order, inherited settings,
warning-only pushed marks, copy/relationship/disk/SQL failure combinations, one-winner
unique race with held copy, unpolled/cancelled work, public/private parent privacy,
raw/default paging, flags and exact depth/cycle boundaries. Four new real-service
fixtures prepare SQL work budgets, only-fork event, private listing, unique race and
copy-failure rollback checks. All Rust/native/HTTP tests are unrun during preparation.

Root acceptance through its exclusive shared helper lane:

- Repository domain `fork_creation` and `fork_queries`, then the existing repository suite.
- Services integration `forks::` (including the native Git fixture), repository settings,
  related repository-access and HTTP/API fork tests; strict affected/workspace Clippy.
- Existing exact AST checker on rbit-repositories plus registered components, physical
  bounds and dependency policy. No new dependency or runtime feature should appear.

The proposed real-store assertions are 0SQL unreadable/self-owner bad-name, 1SQL owner
lookup failure, 3SQL source-pushed successful fork, 2SQL list/count, 1SQL public parent,
0SQL empty fork flags, 2SQL copy failure/metadata rollback and 3SQL unique race with an
unpushed source. These are unexecuted test oracles, not latency or allocation measurements.
The complete fork ledger remains pending native/HTTP acceptance and lifecycle work.
