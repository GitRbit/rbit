---
name: rbit-full-send
description: Complete and land an rbit pull request through its server-side merge queue when the user explicitly authorizes merging it.
---

# Land an rbit pull request

1. Finish the requested change and verify its behavior. Use [file PR](../rbit-file-pr/SKILL.md) to file or recover the PR and [babysit PR](../rbit-babysit-pr/SKILL.md) to address relevant checks and review feedback. Use [independent review](../rbit-review/SKILL.md) when requested or required by the project.
2. Verify the live repository, PR, head, base, review inbox and readiness. Make sure the tested change matches the revision to be queued. For an explicitly authorized merge, run `rbit pr queue N --head SHA --watch --json` with the repository's intended merge method. Follow the server's protection decisions and required checks.
3. If the queue fails or removes the entry, inspect the recorded reason and decide whether another attempt is in scope. A timeout can leave an active entry. Read `rbit api 'repos/{owner}/{repo}/pulls/N/queue'` and the live PR before acting again.
4. Verify the merged PR and merge commit on the server. Report the PR link, merge commit, and observed proof or the remaining blocker.
