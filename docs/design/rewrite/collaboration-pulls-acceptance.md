# Pull request retirement acceptance

Acceptance evidence for the `collaboration.pulls` ledger row ("Cross-fork pulls, reviews,
base selection, merge/squash/rebase"), following the
[identity retirement](identity-retirement.md) pattern. The slices are described in the
[pull request progress](collaboration-pulls-progress.md); review threads have their own
[acceptance](collaboration-review-threads-acceptance.md).

## Scope

| Behaviour | Owner |
| --- | --- |
| Records and values: pull requests, reviews, review comments, mergeability, checks, merge strategies, review states and ids | `rbit_collaboration::pulls` (`records.rs`; `rbit_core::pull`/`issue` re-export them) |
| Approval tallies and the merge box requirements over `rbit_access::protection::MergeRules` | `rbit_collaboration::pulls::{tally, evaluate}` |
| Default merge and squash messages, noreply identities, head specifications | `rbit_collaboration::pulls` (`messages.rs`) |
| Opening a pull request: read gates, fork network, archive, mirror, branch validation, comparison checks, default title, duplicates | `rbit_collaboration::pulls::PullCreation` over `PullCreationPort` |
| Branch names, head repository selection | `rbit_collaboration::pulls::{branch_ref, select_head}` |
| Merge gating: whether a stored trial merge holds, when to recompute it, who gets protection rules and the merge button, unchanged expectations, merge or bypass, and the commit message | `rbit_collaboration::pulls::{current_mergeability, needs_trial_merge, merges_as_writer, still_expected, merge_gate, merge_message}` |
| Review submission, editing and dismissal | `rbit_collaboration::pulls::{ReviewVerdict, verdict_refusal, empty_review_refusal, may_edit_review, may_dismiss_review, dismissal_message}` |
| Draft and base changes | `rbit_collaboration::pulls::{may_edit_pull, plan_base_change}` |
| Push and status handling: when a trial merge is current, which line comments a head push outdates; list badges and paging | `rbit_collaboration::pulls::{trial_merge_is_current, outdated_requests, outdated_comments, list_badges, list_offset}` |
| Protection rules for the merger | `rbit_access::protection` (`collaboration.protection`) |

`rbit-services` `PullService` remains the composition adapter: the concurrent reads of
the merge box, the per-repository merge lock and instance-wide slots, trial merges and
Git work (`rbit_git::merge`, `pull_refs`, overlays), `PushService` reporting, audit of
bypasses, storage writes, timeline entries and `DomainEvent`s, the push and status event
loop, the mergeability worker and its start-up sweep, and the comparison-count cache.
These keep their order; none decides on its own. HTTP routes, JSON, webhook payloads,
templates and catalog text are unchanged.

## Legacy code removed

- The pure model module `rbit-core/src/pull.rs` (now re-exports and its unchanged
  tests), and `MergeStrategy`, `ReviewState` and `ReviewId` in `rbit-core/src/issue.rs`.
- From the services: the opening checks and duplicate handling, `branch_ref`, head
  selection, the staleness and refresh conditions, the bypass gate and its text, the
  commit message choice, review verdict/emptiness/edit/dismissal checks and text, the
  draft/base edit checks and base-change planning, the trial-merge currency check,
  `outdated_requests` and the outdated filter, list tallies and paging.

## Boundaries that fail if legacy paths return

`scripts/rewrite/pulls-retirement.mjs`, run by `scripts/rewrite/check.mjs`, over every
authored production Rust file below `crates/*/src`, comments ignored:

- Pull records or merge requirements defined outside `rbit-collaboration/src`, or
  `evaluate`/`tally` defined again in `rbit-core/src/pull.rs`.
- `branch_ref`/`select_head` defined in `rbit-services`.
- Opening, review and base-change refusal text outside `rbit-collaboration/src`.
- `CreatedPull` outcomes handled outside storage and the creation adapter.
- In `rbit-services/src/pulls/merging.rs`: staleness, bypass, the blocked text or the
  message choice decided inline.
- In `rbit-services/src/pulls*`: `outdated_requests`, the trial-merge currency check,
  the outdated filter, inline tallies or page arithmetic.

`scripts/tests/pulls-retirement.test.mjs` covers each rule, the allowed cases and the
whole current tree.

## Results

Quick node checks against base `f3c7b212`, all passed:

| Run | Tip | Content | Tests of affected crates |
| --- | --- | --- | --- |
| 225 | `981d4484` | review-thread cherry-picks, model move, merge gating, boundary | pass |
| 237 | `d2e6db0e` | plus reviews, drafts and base changes, push and listing rules, boundaries | 1023 run, 1023 passed |

Each run passed `cargo fmt --all --check`, clippy `-D warnings`, the rewrite contracts
(`check.mjs` 0 violations over 2923 authored files at `d2e6db0e`; AST type check,
components and password boundary; Node policy tests 331 passed, 15 skipped, 0
failed), licenses and advisories, and the front-end build. Run 237's tests covered the
changed crates: `rbit-collaboration` 129, `rbit-core` 141 (the pull model and permission
tests through the re-exports) and `rbit-services` 753 (including `pulls`,
`actions_pulls`, `statuses` and `protection`), all unchanged suites passing. New tests:
unit tests for `merging.rs`, `reviews.rs`, `edits.rs` and `pushes.rs` (stale and current
mergeability, bypass gate and text, messages, submission/edit/dismissal order and text,
base-change order and text, outdated comments, paging) and the earlier
`pull_creation.rs`. Not run by the quick checks: the `rbit-web` and `rbit-storage`
suites, the full workspace suite and `rbit-e2e`, left to the full check. Local Windows clippy shows
only the 16 known `large_futures` errors in `rbit-services/tests/pulls.rs`.

## Parity argument

- Moved models are byte-for-byte, re-exported under their old paths; the enums keep
  their stored strings, serde names and `ALL` order.
- Gating: the stale condition, the refresh condition (and the mark-unknown write only
  when the stored value was not already unknown), the protection read only for
  signed-in writers, the gate order (all met, else an allowed bypass, else the first
  unmet requirement's sentence) and the message rules are the same expressions.
- Reviews and edits keep their check order: pending before ownership or write access,
  archive between write access and the message, closed before branch validation, branch
  existence before the head-branch rule, unchanged bases as a no-op.
- Push handling keeps one path lookup per commented commit and the same outdated rule;
  status handling marks unknown only when needed and wakes the worker once.
- No query, store handle or Git call was added on any path.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; migration 0073 remains the latest and
no historical migration was edited. Rollback is the previous binary against the same
data. No release measurement was taken and none is claimed.

## Not covered

- The comparison-count cache keeps its key (base and head ids) and its validity rule
  (counted under the default file limit) in `rbit-services`; it caches Git results and
  decides nothing about pull requests.

## Cost

| Measure | Value |
| --- | --- |
| Code commits (this branch, after the cherry-picked review threads) | 9, plus one base fix and this record |
| Files changed | 19 |
| Lines added / removed | 1134 / 721 (mostly moved model code) |
| Production Rust (`src`) | 15 files, +1040 / −706; new collaboration modules: `records.rs` 199, `merge.rs` 220, `messages.rs` 106, `merging.rs` 110, `reviews.rs` 117, `edits.rs` 84, `pushes.rs` 97 |
| Checker and policy scripts | the pulls guard grew by about 30 lines, its test by about 40 |

Tricky parts: keeping the model move cycle-free (protection's `MergeRules` now comes
from `rbit-access` with collaboration's `StatusContext`); splitting the 730-line core
model into files under the 400-line limit while the core tests keep passing through
re-exports; preserving check order where archive or existence lookups sit between
decisions; and a guard rule that clashed with a moved comparison
(`review.reviewer == Some(actor)`), solved by reordering the comparison. The base
`f3c7b212` had an unused import in `rbit-services/src/issues.rs`, fixed in its own commit.
