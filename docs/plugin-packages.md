# Plugin package storage and installation records

Authenticated host APIs and the `rbit plugin` CLI retain packages and record
explicit administrator review and candidate selection. Native workers and the
sandbox adapter are subsequent stages. Installing or reviewing an archive here
never executes it or enables it.

`rbit-plugin-api` defines bounded declarations for data, native and Wasm packages.
Requested capabilities and resource budgets grant no authority. Portable payload
names are metadata; neither plugin IDs nor versions become archive-store filenames.
The [archive profile and staging contract](../crates/rbit-plugin-packages/README.md)
define strict validation, filesystem preconditions and publication receipts.

## Host coordination

The composition root shares one `PluginPackageService` across `Services` clones.
Construction opens no package directory and makes no package-ledger query. Its
fixed root is `<data_dir>/plugins/packages`; the host must durably provision this
private directory and protect its ancestors and Windows ACL before use. Package
operations never create or relax that root. On Unix its permission bits must be
0700. Callers cannot supply a different object-store root.

For Unix local setup, stop the server and run `rbit package-store provision` as its
service account using the same deployment configuration, then restart after the
command completes. `rbit package-store check` inspects current permissions without
opening databases. Provision creates only missing private components and synchronizes
their parent entries; it never repairs existing modes or paths. Ownership and stable
trusted ancestors remain deployment prerequisites. See [local CLI setup](cli.md#local-package-store-setup).

Installation requires an opaque operation token chosen by the authenticated host
administrator or CLI before upload, never by package/plugin metadata. The token
grants no authority and is never a filesystem name. Two
operations can own admission at once; duplicates and overload fail immediately.
The deadline is capped at 60 seconds and the wire quota at the archive maximum.
These checks are cooperative: callers must additionally enforce deadlines on
blocking source reads. A file read or filesystem synchronization cannot be forcibly
interrupted by dropping its async waiter.

The owner task retains its admission and shutdown-tracker token through real
blocking work and journal settlement. Dropping a waiter requests cancellation;
it does not abort a possible filesystem or SQLite commit. Shutdown terminally
closes admission, cancels cooperative work and drains owners before closing core
services and stores. No background package task is started when packages are unused.

The sequence is:

1. Stream validation, archive hashing and writing into the same private inode;
   synchronize it without extracting payloads.
2. Recheck current administrator authority and durably record the exact descriptor,
   actor and operation token before publication.
3. Publish without replacement and inspect the actual receipt. Never infer rollback
   from an error or delete an object that might have become visible.
4. Only after confirmed durability and successful staging cleanup, atomically pin
   the immutable object, plugin ID/full version and operation completion.

No database transaction spans hashing or filesystem synchronization. A completed
replay still validates supplied bytes and verifies publication; matching metadata
alone is insufficient. A different digest for the same plugin ID/full version
conflicts. SemVer build metadata remains part of the stored identity. A failed or
uncertain SQLite commit is resolved through its operation token; the coordinator
never deletes the archive or blindly overwrites possible completion.

## Durable journal and authority

Migration `0063` adds immutable archive objects, version pins and revision-fenced
operations. Stages distinguish pending, awaiting durability, complete and static
error classifications. Reusing a token requires the same descriptor and actor.
Retries preserve progress and use revision checks. The journal preserves the actor
and timestamps; the host also writes its ordinary administration audit entry.

The ledger uses a dedicated lazy, single-connection `synchronous=FULL` writer. The
ordinary request pool retains `NORMAL`; package operations do not change pragmas
on shared request connections. Both pools close during database shutdown. This
assumes a filesystem and storage stack that honor synchronization requests.

Bounds are 1,024 objects, 1,024 version pins and 4,096 operation records, with
128-row recovery/inventory pages and bounded strict manifests. Capacity exhaustion
fails closed. Automatic history pruning, object garbage collection and complete
crash reconciliation are not implemented; callers must not delete ledger objects
or staging remnants behind active operations.

An installation record is **disabled and unreviewed**. It creates no trust grant,
approved capability, runtime selection, enabled intent, route or worker. Embedded
Actions/i18n desired state remains separate. A digest establishes byte agreement,
not publisher identity or code safety. Trust and effective limits must later bind
to the exact installation digest and host policy revision.

## Review and candidate selection

The SDK defines strict review and candidate-selection request values with exact
plugin ID/full-version/archive-digest identity. Full SemVer build metadata remains
part of identity. Revisions and repository subjects use canonical positive decimal
strings within the signed 64-bit range, preserving exact browser JSON precision.
Grant requests are bounded to 128 unique explicit capability/scope tuples and sorted
canonically for replay comparison. Hosts must cap complete input at 64 KiB before
parsing and revalidate programmatically modified requests.

Clearing a candidate also requires an explicit revision and host policy expectation.
The host must retain and advance a cleared selection's revision; clearing never
turns it back into never-created state. Policy fingerprints must include a persistent
maintenance epoch so requests from before deliberate repair cannot regain validity.

Review intent explicitly distinguishes denial, data-only approval, native full
service-user trust and Wasm-sandbox-required approval. Native code with that OS
identity can access host files, processes and sockets outside a broker; broker
grants cannot sandbox it. Wasm intent never permits a native fallback. Declaration
checks compare supplied manifest identity/kind/requested capabilities; the host must
resolve the actual durable archive, current policy, recognized catalog, actor and
live resource incarnation before any mutation or admission.

Migration `0064` retains administrator review history, current review heads,
candidate-selection heads, operation receipts and a persistent policy gate. The
`Services.package_policy` facade exposes explicit initialization, inspection, review,
selection, clearing and operation recovery through bounded administrator HTTP
routes and `rbit plugin policy` commands. Upload commands only install bytes;
review and selection are separate explicit requests.

Initialization requires a current active administrator and a virgin empty policy
ledger. It generates a persistent random epoch and host-owned fingerprint. Reads
and construction never initialize policy. Inspection reports the exact installed
package's declared kind, capability requests, host API and budgets, without its file
inventory. The enforced capability catalog is empty: every nonempty grant request
is rejected. Requested limits remain declarations, and every candidate reports
execution **Unavailable**, including approved native or Wasm packages.

Policy mutations use the existing lazy FULL writer. Active administrator authority,
exact installation identity, fingerprint and current revisions are checked in the
same short write transaction. A selection must name the current allowed review;
revoking it leaves the old selection inspectable but invalid for future admission.
Review/selection/clear operations use caller-known keys and canonical bounded
receipts. Exact replay acknowledges history without restoring old heads. Bootstrap
has no operation receipt; after a lost response, inspect the authoritative gate.

Each epoch permits 4,096 reviews, 4,096 ordinary receipts and 1,024 heads of each
kind. A valid mutation that would exceed a bound or exhaust a revision commits only
a persistent epoch seal and its bounded provenance, then reports that seal. It
changes no review, candidate or receipt. Invalid/stale requests cannot seal policy.
The seal survives restart and ordinary review/selection APIs cannot reopen it.
Explicit backend renewal below creates a fresh epoch while preserving the sealed
one. Future adapters must fence starts, existing generations and host-call admission
before any live integration is accepted; this metadata stage proves no worker
revocation.

Two owned policy mutations may run at once. A dropped waiter leaves the transaction
owner intact, and shutdown closes admission and drains it before stores close.
Unused policy starts no worker, timer or query. Embedded Actions/i18n composition,
enabled intent and presentation remain separate. Backup validates these bounded
records in its immutable SQLite snapshot and preserves the epoch and seal; restore
never executes a candidate.

## Bounded backend epoch renewal

Migration `0065` retains at most three policy epochs: one current and two retired.
At most two successful renewals fit this fixed retention budget. Within each epoch
the ordinary history/receipt/head bounds above remain unchanged. Renewal is finite
policy-capacity recovery; it does not reclaim archive object/version/installation
operation slots. Export, pruning, deletion and unlimited epoch rotation are not
implemented. A request beyond the retention budget leaves the current gate sealed.

The strict SDK `PackagePolicyRenewalRequest` carries the observed host fingerprint
and seal operation key, with a complete 1 KiB request bound. A caller supplies a
separate known operation key before invoking the backend. The active administrator,
exact sealed fingerprint/seal key, retained graph and global operation-key identity
are checked inside the FULL transaction. The host generates fresh epoch entropy
outside that transaction; it atomically retires the sealed epoch, records the
immutable renewal receipt and publishes an empty current epoch with a new
fingerprint. Current descriptor version 2 binds the finite policy limits; migrated
version 1 fingerprints and canonical historical bytes remain unchanged.

Review heads, selection heads, grants and enabled intent do not carry into the new
epoch. Inspecting the same archive initially finds no current review or selection;
a new decision needs its new fingerprint and revision expectations. An old unrecorded
request remains stale even if it expected `Absent`. Retained ordinary and renewal
receipts remain readable. Accepted exact replay reports history without applying
it again; a sealed current gate still rejects ordinary mutation replay. Policy
ordinary receipts, renewal receipts and rejected-capacity seal keys cannot be reused
for another command across retained epochs. A new epoch never inherits native trust
or supplies a Wasm-to-native fallback.

`Services.package_policy.renew` owns exclusive maintenance admission against the
ordinary policy mutation owners; it refuses overlap rather than running competing
rotations. Losing a waiter leaves that owner tracked through commit, and shutdown
closes admission and drains it before stores close. Immutable renewal receipts are
recoverable through the backend `renewal_operation` method. After an uncertain
response, inspect that receipt and the current gate before an explicit retry with
the same actor, request and key; missing receipts never establish rollback.

Maintenance renewal is **not exposed through HTTP or the CLI yet**. Existing
`rbit plugin policy` commands and HTTP review/select/clear routes remain unchanged;
the ordinary operation endpoint does not expose renewal receipts. The backend
mechanism grants no executable authority: execution remains unavailable for every
package, nonempty broker grants are rejected, and native workers/sandbox enforcement
remain subsequent stages.

Backup format 5 validates both the frozen five-table `0064` graph and the complete
seven-table `0065` graph, rejecting partial schemas even when migration markers are
missing. All retained epochs, renewal links, global keys and bounded ordinary
histories belong to the same immutable SQLite snapshot. Retired archive references
remain retained; shared object bytes are copied once per digest. Restore preserves
old audit and current policy without executing or enabling a candidate.

## Recovery boundary

A crash before publication can leave private staging remnants and pending intent.
Publication followed by metadata failure can leave an unreferenced immutable
object. Recovery enumerates incomplete records without executing or fetching code.
Explicit retries revalidate supplied archives; corrupt existing objects are never
overwritten and a version is never silently repinned.

On Windows, matching existing bytes do not prove namespace durability. A matching
prior durable installation record can establish the previous host assertion;
otherwise the coordinator retains `awaiting durability` and reports that outcome
without installation success. Fresh-object reconciliation/quarantine under an
exclusive maintenance barrier remains subsequent work. Existence alone is never
promoted into confirmed durability.

## Backups

Backup format 5 inventories package objects from the immutable SQLite snapshot,
including retained objects without a selected runtime. It validates version pins
and copies each digest once, through bounded buffers, checking exact size, SHA256
and actual EOF. Source links and unsafe object names are rejected. Missing or
corrupt retained objects fail before the final backup manifest is written.

Pending operations assert no durable archive, so their private/unreferenced files
are omitted. Restore retains journal state without approving trust or activating
code. The initial append-only store avoids backup/garbage-collection races; future
GC requires explicit coordination. See [deployment and restore](deployment.md#backups).

## Administrator HTTP transport

`PUT /api/v1/admin/plugin-package-operations/{operation_id}` accepts the canonical
uncompressed archive as `application/octet-stream`, with absent or `identity`
content encoding. The operation ID contains 1–64 lowercase ASCII letters, digits
or hyphens. Choose and retain it before upload to inspect work after a lost
response. Both PUT and GET at this URL require an Admin-scoped personal access
token belonging to an active site administrator; session cookies and workflow or
runner credentials do not authorize these routes. There is no inventory endpoint.

Uploads have an absolute 60-second deadline and a fixed wire-byte ceiling,
including chunked bodies. Validation and staging are incremental; the transport
adapter may retain one inbound frame alongside the fixed archive I/O buffer.
It yields between frames to keep deadlines/cancellation observable even for
empty frames. HTTP trailers are ignored and supply no metadata or authority.
Dropping the request waiter cancels its body reader and requests coordinator
cancellation; it cannot abort an in-flight filesystem synchronization or commit.
Errors and timeouts after valid ID parsing include the operation URL. A response
lost after publication must be resolved through that URL or host reconciliation,
not interpreted as rollback.

PUT returns 200 for durable completion or 202 for awaiting-durability status. GET
returns a compact journal record with stage, revision, static error classification,
package ID/version, archive digest and byte size. GET 404 means no journal row
yet: staging precedes the journal, so this does not prove there is no active work.
Completion still changes no enabled intent, trust decision, capability grant or
runtime selection. The host must provision the private durable root before use;
these routes accept no filesystem path and never create or relax that root.

Windows HTTP uploads currently fail 503 before polling the archive body because
the HTTP host has no supported root/ACL verification path. Authentication alone
cannot establish the private-root prerequisite. GET journal inspection remains
available. The low-level Windows store/coordinator APIs still require a trusted
host to establish their documented root preconditions; a marker file or matching
archive bytes do not prove private ACLs or durable provisioning.


## Administrator policy transport

Every policy route requires an Admin-scoped PAT belonging to a current active site
administrator. Mutating handlers reload live administrator authority before polling
JSON bodies; the owned transaction rechecks it with identity and revision fences.
Session cookies, runner credentials and workflow task tokens do not authorize these
routes. Successful and structured seal results report `execution: "unavailable"`, including
native full service-user trust reviews and selected candidates. No broker grant,
enabled intent or executable activation is issued.

The exact policy and package-operation route patterns bypass optional request
hooks so an extension cannot hide recovery receipts or block host policy changes.
Host authentication and transport guards still apply; unrelated API routes remain
subject to hooks.

Paths below are relative to `/api/v1`:

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/admin/plugin-policy` | Inspect the gate without initializing it. |
| POST | `/admin/plugin-policy/prepare` | Explicit bootstrap with `{"schema_version":1}`. |
| GET | `/admin/plugin-policy/candidates/{plugin_id}` | Inspect selected/cleared state and its current review. |
| GET | `/admin/plugin-policy/packages/{plugin_id}/versions/{full_version}/archives/{digest}` | Inspect declarations of an exact installed archive. |
| PUT | `/admin/plugin-policy-operations/{operation_id}` | Apply a strict review, select or clear request. |
| GET | `/admin/plugin-policy-operations/{operation_id}` | Recover an immutable receipt for a retained key. |

PUT accepts `{"command":"review|select|clear","request":...}`, where `command` is
one of those three strings and `request` is the corresponding strict SDK request.
Choose the 1–64 character lowercase ASCII/digit/hyphen operation key before PUT.
Each request carries the expected host fingerprint and exact revision expectations;
review/select also name the exact installed full version and archive digest. The
current host accepts only empty grants. Native approval requires the explicit
`native_full_service_user_trust` intent; Wasm approval requires
`wasm_sandbox_required` and never permits native fallback. These decisions record
metadata, not a sandbox or permission to run.

Use exactly `application/json` with absent or `identity` content encoding. Complete
JSON bodies, including unknown-length/chunked input, are capped at 64 KiB before
parsing, with a 10-second body deadline inside the 25-second mutating-handler
deadline. Empty ready frames yield cooperatively. Compact responses are capped at
64 KiB. They exclude canonical requests, complete archive file inventories and
internal storage errors. Exact-package inspection exposes declared kind, host API,
capabilities and budgets, with an empty recognized-capability catalog. Candidate inspection echoes
`plugin_id` even when no selection exists.

An `applied` 200 records the requested metadata. A `historical_receipt` 200 acknowledges
the original receipt without applying it again or restoring an old allow. Receipt
summaries retain the original review intent or selection's expected review revision;
operation GET labels receipts historical. Query current candidate/gate state
separately: an old native allow receipt does not override a later deny. Separate
receipt and gate queries are not one atomic snapshot.

A structured `capacity_sealed` 409 means the permanent seal committed while the
requested review/selection/clear was rejected; no ordinary receipt was created.
`gate_sealed` 409 reports an already closed gate. Neither outcome reopens policy.
Missing receipts, errors and lost responses never establish rollback. Retain the
same key and inspect both operation and gate before an explicit retry; the CLI
never retries automatically or follows a returned URL. Prepare has no receipt:
recover its outcome through gate inspection. Dropping a policy HTTP waiter does not
cancel its admitted transaction owner, which remains tracked through commit.
