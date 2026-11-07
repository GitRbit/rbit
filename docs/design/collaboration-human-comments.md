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

## Actions comment lifecycle

`TaskComments` additionally owns create/edit/delete policy for Actions-authored
comments. The service still computes the same repository equality and
`TaskGrant::can_comment_on` predicate; token authentication, running-task checks,
run trust, permissions and HTTP/API budgets remain with Actions and the API.
The application receives only that resolved predicate and the thread/archive data.

Existing provenance is `CommentSource::Actions`, with no originating run/job/task
column on a comment. Any correctly authorized task in the same repository may
edit/delete an Actions comment from another run. A human comment is forbidden,
including a ghost human comment with no remaining author. This port preserves
that policy rather than introducing unsupported per-run ownership.

Create checks authority, archive and text before constructing one store and reading
the clock. Edit and delete construct one store after authority and reuse it for the scoped
lookup and any subsequent mutation. This removes the legacy duplicate pool-handle
clone, including the early handle on forbidden edits. The constructors are infallible
field initialization plus a shared pool clone; they establish no connection, SQL,
transaction or snapshot. Missing lookup wins before source,
archive and text; source wins before archive. An unchanged normalized edit does
one query, no write/clock/log/publication. Delete still mutates only after the authorized source lookup and archive gate. There is no human content
throttle. Original SQL, snapshots, transaction work, post-commit log text/fields,
actor-less events and consumers are unchanged. Counters pin accepted create at
one acquisition/two statements, unchanged edit at one/one, and changed edit/delete
at two/three. Direct policy tests also pin factory calls, which SQL counters miss.

Prepared fixtures cover repository/kind permissions, locked threads, human source
refusal, scoped lookup/error precedence, source/archive order, cross-run Actions
authority, normalization/no-op behavior and actor-less event payloads. Actions
notification/reference tests remain applicable. Rust validation remains pending;
include collaboration applications tests, service `timeline::task_comments_port`,
Actions task-token tests, notifications/references and HTTP task-comment routes in
the shared acceptance run. No runtime speedup or feature-group retirement is claimed.

The handle-reuse improvement changes no SQL/acquisition budget or permission, error,
throttle, clock, logging or event behavior. Direct factory tests prove zero handles
for denied edits and one for admitted edit/delete/no-op operations instead of two.
No measured latency or runtime speedup is claimed.
