# Pull request and review-thread retirement progress

Progress for the `collaboration.pulls` ("Cross-fork pulls, reviews, base selection,
merge/squash/rebase") and `collaboration.review-threads` ("Line comments, resolvable
conversations and review requests") ledger rows. `collaboration.pulls` stays `baseline-retained` until its background work
moves; review threads were retired later. This slice moves the
decisions of opening a pull request; merging, reviews, base changes and background
work keep their current owners. It follows the [identity retirement](identity-retirement.md)
pattern for what moves, what is removed and which boundary keeps it from returning.

## What moved

| Behaviour | Owner after this slice |
| --- | --- |
| Opening a pull request: hidden repositories and heads outside the base's fork network are `NotFound`; archived bases are `Archived`; mirrors, invalid or equal branch names, missing branches, unrelated histories, nothing ahead and an existing open pull request for the same branches are refused with the legacy text; the default title (the single commit's summary, else the branch); title and body validation; throttle just before the insert | `rbit_collaboration::pulls::PullCreation` (new) over `PullCreationPort` |
| Plausible branch names (`refs/heads/{branch}`) | `rbit_collaboration::pulls::branch_ref` (moved from `rbit-services`) |
| Which repository of the fork network a head specification names | `rbit_collaboration::pulls::select_head` (moved from `PullService::resolve_head`) |

`rbit-services/src/pulls/creation.rs` adapts the readable fork network, the mirror flag,
branch tips, the counted comparison (no file changes, two commits, no behind count) and
the capped insert. `PullService::create` keeps `refs/pull/{n}/head` storage, the
`Opened` event and the mergeability wake-up, in the same order.

## Legacy code removed

- The check sequence and refusal text of `PullService::create`, its title default and
  its handling of `CreatedPull::Duplicate`.
- `branch_ref` in `rbit-services/src/pulls.rs`.
- The candidate filtering in `PullService::resolve_head`.

## Boundaries that fail if legacy paths return

`scripts/rewrite/pulls-retirement.mjs`, run by `scripts/rewrite/check.mjs`, over every
authored production Rust file below `crates/*/src`, comments ignored:

- `fn branch_ref` or `fn select_head` defined in `rbit-services` (rbit-git keeps its own
  validation for the refs it writes).
- The opening-only refusal text outside `rbit-collaboration/src`. Base changes and
  reopening share some refusals and keep them.
- `CreatedPull::Created`/`Duplicate` handled outside `rbit-storage` and the creation
  adapter.

`scripts/tests/pulls-retirement.test.mjs` covers each rule, the allowed cases and the
whole current tree. New Rust tests: `rbit-collaboration/tests/pull_creation.rs` (order
of refusals with the calls each one makes, nothing spent on refusals, title defaults)
and `branch_ref` unit tests.

## Parity argument

Same checks in the same order, with the same calls: read gates, the network lookup
only across repositories, archive, mirror, branch names, equal branches in one
repository, base tip then head tip (any read failure is "does not exist"), one counted
comparison, then title and body, throttle and the capped insert, whose duplicate
outcome writes nothing. `Timestamp::now()` is taken after the throttle, as before.
Merge gating, required checks, protection and review dismissal are not touched.

## Results

A quick node check (format, clippy with `-D warnings`, affected-crate tests, rewrite
contracts, licenses, front-end build) of the branch tip passed against base `f1913082`:

| Gate | Result |
| --- | --- |
| `cargo fmt --all --check`, clippy `-D warnings` | pass |
| Tests of the affected crates | 2181 run, 2181 passed, 1 skipped |
| Rewrite contracts: `scripts/rewrite/check.mjs` | 0 violations (2798 authored files) |
| Rewrite contracts: AST type check, components, password boundary | pass |
| Rewrite contracts: Node policy tests | 331 passed, 15 skipped, 0 failed |
| Licenses and advisories, front-end build | pass |

The existing suites ran unchanged, including `rbit-services` `pulls` (creation,
duplicates, cross-fork heads, review threads, resolution, requests) and the `rbit-web`
pull request routes and views. Not run here: the full workspace suite and `rbit-e2e`,
which the quick mode leaves to the full check.

## Second slice: models, merge gating, reviews and base changes

Once protection's models moved to `rbit-access`, the rest of the pull request decisions
moved too:

| Behaviour | Owner after this slice |
| --- | --- |
| Pull request, review and comment records; mergeability, checks and merge strategy values; review states and ids | `rbit_collaboration::pulls` (`records.rs`, moved from `rbit_core::pull` and `rbit_core::issue`, which re-export them) |
| Approval tallies (latest decisive review per writer, stale approvals, never the author) and the merge box requirements over `rbit_access::protection::MergeRules` | `rbit_collaboration::pulls::{tally, evaluate}` (`merge.rs`, moved) |
| Default merge and squash messages, noreply identities, head specifications | `rbit_collaboration::pulls` (`messages.rs`, moved) |
| Whether a stored trial merge still holds, when to recompute it, who gets protection rules and the merge button, unchanged expectations, whether a merge may go ahead or bypass protection, and the commit message | `rbit_collaboration::pulls::{current_mergeability, needs_trial_merge, merges_as_writer, still_expected, merge_gate, merge_message}` (new) |
| Review submission (authors only comment, closed pull requests take only comments, nothing to submit), editing (own submitted reviews), dismissal (writers, a message, only decisive reviews) | `rbit_collaboration::pulls::{ReviewVerdict, verdict_refusal, empty_review_refusal, may_edit_review, may_dismiss_review, dismissal_message}` (new) |
| Draft and base changes: author or writers, closed refusal, branch validation and existence, the head-branch rule, unchanged bases, the dismissal message | `rbit_collaboration::pulls::{may_edit_pull, plan_base_change}` (new) |

The merge service keeps the concurrent reads, the per-repository lock and slot, the
trial merge, Git work, audit and events; the review and pull services keep storage,
throttle, timeline and events, in the same order. `scripts/rewrite/pulls-retirement.mjs`
now also fails if the records or merge requirements are defined outside
`rbit-collaboration/src`, if the merge service decides staleness, bypass, the blocked
text or the message itself, or if review or base-change refusal text appears outside
collaboration.

## Remaining for `collaboration.pulls`

- Head and base push handling (`pulls/background.rs`): outdated line comments after a
  head push, pull requests merged outside rbit, re-queueing after statuses, and the
  mergeability worker and its start-up sweep. These are event-driven Git orchestration
  with few decisions of their own.
- Listing and the comparison-count cache (`pulls.rs`).

## Review threads

| Behaviour | Owner after this slice |
| --- | --- |
| Conversations: replies kept with their root wherever they were submitted, in order; orphan replies dropped | `rbit_collaboration::pulls::review_threads::group_threads` over the `ThreadEntry` trait (implemented by the web view) |
| A reply's root (`in_reply_to` of the parent, else the parent) | `review_threads::thread_root` |
| Who may resolve or reopen: writers or the root's author, after the comment permission | `review_threads::may_resolve`, used by both `PullService::set_thread_resolved` and the page's resolve button |
| Replies to a pending review's comment: only its reviewer | `review_threads::reply_visible` |
| Comment-only reviews with an empty body and no remaining threads get no card | `review_threads::shows_review_card` |
| The review form (who may comment; closed pull requests only to finish pending comments; authors cannot approve or request changes) and who sees the dismiss action | `review_threads::{review_form, may_dismiss_reviews}` |
| At most 15 requested reviewers and the request refusal text | `review_threads::{MAX_REQUESTED_REVIEWERS, requests_fit, reviewer_refusal, AUTHOR_REVIEW_REFUSAL}` (`rbit-core` re-exports the limit) |

`scripts/rewrite/review-threads-retirement.mjs` fails if the route groups replies or
decides resolve/form/card rules itself, if the review service compares root authors,
normalizes roots or checks pending reviewers inline again, or if the request limit or
refusals are defined outside `rbit-collaboration/src`
(`scripts/tests/review-threads-retirement.test.mjs`).

The rest of `collaboration.review-threads` (line anchoring, pending-review routing and
review requests) moved later; the row is retired with its
[acceptance evidence](collaboration-review-threads-acceptance.md).

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 4 (opening, its boundary, review threads, their boundary) plus this record and three fixes to the base stack |
| Files changed (whole branch) | 23 |
| Lines added / removed | 894 / 130 |
| Production Rust (`src`) | 15 files, +514 / −128 (new: `pulls/creation.rs` application 141, adapter 85; `review_threads.rs` 165) |
| Tests | `rbit-collaboration/tests/pull_creation.rs` (124 lines) and unit tests in `branches.rs` and `review_threads.rs` |
| Checker and policy scripts | two Node guards (44 and 42 lines), their tests, four checker lines |

Tricky parts:

- Choosing slices that do not touch merge gating: the pure pull models depend on
  protection's `MergeRules`, so they stay in `rbit-core` until protection's models move.
- Keeping the opening order exact, including the network lookup only across
  repositories and the throttle just before the insert.
- Sharing one resolve rule between the service's enforcement and the page's button.
- The base stack needed two fixes found by the checks: rustfmt import order in
  `rbit-storage/src/sqlite/mod.rs`, and an explicit `super::filter` path for the AST type
  check in `rbit-collaboration/src/issues/dashboard.rs`; an unused helper in
  `rbit-services/src/issues.rs` was removed as well.

Estimate: the rest of `collaboration.pulls` (merging and gating once protection's models
have a home, reviews and dismissal, base changes, background work) is three to four
sessions of this size; the rest of `collaboration.review-threads` (line anchoring and
pending-review routing) is about one.
