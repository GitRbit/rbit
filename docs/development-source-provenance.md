# Shared compiler source provenance

Cargo's normal source freshness checks compare source mtimes against dep-info.
Sharing output directories across worktrees can therefore select another checkout's
metadata when the requested files have older timestamps. A global writer lease
prevents concurrent builds but does not fix that freshness decision. See Cargo's
[fingerprint implementation notes](https://doc.rust-lang.org/stable/nightly-rustc/cargo/compiler/fingerprint/index.html)
and [upstream shared-build-directory report](https://github.com/rust-lang/cargo/issues/17312).

The default shared helper now validates local-unit provenance under that same lease.
It keeps the existing lane, target directory and intermediate directory. It creates
no source mirror, new target tree or worktree-specific compiler cache. It never
changes source mtimes or runs `cargo clean`.

## Before a compiler command

Read-only Cargo metadata uses `--locked --offline --all-features`; it cannot fetch
dependencies or regenerate a lockfile. Local packages must have bounded, unambiguous
names/roots and inspectable targets/dependencies. The guard hashes actual package
files, including ignored generated public CSS/JS, canonical package/target roots,
local dependency provenance, manifests, lockfile and Cargo configuration/environment
policy. Shared Git-listed files outside package roots cover actual root-level
`include_str!` inputs such as deployment examples. Tracked deletion and sparse absence
are explicit tombstones, not silently omitted reads.

This shared-file policy is conservative: changing tracked documentation/helper files
can invalidate all local units. Different canonical checkouts also require rebuilding
local units even for equal text, since debug paths and `CARGO_MANIFEST_DIR`/debug assets
are part of provenance. Repeating an unchanged command in the same checkout can reuse
verified units. More selective dependency/input discovery needs separate proof and
measurement before relaxing this policy.

The guard discovers actual Cargo fingerprint directories in target/intermediate
layouts, including target triples and custom profiles. For every local package it
checks **all** stored variants, including feature/profile variants not selected by
the current command. Cargo-shaped control files and their small hexadecimal freshness
stamps are validated. Registry packages are not invalidated; ambiguous package-name
ownership or unsupported layouts fail closed.

Live evidence includes strict content hashes for fingerprint controls and dep-info,
plus file identity/size/mtime/ctime signatures for codegen companions of every size.
Small generated text companions may also carry bounded content hashes. Codegen kind
selection is deterministic; scan order or available hash budget never changes a
codegen file between content and stat modes. It covers hashed metadata,
primary/unhashed binary and library mirrors, build-script executables and generated
output. This detects an older helper replacing an artifact between our commands even
if it leaves our ledger and the small Cargo fingerprint hash unchanged. Large codegen
files are not repeatedly hashed in full.

Only derived primary `target/<profile>/*.d` exports use persistent content/size
signatures instead of rewrite timestamps. Cargo may refresh these external-build
integration exports on an otherwise fresh invocation; identical bytes are harmless.
Path/type/bounded-read and within-read stability checks remain mandatory. Changed
export bytes still invalidate provenance. Rustc `deps/*.d`, fingerprint dep-info,
fingerprint controls and codegen retain their strict identity/time checks. See Cargo's
[primary dependency-export implementation](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/compiler/output_depinfo.rs.html).

Unknown legacy units and changed/unproved units lose only their independently
validated small Cargo freshness stamp. Codegen, registry artifacts, source, binaries
and runtime copies remain. Cargo consequently rebuilds the selected dirty units;
unselected unknown variants stay unapproved and cannot be silently admitted later.

## Completion and failures

A pending ledger is written before invalidation. One opaque invocation is attached to
the native Cargo completion receipt. After successful execution the guard compares
source content and a separate file/directory identity/mtime/ctime stability digest.
Absent optional Cargo configuration also observes its nearest existing ancestor,
so a configuration file created and removed during compilation is detected. Ancestor
walking terminates at the filesystem root and has a depth bound.
An A→B→A edit during execution therefore fails even if the old content/mtime is
restored. Source instability fails the command **before executable handoff**.

Only emitted local compiler-artifact/build-script messages may create new approvals.
Fresh compiler messages require an already trusted unchanged unit. Newly rebuilt
messages need observed stamp recreation or changed compiler controls and unambiguous
target/profile/artifact matching. Cached build-script messages retain existing proof;
new runtime-script proof additionally matches the exact approved compiled-script
fingerprint dependency. Mere post-command file presence never approves an unselected
variant. Native failure, unknown completion or changed source approves nothing.

Scoped authenticated ledgers (`.source-proof.json` and its private random key) store
hashes and counters, not source contents, environment values or private paths. Live
unit signatures must still match. Records never supply filesystem mutation paths;
those come only from metadata plus validated Cargo layouts. Malformed/forged control
files refuse work. Signed ledger replay alone cannot hide changed artifacts or a
different source/root. This is not protection against a malicious local user who can
replace helper code, steal its key or bypass the writer lease.

Existing ownership defaults remain unchanged. Explicit `windows-job` execution can
approve units after verified native tree completion. Without that proof, the command
reports `trusted:false` and no new unit approvals; subsequent commands conservatively
invalidate untrusted local freshness. Missing/foreign proof in owned mode retains an
uncertain blocked lease, even when ledger diagnostics also fail. No potentially active
artifact tree is scanned after such a failure. Native ownership on other platforms is
a separate activation task.

`generate-lockfile` remains a noncompiler writer under the lease. It does not require
already-valid locked metadata and cannot approve compilation units. The next compiler
command observes the changed lock/graph before reuse.

The helper exposes bounded `sourceProof` counters (`trusted`, `approved`, `invalidated`,
`reusable`, `fresh`, `rebuilt`). No whole cache generation is thereby approved for
retention, promotion or eviction. This guard is separate from the default-off
multi-command measurement/compaction work.

## Limits and verification

Inputs are bounded to 256 local packages, 4,096 graph packages, 40,000 source entries,
32 MiB per input and 512 MiB total. Layout/control/artifact scans and ledgers have
independent entry/byte bounds. Links/reparse traversal, special files, unknown layouts
and unstable observations refuse work. Build scripts or wrappers reading arbitrary
undeclared inputs outside these observed sources are not a general hermetic-build
guarantee; ignored root-level external includes require additional declared input
discovery. This is a Cargo cache correctness guard, not a compiler sandbox.

Mandatory control hashing remains capped at 128 MiB per scan and 8 MiB per file.
Generated-text hashing has its own 128 MiB scan budget and a 64 KiB per-file threshold;
larger/generated binary outputs use identity/stat observations. Codegen never consumes
the control-hash budget, including caches with many large feature/profile variants.

Temporary tests use three actual Git worktrees with backdated sources, multiple
feature/profile variants, old-helper overwrites preserving mtimes/controls, large
companions, ABA source writes, external includes, sparse/deleted tracked inputs,
unhashed binary/library outputs, warm build scripts, forged/replayed proofs and
failure precedence. Symlink leaf tests can skip when Windows privilege is unavailable;
existing path tests cover Windows junction ancestry. No real Rust compilation or
production-cache mutation was performed while preparing this source change. Native
three-worktree Rust reproduction and actual application checks remain required.

## Fresh primary library exports

Cargo can create a primary library hardlink after compiling that library earlier as
a dependency, without rerunning rustc. Creating the link changes the existing
artifact's ctime. The guard permits this transition only for an observed fresh unit
that was already trusted before the command, with unchanged controls and sources.
Before Cargo starts, it hashes plausible selected library artifacts missing their
known primary export (at most 32 MiB per file and 64 MiB total). After verified native
completion, the new export must have the exact same file identity as the existing
artifact, whose bytes must match the prior witness. Missing companions, independent
copies, unrelated changes and same-size writes with restored mtimes are refused. A
new bounded primary `.d` export may accompany that proved hardlink. An unobserved
transition is never approved.

Witness bounds or an existing primary export with a different identity invalidate
only the affected unit's small freshness stamp before Cargo, requiring a genuine
rebuild. They never delete codegen output. All between-command ctime comparisons
remain strict; this is an invocation-local derivation, not a relaxed ledger format.
