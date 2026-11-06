# Repository lifecycle applications

Prepared source only; Rust/HTTP acceptance remains pending. The plan's repository
lifecycle group remains baseline-retained while creation and related paths are ported
and the whole behavior is accepted. Existing core model/wire paths remain aliases.

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
is added. Creation, admin deletion, transfers/forks/imports and background consequences
remain separate until their own activation and full acceptance.

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
