# Repository transfer and old-address applications

`rbit-repositories::transfer::RepositoryTransfer` owns the transfer command's
administration/confirmation/destination/suspension decisions, the owner suggestions
and its compensating operation ordering. Non-administrators resolve the destination
through the shared `creation::RepositoryOwnership` grant directly. Persistence is the
`TransferRepository` port, implemented by `rbit_storage::TransferStore`, which borrows
the database and opens the same store handle per call as before (repositories for
owner lookup and the owner-change transaction, accounts for suspension, organizations
for suggestions) and maps a uniqueness conflict to `TransferWriteError::Conflict`.
`TransferService` preserves its public API; its borrowed `TransferBridge` supplies
only request effects (reader close, disk relocation, event and audit). The
suggested-owner picker keeps its single membership vector and lazy projection.

`RepositoryAliases` owns current-address precedence, Git `.git` stripping, old-address
fallback and permission-filtered redirect publication. Address lookup is the
`RepositoryAddresses` port, implemented by `RepositoryStore`; `AliasView` borrows the
exact existing repository service/store and viewer for open and read permission.
`RepositoryAliases::resolve` is the single current-then-old lookup used by the LFS,
deploy-key and Actions task-token openers, which previously each repeated it.
Current-address denial never falls back to another old address. Browser lookup does
not strip `.git` or add a current lookup: redirect triggers already remove reclaimed
aliases. Missing aliases skip permission and reader work entirely.

The ledger row is `ported`; see the
[acceptance record](rewrite/repositories-transfer-acceptance.md).

## Preserved boundaries

Compared with pinned main `39f932a00b7aa35b324e06a9ff2343a57d4bed76`:

- Forbidden/empty-owner/bad-confirmation transfers allocate no store and perform no
  SQL. Full-name allocation remains after administration and before validation.
- Destination lookup constructs one repository store; non-admin authorization keeps
  the existing helper and maps only its validation error. Personal destinations
  have one suspension lookup; organizations skip it. Same-owner rejection follows
  suspension checking, as before. No archive or content-write gate is introduced.
- The pinned reader closes before the first disk move. RepositoryStore::transfer
  still constructs its handle after the move, and owns the exact transaction:
  owner/timestamp update, old team grant deletion, redundant collaborator deletion.
  No SQL, clock, migration or relocation implementation changes are made here.
- A metadata error triggers a reverse disk move. The metadata conflict/primary error
  wins even if compensation fails; no success event/audit is emitted on failure.
- Success still clones one returned Repository snapshot, logs, publishes the event,
  then awaits the existing best-effort audit. No intermediate DTO/vector is added.
- Git lookup performs current then, only if absent, old address lookup; one open
  performs the original grant read/setup. Browser redirect lookup performs old
  lookup then, only if present, the same existing grants/privacy policy.

## Existing limits

Disk relocation and SQL are not one atomic transaction. Failed reverse relocation
can leave disagreement. Cancellation during the blocking disk move can outlive the
caller's lease and prevent the subsequent SQL step; cancellation after SQL can omit
later event/audit work. This application port neither adds durable reconciliation nor
claims to solve these existing lifecycle gaps. Migration 0016 remains the only alias
transaction authority. Admin direct deletion and other lifecycle owners are outside
this row.

## Acceptance gates

Source fixtures in `rbit-repositories/tests/{transfer,aliases}.rs` assert exact port
work/order, no unpolled work, admin/organization fast paths, archive preservation,
private-current precedence, missing-alias work and primary conflict retention after
failed compensation. The real service fixture adds archived transfer, private old
address filtering, Git alias opening and reclaimed-address precedence to the existing
team-grant/collaborator/star/event/audit/duplicate rollback cases.

Executed results, the retired legacy paths and the boundary checks that keep them out
are recorded in the [acceptance record](rewrite/repositories-transfer-acceptance.md).
No runtime speed improvement is claimed.
