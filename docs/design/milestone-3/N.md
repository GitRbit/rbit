# N — Email notifications: as built

Workstream N of [milestone 3](../milestone-3.md). Migration `0034` only.

## As built

- **What is mailed.** The in-app fan-out (`NotificationService::handle_event`) is unchanged
  in who it notifies. After writing the inbox rows it hands the recipients (with the reason
  the inbox recorded) to `NotificationEmailService::deliver`. Messages exist for: issue / pull
  request opened (mentions and assignees of the opening post), mentions added by editing a
  description or comment, assignment, review requested, review submitted (approved /
  changes requested / reviewed), comments, closed (completed / not planned), merged, reopened.
- **Who receives it.** Verified addresses only; never the actor; suspended users never;
  read access to the repository is re-checked at send time from the current repository row
  (with grants); the recipient's preference must want the reason; one email per
  (recipient, thread, activity) — the dedup key is claimed in `notification_emails` before
  sending; then the per-address `ThrottleScope::NotificationEmail` budget (30, then one every
  two minutes); delivery through the shared `MailService` queue. Nothing is sent (and nothing
  claimed) with `[mail] transport = "disabled"`, `[notifications] email = false` or
  `[issues] notifications = false`.
- **Preferences** (`notification_email_settings`): `off`, `participating` (default),
  `all`. `participating` = every inbox reason except `manual` (threads only subscribed to by
  hand); `all` adds those. Users who never chose get `[notifications] default_email`.
  Settings → Notifications (`/settings/notifications`) shows the choice, the address, and
  explains when the address is unverified or the server has no mail.
- **Messages** (`rbit-mail` `ThreadNotification`, `templates/thread_notification.{txt,html}`):
  subject `[owner/repo] Title (#N)` (`Re: ` for every message but the opening one), one
  headline sentence, an excerpt (≤ 1000 characters of the Markdown source), a link to the
  thread anchored at the comment/review, "You are receiving this because …", links to the
  notification settings and to stop notification emails. The mail layout gained a
  `{% block footer %}` so this message can replace the generic footer.
- **Headers.** `Mail` gained `headers: MailHeaders` (`message_id`, `in_reply_to`,
  `one_click_unsubscribe`), written by the SMTP and log transports. Thread root id
  `<owner/repo/issues/N@host>` (`pulls` for pull requests; `host` is the public URL's host
  without port). The opening message uses it as its `Message-ID`; the others get
  `<owner/repo/issues/N/{dedup key}@host>` plus `In-Reply-To` and `References` = root.
  `List-Unsubscribe: <…/email/unsubscribe/{token}>` and
  `List-Unsubscribe-Post: List-Unsubscribe=One-Click` (RFC 8058). No `Reply-To`.
- **Unsubscribe token.** `{account id}.{hex HMAC-SHA256("rbit/notification-unsubscribe/v1\0{id}")}`
  keyed by a 256-bit server secret generated on first use and stored in the new
  `server_secrets` table (raw, like webhook secrets — HMAC needs the key; the store is
  generic so later workstreams can keep their own keys there). Verified in constant time.
  `GET /email/unsubscribe/{token}` shows a confirmation (link scanners must not
  unsubscribe); `POST` sets the preference to `off` and shows a confirmation (200), also
  for mail clients posting `List-Unsubscribe=One-Click`. Both are sent `no-store` and
  `no-referrer`; invalid tokens get a 404 page. The POST is exempt from the cross-site check
  — only that exact path shape (`middleware::is_one_click_unsubscribe`).
- **Pruning.** Outbox rows older than `notification_retention_days` are pruned with the
  inbox's daily pruning.
- **Settings layout fix.** `templates/settings/layout.html`: the sidebar `<aside>` got
  `min-w-0`; without it the scrolling tab row forced every settings page wider than a
  375 px screen (it already did with six tabs; the seventh made it worse).

### Files

- core: `config/notifications.rs` (new; `EmailPreference`, `NotificationConfig`), one field
  at the end of `Config`.
- storage: `migrations/0034_notification_email.sql`, `sqlite/notification_email.rs`,
  `sqlite/server_secrets.rs`, accessors `Database::notification_email()` and
  `Database::server_secrets()`; tests `tests/notification_email.rs`, `tests/upgrade.rs`
  (`notification_email_tables_start_empty`).
- mail: `Mail.headers` / `MailHeaders`, `ThreadNotification`, two templates, layout footer
  block; the transports write the headers (the log transport prints them).
- services: `notification_email.rs` (new), `notifications.rs` (hand-off after the inbox
  write, prune, `with_email`), `throttle.rs` (`NotificationEmail` scope), `lib.rs`
  (`notification_email` field and construction), `mail.rs` (test literal).
- web: `routes/notification_email.rs`, `views/notification_email.rs`,
  `templates/settings/notifications.html`, `templates/email/unsubscribe.html`,
  `views/settings.rs` (nav entry appended), `middleware.rs` (exemption),
  `routes/mod.rs`, `views/mod.rs`, `templates/settings/layout.html` (`min-w-0`).
- e2e: `src/lib.rs` `MailLog` (a process-wide `tracing` layer that keeps the log
  transport's messages; filter by recipient and server URL), `Cargo.toml` (`tracing`,
  `tracing-subscriber`), `tests/e2e/notification_email.rs`.

## Deviations

- **Per-repository mute** (optional in the contract) was not built; the per-thread
  unsubscribe of the inbox already stops mail for a thread.
- **Dedup keys for repeatable actions** (assigned, review requested, closed, merged,
  reopened) include the second the event was handled (`closed-1700000000`), because events
  carry no id and these actions can legitimately happen again. Replaying the same event
  within that second still mails once; comments (`comment-{id}`), reviews (`review-{id}`)
  and the opening post (`opened`) are deduplicated forever (until pruned).
- **Rate limit.** A dedicated `ThrottleScope::NotificationEmail` per recipient address
  instead of reusing `EmailRequest`, whose budget (3 then one per 10 minutes) is meant for
  password resets — sharing it would let a busy thread block a user's password reset.
  Throttled emails are skipped (logged at `info`), the inbox keeps the notification.
- **Thread ids use owner/repo**, as the contract says; renaming or transferring a repository
  starts new threads in mail clients.
- **Configuration:** a new `[notifications]` section rather than extending `[issues]`.

## CHANGELOG entry

```
### Added
- Email notifications: mentions, assignments, review requests, reviews, comments and
  state changes on threads you take part in are also emailed to verified addresses
  (Settings → Notifications: off / participating / everything). Messages thread per issue
  (`Message-ID` / `In-Reply-To`), say why you got them, and carry a one-click
  unsubscribe link (RFC 8058 `List-Unsubscribe-Post`). Rate limited per address, never
  sent to the actor or to anyone who can no longer read the repository.
- `[notifications]` configuration section (`email`, `default_email`).
```

## README

Feature line: `- **Email notifications** — the inbox, by email if you want it: threaded per
issue, with one-click unsubscribe.`

Configuration table row:

| `[notifications]` | `email` (true), `default_email` (`participating`; `off`, `participating`, `all`) | Email notifications on top of the in-app inbox; need a `[mail]` transport and a verified address |

## rbit.example.toml

```toml
# Email notifications, on top of the in-app inbox ([issues] notifications). They need a
# [mail] transport; only verified addresses receive them. Users choose in
# Settings -> Notifications; every email has a one-click unsubscribe link.
[notifications]
email = true
# What users who never chose get: "off", "participating" or "all".
default_email = "participating"
```

## ARCHITECTURE section text

> ### Email notifications (milestone 3)
>
> `NotificationEmailService` (`rbit-services/src/notification_email.rs`) mails what the
> inbox fan-out writes: `NotificationService::handle_event` passes the recipients and
> reasons it stored to `deliver`, which keeps verified, non-suspended recipients other than
> the actor whose preference (`notification_email_settings`, default
> `[notifications] default_email`) wants the reason (`participating` = all but manual
> subscriptions), re-checks read access with grants, claims a dedup key in
> `notification_emails` (`UNIQUE(account, issue, key)`: `opened`, `comment-{id}`,
> `review-{id}`, or `{action}-{unix second}`), spends the recipient's
> `ThrottleScope::NotificationEmail` budget and queues the message on `MailService`. Mail
> failures never fail the inbox fan-out.
>
> Messages thread on `<owner/repo/{issues|pulls}/N@host>`: the opening message carries it
> as `Message-ID`, later ones as `In-Reply-To`/`References`. Each carries `List-Unsubscribe`
> + `List-Unsubscribe-Post: List-Unsubscribe=One-Click` to `/email/unsubscribe/{id}.{mac}`,
> an HMAC-SHA256 of the account id under a key kept in `server_secrets` (generated on first
> use). `GET` confirms, `POST` sets the preference to `off`; the `POST` is the single path
> exempt from the cross-site check, as mail clients send it from their own origin.
>
> Schema (`0034`): `notification_email_settings(account_id PK → accounts CASCADE, email
> CHECK IN ('off','participating','all'), updated_at)`; `notification_emails(id,
> account_id → accounts CASCADE, issue_id → issues CASCADE, dedup_key, created_at,
> UNIQUE(account_id, issue_id, dedup_key))` + index on `created_at` (pruned with the
> inbox retention); `server_secrets(name PK, secret BLOB, created_at)`.
>
> Routes: `GET|POST /settings/notifications`, `GET|POST /email/unsubscribe/{token}`.

## Integration requests

- `docs/ARCHITECTURE.md`, `README.md`, `CHANGELOG.md`, `deploy/rbit.example.toml`: the texts
  above. `docs/deployment.md`: mention that notification emails need a mail transport and
  verified addresses, and that `List-Unsubscribe` links use `[server] public_url`.
- Security model section: add `server_secrets` next to webhook secrets as the second kind of
  secret stored raw, and the unsubscribe path as the one CSRF exemption.
- `RecoveryService::handle` matches `DomainEvent` exhaustively; nothing to do for N (no new
  variants), noted for workstreams that add some.
- If another workstream also appends a settings section, the nav test
  `settings_nav_marks_active_section` and `SettingsSection::ALL` need both entries.
