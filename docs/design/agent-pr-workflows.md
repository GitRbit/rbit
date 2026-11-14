# Agent pull request workflows

## Implementation plan

1. Add a read-only PR watcher to the REST API and CLI. Reuse the merge readiness and review services. Provide a cursor and a bounded wait for changes to the head, checks, review activity, threads, conflicts, or PR and queue state. Include the observed revisions so callers can invalidate stale evidence.
2. Package repository skills for filing and monitoring PRs, preparing a human handoff, landing an explicitly authorized PR, modernizing an older PR, and taking over existing work. Include shared comment guidance and a reviewer workflow that uses delegation when the host provides it.
3. Document setup and run focused verification against the normal CLI and API. Verify that watching a PR surfaces changes beyond CI, that a new push invalidates old evidence, and that an agent can assess an old PR before touching its branch.

The forge reports repository state and enforces existing rules. The agent decides whether a change still fits the project, what review feedback warrants action, and which tests establish the intended behavior. The person directing the work controls publishing, review replies, and landing. A bounded watch supports a running session. Resuming a stopped session requires an agent host that can schedule a wake-up.

## Workflow outcomes

- Filing reuses an existing PR on the same branches and can retry creation after a lost response.
- Monitoring considers review activity and unresolved threads even after CI is green. An update to the PR starts a new assessment of the evidence.
- A human handoff leaves the PR available to merge. An authorized landing uses the existing merge queue and reports its result.
- Modernization can recommend updating, reworking, or closing a superseded PR. An authorized update uses the actual base, an isolated checkout, focused validation, and an expected remote head.
- Takeover preserves the goal, contributor credit, and workspace changes. An independent review applies to a specific revision and records evidence and unresolved findings.

## Verification scenarios

- Watch a PR through pending and passing checks, a review with CI still green,
  and a new head. Verify cursor reuse, an unchanged timeout, and permission
  checks for a private repository.
- Assess three older PRs against their actual bases. One still fits and needs
  a mechanical update. One needs a new approach because the base changed the
  owning subsystem. One can close because the base already solves the issue.
  A read-only assessment leaves all three PRs and branches intact.
- For an explicitly authorized update, check the old and rewritten commit
  series, run focused tests, and verify that a concurrent push causes the
  expected-head update to fail.

Inspired by the [tstack agent skills](https://github.com/t3dotgg/tstack).
