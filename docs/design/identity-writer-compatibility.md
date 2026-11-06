# Credential writer compatibility

Migration 0061 installs a used compatibility interlock for identity and credential
writes. It does **not** activate original-proof receipts, epochs, session fencing,
session invalidation, a writer lease, or crash recovery. Current accounts, stored
credentials, sessions, authentication policy, and public service APIs remain intact.

Each official `Database` pool connection registers the safe SQLx 0.9
`SqliteConnectOptions::collation` API before connecting. The fixed
`rbit_credential_writer_v1` comparison is pure lexical ordering; it performs no IO,
credential access, allocation of password work, or lookup of current epochs.
Persistent BEFORE INSERT, UPDATE and DELETE triggers compare fixed literal values
with that exact collation. SQLite resolves the connection-local collation while
preparing the trigger program. A missing protocol raises SQLite's unknown-collation
error, including when schema invalidation reparses a statement cached before the
migration. Registering only a differently named protocol cannot satisfy it.
A registered incompatible comparison that does not consider the fixed literals
equal is explicitly refused by the trigger. The collation name is the version;
there is no fallback to a built-in comparison or optional marker check.

## Exact scope and cost

All three DML operations are guarded on these 13 tables:

- `accounts`, `user_logins`, `credential_revocations`: account creation/deletion,
  names, passwords, administrator/suspension state and future revocation intents.
- `two_factor`, `recovery_codes`, `passkeys`, `external_identities`, `email_tokens`:
  local/provider proofs, recovery and account linking.
- `access_tokens`, `ssh_keys`, `deploy_keys`, `server_secrets`: bearer/Git credentials
  and server-held signing secrets.
- `instance_settings`: live account/registration policy.

Foreign-key cascades which modify a protected table also need the protocol.
Unrelated repository/org/team/Actions/content writes and reads are outside this
interlock; it is not whole-platform writer exclusion. Future credential or protocol
state tables must install the appropriate persisted guard in their own migration.

The triggers add SQLite VM instructions and a Rust comparison callback per affected
row. They do not add SQL statements, acquisitions, SQL read predicates, password work,
or per-request version reads. Existing `count_work` statement counts cannot measure
this added execution cost. No runtime performance result is claimed for this change.

## Direct connection inventory

The source inventory covers all Rust `SqliteConnectOptions`, `SqlitePoolOptions`,
`SqlitePool::connect` and `SqliteConnection::connect` constructors at this change.

| Connection owner | Behavior |
| --- | --- |
| `sqlite/mod.rs` official `Database` pool | Registered options, preserving all WAL/synchronous/mmap/temp-store/busy-timeout/pool tuning. New connections inherit the same registration. Registration precedes migration and resealing. |
| Server `config.rs`, `serve.rs`, `main.rs` migrate/user-create/serve paths | Use the official pool, including offline user creation; no alternate SQLite-only account writer. API-client commands do not open a database. |
| Server `backup.rs` VACUUM INTO | Registers before connecting, without running migrations. Snapshot integrity connection remains read-only and unregistered. |
| Credential inventory tool | Immutable read-only offline connection; intentionally unregistered. Its isolated schema/encoding/drift fixtures are not migrated application databases. |
| Storage credential-fence/intent/upgrade/access-token and issue-deletion fixtures | Direct protected-table writers register. Upgrade/issue fixtures share a registered connection helper. |
| Services password/passkey/TOTP original-proof fixtures | Direct epoch/intent writers register without changing their authentication order. |
| Storage `accounts/fenced/tests.rs` | Migrates an empty memory database and reads EXPLAIN plans only; no protected DML. |
| Storage pool unit fixtures | Synthetic `t` schema, no application migrations or credential writes. |
| Storage issue-search/pull fixtures | Pull/review rows and test failure triggers only; no protected DML. |
| Storage Actions fixture, E2E Actions-disabled fixture, services jobs fixture | Direct read-only inspection. |
| Services issue/timeline port fixtures | Test DDL/failure triggers, issue metadata and repository visibility changes only. Any resulting protected writes on application connections still use the registered pool. |
| Services notification/email fixtures | Read queries, repository ownership/visibility updates and failure-trigger DDL; actual signing-key inserts use the registered application pool. |
| Services recovery lifecycle fixture | Read-only SELECT inspection despite writable connect options. |
| Services imports startup fixture | Writer-blocking transaction/import rows only; no protected DML. |
| New writer-compatibility fixtures | Deliberately registered, missing, unknown-version and incompatible-comparison connections to prove the boundary. |

Queued producer/allocator/MFA branches must register any new direct protected-table
fixture connection when merged. This inventory is not authority to accept new raw
writers without review. Public `writer_compatibility::register` is a deliberate,
versioned opt-in for trusted composition, not an authentication receipt.

## Upgrade and rollback

Stop older application/CLI writers before upgrading and retain an accepted consistent
stopped backup (SQLite, redb and secret key/configuration per the backup contract).
Migration 0061 is append-only and transactional. It does not change existing password
hashes or session bytes. Current binaries register the protocol for every official
connection. Older migrators refuse the new schema version; an already-open old
connection cannot bypass the relevant DML guard merely by caching SQL first.

Mixed-version processes are unsupported: an old process can still read and may write
unrelated tables, while its protected writes fail. An already-running old process
also owns independent redb/session state; this interlock does not stop that process
or make mixed session writers safe. Stop it rather than treating write errors as a
replacement for an upgrade window. No one-time sign-in is introduced by this atom.

Downgrade requires restoring the consistent stopped pre-0061 backup, not removing
triggers, editing migration history, or mixing older SQLite with newer redb. A future
session-fencing upgrade will need its own explicit sign-in and recovery contract.

This is software-version compatibility, **not authorization against a database owner**.
Code with arbitrary SQL/DDL or file access can drop triggers, register the protocol,
replace the database, or change schema. The protocol cannot attest a binary's identity
or enforce an exclusive writer lease. Released v1 binaries still have legacy unfenced
credential writers and register this same v1 capability: fenced activation must advance
the persisted required protocol (or provide proven full older-version exclusion),
not rely on 0061 to distinguish those binaries. Later persistent fencing activation must also
prevent current-version legacy mutation APIs from writing unfenced state after mode
transition, cover all session issuers/cleanup and enforce cross-store recovery.

## Acceptance

Real temporary SQLite tests cover a pre-upgrade cached password statement, legacy
account-create/delete/suspend SQL, unknown versions and all 39 scoped DML guards,
read-only/unrelated writes, registered fresh pool connections, trusted_schema OFF,
actual official account/profile/moderation APIs, transactional DML rollback, failed
migration rollback/retry, and an older migrator's version refusal.

Required shared-lane checks after integration:

- `cargo test -p rbit-storage --test integration writer_compatibility`
- All storage tests (especially upgrade, sealing, account and credential fixtures).
- Services original-proof/MFA tests, server backup/config tests, then workspace tests
  and strict Clippy. No tests are substituted with a real user database.

Source formatting and size checks can run without the compiler lane. At source
preparation time these new Rust tests have not been compiled or executed.
