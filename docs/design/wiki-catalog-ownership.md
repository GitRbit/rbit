# Actual catalog ownership through native preparation

Status: source prepared, Rust/native unrun. This adds concrete ownership composition,
not a prior-head capability, durable publisher, host readiness or enabled wiki route.
`WikiStorageOwner` is not yet installed into the central Services constructor; that
and actual duplicate-Services activation tests remain part of the closed W2 bundle.

## Bind the actual opened catalog

`Database::wiki_catalog_lease` reads main filename and WikiStoreId from the actual
live pool, checks its connection options agree, canonicalizes the actual file and
holds a separate subject handle. It derives the adjacent lock name internally from
that path. There is no caller-supplied lease filename or object-root-based substitute.
No pooled connection is parked: the lease retains only the pool handle, subject,
paths/observations and real OS lock. A one-connection fixture tests continued queries.

Observed symlinks/reparse points and hardlinks are refused for subject/lock paths.
Linux statx MNT_ID compares the held regular file and its parent to reject a file-only
bind mount that could otherwise put the adjacent lock in a different directory.
Missing mount-ID support fails closed; directory bind aliases naturally reach the
same physical adjacent lock. This is not a network/local-volume durability classifier.

Windows opens held subject/lock handles sharing read/write but denying delete/rename.
Already-locked winapi-util 0.1.11 supplies safe handle information/link counts; the
unstable std MetadataExt by-handle methods are not used. Its 64-bit file IDs are
observations, not uniqueness/durability proof on unknown/ReFS/network volumes. A
positive Windows publisher still needs actual local-NTFS/same-volume classification
and owned namespace publication. Drive letters alone are not such evidence.

The check detects observed replacement, closure, option/CWD disagreement and aliases;
it does not claim protection against hostile/concurrent topology ABA or unsupported
platform namespace semantics. All positive release-platform modes must reject cases
outside their verified contract before admitting publication. Manual DB/key/sequence
rewrites and uncoordinated live restore are not supported operations.

## Real worker lifetime, not only the outer future

The pure WikiCatalogOwnership I/O port is a trusted composition contract, not a
cryptographic or authentication capability. Its production implementation has no
public field constructor and owns actual resources. A hand-written port does not
authenticate a user or make a parsed ID a receipt.

`WikiStorageOwner::open` acquires this catalog lease BEFORE creating/opening a chosen
object root. It then binds the native managed owner to that same catalog identity.
Separate pools/compositions of the same actual catalog cannot choose a second root
to bypass the supported namespace lock. Clones share the held owner.

The native preparation job captures the catalog owner itself, checks its binding
inside the admitted blocking worker before/after I/O, and returns it with the private
prepared value. No filesystem check is added to the asynchronous request executor.
Dropping a queued/running awaiter or outer controller cannot release the catalog
exclusion while native cleanup remains independent. No mutex guard or SQL transaction
is held across filesystem work, awaits or future publication.

The prepared value still proves neither current authorization nor durable visibility.
The future coordinator must load the exact committed intent, require unchanged actor
incarnation/current grants/archive, and perform the validated-head/ref/SQL protocol.

## Dependencies and prepared tests

Services now uses the pure wiki dependency normally instead of only from read fixtures.
Storage adds target-only direct edges to already locked winapi-util 0.1.11 (Windows)
and rustix 1.1.5 fs/std (Linux). No registry version/new registry package, unsafe code,
new workspace member, runtime task or numeric policy default is introduced. The
winapi-util release manifest is Unlicense OR MIT; reviewed source wraps the native
handle query safely. Rustix is the existing pinned safe system-call dependency.

Prepared real fixtures cover separate pools and different-root exclusion BEFORE any
second-root creation, prepared-value retention, real queued worker cancellation and
catalog lock release only after actual result destruction. Storage unit tests cover
one pooled connection, Unix hardlink/replacement refusal and Windows subject-handle
rename exclusion after SQL pool closure. Linux privileged file-bind and mapped-drive/
volume classification fixtures remain native acceptance gaps, not asserted passes.

Run storage unit ownership tests and services `--test integration wiki_reads::ownership`
plus prior wiki suites through the shared helper, then strict Clippy/AST/platform CI.
All new types are explicit component targets. No Cargo/metadata/native tests or user
data access occurred in this source preparation.

## Next closed protocol

The next stage must hold these catalog/root leases during bounded/resumable actual
integrity validation and invalidate capabilities before restore/GC. A private bounded
head map may mint owner/epoch-scoped capabilities only from that validation plus a
supported platform durability fence. Reserve map entries before enable; include
pending/orphaned/quarantined scopes in accounting and never silently evict proof.
Normal edits extend proven immutable state with only new blob/tree/commit and ref
bindings, rather than rehashing all current/history bytes. Ref CAS, durable namespace
operations, SQL visible receipts, uncertain ACK recovery and verified abort/rollback
remain required; this owner creates no shortcut around them.

The current general SQLite pool uses WAL with `synchronous=NORMAL`. Its successful
transaction commit is not a power-loss durability receipt. SQLite documents that
NORMAL WAL transactions can roll back after power loss, whereas FULL adds the WAL
sync at each commit ([synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous)).
Before publication, wiki acceptance/target/finalization writes need an explicitly
owned FULL-synchronous connection established from the actual catalog options and
verified outside a transaction. A dedicated connection avoids changing unrelated
request workloads or returning a pooled connection with altered settings after
cancellation. Opening/closing that connection must retain the same catalog owner;
all supported writer-compatibility registration and platform sync assumptions still
apply. This source atom changes no SQLite synchronization policy and mints no such
receipt.
