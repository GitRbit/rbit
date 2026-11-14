---
name: rbit-review
description: Arrange a bounded independent review of an rbit change when requested, including a named reviewer or an opt-in multi-provider council.
---

# Review an rbit change independently

- Resolve the exact revision under review. For a PR, use the base and head commits, the manifest at `pulls/N/manifest` and path or hunk detail under the repository API. Supply relevant proof and intent. For a local change, use an isolated snapshot that preserves the intended diff and accounts for untracked files. Use providers allowed for the repository and scope shared context to the change.
- Start one read-only reviewer with the developer's acceptance criteria, a fixed revision, and the relevant checks. If the user asks for a council, use independent reviewers from the authorized providers supported by the host. Honor named reviewers and report missing capabilities. Reviewers report findings for the parent to verify, with location, severity, and a concrete reproduction or reasoning.
- Verify each finding against the code and record its disposition. Rerun focused checks after fixes. Use up to three rounds by default, focusing later rounds on changed code and open findings. A new material diff requires fresh evidence. If a reviewer is unavailable or the limit is reached with unresolved findings, report the remaining uncertainty.
- Report the revision, observed reviewers, confirmed findings, and checks. Provider feedback is advisory unless the repository explicitly incorporates it into its review policy. Use [PR comments](../rbit-pr-comments/SKILL.md) if publication is authorized.
