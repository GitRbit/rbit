# First isolated Linux validation

The [isolated workflow run](https://github.com/GitRbit/rbit/actions/runs/42169158625)
for PR head `642005f5b79550711c40e5db271ada7188366bd6` passed on November 6,
2026 UTC. GitHub tested its normal pull-request merge checkout. The Linux job
passed five package tests, strict all-target Clippy, exact helper-artifact build
and execution, and JSON evidence upload. No local Linux build or WSL toolchain
installation was used.

The recorded host was x86_64-unknown-linux-gnu, kernel 7.0.0-1017-azure,
rustc 1.99.1, LLVM 23.1.1 and Git 2.56.0. Retained
[host metadata](evidence/linux-2026-11-06-host.json) and
[native observations](evidence/linux-2026-11-06.json) preserve the evidence beyond
the workflow's three-day artifact retention.

Root-first, cooperative cancellation and TERM-to-KILL escalation all observed
known fixture pidfd exit, root reaping and pump completion before admission
release. The root-first case retained the waitable leader while helpers and
stdout stayed alive. Lost-anchor and setsid-escape cases correctly closed
admission and retained quarantine even after capability-based fixture cleanup.
The unrelated sibling survived all five scenarios.

The earlier [run](https://github.com/GitRbit/rbit/actions/runs/42168177675) failed
compilation before native execution because five calls used older Signal enum
spelling. The pinned rustix release exposes TERM/KILL associated constants; the
corrected run above executed all validation stages successfully.

These observations apply to the finite handshake-adopted fixtures on this host.
They establish neither general group emptiness nor hostile descendant containment.
ECHILD is not membership proof, setsid escape remains real, and a successful signal
is not completion. No production process-group/cgroup policy is activated by this
development result. See [the design limits](unix-design.md).
