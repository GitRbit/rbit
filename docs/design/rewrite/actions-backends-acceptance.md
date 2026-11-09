# Actions backends acceptance

Acceptance evidence for the `actions.backends` ledger row ("Container/host execution,
limits, service install and process cleanup"), owned by `rbit-runner`. It follows the
[identity retirement](identity-retirement.md) pattern. The same slice moved the shared
part of `actions.artifacts-caches`; see [its record](actions-artifacts-caches-acceptance.md).

## What "retired" means for this row

The row's target and its recorded source are the same crate, and `rbit-runner` is a
binary that depends on no server crate. Execution was already decided there:

| Behaviour | Owner |
| --- | --- |
| Container jobs: network per task, mounts under `/__w`, `no-new-privileges`, `MKNOD` dropped, `--pids-limit`/`--memory`/`--cpus`, services, pause around built-ins, reclaim helper, janitor | `rbit-runner/src/backend/container.rs` |
| Host jobs (no isolation) and the backend interface | `rbit-runner/src/backend/{host,mod}.rs` |
| Child processes, kill-on-cancel and process-group cleanup | `rbit-runner/src/process.rs`, `shutdown.rs` |
| Label → backend/image configuration, `container.network` never `host`, limits | `rbit-runner/src/config.rs` |
| Service install (systemd/launchd/Windows service) | `rbit-runner/src/service.rs`, `winservice.rs` |
| `container.options` allow-list (no `--privileged`, `--cap-add`, `--network host`, host mounts) | `rbit-workflow/src/options.rs` (already shared) |

No server, web or storage code builds container commands or chooses a backend. What
was held outside the runner was one decision, made twice: which declared labels run
without isolation. `rbit-services` (`runners::Declared::new`) and `rbit-runner`
(`RunnerConfig::host_labels`) each compared `backend == Backend::Host`. The server
stores those labels apart (`host_labels`), shows them as "no isolation" and its claim
query never gives them an untrusted run's job. That decision now lives once in the
runner contract both sides build on: `rbit_workflow::protocol::Backend::isolates_jobs`.

Why `rbit-workflow` and not `rbit-runner`: the server cannot depend on the runner
binary, and the [rewrite plan](../ground-up-rewrite-plan.md) keeps runner/workflow shared
types free of server details ("Runner/workflow shared types do not depend on server
implementation details"). `rbit_workflow::protocol` is the runner's wire contract; the
plan's optional `rbit-runner-protocol` extraction would take it with it. No crate edge
is added.

Out of scope and unchanged: the claim query's host-label filter
(`rbit-storage/src/sqlite/actions_runs.rs`) belongs to `actions.scheduling` and the
fork-policy work in `rbit-actions`; the stored `RunnerBackend` mapping belongs to
`actions.runners`.

## Legacy code removed

- `label.backend == Backend::Host` filters in `rbit-services/src/actions/runners.rs` and
  `rbit-runner/src/config.rs`; both call `isolates_jobs`.

## Boundary that fails if legacy paths return

`scripts/rewrite/actions-runner-retirement.mjs`, run by `scripts/rewrite/check.mjs`, scans
production `src` files of every crate except `rbit-workflow` (comments and closing
`#[cfg(test)]` modules stripped). For this row it rejects:

- a `filter(|…| … backend == Backend::Host …)` host-label classification anywhere;
- container isolation arguments (`"--privileged"`, `"--security-opt"`, `"--cap-drop"`,
  `"--cap-add"`, `"--pids-limit"`, a `docker.sock` path) in `rbit-services`, `rbit-web`,
  `rbit-server`, `rbit-actions` or `rbit-storage`: only the runner's backend builds them.

The cache and artifact rules of the same checker are listed in the
[artifacts and caches record](actions-artifacts-caches-acceptance.md). Run against the
baseline `4868231e` it reports eleven violations (two for this row); against this tree,
none. `scripts/tests/actions-runner-retirement.test.mjs` pins both.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | `isolates_jobs` is `matches!(self, Container)`, the exact negation of the replaced comparison for the closed two-value enum. Stored `host_labels`, the warnings at register/declare and the runner's `check` message are produced from the same labels in the same order. |
| Security | Container isolation, mounts, network and resource limits are untouched: no line of `backend/`, `process.rs`, `shutdown.rs`, `service.rs` or `options.rs` changed. Their argument-vector unit tests pass unchanged. |
| Migration and rollback | No schema, migration, stored format, protocol byte or route change. Latest migration remains 0073. Rollback is the previous binary. |
| Performance | One inlined `const fn` replaces a comparison; no release measurement claimed. |

## Results

See [Results](actions-artifacts-caches-acceptance.md#results): both rows were checked
together at the same commit.

## Cost

Shared with the artifacts and caches slice; see [its Cost section](actions-artifacts-caches-acceptance.md#cost).
For this row alone: 9 lines in `rbit-workflow` (method, documentation), two one-line
filter changes, a 2-assertion test and two checker rules.

Estimate: rows whose target is a runner-side crate and whose decisions are already there
(`actions.backends`) retire in part of a session, the work being to prove nothing leaked
and to fence it. The `rbit-runner` files themselves (`container.rs` 1,161 lines,
`steps/cache.rs` 929, `steps/action.rs` 1,306) remain over the 400-line limit under their
baseline ceilings; splitting them by responsibility is a separate structural slice of
roughly one session each, with the argument-vector tests as the parity gate.
