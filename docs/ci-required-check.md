# Required CI check

`CI required` is the stable aggregate job in `.github/workflows/ci.yml`. It runs
after every existing validation job, including every Linux and Windows test job
(build, shards, doc tests and the `Tests (<os>)` aggregates; see
[CI test shards](ci-test-shards.md)) and the runner-image build/start/tool smoke.
Its summary shows the tested revision, run/attempt link, required-check results,
platforms and artifact names. Individual jobs retain detailed logs, platform results and existing cache behavior.

Every dependency must return `success`. A failure, cancellation, skip, missing
entry or unknown result blocks the aggregate. For example, if the asset build
fails and release compilation is consequently skipped, the summary preserves
both states and the required job fails. A skipped matrix/job is not interpreted
as evidence that its checks passed. Matrix rows report GitHub's combined result;
they do not fabricate per-platform results or unreported timing/cache metrics.

The job uses `if: always()` and lists every validation as a direct dependency.
This avoids GitHub's default skip propagation hiding the aggregate after an
upstream failure. Whole-workflow cancellation or infrastructure failure may
still prevent completion; this does not create a passing required check.
The evaluator itself fails if summary publication fails or metadata is malformed.

The aggregate has only `contents: read`, pinned external Actions and a checkout
without persisted credentials. It calls no API and receives no publication,
signing or repository-write token. Dependency JSON is passed through an
environment variable rather than interpolated into shell source. Only fixed
check labels/results and validated runner identities enter summaries/annotations;
PR titles, branch names, dependency outputs and arbitrary payloads are discarded.
PR execution remains separate from the release publication workflow.

The workflow publishes **CI required**, but the repository audit currently expects
the six individual checks listed in [repository configuration](repository-configuration.md).
Before replacing those live required checks, verify the aggregate on a PR and main,
including one intentionally failing dependency in a temporary PR. Then update the
branch-protection rule and `scripts/audit-repository.mjs` together. Do not reuse this
name for unrelated checks. Merge queues need an explicitly reviewed `merge_group`
trigger before enabling a rule that requires this check there.

When adding, removing or making a validation conditional, update the workflow's
direct `needs` list, `scripts/ci/required-results.mjs` manifest and its contract
test together. All current validations are mandatory; this gate has no optional
skip allowlist. Do not add `continue-on-error` to required jobs: that can conceal
failed matrix evidence. The contract test rejects uncovered jobs and relaxed
error handling while preserving the current runner-image smoke commands.

Run the focused checks locally without compiling Rust:

```sh
node --test scripts/tests/rewrite-ci-summary.test.mjs
```

The aggregate summarizes validation results; it does not change cache keys,
compilation parallelism, dependency selection, release authorization, platform
coverage or publication behavior. The branch-protection rollout is a separate
repository setting change.

GitHub documents [dependency skip propagation and always()](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-jobs)
and the [needs context](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#needs-context).
