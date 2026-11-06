# Shared thread reads and plain issue creation

This slice follows label and milestone catalogs. It activates shared thread row
creation, joined opening and counts, and the complete existing plain-issue creation
workflow with metadata. Search/filter/dashboard queries, metadata read shaping,
deletion, comments, state/lock changes, attachment mutations and references
remain legacy workflows. Neither issues nor collaboration are fully retired.
Title/body editing is activated separately through `threads::ThreadEditor`; its
legacy SQL transaction and event translation are retained.

## Model and dependency closure

`rbit-collaboration::threads` owns `IssueId`, `IssueNumber`, `IssueKind`, `StateReason`,
`IssueTitle`, `IssueText`, `IssueTextError`, their limits, `Issue` and `NewIssue`.
`issues` owns `NewIssueInput` and creation label/assignee limits. Original core,
storage and service paths reexport these same types; constructors, serde numeric
and enum representations, variant order, validation, display and error messages
remain compatible. The simple text error implements equivalent Display/Error
directly rather than adding a registry dependency.

The exact existing inherent `Issue::is_author(&User)` signature requires a direct
`rbit-identity-models` dependency. An extension trait or different actor argument would change
the public API. This edge explicitly disables defaults, as repository models already
do; identity never imports collaboration. Isolated and feature-unified model/application graphs and
consumer feature guards reject password, crypto, runtime and adapter leakage.
There are no new registry packages, tasks, boxed futures, migrations or SQL changes.

## Production authority

`TimelineService::open`, `open_counts` and private `create` delegate to
`ThreadApplication` through a direct `IssueStore` static port. Opening checks read
access before the original joined issue/author query; deleted authors stay absent.
Counts retain their caller-owned visibility gate and one query. Creation checks read
access, then archive state, title, body, content throttle and clock before the original
number-counter/row transaction. Pull creation retains its existing shared `NewIssue`
model and `IssueStore::insert_in` call inside the pull transaction; that orchestration
is not migrated to this application. Returned
viewer permissions still use the original service permission table.

`IssueService::create` delegates orchestration to `IssueCreation`. Its narrow service
bridge retains cross-feature assignee grant checks, storage and legacy timeline/event
translation. It checks visibility/archive first; writers' distinct label IDs are
resolved in first-occurrence order before the label limit; assignees are resolved
before their limit; then the milestone is resolved. Readers ignore submitted metadata
without looking it up. Title/body validation and content throttle occur only after
writer metadata validation, just as before.

The row is created first. If metadata exists, one attachment transaction follows;
individual label/assignee/milestone timeline writes then follow in the original order.
Only after those succeed is one legacy `Opened` event published. Its notification,
reference and webhook consumers are unchanged. No catalog-style extra audit/event
is introduced and no duplicate `Opened` is published by the row application.

## Existing commit/cancellation boundary

This is a parity port, not durable effect delivery. The row transaction commits
before metadata; metadata commits before each separate timeline transaction; event
publication is in memory. Failure or cancellation after row commit can therefore
leave a committed issue with absent/partial metadata or timeline and no `Opened`.
A successful metadata transaction followed by a failed timeline write can leave
fully attached metadata with partial history. Cancellation after publication cannot
retract consumers' work. There is no new outbox, rollback or detached finalizer.

An independent static-port cancellation test exposes the post-row gap. A real-store
fixture trigger rejects metadata insertion after the row commits and checks the
remaining row, rolled-back metadata, missing timeline and absent `Opened`. Changing
these semantics requires a later transaction/outbox/lifecycle decision, including
notification and hook replay/idempotency, before claiming crash-safe completion.

## Validation and acceptance

Source preparation verifies both stores' SQL methods/transactions remain byte-for-byte
unchanged. The existing real metadata/event creation test moves intact into a bounded
child module. Added tests cover independent wire and text boundaries, row policy
precedence, metadata dedup/order/limits, reader suppression, missing data, post-commit
failure/cancellation, exact core/storage aliases, actual SQL work, authorization,
concurrent number allocation and one publication per successful row.

Empty-metadata creation retains one acquisition/two statements in the original
transaction. Joined opening and counts each retain one acquisition/statement. Denied
or archived creation/opening does no SQL; missing writer labels win before title
validation and cost the original lookup. Attachment failure retains the original
label lookup + two row statements + failing metadata insert. Rich metadata retains
the existing per-ID/grant reads, attachment transaction and per-event transactions.

Rust compilation/tests/Clippy and runtime acceptance remain pending the shared lane.
Run after both catalog commits:

```sh
node --test scripts/tests/collaboration-features.test.mjs scripts/tests/identity-features.test.mjs
node scripts/rewrite/check.mjs
node scripts/workspace.mjs test --package rbit-collaboration --no-default-features --locked
node scripts/workspace.mjs test --package rbit-collaboration --no-default-features --features applications --locked
node scripts/workspace.mjs test --package rbit-services --lib timeline:: --locked
node scripts/workspace.mjs test --package rbit-services --test main issues:: --locked
node scripts/workspace.mjs test --package rbit-core --package rbit-storage --package rbit-services --locked
node scripts/workspace.mjs clippy --package rbit-collaboration --package rbit-core --package rbit-storage --package rbit-services --all-targets --locked -- -D warnings
node scripts/rewrite/check-types.mjs
node scripts/workspace.mjs test --package rbit-web --test main --locked
```

The normal full service/web suites remain necessary for pull creation and consumers
of the shared model aliases. The existing hosted collaboration model/application
matrix automatically includes the new tests. No compile or runtime speedup is claimed.
