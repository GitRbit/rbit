# Proposed Windows namespace durability adapter

Status: API/safety contract for review BEFORE authored FFI or dependency changes.
This is a usable ordinary-Windows implementation proposal, not an Unsupported
endpoint, a generic renderer/process framework or an assertion of native proof.
Existing W1 managed preparation does not yet implement this protocol.

## Placement and dependency decision

Prefer one small `rbit-platform` crate providing safe filesystem primitives. It has
no auth/SQL/HTTP/domain dependency and no Tokio/runtime task; callers own blocking
admission and lifetimes. Git owns closure/CAS/publication policy above it. The crate
is a genuine platform boundary, not a separate crate per API/type.

Use the EXISTING locked `windows-sys = 0.61.2` bindings, target-Windows only, with
Win32_Foundation, Win32_Storage_FileSystem, Win32_System_IO, Wdk_Storage_FileSystem,
Wdk_System_SystemServices. These modules exist in that exact cached release. No new
registry package/version is required. Windows-link is already transitive. Normal
Linux implementations use std/rustix safe APIs and do not compile Windows FFI.

The whole workspace keeps `unsafe_code=forbid`. This one manifest would explicitly
opt into reviewed local lint policy instead of inheriting that forbid, with
unsafe_code=deny and unsafe_op_in_unsafe_fn=deny. Only named private FFI modules may
locally allow unsafe; all other workspace Clippy/Rust lints are retained explicitly.
A source guard must reject any other exception/module or unsafe export. No unsafe
trait impl, public raw handle/pointer argument or unchecked user buffer is needed.
Approval covers only the reviewed file/call list; adding calls requires review.

Stable std has no held-handle NTFS/remote classifier or WT rename. Existing safe
winapi-util supplies handle IDs but not this missing closure. Atomicwrites 0.4.4 is
a public WT-rename precedent, but its immutable published release has not been
audited here (docs.rs source fetch failed), and it does not provide the full volume/
bootstrap contract. Depending on it alone would not close this gap. Prefer the small
auditable binding adapter over a new broad sysinfo/NTFS/parser dependency.

## Safe API surface

Proposed concrete types/methods (names may change during reviewed implementation):

- `OwnedDirectory::open_local_ntfs(path)` opens an actual synchronous directory
  handle, checks directory/reparse identity, classifies through that handle and
  retains it. It is not a durability receipt from merely opening an existing path.
- `OwnedDirectory::create_child(name)` creates a private exclusive staging directory
  and durably publishes that one binding with WT move; returns another owned handle.
- `OwnedDirectory::stage_file(name)` returns an opaque owned same-volume create-new
  file with write-through settings; stream native Git header/compressed bytes into
  it, finish the codec and `sync_all` before publication. No pack/body buffer required.
- `publish_new(staged, name)` performs a no-replace WT move; an existing target is
  separately verified exact and is never overwritten for immutable objects.
- A distinct mutable-ref replacement operation uses WT|REPLACE_EXISTING only after
  Git's actual expected-ref lock/check and owner scope. It is not object publication.
- `retire_ref` WT-moves the expected live ref into an owned trash namespace to prove
  durable live-name absence. Ordinary DeleteFile/remove_file is not that proof.

Constructors/children use private generated component names, reject separators/NUL/
ADS/trailing-dot-space/Windows device aliases and never accept a wiki title as a
filesystem path. Root config may contain Unicode; preserve OsStr/WTF-16 losslessly,
use bounded NUL-terminated buffers and never convert with lossy display text.

No method returns Wiki prior-head/visible authority. Native Git privately combines
actual platform operation results, exact object closure and SQL command identity.
Holding any scope/capability keeps the real root/catalog resources through queued
work, result destruction and final SQL waits. No guard spans an asynchronous SQL wait.

## Held-handle local-volume classification

Create owned synchronous handles through safe std OpenOptionsExt (backup semantics,
open-reparse-point, explicit sharing; no FILE_FLAG_OVERLAPPED). Refuse observed
reparse points and keep root/critical parent handles excluding external delete/rename.
Do not accept arbitrary caller-opened potentially asynchronous handles as this type.

Use `GetVolumeInformationByHandleW` on the SAME held handle, require an exact bounded
NTFS name and a writable supported volume. This reports the filesystem of the file,
not an inferred drive letter ([Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationbyhandlew)).

Query `NtQueryInformationFile(FileIsRemoteDeviceInformation)` on that synchronous
handle. Require STATUS_SUCCESS, exact returned byte count and IsRemote=0; a failure,
unsupported information class or noncanonical result refuses the mode. Microsoft
documents IsRemote as whether that file's filesystem is remote
([structure](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_file_is_remote_device_information)).
An optional additional FileFsDeviceInformation check can reject non-disk/virtual/
removable/WebDAV device classes; it is not a substitute for the direct remote fact.

This Native API has an existing windows-sys binding. Microsoft explicitly documents
the Nt name for user-mode volume queries
([Native API](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntqueryvolumeinformationfile))
and exact query buffer lengths for file information
([file API](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntqueryinformationfile)).
Check actual NTFS/UNC/mapped-share results in native fixtures; no error-as-local fallback.
Logical local classification cannot prove hardware/backing-media honesty; that is
an explicit OS/filesystem/device contract, including virtual-storage limitations.

Same-volume publication uses the same owned parent lineage AND omits COPY_ALLOWED;
Windows must refuse cross-volume movement. A matching serial alone is not a global
volume identity proof. Reparse/static-topology and exact file identity checks still
apply before/after operations. No administrative volume handle/volume-wide flush.

## Memory safety obligations before FFI merge

Keep every borrowed handle, buffer and UTF-16 path alive for the whole synchronous
call; do not close handles borrowed from std File or transfer ownership accidentally.
Use binding-defined repr(C) output structures, initialized integer/byte buffers and
checked size conversions. Validate status and returned lengths BEFORE interpreting
outputs. BOOLEAN is a byte, not a Rust bool. Do not read an inactive union member.

Synchronous-handle creation must be proved from the actual std/native open flags.
In particular, an unexpected STATUS_PENDING must NOT merely return while a native
request could still access stack buffers. Either the documented synchronous contract
excludes it or a reviewed bounded pending-request retention/drain protocol keeps
heap-backed buffers AND the actual handle alive. This is an FFI safety review gate,
not an error branch to handwave. Tests of fake status values do not prove native
completion. Cancellation occurs outside these blocking calls; real owned work drains.

Error conversion returns bounded typed codes/context, no private content/path logging.
No public caller supplies flags to accidentally enable delayed reboot/copy/overwrite.
Source checks pin the actual constant combinations at each call site.

## Cover every new namespace binding

`MoveFileExW` WT reports completion of disk movement; never set COPY_ALLOWED or
DELAY_UNTIL_REBOOT. Immutable destinations never set REPLACE_EXISTING
([move contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)).
CreateFile write-through additionally documents NTFS metadata flushing caused by
requests, including renames
([caching contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew#caching-behavior)).
These are conditional API contracts, not a hardware power-loss experiment.

Own ALL newly created root/bootstrap parents, scope directories, object fanout,
markers, refs/locks and trash bindings. Walk from a defined already-durable host
storage base, publishing each new directory binding through the adapter; do not use
ordinary create_dir_all then infer proof from reopening/flush of one leaf file.
The host base itself needs an owned bootstrap producer or an explicit established
durable deployment boundary; a parsed caller path/bool/marker cannot mint that fact.
Fresh-install host storage creation must join this same reviewed startup protocol.

For new bare wiki repos, own the small required Git layout/config/HEAD/marker files
instead of calling gix::init_bare and assuming its directory creation was WT. Preserve
the approved flat numeric .md tree and exact incarnation markers; no draft format is
silently adopted. Stream loose objects with the pinned public
`gix::zlib::stream::deflate::Write` codec (gix 0.90.0/gix-zlib 0.4.0 already locked),
then use owned file publication rather than gix's ordinary persistence rename.

Existing prepared/restore namespaces are not durable merely because their marker is
valid. Under stopped admission, invalidated epoch and actual leases, validate them and
rebuild into a separately owned WT namespace or use an explicitly reviewed rebind
protocol before any cap. Root switching also needs a stable parent-adjacent exclusion
and durable recovery intent; an inside-the-root lock alone cannot cover a temporary
rename-away gap. Preserve original bytes/evidence until the replacement is durably
selected. This restore/bootstrap producer is mandatory, not a post-hoc fsync shortcut.

Normal edits publish at most the new blob/tree/commit, required new fanout bindings,
and the expected ref. Reuse owner-scoped proven old immutable closure. Do not copy/
reflush all history per edit. Native current-head ref CAS and initial-ref rollback
must both use the owned namespace API before SQL visibility or terminal abort.

## Review and implementation sequence

1. Approve isolated crate/lint exception, exact safe API, synchronous NT query safety
   and already-durable-base/bootstrap boundary. No FFI source before this decision.
2. Implement narrow wrappers and deterministic status/bounds/name/flag tests. Audit
   every unsafe block independently; build x64 Windows and supported cross targets.
3. Run unprivileged actual local NTFS positive tests, UNC/mapped/unknown-filesystem
   refusals, cross-volume no-copy failure, existing equal/corrupt object preservation,
   staged-file failure, missing-parent bootstrap and ref replace/retire evidence.
4. Integrate actual managed bootstrap/immutable publication and bounded startup scan.
   Native fault fixtures interrupt every boundary and reopen/reconcile original
   request/intent. No benchmark/performance adoption claim from mocked calls alone.
5. Measure cold/warm startup, three-object edit overhead, memory, artifact/dependency
   footprint, restore RTO and fault-state resource retention against agreed budgets.
   Only the closed reviewed bundle may expose positive Wiki caps/publisher routes.
