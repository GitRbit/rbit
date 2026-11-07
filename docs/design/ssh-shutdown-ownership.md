# SSH shutdown ownership

The SSH listener owns a single descendant scope for connection waiters, russh
handlers, channel replies, Git exchanges and input pumps. This is a concrete
transport lifecycle boundary, not a feature registry or task scheduler.

Listener shutdown stops connection admission and requests cancellation. Connection
waiters have the existing ten-second drain window (cancellation may close I/O sooner); remaining waiters are aborted
and then joined. Aborting a waiter alone is not a drain acknowledgement. The
scope closes admission and waits until every tracked future/handler is destroyed.
The public shutdown method repeats this drain after a listener panic, so logging
`SSH server stopped` follows descendant completion rather than a timeout.

russh 0.65 internally spawns its session task, and dropping `RunningSession` only
detaches its JoinHandle. Each handler therefore owns a tracked lifetime token as
its last field, released after its Shared/Services/channel capabilities. Its TCP
adapter observes a per-connection cancellation token. Cancelling or dropping the
connection waiter interrupts the internally owned socket I/O; it does not pretend
that the handler has already stopped. Final scope drain still waits for actual
handler destruction. Stalled pre-authentication connections follow the same rule.
An independent tracked session-join owner also retains `RunningSession` and awaits
its real internal JoinHandle completion. Connection waiters observe its oneshot
result; aborting those waiters cannot detach this join. An already-counted parent
may adopt this existing library task during scope close, before releasing its own
token. Closing admission cannot erase work the library has already started.

An SSH Git exchange retains both its global Git-process permit and channel slot
through the controlled transport operation, input-pump destruction and successful
push bookkeeping. It uses one absolute session deadline across advertisement,
command parsing and Git. The controlled advertisement/interactive/receive calls
await owned Git termination/reaping and feeder destruction before returning.
There is no outer timeout that drops a running child-owning transport future.
Only non-process protocol I/O is cancellation-selectable. Final best-effort
channel output is limited to one second and skipped during host shutdown.

Started database/post-push effects finish rather than being abandoned when the
session deadline expires. Deadline/network grace is bounded, but an operating
system process wait or already-started store operation may delay final drain.
A caller timing out its shutdown wait must not interpret that as drained or
close shared stores on that basis. Dropping the public server immediately closes
child admission, requests cancellation and closes the listener signal; the
independently owned listener still drives cancellation and cleanup. Drop itself
does not report that cleanup finished.

Authentication, key/deploy-key authorization, Git/LFS protocols, repository and
per-client limits retain their existing checks. Required Git and channel permits
are not released merely because a network waiter or cancellation signal completed.
Existing full Git-over-SSH and branch-protection fixtures remain the protocol gate.

Successful controlled completion proves the direct Git child and owned async
descendants. An I/O error while waiting is unproved exit, not a stopped receipt;
fail-closed admission and exceptional-error retention still need their own gate.
This change does not
claim OS process-tree containment for Git helper grandchildren, nor verified reap
after arbitrary external abort/panic drops of a legacy transport future. Those
are separate transport ownership boundaries; kill-on-drop alone is not a verified
process-stop acknowledgement. The controlled path must continue to be awaited.

Targeted validation, through the existing shared compiler lane:

```sh
node scripts/workspace.mjs test --package rbit-ssh --locked
```

Unit fixtures cover cancelled drain waits, closed admission, listener-drop stop,
aborted connection/reply destruction, hidden handler lifetimes and cancelled TCP
I/O. Integration fixtures cover stalled pre-authentication on all platforms and,
on Unix, an owned live Git process, retained permit, shutdown reap and reopened
redb after dropping the host handles. The Unix process fixture does not substitute
for the existing real Git protocol fixtures or the controlled transport's own
cross-platform child-reaping tests.
