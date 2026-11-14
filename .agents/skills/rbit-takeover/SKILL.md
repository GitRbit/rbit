---
name: rbit-takeover
description: Prepare a handoff or continue an existing rbit issue, branch, pull request, or agent's work while preserving its goal, workspace changes, and contributor credit.
---

# Take over rbit work

1. Resolve the source issue, PR, branch or worktree. Read the original request, subsequent decisions and open feedback. Record the actual base and head, current CI on that head and unresolved review threads. For an existing rbit task worktree, use `rbit task resume BRANCH --json` to locate it and see whether it contains unpublished work. Inspect its state only within the user's scope.
2. For a handoff, include the goal, relevant refs and commit IDs, local work that needs preserving, proof tied to a commit, remaining findings and the next action. Share local workspace details only in the private handoff.
3. For continued implementation, verify reproduction and evidence yourself. Honor an explicitly requested in-place, replacement or stacked continuation. Otherwise use a separate worktree for your changes. Preserve existing commits and authorship when carrying code forward, retain verified co-author trailers, and link the source PR on a replacement. Preserve credit in any authorized squash.
4. If the base has evolved, apply [modernize](../rbit-modernize/SKILL.md) before deciding how to continue. Reread the source head before publishing a replacement or updating in place. Use [file PR](../rbit-file-pr/SKILL.md) and [babysit PR](../rbit-babysit-pr/SKILL.md) when those outcomes are requested.

Report what you inherited, what you changed, verification, and any remaining decision. Treat closing or merging the source PR as distinct actions.
