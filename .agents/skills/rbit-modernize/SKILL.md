---
name: rbit-modernize
description: Assess an older rbit pull request against its current base and update it when requested. Use for stale PRs, suspected supersession, or a requested rebase.
---

# Modernize an rbit pull request

1. Resolve the instance, repository and PR. Read the original goal from the PR, linked issues, commit series, full diff and relevant review history. Record the actual base and head branches and their commit IDs. Establish the head repository before fetching a fork.
2. Fetch the current base and head. Use Git's merge base, or `rbit api 'repos/{owner}/{repo}/agent/compare' -X GET -f base=BASE -f head=HEAD`, to identify where they diverged. Read the changes since then on both sides. The PR manifest at `pulls/N/manifest` helps narrow the changed paths under the repository API. Use `rbit issue search WORDS --state all` and read the relevant code and tests to find replacement work. Commit ancestry is useful evidence, but equivalent behavior can arrive in a squash or a different implementation.
3. Recommend **Update** if the goal and implementation still fit, **Rework** if the goal remains but the implementation needs to change, or **Close as superseded** if current behavior already covers it or the goal was deliberately dropped. Cite concrete changes and any evidence gaps. When asked for an assessment, finish here.
4. For an authorized update, isolate work at the observed PR head. Before changing the remote, reread the PR and confirm that its base and head still match the assessment. When a same-repository merge update is intended, `rbit pr update-branch N --head SHA` guards the head and uses the server's observed base. Fetch the returned head, verify which base it incorporated, and test that exact result before calling it verified. If the update must use a particular base, prepare and test it locally. For a rebase, compare the old and new series with `git range-diff` and the final diff. Resolve conflicts according to current behavior and rerun focused checks. Push a rebased branch using `--force-with-lease=refs/heads/BRANCH:OBSERVED_SHA` to the verified head remote and branch. If the lease fails or the base has moved, reread the PR and reassess.

Treat rework, closing, retargeting, commenting and merging as separate actions that require the user to request them. Report the disposition, relevant base changes, observed revisions, checks and PR link.
