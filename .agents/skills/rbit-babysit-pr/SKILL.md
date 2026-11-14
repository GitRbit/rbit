---
name: rbit-babysit-pr
description: Monitor or troubleshoot an rbit pull request through CI and human or automated review, with evidence tied to its observed head.
---

# Babysit an rbit pull request

1. Start with `rbit pr watch N --json`. Save the returned cursor for subsequent calls as `rbit pr watch N --cursor CURSOR --timeout 25 --json`. One cursor stream is sufficient. `changed: false` means the wait expired without a detected change. Recheck the current state after reconnecting. A bounded call waits inside a running session. Use the host's wake-up mechanism if one is available and the user expects monitoring after the session stops.
2. Inspect the live PR state, head and base, readiness requirements, and review threads. On a new head or base, reassess proof and findings against the new revisions. Read review details through the PR's `reviews` and `comments` API endpoints as needed. Verify each finding against the code before changing it. A moved or ambiguous review anchor calls for checking the actual path and surrounding diff.
3. For CI failure, use `rbit run failure --sha SHA --json` and, when needed, `rbit run log JOB --step STEP --tail 100`. Identify whether the failure belongs to the current change, also occurs on the base, or was caused by runner infrastructure. Pending or missing evidence stays pending. Fix confirmed problems within the requested scope, validate, and watch the new head if an update is authorized.
4. Use the server's readiness requirements and the review inbox together. If `readiness_checks_protection` is false, confirm readiness using a credential authorized to evaluate the protected branch before calling the PR ready. Distinguish an unresolved thread, required approval, or failed check from an advisory finding. For an authorized reply or thread resolution, use [PR comments](../rbit-pr-comments/SKILL.md).

Stop when the requested condition holds, the PR closes or merges, or a decision requires the user. Report the PR, current head, proof and any actionable blocker. On an older server without the PR watcher, use `rbit pr ready N --head SHA --json`, `rbit run watch --sha SHA`, and `GET /pulls/N/inbox` under the repository API, checking review activity explicitly.
