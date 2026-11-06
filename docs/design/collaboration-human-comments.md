# Human comment lifecycle ownership

`rbit-collaboration::comments` owns `CommentId`, `Comment`, `CommentSource` and the
human create/edit/delete application. `rbit-core::issue` and root exports retain the
same types, derives, source strings and numeric ID serde behavior. Actions comments
use these shared records; task-token authorization and task workflows remain in the
existing adapter. This slice does not retire comments, timelines or collaboration.

`HumanComments` dispatches through one concrete repository acquired by a closure.
Creation preserves permission, archive, text normalization and content-throttle
checks before constructing the store, then reads the clock and inserts. Edit and
delete construct one store and look up the scoped comment before authorization,
archive checks or text validation. The service recomputes the unchanged legacy
permission table for the actual actor; it never trusts the context's viewer flags.

An edit with the same normalized body returns the fetched comment before throttle,
clock, transaction or publication. Changed edits reuse the lookup's store and move
the old `IssueText` into the legacy previous-body string, without another clone.
Deletion has no content-throttle charge or application clock; the original store's
delete transaction still reads its clock when updating the thread. A concurrent
row disappearance remains `NotFound` for edit and empty success for delete after
its successful initial lookup. No conditional-update or snapshot guarantee is added.

SQL, migrations and transaction bodies are unchanged. Creation uses one acquisition
and two statements; changed edit/delete each use two acquisitions and three
statements including lookup. Unchanged, missing and unauthorized edits retain the
one lookup. Failed comment insertion rolls back the preceding thread count update.
The service still publishes only after a successful commit; this in-memory event
has no durable outbox or crash guarantee. There is no new task, await between commit
completion and publication, or detached finalizer. Existing cross-feature consumers
remain in their original order.

Prepared acceptance includes independent static-port ordering and unpolled-future
fixtures, type/wire identity assertions, the relocated original real-store policy
and event test, SQL-work assertions, and an insert-trigger rollback failure test.
Source/Node checks and formatting pass; Rust execution remains pending the shared
compiler lane. Required gates are collaboration models/applications tests, service
`--test main timeline::human_comments_port` and complete timeline/issue suites,
core/storage tests, strict all-target Clippy, structural checks, and web HTTP tests.
