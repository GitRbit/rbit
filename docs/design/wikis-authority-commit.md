# Wiki data authority and commit protocol (ADR)

W0 decision record for `rbit-wikis`, derived from the
[repository wiki plan](repository-wikis-plan.md). Status: design decision only. No wiki
crate, schema, Git store, lock, intent, reconciliation pass or test described here is
implemented. It turns the plan's four-step candidate protocol into one concrete
protocol; nothing below proves it. The tables, refs and formats it names are specified
in the [storage format contract](wikis-storage-format.md); limits come from the
[budgets](wikis-budgets.md); threats from the [threat model](wikis-threat-model.md).

## Context

Wiki state spans three stores with no shared transaction: SQLite (WAL, writes through
`Pool::begin_immediate` in `crates/rbit-storage/src/sqlite/pool.rs`), Git (objects and
refs moved by `git update-ref <new> <old>` under the `hardened` invocation in
`crates/rbit-git/src/merge.rs`) and redb (`crates/rbit-storage/src/kv/mod.rs`). The
closest existing write, the browser file editor (`crates/rbit-git/src/edit.rs`), has
a single authority (the branch ref) and needs no SQLite pointer. Wikis add a SQLite
catalog that must never disagree visibly with Git content, so they need a protocol of
their own. `docs/design/rewrite/security-contracts.md` already states that these stores
require explicit recovery and reconciliation because there is no atomic transaction.

## Decision summary

1. Each wiki is a separate bare Git repository, never a namespace in the code repo.
2. SQLite is the only authority for what readers see. Git holds immutable bytes.
   redb holds rebuildable caches only.
3. At most one mutation intent per wiki is pending. Writers in one process are also
   serialized by a per-wiki writer lock.
4. The committed intent row is the linearization point. After it commits, recovery
   rolls the mutation forward when its target objects exist, and aborts only when they
   are provably absent.
5. Readers use finalized SQLite pointers only. They never read the wiki ref, wait on
   an intent or run reconciliation.
6. Reconciliation runs in three places: on the next write to the same wiki, in the
   repository lifecycle owner before rename, transfer, archive, delete or backup, and
   in a bounded synchronous pass during startup. There is no background worker.
7. A stale expected revision is a conflict. No mutation is last-write-wins or
   auto-merged.

## Authority per store

| Store | Authoritative for | Never holds |
| --- | --- | --- |
| SQLite | Wiki catalog and incarnation, page IDs, visible revision pointers, revision metadata/authorship, canonical paths and aliases, media ownership/quota, mutation and cleanup intents, catalog and search-index revision, the FTS index rows (derived from Git but transactional with pointers) | Page bytes as the only copy |
| Git (wiki repository) | Immutable page bodies, per-revision page metadata and media manifests, commit history; the single wiki ref as a reachability anchor and CAS fence | Visibility, ACLs, aliases, the current pointer readers use |
| Media object store | Immutable media bytes by content ID | Ownership, quota, which revision references them |
| redb | Versioned rendered-content and navigation caches | Any page, ACL, pointer or pending mutation |

Readers follow SQLite pointers to Git object IDs and read blobs by ID. If a pointer
names an object that is missing or corrupt, the reader gets an explicit "page content
unavailable" state with a stable error code. It never falls back to the ref, to another
revision or to a cache entry that has no authority behind it. A missing or corrupt
redb entry is a cache miss.

## Separate wiki repository

Wiki content lives in its own bare repository at
`<data_dir>/wikis/<repo_id>/<wiki_id>.git`. It is never inside the code repository and
never under the owner directory. Reasons, from the code:

- Push events come from comparing complete ref snapshots
  (`crates/rbit-git/src/push.rs`). `evaluate_push` in
  `crates/rbit-services/src/actions/triggers.rs` has no ref-prefix filter. A wiki ref
  inside the code repository could therefore reach webhooks and Actions.
- Forks use `git clone --bare --local` (`crates/rbit-git/src/fork.rs`), which
  hard-links the parent's object files where the filesystem allows, so wiki objects
  would leak into forks.
- Code repositories live at `<root>/<owner_id>/<repo_id>.git` and move on transfer
  (`crates/rbit-git/src/storage.rs`, `crates/rbit-git/src/relocate.rs`). A wiki
  directory keyed by repository ID alone never moves, and needs no transfer lease.
- `repositories.id` is `INTEGER PRIMARY KEY` without AUTOINCREMENT
  (`crates/rbit-storage/migrations/0001_initial.sql`), so a deleted top ID can be
  reused. The wiki incarnation `wiki_id` is AUTOINCREMENT. A recreated repository whose
  ID was reused therefore gets a fresh directory, even if cleanup of the old one is
  still pending.

No transport route resolves a wiki repository. Every Git invocation on it goes through
`hardened`: hooks run through the null device, `gc.auto=0`, and system and global
configuration are ignored. Nothing emits `RepositoryPushed` for it. The alternative of
a reserved, push-refused, unadvertised namespace inside the code repository, plus a
trigger filter, is rejected (threat model T5). The code gives no reason to prefer it,
and it would have to change three existing paths.

## Write serialization: one pending intent per wiki

There is one wiki ref (`refs/rbit/wiki/head`). Each mutation makes exactly one commit
on it. A partial unique index enforces at most one `pending` intent per wiki
(see the storage contract). The decision is per wiki, not per page, because:

- The ref CAS already serializes every mutation of a wiki. Per-page intents would need
  per-page refs (10,000+ refs per wiki, packed-refs churn) to buy any concurrency.
- Rename, alias reclaim, delete/undelete, link projections, quotas and the catalog and
  search revisions are wiki-wide. Per-page intents would need ordered multi-intent
  acquisition and cross-intent recovery.
- Recovery stays bounded and simple: at most one pending intent per wiki to resolve,
  with one ref to compare.

The cost is that writes to different pages of one wiki run in series during the
commit phase (one tree/commit build, one SQL transaction, one `update-ref`, one SQL
transaction). Content blobs are written before the lock, so the serialized window
does not grow with page size. W2 must measure this at concurrency 1, 4, 16 and 64.

A writer that cannot get the per-wiki lock within its bounded wait (budgets: writer-lock
wait) returns Busy, not a conflict. Busy is the existing overload response: `503` with
`Retry-After` and `Cache-Control: no-store` (`WebError::Unavailable` and
`unavailable_response` in `crates/rbit-web/src/error.rs`, the same status that
`max_git_processes` refusal uses). A conflict (`409`) means the user's expected
revision is stale.

## Commit protocol

Step numbers match the plan's; step 0 and step 1b are added.

0. **Admit and stage (no lock).** Authenticate, authorize, validate bounded input,
   reserve a write slot. Write each new page body with `hash-object -w` into the wiki
   repository. Stage media bytes (storage contract). Nothing visible changes.
1. **Build (per-wiki lock held, no SQL transaction).** Take the per-wiki writer lock
   and reconcile any pending intent (see Reconciliation). Read the finalized head `H`,
   the catalog revision `C` and the affected page pointers from SQLite. Return a
   conflict early if an expected revision is already stale. Build the trees and one
   commit `T` with parent `H` using plumbing in the wiki repository. `T` and every
   object it reaches now exist, unreferenced.
2. **Intent (one `BEGIN IMMEDIATE` transaction).** Recheck that the repository row
   exists, that there is no `repo_archives` row, that the actor's current write grant
   and account state hold, and that the wiki is `active` with a supported format.
   Check `wikis.head_oid = H` and `catalog_revision = C`, every page expected revision,
   path/alias uniqueness and quotas. Insert the intent as `pending`. It fixes `H`, `T`,
   the request token and every affected page row with its new revision number and
   object IDs. Commit. **This commit is the linearization point.**
3. **Publish.** Run `update-ref --no-deref refs/rbit/wiki/head T H`. If it fails, read
   the ref back: `T` counts as success, `H` means retry (bounded), and any other value
   puts the wiki on integrity hold.
4. **Finalize (one `BEGIN IMMEDIATE` transaction).** Compare-and-set the intent from
   `pending` to `finalized` and `wikis.head_oid` from `H` to `T`, and set
   `catalog_revision = C + 1`. Apply the page pointers, revision rows, path/alias
   rows, media commitment and FTS rows from the intent's fixed rows, then set
   `indexed_catalog_revision = C + 1`. Commit. If the compare-and-set matches no row,
   another reconciler finished first. Re-read the intent: `finalized` with target `T`
   is success, and anything else is an internal error, never a silent success.
5. **Respond.** Release the lock and return `303` to the page.

No SQL transaction is held across Git work or network I/O. Steps 2–4 form the commit
phase. It must run as one unit tracked by the kernel, and it holds the writer lock and
write slot until the `update-ref` child has exited. The existing `bounded_output` in
`crates/rbit-git/src/process.rs` kills its child when the caller is cancelled, so the
commit phase must not run inside the cancellable request future. Request cancellation
must not stop it. Shutdown drain waits for it up to the
drain bound. If the process exits first, recovery finishes the mutation.

## Linearization and stale authority

After step 2 commits, the mutation is accepted, as a completed write would be. Later
steps are redo work and do not re-authorize. A permission change committed after step 2
does not undo the edit. A permission change committed before step 2 defeats it,
because step 2 rechecks inside the transaction (threat model E1). Archive, transfer
and delete must not be ordered between steps 2 and 4. The lifecycle owner takes the
per-wiki lock and resolves the pending intent before it commits its own change. A
trigger guard in SQLite refuses to delete a wiki row that has a pending intent.

## Crash, cancellation and lost-acknowledgement windows

"Readers" means every authorized read path: page, history, search and navigation.

| # | Window | Durable state | Recovery outcome | Readers see |
| --- | --- | --- | --- | --- |
| 1 | Before any object | Nothing | None; request failed or cancelled | Old state |
| 2 | After blobs/commit `T` written, before intent | Unreferenced objects | None; objects are orphans for prune | Old state |
| 3 | Intent transaction fails or rolls back | Orphan objects only | None; caller gets error/conflict/Busy | Old state |
| 4 | After intent commit, before `update-ref` | Pending intent; ref = `H`; `T` present | Roll forward: CAS `H`→`T`, finalize | Old state until finalized |
| 5 | During `update-ref` (cancel/kill) | Ref = `H` or `T`; possible stale `.lock` | Ref `T`: finalize. Ref `H`: roll forward. Lock file present: retry bounded, then integrity hold | Old state until finalized |
| 6 | After ref = `T`, before finalize | Pending intent; ref = `T` | Finalize only | Old state until finalized |
| 7 | Finalize transaction fails | As 6 | As 6, on next owner | Old state |
| 8 | After finalize, before response (lost ack) | Finalized | None needed; retry with same token returns the finalized result | New state |
| 9 | Retry, same request token | Intent row with that token | Pending: reconcile, then report. Finalized: `303` to its revision. Aborted: report failure, form keeps text | Per row |
| 10 | Retry without token, or new edit on a stale base | None for this request | Conflict; panel shows the current revision (and its author) | Current state |
| 11 | Concurrent different target, same page | One winner's intent | Loser fails step 1 or 2 check: conflict, no overwrite | Winner's revision |
| 12 | Concurrent writers, different pages | Serialized by lock/index | Both succeed, or Busy after the bounded wait | Each as finalized |
| 13 | Pending intent, `T` missing (power loss, restore) | Ref = `H` | Abort intent (`aborted_missing_objects`) | Old state |
| 14 | Ref equals neither `H` nor `T` | Unexplained ref | Integrity hold: writes refused with stable code; health reports it; no automatic repair | Finalized state, if its objects exist |
| 15 | Ref ahead of `head_oid`, no pending intent (restore only) | Snapshot older than mirror | Restore resets ref to `head_oid` iff it is an ancestor; else integrity hold | Snapshot state |
| 16 | Orphan objects | Unreferenced loose objects | Pruned by wiki maintenance with Git's prune grace, only under the lock with no pending intent | Unaffected |
| 17 | Orphan wiki directory (create crashed, or delete) | Directory without `active` row | Provisioning row: finish or remove. Cleanup intent: remove. Neither: report only, never delete unknown data | Not found |

The only ref that recovery ever moves is the wiki head, and only from `H` to `T`.
Recovery never moves it backward during live operation. Window 15 is the one
exception, and it applies only during restore, before writes reopen. Roll-forward is
chosen over abort for windows 4 and 5 because the two converge on the same target: a
lingering `update-ref` child from a crashed process fails its CAS harmlessly once the
ref is `T`. An abort that leaves the ref at `H` could not fence that child. Window 13
is safe because `update-ref` refuses a missing target object.

## Reconciliation

Decision function for one pending intent, under the per-wiki lock:

| Wiki ref | Target `T` verifies (`cat-file -e` on commit, tree, blobs) | Action |
| --- | --- | --- |
| `T` | n/a | Finalize |
| `H` | yes | `update-ref T H`, then finalize |
| `H` | no | Mark `aborted` with resolution code; ref untouched |
| other | n/a | Integrity hold; intent stays pending |

Owners. No owner runs reconciliation in a long-lived task:

- **Writer path.** Step 1 resolves the wiki's pending intent before it builds. A
  retry that carries the same request token resolves its own intent.
- **Repository lifecycle owner.** Rename, transfer, archive, delete, backup and
  restore take the lock and resolve the pending intent before they commit. This
  ordering is required. Repository deletion today removes the bare repository and
  then the row (`delete` in `crates/rbit-repositories/src/settings/application.rs`),
  so wiki draining must come before both steps.
- **Startup.** The kernel startup phase runs one bounded, synchronous pass of at most
  the budgeted batch of wikis with pending intents (32 by default), before wiki write
  admission opens. Reads are admitted throughout. Any intents left over stay pending,
  get resolved on access and are counted in truthful health.

Health also reports wikis on integrity hold. A later step in the existing periodic
maintenance (`maintenance_interval_minutes`,
`crates/rbit-core/src/config/operations.rs`) may reuse the same bounded pass. This
ADR does not claim that the step exists.

## Conflict semantics

- Every edit, restore, rename, delete and undelete carries the expected per-page
  revision. Create carries "expected absent". Media attach and detach go through the
  page revision that references the media.
- A stale expectation returns `409` with the submitted text preserved and a compare
  link ([navigation contract](wikis-navigation-contract.md)). The user rebases only
  by resubmitting explicitly.
- Path/alias clashes are conflicts decided by the SQLite primary key inside step 2,
  never by read-then-write.
- Identical content on a current base still creates a revision only if metadata
  changed. Otherwise it returns "nothing changed", as `EditError::NothingChanged`
  does.
- Content is never auto-merged, and the ref is never forced.

## Request token

Each mutation form and API call carries a random 128-bit request token, generated
when the form is rendered. Forms send it as the hidden field `request_token`, 32
lowercase hex characters (navigation contract); it is stored as the 16-byte
`wiki_mutation_intents.request_token` (storage contract) and is unique per wiki in the
intent table. It is an idempotency key, not a CSRF defense. The token is the
only way a retry recognizes "its" already-applied target. Commit IDs cannot serve,
because they include time. API clients without a token get conflict semantics on
retry. Finalized and aborted intents are kept for a bounded retention period so
lost-ack retries resolve. After that, a retry returns a conflict.

## Alternatives rejected

| Alternative | Why rejected |
| --- | --- |
| Wiki namespace in the code repository | Push snapshots, Actions triggers, forks and backup copy it (see above) |
| Per-page refs and intents | Ref count, cross-page operations, multi-intent recovery |
| Abort-on-recovery for windows 4–5 | Cannot fence a late `update-ref`; would need ref rewind |
| Git as pointer authority (read ref on each request) | Readers would see published-but-unfinalized state; no SQL join for ACL/search |
| Commit `T` built before the lock with rebuild-on-race | Retry loops; pointless with one ref |
| Background outbox worker | Not required for correctness here; plan forbids claiming one |

## Tests required before adoption

No test below exists yet. Fault points are seams at the Git and SQLite ports, used with
a real SQLite file and a real `git` binary. Restart tests reopen the same data
directory in a new process.

| Window(s) | Proof |
| --- | --- |
| 1–3 | Fault before, at and after object writes and in the intent transaction: no intent row, `head_oid` unchanged, objects unreferenced |
| 4, 5 | Kill or cancel after intent commit, before and during `update-ref`. Restart: roll-forward publishes exactly `T` once. A simulated late `update-ref T H` after recovery fails harmlessly |
| 5 | Stale `head.lock` left behind: bounded retry, then integrity hold, with reads unaffected |
| 6, 7 | Fault after ref CAS and in the finalize transaction: next writer, lifecycle owner and startup pass each finalize. Duplicate finalize is a no-op |
| 8, 9 | Drop the response after finalize. Same-token retry returns the same revision and creates no second intent or commit |
| 10, 11 | 2/4/16/64 writers on one page: exactly one revision per base, the rest `409`, no lost update |
| 12 | Writers on distinct pages: all succeed or get Busy, history is linear, every revision is readable |
| 13 | Delete `T` objects before restart: intent aborted, ref and readers unchanged |
| 14, 15 | Foreign ref value and restore with ref ahead: hold and reset rules as tabled |
| 16, 17 | Maintenance never prunes a pending intent's target. Orphan directories are reported, never auto-deleted without an intent |
| Linearization | Revoke write or archive between steps 1 and 2: refused. Revoke after step 2: edit published. Archive and delete wait for the pending intent |
| Readers | Concurrent readers during every window only ever observe finalized pointers |

## Consequences

- Wiki writes cost two short SQLite write transactions and one `update-ref` on the
  serialized path. The benefit is that there is never a visible mismatch.
- Recovery needs no detached worker. In exchange, a lost-ack edit whose writer
  crashed can stay pending until the next write, lifecycle operation, retry or
  restart.
- Backup must cover the wiki repositories and media separately.
  `crates/rbit-server/src/backup.rs` covers neither today.
- Existing push, fork, mirror and Actions code needs no change for isolation.

## Open questions

1. Can the kernel's tracked-unit API keep a commit phase alive past request
   cancellation, within the drain bound, without adding a detached task?
2. Retention period for finalized intents and their request tokens. Request-token
   storage for API clients.
3. Integrity-hold repair: an operator command, or restore only?
4. Should the startup pass block write admission for all wikis, or only for the wikis
   it has not yet visited?
5. Interaction with a future user-facing wiki Git remote (W4). Such a push would have
   to create intents through this same pipeline.
