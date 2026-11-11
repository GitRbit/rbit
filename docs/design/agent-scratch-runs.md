# Agent loop: CI on a commit with no branch

Status: built (first part). Work packages 3.1, 3.2 and 3.3. Testing a change meant making a
branch for it, and every run tested every job.

## As built

- Namespace `refs/scratch/<owner>/<name>` (`rbit_git::scratch`). The push check refuses
  it to anyone but the person `<owner>` names (a deploy key included) and a new one past
  `SCRATCH_REFS_PER_OWNER` (20) per person and repository; deleting one's own is always
  allowed. Write access is required as for any ref. Branch protection does not apply.
- Migration `0078_actions_scratch_refs.sql`: each scratch ref the push check allowed, with
  its owner, commit and time (a deletion forgets it), whether or not Actions is on. The
  Actions sweep deletes a ref `SCRATCH_LIFETIME_SECONDS` (a day) after its
  push with a compare-and-swap `update-ref -d` (a ref that moved is left alone) and drops
  the record. The count is `expired_scratch` in the sweep's result.
- Triggers: a push of a scratch ref is a push of the branch `scratch/<owner>/<name>` for
  `on: push` and its `branches` filters. Runs carry the real ref and are found by commit.
- Subsets: a scratch commit whose message has an `Rbit-CI-Jobs: a, b` trailer runs only
  those jobs and every job they need; a workflow with none of them starts no run. The
  trailer means nothing on any other ref, so a branch head never gets a partial run.
- `rbit ci run [--jobs] [--commit] [--no-watch] [--timeout]`: builds a commit of the
  working tree through a temporary index under the Git directory (`HEAD`, the index and
  the files are untouched), pushes it with Git's own credentials, then watches by commit
  (`agent/watch`). `--json --no-watch` prints `rbit.ci-run/1` with `sha`, `ref`, `jobs`.

## Readiness

A required check is a job's own context. A subset run posts contexts only for the jobs it
ran, each a real, complete run of that job with its `needs`, so a missing job stays missing
and a subset never reads as a full pass. Scratch commits are never branch heads unless
someone pushes the same commit to a branch, where its contexts are exactly as true.

## Tests

Storage (`actions_scratch.rs`): recording, the quota count, lifetime restarts on a newer
push, expiry and compare-and-forget. Unit: owner parsing, the trailer, the CLI's commit message, scratch refs as branches in
the workflow and Actions trigger filters. End to end (`agent_scratch.rs`): a scratch commit
with `Rbit-CI-Jobs: tests` runs `build` and `tests`, not `lint`; the branch list shows
only `main`; a push under another login is refused.

## Left out

Subsets in `workflow_dispatch` (3.3's dispatch part), lifetime and quota as configuration
settings, and a scratch list in the API. Expiry is not tested end to end.
