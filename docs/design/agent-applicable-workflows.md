# Agent loop: which workflows apply to a commit

Status: built (first part). Work package 2.2. A workflow skipped by its `paths` filter
posted nothing, so a required check for it stayed pending forever and "nothing yet" could
not be told from "nothing expected".

## As built

- Migration `0077_actions_commit_decisions.sql`: per repository and pushed commit, the
  number of workflow files, the number that started a run, and why none did:
  `skip_marker`, `no_workflows`, `filtered`, or `started`. Recorded by the push trigger for
  every considered ref; a later evaluation of the same commit that started more wins.
  Migration `0083` adds `too_many_refs` (a ref past a push's cap of 16) and `0084` adds
  `no_jobs`: a scratch commit whose `Rbit-CI-Jobs` trailer names no job of its workflows.
- `rbit status` / `agent/watch`: a commit without runs reads `none_expected` with the
  recorded reason (filters, skip marker, no workflow files, no job named by the trailer) instead of `not_started`.
- Merge readiness (the merge box, and so the merge): a required context of the rbit
  Actions shape `<workflow> / <job> (<event>)` is dropped for a head commit whose decision
  says no workflow started because of filters or a skip marker. Other contexts stay
  required.

## Tests

End to end (`agent_paths.rs`): `paths-ignore: [docs/**]`, a required `CI / test (push)` on
`main`; a docs-only branch reads `none_expected` with the filter reason and its pull request
merges; a code change's pull request is `blocked`.

## Left out

Per-workflow decisions (one workflow filtered while another runs) and pull request event
decisions; the readiness states `expected`, `running`, `failed` per required check in the
API come with `pr ready` (4.2).
