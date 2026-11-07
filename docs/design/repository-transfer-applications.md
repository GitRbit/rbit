# Repository transfer and old-address applications

`rbit-repositories::transfer::RepositoryTransfer` owns the transfer command's
administration/confirmation/destination/suspension decisions and its compensating
operation ordering. `TransferService` preserves its public API and delegates through
one borrowed `TransferBridge`; no request store or context is cloned for delegation.
The suggested-owner picker retains its existing single membership vector and lazy
iterator projection in the service adapter. Shared creation-owner authority remains
the existing `authorised_owner` bridge, including its personal-owner short circuit.

`RepositoryAliases` owns current-address precedence, Git `.git` stripping, old-address
fallback and permission-filtered redirect publication. `AliasView` borrows the exact
existing repository service/store and viewer. Current-address denial never falls
back to another old address. Browser lookup does not strip `.git` or add a current
lookup: redirect triggers already remove reclaimed aliases. Missing aliases skip
permission and reader work entirely.

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
this atom; the feature group remains baseline-retained until its acceptance gates.

## Acceptance gates

Source fixtures in `rbit-repositories/tests/{transfer,aliases}.rs` assert exact port
work/order, no unpolled work, admin/organization fast paths, archive preservation,
private-current precedence, missing-alias work and primary conflict retention after
failed compensation. The real service fixture adds archived transfer, private old
address filtering, Git alias opening and reclaimed-address precedence to the existing
team-grant/collaborator/star/event/audit/duplicate rollback cases.

Rust tests and AST type checks are prepared, not executed in this source-only atom.
Run the shared helper's package tests for rbit-repositories; the services integration
binary with filter `transfers::`; existing redirect/access/rename integration cases;
and strict all-target Clippy for both packages. Run the structural AST component gate
for the added legacy adapter mappings. No runtime speed improvement is claimed.

Source validation: formatting and diff checks passed; authored-source/legacy-size
checks found 1,549 files and zero violations. The full Node suite passed 541 tests
with seven skips. These checks do not substitute for the pending Rust/AST gates.
