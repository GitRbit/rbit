# Linux process-group development proof

Status: source prepared; native Linux execution is pending. This is not a
production containment implementation or a claim of general group emptiness.

## APIs and ownership

The tool reuses locked rustix 1.1.5, checksum
`891efababe418670775f199f0d233d84843c227a0949a883ce15b37c78d6629d`.
Its published source records VCS `287214b889865d8e1406a0ee71cc409b6f6191c8`.
The Linux-only dependency enables std, process and event, with defaults disabled.
No new package version is required; the tool's lock entry gains rustix. Its
existing MIT/Apache license alternatives and syscall dependencies remain.

The stable safe [`CommandExt::process_group(0)`](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.process_group)
creates a leader before fixture code runs. A private `AnchoredGroup` owns the
`std::process::Child`. No Tokio Child owner, `try_wait` or reaping wait is used
until every group signal is finished. Safe
[`waitid`](https://docs.rs/rustix/1.1.5/rustix/process/fn.waitid.html) with EXITED,
NOWAIT and NOHANG observes exit without consuming the leader. The final reaping
method consumes and revokes signal authority first. Failure/lost authority must
not permit signaling a subsequently reused numeric PGID.

Known fixtures stay alive until the controller adopts their
[`pidfd_open`](https://docs.rs/rustix/1.1.5/rustix/process/fn.pidfd_open.html)
capabilities and checks their `/proc` creation identity. The escape handshake is
released only after that adoption. Safe
[`poll`](https://docs.rs/rustix/1.1.5/rustix/event/fn.poll.html) observes each pidfd's
exit readiness. Stdout is converted using Tokio's safe `ChildStdout::from_std`;
the pump copies to a sink without collecting an unbounded output buffer.

Only this isolated tool enables child-subreaper behavior, before its runtime or
any child exists. Cleanup waits target the known helper IDs, never wait-any.
ECHILD during this cleanup is accepted only after independent pidfd exit evidence;
it is never interpreted as a process-group membership result.

## Scenarios and completion boundary

| Scenario | Required observations |
| --- | --- |
| root-first | Repeated WNOWAIT observes the same unreaped leader; helpers remain alive, stdout remains open, and admission remains occupied. |
| cancel | Group TERM is followed by known pidfd exits, direct-child reap and pump completion before admission release. |
| escalation | All fixtures acknowledge safely handled TERM and remain alive; KILL is requested while the anchor remains owned, followed by actual completion observations. |
| lost-anchor | A deliberately consumed leader produces real ECHILD; admission closes and no group signal is issued. Stable fixture pidfds perform cleanup. |
| escape | A nonleader calls safe setsid after pidfd adoption, escapes the group and keeps stdout open after TERM. Admission closes; owned pidfd cleanup does not turn the outcome into ordinary verified completion. |

Every scenario checks an unrelated owned sibling outside the group survives.
Contract tests reject partial fixture receipts and reject signaling after anchor
consumption/loss even if a numeric group ID could have been reused. This does not
claim a test can force deterministic kernel PID reuse.

`FixtureCompletion` names its limited evidence: root reaped, known fixture pidfds
exited, and pump completed. It does not reuse the Windows membership receipt.
Neither a successful signal, a wait result, pipe EOF alone nor a `/proc` census
proves arbitrary descendants stopped. Error/unwind cleanup is best effort and
cannot manufacture a completion report. A timeout fails the experiment.

The [Linux wait documentation](https://man7.org/linux/man-pages/man2/waitpid.2.html)
distinguishes retained waitable state from reaping. The
[kill documentation](https://man7.org/linux/man-pages/man2/kill.2.html) describes
group signaling; successful group delivery does not establish completion.
Groups cannot contain hostile setsid/setpgid descendants. A later Linux production
decision may need delegated cgroup v2 capability and an explicit fail-closed policy.

## Git maintenance and CI

Future controlled Git commands must keep maintenance in the foreground with both
`gc.autoDetach=false` and `maintenance.autoDetach=false`, alongside the existing
scrubbed configuration policy. Git documents the
[maintenance setting and gc fallback](https://git-scm.com/docs/git-config#Documentation/git-config.txt-maintenanceautoDetach).
Current production gc sets only gc.autoDetach; this experiment changes no Git
command, hook policy or production behavior.

The separate optional Linux workflow uses GitHub-hosted runners, read-only
repository permissions, no persisted checkout credentials or secrets, pinned
actions, a ten-minute job deadline, and three-day JSON evidence artifacts.
PR source-path changes trigger it; workflow dispatch becomes available once its
definition exists on the default branch. It does not change existing required
checks. The fixed tool launcher builds through the shared helper and runs the
exact copied executable, never the generic server command. Unsupported pidfd or
subreaper capability is a failed/unproved experiment, not a passing skip.

WSL was inspected read-only: a Linux host exists but has no Rust toolchain. No
toolchain installation or local Linux build was performed. The hosted report must
record actual kernel/toolchain details and the scenario evidence before adoption
is considered.
