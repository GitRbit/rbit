# Bounded compiler measurement sessions

This is a development-only, explicit measurement path. It does not alter Rust
profiles, ordinary builds, compiler-cache trust, pruning or application data.
It allocates at most two arm lanes inside one repository-owned measurement
candidate, never a compiler target directory per worktree. Automatic rotation and
promotion remain absent. The existing current/legacy lane stays protected.

## Closed plan and execution

A plan has `schema: 1` and one or two `arms`. Each arm contains one to sixteen
`steps` and optional `environment` entries limited to declared Cargo profile
fields and `CARGO_INCREMENTAL`. Each step contains only `command` and `args`.
Supported commands are `build`, `check`, `clippy`, `test` and `nextest run`.
Unknown switches/selection semantics, output/config overrides, arbitrary commands,
per-arm rustflags/wrappers/environment and undeclared profiles refuse before
allocation. Two arms must have the same commands/arguments except Cargo profile
selection. Nextest runner profiles are not Cargo profiles.

For example, an initial bounded smoke comparison can use the existing dev profile
and a declared debug-info override, without adding or activating a new Rust profile:

```json
{
  "schema": 1,
  "arms": [
    {"steps": [
      {"command": "check", "args": ["-p", "rbit-auth", "--locked"]},
      {"command": "check", "args": ["-p", "rbit-auth", "--locked"]}
    ]},
    {"environment": {"CARGO_PROFILE_DEV_DEBUG": "0"}, "steps": [
      {"command": "check", "args": ["-p", "rbit-auth", "--locked"]},
      {"command": "check", "args": ["-p", "rbit-auth", "--locked"]}
    ]}
  ]
}
```

Save the plan inside the selected source checkout. Bootstrap existing dependency
availability using the normal `fetch --locked` helper **before** enabling this
mode. On supported Windows, after the compiler owner grants the lane and actual
free-space observations justify an explicit positive byte threshold, run:

```powershell
$env:RBIT_BUILD_MANAGED_RETENTION = 'measure-session'
$env:RBIT_WORKSPACE_CARGO_TREE = 'windows-job'
# Set RBIT_BUILD_CANDIDATE_HEADROOM_BYTES to the reviewed positive integer first.
node scripts/workspace.mjs measure-session measurement-plan.json
```

The threshold is required; there is no default and it is not a reservation or a
measured candidate-size estimate. Insufficient space refuses before allocation,
and free space is checked before each command. Do not use a token threshold for
real application measurements. A first real run should be deliberately bounded;
the example is a functional smoke, not acceptance of a compact profile or the
full build/check/Clippy/test/nextest matrix. The plan can express that full matrix
with warm repetitions once its actual cost/headroom has been reviewed.

Inherited flags, wrappers and configuration remain in force and are hashed. Each
arm applies only its explicit profile choices; this is not a canonical-profile
claim if common profile overrides exist. Mandatory SQLite QPSG remains present.
Only the two Cargo output overrides select fresh owned arm storage. Arbitrary
build scripts, wrappers and flags may use external storage/caches; that state is
unproved, not erased or reported as fully cold. No compiler cache, global Cargo
home, source, registry, runtime copy or application data is cleaned.

## Proof, lifecycle and report boundaries

The same global management lease spans allocation, every command, current source
admission, exact executable copying, usage observation and record publication.
Step order alternates arms by position (AB, BA, AB); each arm retains its own
command order and outputs. Individual steps are not parallelized. Normal builds
continue using the existing shared lane.

Every step uses the current source-proof begin/observe/finish path and its own
opaque invocation. Native receipt replay, missing/foreign proof and uncertain
execution stop the matrix. Source/provenance changes refuse before copies; no
unbuilt or unselected unit becomes trusted merely because a session succeeded.
Actual output copies use the ordinary build algorithm and independent byte hashes.
The session's extra seal covers Git source, manifests/lock, explicit tracked
absence and ignored public CSS, including file-identity/time stability to detect
edit/revert. Current inputSnapshot separately covers package ignored source/assets
and effective Cargo configuration. Neither is a hostile-local-writer sandbox.

Reports include source/policy hashes, fixed statuses, actual observed child exit,
per-command current source-proof/fresh/rebuilt counters, exact handoff hashes,
completed usage and command wall time. The timed interval starts immediately
before executor launch and ends after owned native completion/stdio drain; it
excludes source admission preparation and subsequent validation/copies/scans.
Native launcher startup is included. Peak memory is not measured. Nextest may not
forward compiler frames, so zero counters do not prove zero compiler work.

Raw selectors, arguments, paths, environment values and error text stay out of the
record. Source hashes and known Git revision are provenance, not readiness or
promotion authority. Source/config metadata proof limits remain those documented
by the shared helper. Fresh-output state does not qualify a fully cold build or
satisfy automatic retention's cold-cost gate.

A known stopped failure retains a failed candidate and stops all later steps. An
uncertain failure retains candidate/control evidence and the blocked global lease,
even if diagnostics or ledger publication fail. Records are bounded, partial
publication remains blocking, and no serialized plan/report resumes a session or
authorizes promotion, deletion or cleanup. There is no automatic retry or rotation.
A successful singleton candidate also blocks a new allocation; cleanup/recovery
requires a separately reviewed ownership-preserving executor.

## Validation and remaining storage work

Temporary Node fixtures exercise the real current source guard with injected
metadata/compiler frames and branded fake-native completion, including warm reuse,
source ABA, configuration changes, exact copies, known/uncertain failures and
receipt replay. These are not Rust/compiler/native execution measurements. Real
Windows smoke execution and representative full matrix/profile cost/storage
measurements remain pending. Unix-owned Cargo mode remains inactive pending its
separate hosted ownership proof. The planner's promotion/eviction executor, bounded
failed-candidate recovery, actual free-space/cold-cost policy and whole-lane
rollback/promotion acceptance still remain. The 16 GiB watermark is still soft;
this increment does not complete the bounded-storage goal or retire legacy bytes.


## Prepared first full-command smoke plan (not executed)

[`auth-profile-smoke-plan.json`](design/rewrite/measurements/auth-profile-smoke-plan.json)
is an exact bounded twelve-command plan: two profile arms, each with package-only
build/check/all-target Clippy/test/nextest-ci and a repeated warm build. It uses
only existing dev/test profile debug overrides, not the separately pending compact
profile. The source checkout and helper revision must be frozen and recorded
before execution. If importing the helper from a different checkout, record that
helper's separate Git revision/dirty state as well; the source seal describes the
selected source checkout, not an arbitrary external caller's already-loaded code.

After actual free-space review, the build owner can choose an explicit starting
headroom threshold (for example 8 GiB is an operator threshold, not an estimate)
and invoke the plan with the same command above, using this JSON path. Do not run
it while another compiler owner holds the lane or treat this prepared plan as
permission to remove an existing singleton. Its expected raw exit status is zero
for every step; any failure stops the remaining matrix and retains evidence.
The command remains unexecuted in this source checkpoint.
