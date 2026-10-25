# Personal issue and pull request dashboards

Open **Issues** or **Pull requests** from your home dashboard or account menu. Sign in to
see threads you created, threads assigned to you, or threads that mention you. Pull requests
also have a **Review requested** tab, which lists current review requests.

The search uses the same syntax as repository issue lists: `is:open`, `is:closed`,
`author:@me`, `assignee:name`, `label:bug`, `-label:duplicate`, `milestone:"v1.0"`,
`no:assignee`, and `sort:updated-desc`. Plain text searches titles and descriptions.
The page type fixes the thread kind; involvement tabs stay selected when you search or
change pages. Results are globally ordered, with 25 threads per page and open/closed counts.

Only repositories you can currently read contribute results or counts. Losing collaborator
or organization access removes private threads even if you previously created, reviewed,
or were mentioned in them. Suspended accounts cannot retrieve a dashboard.

Once indexed, mention membership persists after inbox notifications are read or pruned, subscription
reasons change, or a mention is later edited out. It includes Markdown mentions of existing
users in issue/PR bodies, comments and submitted review bodies/comments. Code spans and
code blocks do not create mentions. Pending reviews are excluded. New activity is indexed
by the existing issue event subscriber, independently of notification preferences. Updates
are eventual. As with other event subscribers, an event-bus overflow can skip activity;
the one-time upgrade backfill does not provide ongoing replay or lossless history.

Upgrade applies migration `0043_dashboard_mentions.sql`. First startup scans existing
bodies and comments in batches of 100 and resolves mentions using the Markdown scanner;
startup may take longer on large databases. This is restart-safe and idempotent. The completion
marker avoids subsequent full scans. No configuration changes are needed.
