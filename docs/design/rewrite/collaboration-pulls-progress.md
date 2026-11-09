# Pull request retirement progress

Progress for the `collaboration.pulls` ledger row ("Cross-fork pulls, reviews, base
selection, merge/squash/rebase"), which stays `baseline-retained`. This slice moves the
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

RESULTS_PLACEHOLDER

## Remaining for `collaboration.pulls`

- Merging (`pulls/merging.rs`): merge box facts, gating through `rbit_core::pull::evaluate`
  and the protection rules (`MergeRules`, owned by `collaboration.protection`), strategy
  and message defaults, locks and ref compare-and-swap.
- Reviews and requests (`pulls/reviews.rs`): approvals, dismissal on new commits, review
  requests and line comments.
- Base changes, drafts, head/base push handling and the mergeability worker
  (`pulls.rs`, `pulls/background.rs`), the comparison cache and listing.
- The pure pull models (`rbit_core::pull`) depend on `MergeRules`; they move once
  protection's models have a home `rbit-collaboration` can depend on.

`collaboration.review-threads` (`rbit-web/src/routes/pulls.rs`) was not started in
this slice.

## Cost

COST_PLACEHOLDER
