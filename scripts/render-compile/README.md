# Render-probe compiler measurements

This development experiment compares the existing synthetic Askama variants:
A inheritance, B rendered segments, and C blocks, at 1/8/32/96 derives. It makes
no production renderer recommendation. Review the runner before collecting data.

From a Git worktree with the existing toolchain and dependencies available:

```sh
node scripts/rewrite/measure-render-compile.mjs --command check --profile dev --jobs 1 --output render-check.jsonl
node scripts/rewrite/measure-render-compile.mjs --command build --profile release --jobs 1 --output render-build.jsonl
```

The command, profile, job count and report filename are required. The output must
not already exist; its parent directory must exist. Keep reports local until
reviewed. Optional `--rounds` accepts multiples of six from 6 through 60;
`--sample-ms` accepts 50–5000 milliseconds, default250. On Windows the default
`--memory windows` uses the installed Windows PowerShell CIM provider. Other
systems must use `--memory none` (their default). No dependencies are installed.

Each invocation first warms all twelve feature combinations, then measures six
serial rounds by default (72 samples). Every mode occurs twice in each position
for every count across six rounds. Count order alternates forwards/backwards.
No asynchronous jobs overlap. Separate check/build/profile/job experiments need
separate reports; their results are different workloads.

Every job goes through the existing shared Cargo helper. While holding its
repository-wide lease and BOTH compiler output overrides, each measured job runs
only `cargo clean --package rbit-render-probe --profile <selected> --locked`.
The package/profile are fixed by this tool. There is no arbitrary clean/path
option, global clean, per-variant target directory, or new compiler cache layout.
Dependency artifacts stay warm. A sample is invalid if Cargo reports any rebuilt
dependency, a fresh probe library, or features other than its selected mode/count.
Successful clean exit/status and hashed output are retained. Actual behavior of
the installed Cargo, including separate intermediate storage, must satisfy these
checks before comparing the samples.

Compiler wrappers and incremental compilation are explicitly disabled; the
effective Rust compiler is selected through RUSTC (default `rustc` on PATH),
overriding a Cargo config's compiler selection. Automatic cache eviction is
disabled for the experiment. The existing namespace and lease still apply.
Ensure sufficient free disk space before running. Normal helper cache policy
resumes for subsequent ordinary commands. Do not run direct Cargo concurrently.

Reports are append-only JSONL, flushed after each record. Identity records include
source/lock/helper/runner hashes, revision, ancestor/home Cargo config content
hashes, effective-environment hash, rustc/Cargo versions, selected feature/profile/
job identity, machine memory/CPU/OS and Node version. Absolute paths, environment
values, command lines and compiler diagnostics are not serialized. Diagnostic
bytes are hashed. Source/config changes invalidate samples and stop the matrix.
Effective output-directory identities and the compatibility namespace are recorded.
An interrupted/failed child and already-written invalid samples remain in the
report. Abnormal Cargo termination retains the helper's fail-closed lease; inspect
the owned process tree using the existing recovery procedure before unlocking.
Observer startup requires an actual ready frame; errors, early exits and startup
timeouts stop the experiment before a timed compiler starts. Shutdown must prove
both process closure and drained output pipes, including after kill escalation.
If closure cannot be proved, the matrix stops and keeps its private control
directory/stop file. The summary records its run UUID for local inspection under
the worktree state's `render-compile` directory; no private path is serialized.

`wallMs` is elapsed Cargo command time, including process startup, Cargo work,
diagnostic capture and observer overhead. It excludes lease waiting, package clean,
observer startup, source hashing and helper handoff/cache maintenance. Warmup is
excluded explicitly. The runner emits raw observations and validity reasons; it
does not silently discard outliers or calculate comparative aggregates.
Summary counts distinguish compile-comparable/invalid and memory-comparable/invalid
samples. A completed matrix with usable timing may still have zero usable memory
observations. When memory was requested, unusable memory also yields a nonzero exit.

The Windows observer reads process metadata without changing compiler processes.
Ownership requires Cargo's parent/creation identity and an observed chain of
descendants; PID reuse and unknown ancestry cannot establish ownership. Only
owned cargo/rustc identities enter reports. The global census never requests
command lines. Separate argument queries require a proved Node/Cargo/rustc chain,
filter by PID AND creation time at the provider and revalidate identity on return.
Strict crate-name/target-triple tokens are extracted locally from these rustc
arguments; full command lines never leave the observer. Other compiler processes contribute only an anonymous contamination
count. Both working set and private committed bytes are sampled via Win32_Process.
CIM enumerations are non-atomic and may miss short-lived processes. Each process's
sampled maxima are lower bounds; different process lifetime peaks are NEVER added.
No concurrent-tree peak is claimed. Missing Cargo/rustc or observer failure makes
memory comparisons invalid; external compilers invalidate timing comparisons too.
Library memory comparability specifically requires observing an owned
`rbit_render_probe` rustc process. Observing only `build_script_build` is insufficient.

Unmeasured: true lifetime or concurrent-tree memory peaks, linkers/other children,
CPU time, OS-cold dependencies, incremental edit builds, test/doctest compilation,
expansion size, runtime rendering latency and real application compilation.
These synthetic library-only results cannot establish the application's memory
ceiling, page-rendering winner, or production regression budget.

Validation without invoking Cargo:

```sh
node --test scripts/tests/render-compile.test.mjs scripts/tests/render-observer.test.mjs scripts/tests/workspace-build.test.mjs
```
