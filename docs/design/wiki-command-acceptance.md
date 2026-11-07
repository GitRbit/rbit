# W2 command acceptance foundation

Status: source prepared, Rust unrun. This is a real SQLite acceptance operation,
not an active publisher, completion receipt, user flow or W2 completion claim.
Migration 0066 follows the reviewed 0063/0064/0065 prerequisites. No live catalog
was opened or migrated during preparation. Existing read SQL remains unchanged.

## Accepted is different from visible

An authenticated adapter supplies an exact fixed command identity, original global
head/incarnation expectation, path/title and bounded body. It never accepts a target
OID, publication flag, actor identity or refreshed expected head from an HTTP form.
After an uncertain commit/response, retain this original command identity and payload.

Numeric account IDs are reusable too. `wiki_actor_incarnations` allocates a separate
AUTOINCREMENT binding to each current account; account deletion cascades only that
binding, never its retained intents or sequence. Acceptance loads the current binding
in the same writer snapshot. Replay queries include it before fetching old payloads;
a recreated account with the same numeric ID/grants cannot adopt the old command.
New binding allocation happens after permission/archive/CAS/capacity checks and commits
with acceptance. It is namespace data, not authentication proof: network adapters still
require the platform's trusted authenticated issuer and applicable MFA/IP/token gates.
Recovery, pre-CAS and finalization must recheck the exact same binding inside their
fresh authority transaction. Historical attribution may link an account only while
its current binding still matches; do not resolve an old actor ID to a new user.

`WikiMutationStore::accept` begins an immediate transaction, loads current active
viewer grants/archive plus the current catalog incarnation/head and store identity,
uses the existing pure repository-access lattice, checks privacy/write/archive before
exposing replay metadata, and compares every
pending replay field/body byte. It admits one active intent per wiki and an explicit
host-supplied global pending cap. Capacity is tested before page allocation. The
host must validate the cap; there is no numeric production default or hidden queue.

Create allocates a page ID above both persistent allocation and catalog sequences.
The allocator row is removed in the same transaction while AUTOINCREMENT high-water
remains; rejection/rollback cannot make a previously committed allocation reusable.
Acceptance commits that allocation, exact command, original timestamp and prospective
generation together BEFORE any Git work. It does not insert visible pages/revisions
or move refs. An edit addresses an existing page and cannot silently rename its path.

The intent has no cascading parent FK: deleting/recreating a repository may not erase
its pending recovery evidence. The existing WikiId/incarnation boundary prevents old
work from publishing into the new catalog. Cleanup/recovery still must classify these
orphaned intents; this foundation does not perform it or make deletion crash safe.
Orphaned/quarantined work continues consuming global pending capacity until a proven
terminal transition; do not silently clear it to make room for new commands.

## Required remaining closed publisher

The next concrete coordinator must own bounded asynchronous workers before their
first SQL wait, retain ownership through waiter cancellation and drain before stores
close. It must reject unsupported durability modes BEFORE accepting network commands.
A dedicated native publisher root owner must exclude independent cooperating writer
processes; cloned service owners share its budget/lock. Startup reconciliation must
finish or quarantine existing work before readiness/new admission.

Native preparation must derive its target from the exact committed command and
persist it once, never from a client OID. A private prepared target/owner-bound receipt
must cover the required bounded commit/tree/blob closure (including content hashes),
retained roots, expected-ref CAS and successful platform durability operations. An
already-target ref still requires closure/durability verification. Any error after a
possible ref change is uncertain and retains the intent; it is not an abort receipt.
The selected page blob must ALSO match the accepted body bytes exactly; recomputing
an OID is not payload equality. Bind page/scope/path/title/actor/timestamp and target
metadata to the same accepted command, including when an object already exists.

Finalization must freshly recheck active writer grants/archive, same store/repo/wiki
incarnation, original expected head and exact intent/target. The matching transaction
updates visible history/page/head together. Only then may a request report visible
success. A later replay must compare the exact historical Git body and original
metadata before acknowledging success; a hash alone is not payload equality. Pending
body bytes may be cleared only after this durable visible transition, or after a
verified rollback establishes terminal Aborted. Completed
identity/history retention and quotas must be fixed before unbounded network writes.

If current permission/archive state defeats finalization after ref CAS, recovery
needs a verified target-to-original ref rollback and durability before marking the
exact intent Aborted. Drift/unknown outcomes remain Quarantined and block admission.
No such rollback/finalizer API exists yet. Pending/quarantined count toward capacity;
visible/aborted are terminal journal records, not active writers.

This first adapter deliberately returns `VisibleReplay` as an error rather than
pretending a missing replay body or parsed target ID is verification. No public target
setter/finalizer or caller-asserted publication proof exists in this atom.

## Native evidence and limits

Pinned gix 0.90.0 uses gix-odb 0.87.0 and gix-ref 0.69.1. The reviewed loose-object
writer flushes its compression buffer and persists the tempfile; ref commit moves
the locked file into place. Those paths do not perform file/directory fsync. Successful
native object/ref API calls alone cannot issue a durable-publication receipt.

Linux [`fsync`](https://man7.org/linux/man-pages/man2/fsync.2.html) requires syncing
the containing directory as well as file contents for directory-entry persistence.
The safe native adapter must check every required file/directory sync error and
explicitly bound its supported filesystem assumptions. Windows
[`FlushFileBuffers`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
documents file flushing and privileged volume flushing; this review has not established
a supported ordinary-user parent-namespace flush protocol. Windows publication must
remain typed Unsupported until a separately reviewed implementation/evidence closes
that gap. This is not a claim that Windows durable publication is impossible. Other
release platforms also need their own supported semantics; generic Unix is not proof.

Safe standard [`File::try_lock`](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
is a candidate for the held cooperating-publisher root owner. It does not stop hostile
filesystem/DB owners or replace native ref CAS, immutable scope checks and recovery.

## Prepared acceptance

Real SQLite fixtures cover exact retry preserving page/generation/time without Git
or visible rows, changed payload/title rejection, one-winner concurrent acceptance,
fresh private/archive precedence and global capacity before allocation. Pure tests
check command identity bounds and redacted Debug. Rust/native/AST/Clippy are unrun.
An additional real fixture deletes the highest account, recreates/regrants its reused
ID, and proves the original command is inaccessible while a new binding is distinct.
Run the domain crate and `rbit-services --test integration wiki_reads::acceptance`
through the shared helper after registration. Full publisher crash/target/replay/ACK,
platform durability, worker cancellation/drain and UI acceptance remain mandatory.
