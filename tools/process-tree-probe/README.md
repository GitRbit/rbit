# Native process-tree development proof

This unpublished tool evaluates a Windows root Job plus a safe per-operation Job.
It is not used by the server, transport or runner. Source preparation does not mean
the experiment passed; retain its machine-readable execution report before making
an adoption decision. The repository's unsafe-code prohibition remains in force.

Exact published dependencies are `win32job = 2.0.3` and `processkit = 3.3.4`, selected
only on Windows. See [the release audit](dependency-audit.md). No shell, PowerShell,
native FFI or raw-PID kill is authored by this tool: every fixture is this Rust
executable in another role, and cleanup targets owned Child or Job capabilities.

The controller installs a process-lifetime kill-on-close safety root before its
runtime or any child exists. Each scenario parent is also started in an outer
cleanup Job. That parent installs its own root before its runtime, then starts a
per-operation Job where applicable. The unrelated fixture sibling is outside all
scenario Jobs but inside the controller's safety root. It must remain alive until
the controller explicitly cleans up its own Child after every scenario.

| Scenario | Evidence required |
| --- | --- |
| `gap` | Native `CREATE_SUSPENDED` child is already in the parent's root Job before any nested assignment. Abrupt parent termination kills it while the outer Job remains open. This behaviorally verifies noninheritance of the parent's root Job handle. |
| `live` | Abrupt parent termination stops a root/helper/grandchild tree; nested host Jobs remain compatible. |
| `root-exits` | The direct root exits first while helpers keep the Job and stdout pipe alive. Admission remains occupied until member emptiness, direct wait and pump completion. |
| `cancel` | Requested stop kills the Job, observes an empty member query, awaits Child status and joins the stdout pump before verified release. |
| `fault-assign` | A real invalid-handle assignment error occurs while a real suspended child remains root-contained. Admission closes; the report says quarantined and does not claim a drain. Parent exit performs OS cleanup. |
| `fault-wait`, `fault-query`, `fault-resume` | Explicitly simulated contract faults after actual native cleanup close admission. These are not injected failures inside the dependency's OS calls. |

The three unit tests separately prove that incomplete observations cannot create a
verified receipt, wait/query/assign/resume uncertainty closes admission, and unwind
defaults to quarantine before permit destruction. Release `panic=abort` does not
run those destructors: the process exits and its root Job is the backstop, rather
than a successful-drain report. Native abrupt-parent cases exercise that OS boundary.

Register `tools/process-tree-probe` as a workspace member, leaving default members
unchanged. No workspace dependency aliases or production dependency edges are needed.
The integration owner resolves the lockfile through the shared compiler lane. Do
not create an isolated Cargo workspace or a per-worktree target/build directory.

Once the compiler lane is granted:

```text
node scripts/workspace.mjs test --package rbit-process-tree-probe
node scripts/workspace.mjs clippy --package rbit-process-tree-probe --all-targets -- -D warnings
node scripts/workspace.mjs build --package rbit-process-tree-probe
```

Run the exact executable returned by that successful build with `verify report.json`.
The generic workspace `run` command is for the server and must not be used here.
On non-Windows hosts only the pure contract tests are applicable; the native driver
refuses to manufacture Windows evidence. Each native rendezvous has a bounded
observation deadline. An observation timeout means failure/unproved cleanup, never
permission to label a live tree stopped. Outer Job handles provide failure cleanup.

A first real Windows run passed all eight scenarios; see [the validation record](validation.md)
and the machine-readable evidence. This is one host and toolchain, not universal
platform compatibility. Remaining obligations include broader host coverage, native
wait/query/resume failure injection, and production lease/health integration. No production adoption follows
automatically from this development experiment. Unix still requires an unreaped
group anchor, foreground Git maintenance and honest treatment of `setsid` escape;
this tool does not claim hostile Unix process containment.
