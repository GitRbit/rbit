# Native agent pull request workflows

## Implementation plan

1. Expose a read-only PR watch endpoint and CLI command. Reuse readiness,
   review, and merge queue services. Include the observed head and base so
   clients can detect when their evidence is stale. Return a cursor for bounded
   waits and recheck requester access during a wait.
2. Add read-only CLI workflows for modernization and takeover. Combine existing
   PR state, revisions, and review evidence. Add divergence and changed-file
   evidence for modernization. Recheck revisions when gathering evidence across
   multiple requests.
3. Add `rbit pr babysit` and its `full-stop` alias over the PR watcher. Wait for
   handoff readiness, actionable changes, or a terminal condition. Report
   verification barriers, including feedback outside the server merge rules.
4. Add `rbit pr full-send` for an explicitly authorized head. Validate the
   observed head and review state before entering the existing merge queue.
   Follow the queue to its result. Existing merge protection and candidate
   checks govern landing.
5. Document the native workflows and verify the normal CLI and API entry
   points. Cover access checks, cursor behavior, handoff barriers, and stale
   revisions.

The forge provides deterministic evidence and executes authorized Git and merge
operations. The agent or person decides whether a change fits, whether review
feedback is correct, and which additional tests establish intended behavior.
Independent review can use the same evidence through delegation supplied by
the agent host. A host scheduler is required to resume an agent after it exits.

## Verification scenarios

- Watch a PR through pending and passing checks, then submit a review while
  checks remain green. Confirm cursor changes for the review and a later head,
  and that an unchanged cursor can time out. Check permissions on a private PR.
- Test modernization evidence for a needed update, a branch working on a
  subsystem since replaced, and a change that already landed independently.
  Assess relevance against current behavior and compare source refs before and
  after.
- Follow a PR through handoff and verify that unresolved threads and a changed
  head produce actionable results. Confirm that the handoff leaves the PR open.
- Exercise authorized landing through the existing queue. Refuse a stale head
  or unresolved thread before queueing, and verify the commit that lands was
  tested against the current base.
- For an isolated rebase, compare the old and new series and verify that a
  concurrent push causes the explicit expected-head lease to fail.

Inspired by the [tstack workflows](https://github.com/t3dotgg/tstack).
