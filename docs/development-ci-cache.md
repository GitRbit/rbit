# CI dependency-cache boundaries

CI permits dependency-cache saves for `pull_request` merge refs and for main-branch
`push` or `workflow_dispatch` runs. Other contexts do not satisfy the save expression.
No write permission, custom token, cache-mode override or privileged follow-up is
added. Required job names, platform checks and runner smoke commands remain unchanged.

Every CI checkout disables credential persistence. Later compiler, test, Git and
container commands do not require authenticated Git operations against the source
repository. This uses the pinned checkout action's documented
[`persist-credentials` option](https://github.com/actions/checkout/blob/3d3c42e5aac5ba805825da76410c181273ba90b1/README.md).

## Scope and trust

GitHub stores PR caches under their merge ref. Only that PR can restore them; main
and other PRs cannot. Fork PRs may restore base-branch caches, so cached files must
never contain credentials. These backend ref restrictions provide the security
boundary; cache-key strings are not authorization. GitHub also restricts default-
branch cache writes from low-trust follow-up triggers. We do not override that
restriction. See [GitHub's cache access rules](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache).

The existing tag-publication workflow does not restore Rust compiler caches. There
is no `workflow_run` consumer of these archives. Adding a privileged cache consumer
requires a fresh trust review; an archive is neither signed source provenance nor
a trusted executable handoff. The developer shared-lane provenance guard and its
native-completion requirements are separate and unchanged.

## Pinned action behavior

The action remains pinned to
[`d1527c8f67ef9af357290475d4af4b0fab8f12e0`](https://github.com/Swatinem/rust-cache/tree/d1527c8f67ef9af357290475d4af4b0fab8f12e0).
All six cache steps explicitly disable workspace-crate, all-crate,
Cargo-bin and failure caching. The configured intermediate directory still matches
the repository's Cargo build directory; final executables, runtime copies, private
data and the developer shared cache are not added to cache paths.

The pinned [save implementation](https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/src/save.ts)
skips publication after an exact-key hit. Otherwise it obtains packages outside the
workspace root and cleans target contents before saving; workspace members are
included only when explicitly enabled. Cleanup is best effort, not an archive
integrity guarantee. The [target pruning code](https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/src/cleanup.ts)
retains dependency fingerprints/build outputs and removes other target entries.
Dependency-only intent does not make a PR-controlled archive safe for privileged use.

Existing [key construction](https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/src/config.ts)
continues to distinguish jobs, OS/architecture, Rust environment and manifest/lock/
configuration inputs. Release checks keep their additional release key. No commit
SHA or run number is added, avoiding a new archive for every small source edit.
Disabling Cargo-bin caching changes archive paths and may cause an initial miss
against earlier archives even when textual keys match.

## Cost and validation

There are six job/platform combinations with Rust caches: Linux lint, the Linux and
Windows test builds, the Linux and Windows doc-test jobs, and Linux release checks. Test
shards compile nothing and restore no cache; they run the test build's nextest archive
(see [CI test shards](ci-test-shards.md)). PR-local saves can add entries
for each combination and dependency/toolchain change. GitHub expires entries not
accessed for seven days and evicts least-recently-used entries when repository
storage is exhausted. Increased quotas can incur charges; this change does not
raise the repository quota or configure automatic deletion. See
[GitHub's storage limits](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy).

The intended benefit is reusing PR-specific dependency work after a dependency-key
miss. It does not cache application code or eliminate its compilation. Actual hit
rates, archive sizes, restore/save time and total job time must be observed on
hosted runs before claiming a speed improvement or expanding cache coverage.

Node policy tests evaluate the actual YAML save expressions across main, PR,
feature, tag, malformed-ref and privileged-trigger contexts. Negative fixtures
reject workspace/tool/failure caching, shared-key changes and permission expansion.
Existing aggregate-check tests continue to cover all required jobs and runner smoke.
These are source-policy checks, not a hosted cache-service integration test.
