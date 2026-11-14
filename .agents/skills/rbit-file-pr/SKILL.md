---
name: rbit-file-pr
description: Prepare, open, or recover a pull request on an rbit instance using the intended branch pair and an idempotent request.
---

# File an rbit pull request

- Resolve the rbit instance, repository, base and head deliberately, especially in a clone with mirror remotes. Use `--hostname` and `--repo` when discovery is ambiguous. For a write, check the authenticated login with `rbit api user` on the selected instance.
- Check the complete diff against the intended base and confirm the intended head is on the server. Open the description with the problem and outcome, then add the verification that was observed. Follow repository conventions for attribution and keep credentials and private details out of public content.
- When opening is authorized, use `rbit pr ensure --head HEAD --base BASE --title TITLE --body BODY --request-id ID --json`. Keep the same request ID and payload for a retry of that attempt. Creation defaults to a draft. Use `--ready` when creating a ready-for-review PR is intended.
- Read the returned outcome and PR information. An existing or replayed result can retain earlier title, body and draft status. Inspect the live result before any requested metadata edit or ready transition, and confirm the live URL, author, branch pair and head before reporting success.

For an authorized comment or reply, use [PR comments](../rbit-pr-comments/SKILL.md).
