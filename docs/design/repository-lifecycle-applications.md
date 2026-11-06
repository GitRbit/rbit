# Repository lifecycle applications

Prepared source only; Rust/HTTP acceptance remains pending. The plan's repository
lifecycle group remains baseline-retained while the prepared replacements and related
paths receive Rust/HTTP and whole-behavior acceptance. Existing core model/wire paths remain aliases.

## Complete settings boundary

RepositorySettings owns general description/visibility/default branch updates,
rename, archive/unarchive and confirmed deletion. UpdateRepository and
RepoGeneralSettings move byte-for-byte behind existing service/storage aliases.
Static store ports forward the original SQL futures directly. SQL statements,
row handling and timestamp calls in RepoSettingsStore are identical to pinned main
39f932a; uniqueness, redirects, archive winners and cascades stay in SQLite.

The concrete RepoSettingsService now owns Database, RepositoryStorage, EventBus
and audit only; composition passes them explicitly. A request borrows SettingsEffects.
No per-request owner clone, store preconstruction, DTO vector, query, runtime task,
dynamic dispatch, normal dependency or feature is added. The service and effect view
have qualified continuous component-size gate entries. Startup retains fewer broad
handles; no runtime latency or aggregate clone-count improvement is asserted.

Authority/fast paths preserved from main:

* Administrative grant precedes description/name/confirmation validation. Settings
  remain usable while archived. HTTP/API token, MFA, origin/IP, throttle, cache and
  form/redirect boundaries stay in their existing callers/middleware, untouched.
* Description trims, counts Unicode characters to 350, then rejects controls.
  Only nonpublic-to-public changes consult the unchanged private-ancestor authority.
* Missing/unchanged default branch skips Git refs and set_head. Changed branches
  read the original refs vector, set HEAD before SQL, and restore old HEAD on SQL
  failure; a restore failure logs but does not replace the original SQL error.
* Same-name rename creates no store and performs no SQL. Case-changing names retain
  original Eq/unique-index behavior. Stable ID paths and redirect triggers are unchanged.
* Archive/unarchive always use conditional SQL, even with stale context. Archive store
  construction precedes the original clock sample; unarchive samples no app clock.
  Only the winner logs, awaits best-effort audit and publishes the same domain event.
* Delete validates full owner/name before closing its pinned reader, awaits original
  disk deletion, then constructs the metadata store and deletes SQL. It publishes the
  same event/full-name String afterward, retaining cache/background event inputs.

Original gaps remain explicit: audit cancellation can leave an archived row without
its transient event; a failed metadata delete can leave removed disk data with a row,
with idempotent disk removal enabling retry. No outbox/transaction/durable drain claim
is added. Admin deletion, transfers/forks/imports and background consequences remain separate
until their own authority/ownership and full acceptance. Creation is prepared below.

Prepared independent tests pin factory/clock/Git/effect order, no-op/rejected/unpolled
paths, restore-error precedence, confirmation, conditional winner and cancellation.
Actual stores add type identity, exact work, real HEAD restoration on forced SQL
failure, racing archive winner, and disk-first failure/retry fixtures. Expected SQL
budgets: rejected/rename-noop 0; ordinary update 1; archive loser 1; winner/unarchive
winner 2 (mutation+audit); failed/retried metadata deletion 1 each.

Run through the shared build helper when its lane is granted:

```text
cargo test -p rbit-repositories --test settings
cargo test -p rbit-services --test main repo_settings
cargo test -p rbit-services --test main archived
cargo test -p rbit-services --test main forks
cargo test -p rbit-services --test main transfers
cargo clippy -p rbit-repositories -p rbit-storage -p rbit-services --all-targets -- -D warnings
```

Require repository settings/API authorization, archived write guards, native UI,
public/private discovery/cache, archive webhooks and deletion background-cleanup
acceptance before declaring the broader lifecycle group complete.

## Creation and shared owner authorization

RepositoryCreation and RepositoryOwnership now own the active normal creation flow
and the existing reusable owner rule invoked by forks/transfers. CreateRepository and
NewRepository keep their original derives/fields and concrete service/storage aliases.
The original create/delete/find_owner/org_role/grant_collaborator store implementations
are byte-identical to pinned main. The adapter projects only Member/Owner membership
facts without replacing the organization wire enum or buffering rows.

Empty/case-insensitive self-owner returns the original Account clone without a query.
Shared owner resolution does not add trimming; only create_in retains its prior trim.
Unknown/personal-other owners fail before name/description validation. Site admins
may use organizations without a membership query, never arbitrary other personal owners.
Creation keeps its distinct multiline-description behavior; general settings still
reject controls. Name parsing and 350-character counting precede cloning the configured
default branch. The existing two branch String copies are preserved, not called a speedup.

Metadata SQL remains first; Git bare initialization follows with the same directory
and blocking-worker implementation. Disk error logs before row compensation, and a
failed compensation still overrides the original disk error. An organization member's
role is reloaded AFTER disk creation; only a still-current Member receives the same
creator-admin collaborator grant, and publication follows that grant. Do not cache the
first role or add a per-request store clone/query/authorization pass.

Existing failure gaps remain: cancellation while filesystem work is pending may leave
metadata or independently completing blocking work; creator-grant failure can leave
row+disk without a created event. This port does not introduce async Drop, a detached
cleanup task, a cross-store transaction or a durable event outbox. The existing Git
storage lifecycle/lease and complete ownership goals still apply separately.

Eight independent creation cases cover owner/validation precedence, zero-lookup and
unpolled fast paths, admin/nonorg rules, fresh postdisk membership, compensation-error
precedence, grant failure, multiline behavior and cancellation. Five real-store cases
cover alias identity, personal-create 1 acquisition/statement, rejected personal-name 0,
unknown-owner 1, concurrent unique-name winner, actual disk-failure compensation
(2 statements including rollback) and member-grant failure (5 statements) preserving
its original row/disk/no-event gap. Existing owner/access/org/fork/transfer tests stay.

The entire remaining RepositoryService now measures 148 cumulative authored type lines,
RepoSettingsService 34, SettingsEffects 61 and CreationEffects 23, and all have explicit
legacy-component gate entries. New feature owners are bounded (settings 135, creation
56, owner authorization 23). Source/graph guards and exact AST scans pass; Rust tests
and runtime work counts are PREPARED, not observed passing or performance results.
No new normal/dev dependency, registry package, Cargo feature or lock edge is required.

Additional shared-lane acceptance:

```text
cargo test -p rbit-repositories --test creation
cargo test -p rbit-services --test main repo_settings::creation
cargo test -p rbit-services --test main orgs
cargo test -p rbit-services --test main access
cargo test -p rbit-services --test main imports
```

All service/UI/API ingress and event consumers remain at their existing paths; keep
full workspace, archived enforcement, native repository UI, redirects/fork visibility,
public cache and background deletion acceptance before feature retirement.
