# Collaboration-owned label catalog workflows

This slice activates the complete existing repository label catalog in
`rbit-collaboration`, as assigned by the rewrite plan. Issues, pull requests,
timeline changes, label attachment/removal, milestones and assignees retain their
existing implementations; this does not retire the whole collaboration domain.

## Ownership and compatibility

The feature owns `LabelId`, `LabelSnapshot`, `Label`, `LabelFields`, `LabelUsage`,
`LabelInput`, catalog limits, default labels, normalization and validation. Their
fields, derives and inherent `Label::snapshot` semantics are preserved. Core's
original root/`issue` paths, storage's `sqlite::issue_meta` paths and services'
`issues` paths reexport the same concrete types. Integer IDs retain all original
values and serde representation. The storage-local `From<LabelRow> for Label`
remains legal because its source row is local; no orphan implementation is lost.

The default feature graph contains contracts, serde and credential-free identity
models (required by the later shared thread model's existing `is_author` API). Core explicitly
disables defaults. Storage and services explicitly request `applications`, which
adds only a static `std::future::Future` port and application policy. Neither graph
imports legacy core, concrete persistence, HTTP, Tokio or credential work.
Source policy checks enforce explicit consumer features and both isolated graphs.

`IssueService` keeps every original public method/signature and delegates actual
list, usage, single/multiple name lookup, create, update, delete and default seeding
to `LabelApplication`. It translates typed feature outcomes into the existing
`ServiceError` variants. The existing resolved `RepoContext` supplies repository ID,
write access and archive state; no new authorization/storage reads are introduced.
Reads retain the caller's existing repository visibility gate. Mutations check
writer access before archive state, preserving the privacy/error precedence.

## Ordering, persistence and side effects

| Operation | Original order retained | SQL statements |
| --- | --- | --- |
| List / usage / named lookup | Existing SQL ordering and issue-only usage count | 1 |
| Multiple names | Existing SQL; first missing input checked with ASCII case equality | 0 empty, otherwise 1 |
| Create | Writer, archive, validation, count limit, content throttle, clock, insertion | 2 |
| Update | Writer, archive, validation, throttle, clock, scoped update / not found | 1 |
| Delete | Writer, archive, scoped deletion / not found; no content throttle | 1 |
| Defaults on nonempty catalog | Writer, archive, count, return zero | 1 |
| Defaults on empty catalog | Writer, archive, count, throttle, transaction of nine conflict-safe inserts | 10 |

Clock and throttle are synchronous caller-owned callbacks evaluated at their
original points. No task, box, intermediate repository snapshot or dynamic registry
is added. Catalog mutations emit no new domain events, security audits or timeline
entries; deleting a label still cascades thread associations without timeline events.
Thread labeling remains the existing separate event-producing workflow.

All existing `IssueMetaStore` SQL methods and transaction bodies are unchanged
byte-for-byte from the slice's starting revision. The count-before-create cap is
still not a transactional quota: concurrent creates can exceed the advisory cap.
Default seeding still counts before a separate transaction; concurrent insertions
may make its reported count smaller than nine. SQL unique constraints, ASCII NOCASE,
repository-scoped updates/deletes, monotonic update timestamps, and transaction
rollback remain authoritative. This slice deliberately does not change concurrency
semantics or migrations.

## Validation status and acceptance gates

Source-only preparation passed seven Node policy tests, the full source/dependency
check (1,235 authored files, zero violations), formatting, registry-lock stability,
and the SQL-body comparison. No Rust compiler, build or Rust test was run here.
Only one path package and three consumer lock edges are added; no registry package
record or version changes.

New Rust tests cover wire models, Unicode/validation precedence, adversarial static
port ordering, throttle/cap/error precedence, partial concurrent default insert counts,
legacy type identity, actual SQL work counts, archive/authorization precedence,
cross-repository mutation rejection, concurrent default seeding, and absence of new
catalog events/audits. The existing labels/attachment/event parity test moves intact
into the bounded `issues::labels_port` module, sharing its existing real-store fixture.

Run through the shared compiler lane before integration acceptance:

```sh
node --test scripts/tests/collaboration-features.test.mjs scripts/tests/identity-features.test.mjs
node scripts/rewrite/check.mjs
node scripts/workspace.mjs test --package rbit-collaboration --no-default-features --locked
node scripts/workspace.mjs test --package rbit-collaboration --no-default-features --features applications --locked
node scripts/workspace.mjs test --package rbit-services --test main issues:: --locked
node scripts/workspace.mjs test --package rbit-core --package rbit-storage --package rbit-services --locked
node scripts/workspace.mjs clippy --package rbit-collaboration --package rbit-core --package rbit-storage --package rbit-services --all-targets --locked -- -D warnings
node scripts/rewrite/check-types.mjs
```

The feature matrix is also registered in hosted CI. Existing HTTP/API label and
issue tests remain required through the ordinary web suite; their route contracts
and authorization entry points are unchanged.
