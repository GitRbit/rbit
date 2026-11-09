# Review-thread retirement acceptance

Acceptance evidence for the `collaboration.review-threads` ledger row ("Line comments,
resolvable conversations and review requests"), following the
[identity retirement](identity-retirement.md) pattern. The first half of the row moved
with the pull request opening slice; see the
[pull request progress](collaboration-pulls-progress.md#review-threads).

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Conversations: replies grouped under their root wherever submitted, roots, orphan replies dropped | `rbit_collaboration::pulls::review_threads::{group_threads, thread_root}` |
| Who may resolve or reopen (writers or the root's author), replies to pending reviews (their reviewer only), review cards, the review form, the dismiss action | `review_threads::{may_resolve, reply_visible, shows_review_card, review_form, may_dismiss_reviews}` |
| Line anchoring: the diff side (`ReviewSide`, moved from `rbit-core`, which re-exports it) and the up to four diff lines kept with a comment, after the hunk header | `rbit_collaboration::pulls::{ReviewSide, hunk_excerpt, DIFF_HUNK_LINES}` (new; the service adapts `rbit-git` hunks through `AnchorLine`) |
| Where a line comment goes: the commenter's pending review when they start one or already have one (the pending lookup only when they do not start one), else a comment-only review | `review_threads::joins_pending_review` (new) |
| Review requests: triagers or the author while they may comment; trimmed names, blanks skipped; unknown names, organisations and people who cannot read the repository refused with one answer, before any access lookup for organisations; the author refused; at most 15 requested; existing requests skipped; withdrawals name by name, unknown names skipped | `rbit_collaboration::pulls::{ReviewRequests, may_request_reviews, requested_names}` (new) over `ReviewRequestPort`; limit and refusal text in `review_threads` |
| Persistence of comments, pending reviews, resolution and requests | `rbit-storage` `reviews` (unchanged SQL) |

`rbit-services/src/pulls/review_requests.rs` adapts account lookup by name, the
current read access of the person asked and the existing requests.
`PullService::request_reviewers`/`remove_reviewers` keep the writes, timeline events
and `DomainEvent`s in the same order; `comment_on_line` keeps the diff read, throttle and
storage calls. `rbit-web/src/routes/pulls.rs` keeps parsing, rendering and the HTTP
responses. Error text, routes and stored data are unchanged.

## Legacy code removed

1. `hunk_excerpt`'s search and formatting and the `DIFF_HUNK_LINES` constant in
   `rbit-services/src/pulls/reviews.rs` (a thin adapter remains); the `ReviewSide`
   definition in `rbit-core`.
2. The pending-review routing condition in `comment_on_line`.
3. The name loop, kind and access checks, author refusal, existing-request filter and
   limit in `request_reviewers`, and the name loop of `remove_reviewers`.
4. The triage-or-author condition of `require_triage`.

## Boundaries that fail if legacy paths return

`scripts/rewrite/review-threads-retirement.mjs`, run by `scripts/rewrite/check.mjs`, over every
authored production Rust file below `crates/*/src`, comments ignored:

- In `rbit-web/src/routes/pulls.rs`: reply grouping, the comment-only card rule, the
  writer-or-author resolve rule or the review form condition.
- In `rbit-services/src/pulls/reviews.rs`: root-author comparison, root normalization,
  the pending-reviewer check, the `start_review ||` routing, account lookup by name, the
  user-kind check, the triage-or-author condition or the excerpt window arithmetic.
- Outside `rbit-collaboration/src`: the request refusal text and limit, a
  `DIFF_HUNK_LINES` or `MAX_REQUESTED_REVIEWERS` constant, or a `ReviewSide` definition.

`scripts/tests/review-threads-retirement.test.mjs` covers each rule, the allowed cases and
the whole current tree.

## Results

Quick node check of `5df0dd1d` against base `2fa415bb` (format, clippy `-D warnings`,
tests of the affected crates, rewrite contracts, licenses, front-end build): all passed.

| Gate | Result |
| --- | --- |
| `cargo fmt --all --check`, clippy `-D warnings` | pass |
| Tests of the affected crates | 1050 run, 1050 passed |
| Rewrite contracts: `scripts/rewrite/check.mjs` | 0 violations (2802 authored files) |
| Rewrite contracts: AST type check, components, password boundary | pass |
| Rewrite contracts: Node policy tests | 331 passed, 15 skipped, 0 failed |
| Licenses and advisories, front-end build | pass |

Run 203's tests covered the changed crates, unchanged: `rbit-collaboration` 122,
`rbit-core` 146 and `rbit-services` 782 (including `pulls`: line comments, replies,
pending reviews, resolution, review requests and withdrawals). The `rbit-web` route and
view changes of the first review-thread slice were tested by quick run 187 (684
`rbit-web` tests). New: `rbit-collaboration/tests/review_requests.rs`
(one answer for unknown names, organisations and unreadable repositories, no access
lookup for organisations, author refusal, existing requests, the limit, withdrawal
order, the pending lookup skipped when starting a review) and unit tests for
`hunk_excerpt` and the thread rules. Not run here: the full workspace suite and
`rbit-e2e` `review_threads`, left to the full check.

## Parity argument

- `hunk_excerpt` is the same search and formatting over a borrowed view of each line: the
  first hunk with the line on that side wins, `start = (end + 1) - 4` saturating, signs
  `+`, `-` and space.
- Pending routing short-circuits exactly as before: the pending review is looked up only
  when the commenter does not start one.
- Review requests make the same calls in the same order: per name, one lookup, then the
  access lookup only for users; after all names, one read of the existing requests;
  then the writes per new reviewer. Withdrawals still look up and remove name by name.
  Store handles are taken per call (pool handle clones, no extra query).

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; migration 0073 remains the latest and
no historical migration was edited. Rollback is the previous binary against the same
data. No request path gains a query. No release measurement was taken and none is
claimed.

## Not covered

- Review submission, editing and dismissal policy, and approval tallies, belong to
  `collaboration.pulls` (merging waits for protection's models).

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (this slice) plus 2 in the earlier review-thread slice |
| Files changed (this slice) | 11 |
| Lines added / removed | 458 / 74 |
| Production Rust (`src`) | 8 files, +316 / −72 (new: `anchors.rs` 123, `review_requests.rs` 98, services adapter 37) |
| Tests | `rbit-collaboration/tests/review_requests.rs` (133 lines) and `hunk_excerpt` unit tests |
| Checker and policy scripts | 9 changed lines in the existing guard and its test |

Tricky parts: anchoring had to stay independent of `rbit-git` (collaboration cannot
depend on it), so the excerpt reads lines through a borrowed `AnchorLine` view;
`ReviewSide` moved with it and is re-exported so stored values and callers are
unchanged. Withdrawals keep their per-name interleaving of lookup and write through a
callback rather than resolving all names first. `reviews.rs` stays under its size
ceiling by putting the adapter in its own file.
