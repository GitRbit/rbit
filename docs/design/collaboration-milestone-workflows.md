# Collaboration-owned milestone catalog workflows

This follows the label catalog slice and activates all eight existing milestone
catalog entry points in `rbit-collaboration`: list, counts, lookup with progress,
title lookup, create, update, close/reopen and delete. Thread attachment, timeline
publication, issue/pull-request policies and metadata batching stay in their
existing adapters; the collaboration group is not retired by this slice.

## Model closure and public compatibility

The feature owns `MilestoneId`, `Milestone`, `MilestoneFields`, `MilestoneProgress`,
`MilestoneInput`, `MilestoneRef`, validation and limits. `Milestone.state` requires
the existing shared `IssueState`; its former core text-enum expansion moves into a
pure feature model with identical derives, serde names, variant order, `ALL`,
`as_str`, `parse` and Display behavior. No duplicate state enum or ID type is added.
Core's root and `issue` paths reexport the exact feature types, as do the existing
storage and service paths. Original constructors, fields and signatures remain.
`IssueMilestone`, the storage thread-attachment record, stays in storage because
its `IssueId` dependency belongs to the later attachment closure.

Both the default model graph and opt-in `applications` graph remain contracts plus
serde, without runtime, password, crypto or legacy adapter imports. The existing
explicit consumer feature guard and isolated graph checks apply unchanged. This
slice adds no feature, dependency, package, lock entry or migration.

## Application and SQL authority

`MilestoneApplication` uses a statically dispatched `MilestoneRepository`. The
service supplies the existing resolved repository access and synchronous throttle
and clock callbacks. Writer permission wins before archive state, which wins before
validation, content throttle and persistence. Create/update spend the original
content budget; close/reopen/delete retain their original lack of that charge.
Store conflicts, missing rows and validation errors retain their existing service
variants/messages. Reads retain the caller's existing repository visibility gate.

All eight catalog operations retain one SQL statement. ID lookup intentionally
loads the original list-with-progress statement and selects the ID in memory; it
does not substitute a new lookup or lose progress. List ordering stays due date then
title for open milestones and most recently closed first for closed ones. Progress
still counts attached issues and pull requests, and title lookup retains SQL's
ASCII case-insensitive comparison without adding trim normalization.

Every existing `IssueMetaStore` SQL body and transaction boundary is byte-for-byte
unchanged from the label-port parent. SQL remains authoritative for repository-scoped
mutations, unique titles, monotonic update timestamps, preserving the first close
time on repeated close, clearing it on reopen, row corruption checks and cascading
deletion of thread associations. Catalog mutations emit no new domain events,
audits or timeline entries. Existing attachment events stay on the original path.

## Validation status and required acceptance

Source preparation passes the Node dependency/source guards and formatting. Rust
compilation/tests/Clippy remain pending the shared lane; no performance or runtime
parity result is claimed. New tests cover:

- Original state/ID serde and parse contracts, century leap/month-end dates, Unicode
  scalar title limits, normalized description byte limits and exact error precedence.
- Independent static-port authorization/validation/throttle/clock/store ordering,
  list-with-progress lookup and one-call missing mutations.
- Legacy type identity, all actual catalog SQL work counts, duplicates, missing and
  foreign-repository mutations, archive/privacy precedence, no catalog event/audit
  changes, due-date ordering and repeated-close/reopen timestamp behavior.

The existing real-store attachment/timeline test moves intact into the bounded
`issues::milestones_port` test module and continues to exercise the legacy path.
No router behavior changes; compile and run the full web integration suite because
there is no pre-existing dedicated milestone-router test filter to cite.

Apply after the label activation, then run:

```sh
node --test scripts/tests/collaboration-features.test.mjs scripts/tests/identity-features.test.mjs
node scripts/rewrite/check.mjs
node scripts/workspace.mjs test --package rbit-collaboration --no-default-features --locked
node scripts/workspace.mjs test --package rbit-collaboration --no-default-features --features applications --locked
node scripts/workspace.mjs test --package rbit-services --test main issues:: --locked
node scripts/workspace.mjs test --package rbit-core --package rbit-storage --package rbit-services --locked
node scripts/workspace.mjs clippy --package rbit-collaboration --package rbit-core --package rbit-storage --package rbit-services --all-targets --locked -- -D warnings
node scripts/rewrite/check-types.mjs
node scripts/workspace.mjs test --package rbit-web --test main --locked
```

The existing hosted collaboration feature matrix automatically includes these
new model/application tests. Public HTML/API milestone forms, redirects, token
scopes and error responses remain owned by their unchanged route adapters.
