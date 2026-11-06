# Shared build cache retention

Status: graph normalization and a non-mutating decision planner are implemented;
automatic graph rotation remains deferred pending measurement and an executor review.

The shared helper serializes compilation, tests, publication, and maintenance with
one repository management lease. Compiler compatibility namespaces are shared
across worktrees. Successful binary builds copy the exact Cargo-reported
executable into a separate runtime generation before maintenance. Cache pruning
must preserve those copies, pending handoffs, and runtime ownership markers.

## Observed storage

On November 5, 2026, a completed-command report retained 17,787,384,033 bytes in
the current namespace: 607,514,849 bytes above the 16 GiB soft limit. The current
namespace is protected, so deleting older namespaces cannot eliminate this
overage. Different package selections and unified feature graphs accumulate
different Cargo artifacts within that namespace.

A subsequent read-only sample during compilation observed approximately 18.36 GB.
This was a live, non-atomic sample, not a reproducible completed-build total:

| Category | Bytes |
| --- | ---: |
| Debug intermediates | 17,570,643,574 |
| Debug final target artifacts | 690,801,419 |
| All `.rlib` files | 5,868,111,312 |
| All `.pdb` files | 5,807,472,640 |
| All `.exe` files | 3,351,471,616 |
| All `.rmeta` files | 2,772,071,435 |

The two classifications overlap. The sample contained seven hashed base64 stems,
five rustls stems, and eleven sqlx-core stems. These are artifact variants, not a
count of feature graphs: checking, building, profiles, and compiler units can
produce different variants too.

The dev profile already disables incremental compilation, uses line tables for
workspace debug information, and disables dependency debug information. Removing
PDBs or selected hash-named artifacts is not an established safe compaction
policy. Cargo metadata, fingerprints, build-script outputs, and debugging
relationships must remain consistent.

## Candidate bounded retention policy

Retain complete Cargo lanes rather than interpreting private Cargo artifact
relationships. Within a compatibility namespace, permit one current generation
and at most one retired warm generation. These are repository-wide generations,
never directories per worktree, branch, or source revision.

Track graph selection as scheduling metadata, not executable identity. Normalize
package/workspace/exclusion selection, features and defaults, target selection,
profile, and command mode. Test-name filters and harness arguments do not define
a new graph. Unknown switches require conservative treatment. Root profile and
configuration policy, and relevant profile environment overrides, must affect
compatibility policy identity. Do not create a directory for every lockfile or
commit change; Cargo already validates source and dependency freshness.

Reuse the existing lane while under budget. When completed-command maintenance
detects overage, record a pending compaction decision. A later materially
different graph could build into a fresh generation under the same management
lease. Only successful execution and exact executable publication would promote
that generation. Whole retired generations could then be evicted by recency and
budget. A failed candidate must not destroy the last successful generation or
allow failed generations to accumulate. Uncertain compiler termination retains
the blocked lease and forbids promotion or pruning.

A single oversized graph remains an explicitly reported soft-limit overage.
Repeated requests for that graph must not cause endless cold rebuilds. Two
persistent graph-specific caches would duplicate common dependencies and could
increase storage; they are not the default recommendation.

## Tradeoffs and activation gate

Preserving the previous successful generation until replacement succeeds can
temporarily require both complete lanes. Rotation also requires a cold rebuild.
Measure graph sizes, replacement build time, and available space before enabling
this policy. Insufficient headroom must defer rotation and report the reason;
it must not delete live output or claim a hard byte bound. For now, retain the
existing honest protected-namespace overage report.

Tests for any future implementation must cover graph-equivalent filters and
argument ordering; graph/profile distinctions; successful handoff before
eviction; failed replacement preservation; bounded failed candidates; repeated
oversized-current requests; insufficient headroom; blocked/stale lease refusal;
symlink containment; and unchanged runtime copies and pending handoffs. Tests
should inject execution and storage observations rather than launch unnecessary
compiler processes.

## Implemented policy tools

The modules are `scripts/workspace/retention/graph.mjs`, `planner.mjs`, and
`scripts/workspace/retention-plan.mjs`. These dependency-free Node modules perform
no Cargo execution, repository discovery, cache measurement, directory creation,
copy, move, prune or rotation. The existing build helper and manual pruning behavior
are unchanged. Every decision has `apply: false`, including simulated enabled
rotation. No environment variable activates an executor because none exists yet.

The graph normalizer accepts a command, an argument array, explicit environment
observations and observed compatibility/manifest/configuration digests. It sorts
package, exclusion, feature and target selections and removes duplicates. It
separates command mode, Cargo profile and target selections from test-name filters,
harness arguments, known output options and nextest runner profiles. Unknown
switches or compiler arguments, conflicting options and missing policy observations
produce a conservative graph which cannot justify replacement. Cargo still checks
package/feature existence, source/dependency freshness and command validity; this
is not a substitute argument validator or a claim to solve arbitrary Cargo aliases.

Relevant profile environment entries are hashed in sorted order as whole-namespace
policy, including custom/inherited profiles. Unrelated environment is ignored.
Root manifest and effective configuration digests must be supplied by an eventual
verified helper adapter; the dry tool never reads live root inputs. The configuration
digest must cover effective workspace, nested and Cargo-home configuration and
applicable profile policy, not just one convenient file. Compatibility must come
from the existing actual compiler/flag identity. A changed compatibility policy
requires namespace handling before rotation, rather than silently promoting a
candidate under a different policy. Lockfile and commit IDs are not graph directory
keys. `normalizeGraph` retains canonical selectors in memory; public dry-tool output
contains only hashes, confidence, fixed reasons and validated generation identifiers.

The planner consumes injected observations with:

- `phase`: `after-success`, `before-command` or `candidate-complete`;
- an owned lease with a verified token and certain stopped process tree;
- one successful current lane, at most one retired warm lane and one candidate;
- completed namespace/free-space observations, a soft byte budget and pending flag;
- normalized requested/current/candidate graph identities;
- independent runtime-copy and safe pending-handoff observations;
- explicit cold-cost/headroom measurement and simulated `rotationEnabled` control.

Lane records carry an opaque ID, bytes and last-used observation. IDs are never
interpreted as paths. A candidate also needs status, completed execution and exact
completed handoff before promotion. A command producing no external executable
still needs an explicit verified no-handoff-needed completion, represented by exact
completed handoff observations; it is never inferred from absent fields.

The planner records completed overage first, preserves oversized same-graph reuse,
defers unknown graphs/unmeasured cold costs/insufficient headroom, and blocks stale,
blocked or uncertain ownership. A failed stopped candidate can only be discarded
while retaining the last successful current generation. Successful promotion keeps
at most one retired warm lane by recency and budget, after exact handoff. Unknown
bytes are retained and included in overage reporting. Current-only overage remains
explicitly soft; there is no false hard bound or repeated cold rebuild for filters.

For a dry JSON request, call:

```sh
node scripts/workspace/retention-plan.mjs < retention-observations.json
node --test scripts/tests/workspace-retention*.test.mjs
```

The input object has `request` and `observation` fields. `request` is the graph
normalizer input; `observation.current.request` and optional
`observation.candidate.request` use that same shape. Other observation fields match
the planner contract above. The CLI caps input at 64 KiB, writes JSON only to stdout,
omits raw selectors/profile environment, and never touches compiler/runtime/data
paths. Fixtures in the two test files provide complete runnable observation examples.
The boolean ownership and protection observations are not self-authenticating proof;
the dry CLI cannot verify a live lease and cannot authorize a real mutation.

## Remaining activation work

Measure representative graph cold rebuild time/size and temporary free-space needs
first. Then review a separately implemented executor/adapter which obtains these
observations under the existing management lease, validates the exact owner token,
captures complete post-command usage, and guards every generation/leaf path against
symlinks and reparse traversal. It must retain that lease across candidate creation,
build/test, exact publication, promotion and cleanup; it must preserve blocked leases
on uncertain termination and bound failed-candidate recovery. Source, runtime copies,
data, downloads and unknown paths must never be deletion candidates. Symlink
containment and real process/copy lifetime tests belong to that activation, not a
claim made by this pure planner's injected metadata tests. No automatic rotation or
new cache generation directories are enabled by this policy implementation.

Argument semantics are grounded in the official [Cargo test options](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
and [nextest Cargo/runner options](https://nexte.st/docs/running/).


## Opt-in Cargo completion foundation

The managed-retention planner cannot treat ordinary Cargo close as proof of all
compiler descendants stopping. `RBIT_WORKSPACE_CARGO_TREE=windows-job` now selects
a strict Windows-only execution path in the shared helper; absent or `off` retains
the existing behavior. Unknown values and unsupported operating systems fail before
execution, with no process-group/PID fallback. This mode remains off by default and
is not automatic cache rotation, compaction, pruning or a storage-budget guarantee.

The path requires the actual management lease and reuses the existing Windows
JOB_LIST launcher: the Cargo proxy is associated at creation, KILL_ON_JOB_CLOSE
remains enabled, and the launcher acknowledges only a verified empty job. JSON
stdout, optional captured diagnostics, test output, cwd/environment and proxy
invocation name are preserved. All success/error decisions wait for native close,
readline drain and the completion proof. Missing/unreadable proof or an uncertain
launch retains private per-run controls and blocks the lease, even if diagnostics
cannot be written. A proved compiler failure remains an ordinary failure and cannot
publish executables. Callback, output and pipe failures also prevent publication.

An in-process receipt is bound to the current lease by private WeakMap identity.
Serialized booleans or copied receipt JSON are not proof. This is a prerequisite
for trusted retention observation, not deletion authority: the stdin planner stays
simulation-only. Existing protected namespaces, runtime copies and exact binary
handoffs retain their current rules. The launcher is an ownership mechanism for
cooperative developer workloads, not isolation against hostile same-user code that
can modify control files or stop the controlling host. Abrupt host/launcher failure
without proof requires investigation; no certainty is reconstructed from PID absence.

Focused fake-process tests cover actual lease exclusion/blocking, proof and launch
failures, compiler and callback/output/pipe errors, final JSON drain and receipt
forgery/reuse. A real native Node fixture (no Cargo or rustc; its launcher may compile C#) is explicitly gated:

```powershell
$env:RBIT_RUN_NATIVE_CARGO_PROOF = '1'
node --test scripts/tests/workspace-cargo-native.test.mjs
```

On November 6, 2026, the real Windows fixture passed, including verified termination
of its lingering compiler-style child. Strict-mode `check --package rbit-kernel
--locked` and the corresponding `ide-check` passed, with valid Cargo JSON ending in
`build-finished` success. The structural-checker build, exact executable handoff
and checker execution also passed under this mode, followed by strict workspace
Clippy. These are local cooperative Windows workload observations, not hostile
descendant or production Git containment acceptance.

Linux compiler ownership, trusted persisted generation records, a retention executor,
cold-build cost/headroom measurement and an activation decision remain pending.
No cache deletion or rotation was performed for this validation; the existing
protected namespace still exceeds its soft watermark.


## Trusted managed observation

`RBIT_BUILD_MANAGED_RETENTION=observe`, together with strict `windows-job` Cargo
ownership, opts the actual shared build helper into bounded observation. It remains
off by default. The helper captures input provenance before execution and publishes
only after successful native completion, exact executable handoff, and completed
post-command maintenance while still holding the same verified management lease.
No new Cargo process, metadata command, rotation or artifact deletion is introduced.
The optional filesystem-reader seam exists for temporary fixtures; the CLI accepts
no observation document, graph override, read callback or deletion authority.

Graph selection comes from the actual helper command, args and environment. Only
its hashes, confidence and enumerated reasons are retained. A bounded literal member
list identifies root/member manifests and the lockfile; unsupported workspace forms
remain conservative instead of attempting another TOML implementation. Root/ancestor
and Cargo-home configuration follow Cargo's hierarchical search. Configuration
includes, inaccessible/linked/oversized inputs and input drift produce conservative
records. Each input is bounded to 4 MiB, read with one lookahead byte, and reduced to
SHA256 before retaining the next input. No manifest/configuration content or paths,
Cargo package selectors, raw argv or environment values enter the record. Cargo
remains responsible for dependency/source freshness; this is scheduling provenance.
Configuration locations/overrides are conservatively significant, not proof that
all otherwise identical worktree configurations have identical semantics.

Namespace usage walks metadata with bounded streaming directory iteration, never
following links, with at most 200,000 entries and 32 levels. Partial observations carry
`complete:false`; free bytes are an observation at that instant, not reserved headroom.
Copied executables are compared by streaming SHA256/size and refused if they are the
same underlying file, so caller-supplied `exact:true` cannot establish independent
copies. The record retains only output lengths and SHA256, never filenames or paths.
These checks operate within the helper's cooperative management lease; they do not
isolate against hostile same-user writers that can change source/control files.

One atomically replaced, maximum 64 KiB `managed-observation.json` lives in each
compatibility namespace. Unknown existing files are refused rather than overwritten.
Per-command IDs do not create accumulating history directories. The record itself
is evidence, not authority: an opaque in-process observation plus a fresh branded
native completion receipt for the same live lease is required for publication.
Serialized observations, boolean fields, replayed handles and foreign-lease receipts
are refused. Existing record JSON is never used as process or deletion proof.
The measured byte count explicitly precedes replacement of this small record.

Crucially, the accumulated `shared` directory remains **legacy-shared and
unevictable**. Completing one command does not retroactively prove generation
ownership for its older contents, runtime copies, or pending handoffs elsewhere.
The existing pure planner receives current-success-unproved and cannot promote or
evict that lane; its returned decision still has `apply:false`. The observer does not
resolve the current protected-namespace overage or establish a hard storage bound.

Temporary-fixture tests cover actual helper adoption/default-off behavior, native
receipt/observation forgery and replay, disk-derived input changes, exact versus
mismatched/hard-linked copies, record replacement/refusal, private-field absence,
bounded walks and linked outside sentinels. No real cache was scanned or changed by
those tests. Actual opt-in observation validation, managed generation creation,
trusted persisted-generation activation, the bounded executor, measured cold cost
and reserved headroom remain required before rotation can be enabled. Privacy-safe
records are not automatically published or uploaded.

[Cargo configuration hierarchy](https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure)
is the source for repository/ancestor/Cargo-home lookup; unsupported configuration
mechanisms are deliberately unproved.


## Explicit fresh-output candidate measurement

`RBIT_BUILD_MANAGED_RETENTION=measure-candidate` is a separate, default-off helper
path. It requires `windows-job` ownership and an explicit positive integer
`RBIT_BUILD_CANDIDATE_HEADROOM_BYTES`. The helper checks actual free bytes against
that operator-selected threshold before allocation. This is not reserved storage,
a proven candidate-size estimate, or an automatic cold-cost/headroom approval.
There is no default threshold or automatic activation.

Within the live lease it reserves a singleton pointer with exclusive creation, then
creates one fresh `managed/candidate-UUID` directory in the compatibility namespace.
Cargo target and intermediate paths both point inside that directory. The legacy
`shared` lane is retained and never adopted as owned generation output. Existing
candidate, orphan, malformed, failed, uncertain or successful measurement state
refuses another candidate; no JSON eligibility boolean authorizes reuse or cleanup.
Exclusive pointer reservation also prevents two callers sharing one lease from
creating two directories. Interrupted allocation leaves blocking evidence.

The actual helper executes the selected command in that fresh lane, then publishes
its exact independently copied executable handoff through the existing generation
mechanism. Native completion and disk-derived graph/input checks precede successful
measurement records. Per-generation and singleton records contain fixed statuses,
hashes, counters, command wall time and boolean observations; no raw arguments,
selectors, error text, environment values or local paths are retained. The two
records are not an atomic transaction: partial publication stays blocking and is
not promotion authority. The command wall interval includes native-launcher startup
and excludes later byte scans/handoff validation; peak memory is not measured.

Fresh Cargo output directories do **not** establish a cold compiler, registry or
operating-system cache. The record explicitly says compiler-cache state unproved,
OS/page-cache state unmeasured and `coldCostGateEligible:false`. A detected wrapper
is recorded only as a boolean; Cargo-config/custom-toolchain caches may still exist.
No environment is silently changed, and no claim of fully cold compilation follows.
A later executor must not treat this fresh-output observation as approved fully cold
cost evidence without a reviewed scope and measurement policy.

A failed command with branded same-lease native completion is marked failed while
retaining its candidate. Missing or uncertain completion retains uncertain evidence
and the blocked lease. Preparation failure records that execution was not attempted;
it does not invent a stopped-tree proof. Quarantine errors cannot replace the
original uncertainty. Potentially active failed output is not scanned or hashed.
No candidate is automatically promoted, retired, retried or recursively deleted.

This measurement mode skips automatic cache pruning for its command. Independently,
normal automatic maintenance and manual prune now protect **every namespace with
managed state present**, without reading an eligibility boolean. Ordinary legacy-only
namespace eviction is unchanged. This prevents a later command in another namespace
from erasing measurement/quarantine evidence. Retained managed bytes can exceed the
soft watermark; this slice does not implement a hard storage budget.

Temporary-fixture tests exercise actual helper target selection/handoff, completed
compiler failure and lease release, uncertain failure plus diagnostic failure,
singleton races, opaque-handle/proof forgery, orphan refusal, cross-namespace/manual
prune protection, headroom validation and cache-confidence/privacy fields. No real
candidate, native job or Cargo invocation was executed during source preparation.
Actual measurement acceptance, approved cold/headroom evidence, bounded promotion
and the separately authorized retention executor remain required.


Generation record replacement retries transient Windows sharing denials at most
three times, rechecking the same lease, path and existing record identity each time.
It never removes the old record to force replacement. Deterministic IO fixtures
cover transient/persistent refusal and an unrelated record appearing between retries.
Diagnostic attachment is best-effort for frozen errors: failure to record or attach
a secondary error must not replace the original uncertain outcome or release its lease.
