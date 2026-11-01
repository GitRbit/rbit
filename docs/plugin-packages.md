# Plugin package storage and installation records

Authenticated host APIs and the `rbit plugin` CLI retain packages. Publisher
review, native workers and the sandbox adapter are subsequent stages. Installing an archive here never executes it or enables it.

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

## Passive review and selection contracts

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

These are passive request contracts. They create no review ledger, effective grant,
runtime selection or executable authority. Durable review coordination is the next
stage. Capacity or revision exhaustion must never leave old authority usable because
a deny cannot be recorded; the host needs a durable deny path or a persistent closed
execution/host-call gate that also revokes existing managed generations.

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
