# Shared build cache retention

Status: investigation; automatic graph rotation is deferred pending measurement.

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
