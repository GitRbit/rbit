---
name: rbit-pr-comments
description: Draft or post an authorized rbit PR comment, inline review reply, or explanation accompanying a review thread resolution.
---

# Write an rbit PR comment

Lead with the finding, fix, or decision. Cite the checked behavior with a path, test, or run and distinguish what you reproduced from what you infer. Preview the Markdown and check quoted text and attachments for private details. Use the repository's attribution policy and verify the publishing identity.

Use `rbit api` with a structured JSON body for a write. Under `repos/{owner}/{repo}`, post a timeline comment to `issues/N/comments` with `body`. Post an inline reply to `pulls/N/comments` with `body` and numeric `in_reply_to`. Check the returned comment ID and link. When resolution is also authorized, confirm the reply was posted successfully before putting `{"resolved":true}` to `pulls/N/comments/THREAD_ROOT_ID/resolution`.
