# Controlled Git process completion

SSH session cancellation and deadlines must not drop an active Git future and then
release its admission permits. Tokio child `kill_on_drop` requests termination; it
does not await process reaping or destruction of the exchange's spawned pipe tasks.

`ProcessControl` carries the caller's cancellation token and one absolute deadline
through advertisement, push gating and the exchange. The additive controlled APIs
reject an already interrupted stage before spawning, then observe interruption at
I/O and child-wait boundaries. They destroy borrowed output-copy futures, request
termination, await the actual child exit and abort **and join** the input feeder.
They return typed `Cancelled` or `TimedOut` errors only after that cleanup. No
deadline expiry is treated as evidence that cleanup has completed.

The owning SSH task must await controlled completion without an outer dropping
timeout or abort, and retain process/channel permits through that completion and
its existing post-push effects. `ProcessIo` bundles the three existing generic
stream directions without buffering packs or adding dynamic dispatch. Child
environment scrubbing, hooks policy, input caps, streaming and exit codes remain.

HTTP streaming shares its admission guard between the response supervisor and the
existing independently spawned child owner. Body drop requests cancellation but
cannot release the last guard while normal child/pump cleanup remains. Conversely,
a child that exits before the response finishes does not prematurely release the
body's existing occupancy. Completion follows request/stderr pump destruction.

HTTP advertisements use `advertise_refs_owned`, with one normal owner per admitted
slot. Dropping the actual request waiter signals that owner; it retains admission
while cooperatively killing/waiting for the direct child. The owner also applies
the original advertisement deadline internally. The route no longer drops a live
process future with an outer timeout. Successful advertisement bodies, protocol
headers, privacy checks and the existing deadline response status (503) remain.
The guard is supplied explicitly; this is not an unbounded detached fallback queue.

Legacy transport signatures remain source compatible, including borrowed output
streams. Arbitrary drop of those futures still relies on the last-resort child
kill request and pump abort; it is explicitly **not** a stopped proof. There is no
unsafe asynchronous destructor or unbounded detached reaper. Runtime teardown,
unexpected task panic and full descendant process containment remain separate
ownership obligations; the new contract covers cooperative callers that await it.

Only a successful child wait proves the direct `git` child exited. An I/O error
while waiting is unproved exit, not successful drain; all-error-path permit retention
and fail-closed host handling remain required. Arbitrarily dropping a controlled
operation before it returns also falls outside the cooperative completion guarantee.

Reaping here proves only the direct `git` child exited. Commands currently establish
neither a Unix process group nor a Windows Job Object. Hooks remain disabled by the
existing command policy, but Git may spawn pack helpers or other descendants.
This slice does not prove those descendants exited and must not advertise complete
process-tree drain. Cross-platform tree ownership and helper cleanup remain a
required later slice, with real descendant regressions and admission ownership.
