# Backup retirement acceptance

Acceptance evidence for the `operations.backup` ledger row ("Immutable migration
history, consistent backups and original sealing key"), following the
[identity retirement](identity-retirement.md) pattern. The guarantees themselves are
the seventh group of the [security contracts](security-contracts.md#backup-and-restore-group-status).

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Consistent snapshot (`VACUUM INTO`, opened without migrating the source) | `rbit_storage::sqlite::backup::snapshot_into` (moved) |
| What a snapshot says the backup holds, per format: repositories, release files, LFS objects, artifacts, caches, logs, with recorded size and digest | `rbit_storage::sqlite::backup::inventory` (moved) |
| Rows whose repository or file vanished during the backup leave the snapshot, in one transaction | `rbit_storage::sqlite::backup::prune` (moved; takes a `Missing` port value instead of the server's copy results) |
| Wiki catalog to copy (format 6) and page forgetting for older formats | `rbit_storage::sqlite::backup::wikis` (moved) |
| Backup format (6) and the format each kind arrived in | `rbit_storage::sqlite::backup::{FORMAT_VERSION, inventory::Kind::since_format, wikis::SINCE_FORMAT}` (moved) |
| Actions secrets that need the separate key after a restore | `rbit_storage::sqlite::backup::sealed_actions_secrets` (moved) |
| Restore checks: `quick_check`, unknown and failed migrations, sealing key opens the database | `rbit_storage::sqlite::snapshot::{inspect, check_key}` (already storage) |
| Immutable migration history | `rbit-storage/migrations`, embedded by `snapshot::embedded_migrations`; hashes pinned by `scripts/rewrite/check.mjs` (unchanged) |
| Original sealing key | `rbit_storage::sealing` (unchanged) |

`crates/rbit-server/src/backup` remains the composition and IO adapter, as the
[rewrite plan](../ground-up-rewrite-plan.md) assigns backup/restore composition to
`rbit-server`: Git mirroring and `git fsck`, file copies and digests, wiki-object copy
and verification through `rbit-git`, the secret key file, the durable sync and
manifest, and the restore orchestration with its marker file and refusal rules. It no
longer runs SQL or opens a database connection itself.

No new internal crate edge: `rbit-server` already depended on `rbit-storage`, and its
`rbit-wikis` dependency is dropped (the wiki ids now arrive through storage's catalog
types). `ADDED_EDGES` still allows `rbit-server -> rbit-wikis`; it is left for the
other `rbit-server` retirements to tidy rather than conflict with them.

## Legacy code removed

1. `crates/rbit-server/src/backup/inventory.rs` (161 lines): the inventory queries,
   format gating and the read-only connection helper.
2. `crates/rbit-server/src/backup/prune.rs` (66 lines): the pruning transaction.
3. From `backup/mod.rs`: `snapshot_database` and the `FORMAT_VERSION` definition (now a
   re-export).
4. From `backup/wikis.rs`: the catalog reader, the `Wiki`/`Wikis` types and
   `forget_pages`.
5. From `backup/restore.rs`: the direct `actions_secrets` count, the hand-built
   writable connection for older formats and the server's own `sqlx` use.

## Boundaries that fail if legacy paths return

`scripts/rewrite/backup-retirement.mjs`, run by `scripts/rewrite/check.mjs`, over every
authored Rust file below `crates/`, comments ignored:

- `crates/rbit-server/src/backup/inventory.rs`, `prune.rs` or a directory of either name.
- `sqlx`, `writer_compatibility` or SQL statements (`SELECT`, `INSERT INTO`,
  `DELETE FROM`, `UPDATE … SET`, `VACUUM`) in `crates/rbit-server/src/backup/`.
- `VACUUM INTO` in production code outside `rbit-storage`.
- A `FORMAT_VERSION`, `SINCE_FORMAT` or `since_format` definition in `rbit-server`.

`scripts/tests/backup-retirement.test.mjs` covers each rule, the allowed cases
(storage, tests, comments, the re-export, the telemetry filter) and the whole tree.

## Preserved

- Backup format 6 and the manifest's fields and file layout: the manifest type and
  `copy_secret_key` are untouched; the inventory produces the same paths in the same
  order.
- Restore's refusal rules (no manifest, newer format, non-plain database name,
  non-empty directory, missing repository or file), the `.rbit-restore-incomplete`
  marker, and the integrity checks (database digest, `quick_check`, `git fsck`, stored
  file size and SHA-256, wiki-object verification) are the same code in the same order.
- Secret key handling: the key file is copied or reported exactly as before and
  `check_key` still decides `MissingKey`/`WrongKey`.
- Error text: storage's `BackupError` reproduces each former `anyhow` context message
  (`cannot open …`, `VACUUM INTO failed`, `invalid LFS object id …`, `invalid wiki …`,
  `cannot leave repository …`) and passes database errors through transparently, so the
  printed chains are unchanged.

## Results

Passed. The Linux check of the final code (run 210: format, all-target Clippy with
`-D warnings`, tests, rewrite contracts, front-end build, licenses) passed all five jobs.
Its test job ran the `rbit-server` and `rbit-storage` suites: 482 tests run, 482 passed,
1 skipped. That includes the three new `rbit-storage` backup cases, the seven
`rbit-server` backup tests, the seven backup/restore round-trip and damage tests and the
durable-copy unit test. An earlier run failed on a trailing blank line (rustfmt), a
`&str` test helper that the `sqlx` query API needs as `&'static str`, and the base
`AccessTokenStore` type-check fix, which the branch now carries. Locally:
`node scripts/rewrite/check.mjs` reports zero violations and `node --test
scripts/tests/*.test.mjs` passes 660 of 669 with 8 skips; the one failure (`htmx-vendor`)
needs `node_modules` and is unrelated.

New tests: `crates/rbit-storage/tests/backup.rs` (three cases: format-gated inventory
paths, sizes and digests against a real migrated database; pruning with cascades and
the untouched live database; error messages) and the boundary test above.

## Migration, rollback and performance

No migration, schema or stored data changed; migration 0073 remains the latest and no
historical migration was edited. Rollback is the previous binary against the same
data and the same backups. The same statements run in the same order on the same
connections; the only difference is that wiki catalog reads open their own read-only
connection inside storage, as the server already did around them. No release
measurement was taken and none is claimed.

## Not covered

- What the security contracts already list as not covered (real power cuts, Git's own
  durability, formats 1–4 beyond the compatibility path, production-scale timing).
- Moving the manifest and the copy orchestration behind storage ports: the plan keeps
  backup/restore composition in `rbit-server`, so they stay there.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus this record) |
| Files changed by code commits | 16 |
| Lines added / removed | 525 / 194 |
| Production Rust (`src`, non-test) | 9 files, +313 / −191 (two files moved, two new storage modules); manifests −3 |
| Tests | 2 files, +125 |
| Checker and policy scripts | one 41-line Node guard, a 44-line test, two checker lines |

Tricky parts:

- Deciding the split: the ledger names `rbit-storage`, the plan names `rbit-server` for
  backup/restore. Every database decision moved; file, Git and manifest IO stayed.
- Keeping each error's printed chain identical across the crate boundary without
  `anyhow` in storage: a dedicated error enum with transparent database errors.
- The wiki types carried an `rbit-git` scope helper; storage cannot depend on
  `rbit-git`, so the helper became a server function and the dependency on
  `rbit-wikis` left the server.
- The shared Windows build lease was stale, so nothing was compiled locally; the Linux
  CI run was the compile and test gate.

Estimate for similar rows: where the target crate is the storage adapter itself and the
legacy code is already isolated in one module, retirement is one session of 2–3 code
commits and 500–700 changed lines.
