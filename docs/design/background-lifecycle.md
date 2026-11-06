# Cooperative background lifecycle

The first lifecycle integration gives the recovery email subscriber and session
purger explicit static owners. They previously used an untracked subscriber and
an abort-only maintenance handle. HTTP binding must succeed before either starts,
and server cleanup must run on HTTP serving errors as well as graceful shutdown.

`rbit_kernel::TaskLifecycle` owns exactly one async worker, admits one start across
clones, issues a read-only cooperative stop signal and tracks real completion.
There is no registry, service lookup or dynamic dispatch. Features choose safe
stop boundaries and the composition layer chooses startup and shutdown order.

State is a health snapshot for that worker: `Idle`, `Running`, `Stopping`, `Stopped`
or `Failed`. `Stopped` means its future returned normally; `Failed` means it
unwound or was dropped without returning. Neither state claims that a database,
external service or an entire feature is healthy. A component configured off
creates no task. Existing HTTP operational health response contracts stay intact.

Shutdown stops admission and signals cancellation before waiting for the same
tracked worker. Cancelling or timing out the wait retains the worker and its
`Stopping` state; a later waiter can continue draining. No deadline or abort policy
is hidden in the kernel. Dropping the last owner signals cancellation, but cannot
synchronously wait. Worker futures should capture their stop signal and narrow
feature capabilities rather than their own supervisor, avoiding ownership cycles.

Recovery stops receiving new events once signalled, finishes the current handler
and then returns. Its drain covers database/token changes and admission into the
existing bounded mail queue. Mail transport delivery is separately owned by the
existing mail service and is not falsely reported as drained by the subscriber.
The session purger finishes a started purge before acknowledging stop. SSH keeps
its existing signal/restart handling and also observes the host stop signal so an
HTTP serving error cannot leave its listener waiting forever.

Future migrations should adopt the primitive where they resolve an actual
ownership gap. Existing feature-specific job deadlines, subprocess cancellation,
disabled startup and transport health semantics remain feature responsibilities.
