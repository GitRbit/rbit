---
name: rbit-full-stop
description: Prepare and verify an rbit pull request for the user to merge, including an explicit full-stop handoff request.
---

# Prepare an rbit PR for handoff

Before changing a branch or marking a PR ready, resolve any existing PR, its actual base and live queue state. An active entry can land while work is in progress. If removing that entry is authorized, use `rbit pr dequeue N` and confirm the PR is still open and the entry is inactive. Otherwise surface the active entry before making branch or readiness changes.

Use [file PR](../rbit-file-pr/SKILL.md) and [babysit PR](../rbit-babysit-pr/SKILL.md) to finish and verify the change within the requested scope.

For an authorized draft transition, use `rbit pr ready N --head SHA --mark --json`. For the final verification use `rbit pr ready N --head SHA --json`, the live queue, and the PR's review inbox. Review any unresolved thread that could affect the handoff. Record proof against the final head and base, and distinguish ready, awaiting approval and blocked outcomes.

Leave an open PR for the user to merge. Report its link, head, proof and outstanding decisions, or the observed final state if another actor closed or merged it.
