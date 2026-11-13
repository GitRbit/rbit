# 0.1.5 to 0.2.0 upgrade and rollback rehearsal

This page records the runbook and scripted rehearsal for the 0.1.5 to 0.2.0
upgrade and rollback. Its migration numbers, backup formats and measured results
belong to that transition. For upgrades from a later release, follow the current
[deployment guide](deployment.md#upgrades-and-migrations) and the applicable
[changelog](../CHANGELOG.md) entry. The general backup and restore contract
(format, checks, recovery objectives) is in [deployment.md](deployment.md#backups).

## Runbook

### Upgrade

1. **Drain.** Stop accepting work: stop the runners (`systemctl stop rbit-runner`) after
   their current jobs, and announce the window. Runners are not upgraded by this runbook.
2. **Stop rbit** (`systemctl stop rbit` or `docker stop rbit`).
3. **Back up** with `rbit backup --output DIR`, using either the installed (old) binary
   or the new one. A backup never migrates: taken with the new binary, the database in
   the backup and the live database both stay at the old schema. The new binary's backup
   (format 6) also copies Actions artifacts, caches and logs; an old binary's (format 4)
   does not, so with the old binary also copy `<data_dir>/actions/` next to the backup.
   Keep the key: `secret.key` is in the backup unless `storage.secret_key_file` is set;
   `actions/secrets.key` (or `actions.secrets_key_file`) never is.
4. **Replace the binary** (or the image) and keep the old one with the backup.
5. **Migrate**: `rbit migrate` with the stopped server (or let `rbit serve` do it at
   start-up). Each migration is one transaction: if the process dies, the database is at
   the last completed migration and running `rbit migrate` again finishes the rest.
6. **Start** and check `/healthz`, sign in (an account with two-factor authentication
   proves the key opens sealed secrets), browse and clone a repository, download a
   release file, open an Actions run. Existing sign-in sessions survive the upgrade.
7. **Reopen**: start the runners. Repository wikis (new in 0.2.0) start turned off; a
   repository administrator turns each on from its Wiki tab.
8. **Back up again** with the new binary once the upgrade is accepted: from then on,
   backups are format 6 and include wiki objects.

### Rollback

The old binary cannot run on the migrated database: it refuses to start ("cannot read
admin configuration: migration 60 was previously applied but is missing in the resolved
migrations") and changes nothing.
Rolling back therefore means restoring the backup from step 3:

1. Stop rbit and the runners.
2. Move the upgraded data directory aside: `mv /var/lib/rbit /var/lib/rbit.upgraded`.
   Keep it until the rollback is verified; it holds every write since the upgrade.
3. Restore with the **new** binary (the old one has no `restore` command), using the
   same configuration: `rbit restore --from DIR`. `restore` never migrates; the database
   stays at the old schema.
4. If the backup was taken with the old binary (format 4), copy
   `/var/lib/rbit.upgraded/actions/artifacts`, `caches` and `logs` back (they are
   unchanged by the upgrade), and `actions/secrets.key` in every case.
5. Put the old binary back and start it. Everyone signs in again (sessions are not in
   backups). Check `/healthz`, sign in, clone.

**What a rollback loses** is everything written after the backup: issues, comments,
pushes, branches, tokens, wiki pages (a 0.2.0 feature) and any other change made on the
new version. The rehearsal below enumerates them. Re-apply what matters by hand (push the
lost commits again from clones, re-create issues) or, before rolling back, export what
you need from the moved-aside directory.

### Refusals that protect the data

| Situation | What happens |
| --- | --- |
| Old binary on the upgraded database | Refuses to start; nothing changes. |
| 0.2.0 on a database migrated by a newer build | Refuses to start ("the database was migrated by a newer rbit (migrations […] are unknown to this version) and is left unchanged"); nothing changes. |
| `rbit restore` of a backup from a newer build | Refused before anything is written. |
| Migration killed part-way | Database at the last completed migration; `rbit migrate` finishes. |
| Interrupted `rbit restore` | Every other command refuses the directory; `restore` again starts over. |

## The rehearsal

`scripts/release-rehearsal.mjs` runs these steps against real binaries and real data
directories. Linux is the supported server platform and the rehearsal's target.

```sh
# Fast mode: candidate only (CI runs it on every pull request and on the release build).
node scripts/release-rehearsal.mjs --candidate target/debug/rbit

# Full mode: the previous release seeded through its own interfaces, upgraded and rolled back.
node scripts/release-rehearsal.mjs --candidate NEW/rbit --baseline OLD/rbit \
  --baseline-runner OLD/rbit-runner [--work DIR] [--keep]
```

The report (`report.md` and `report.json` in the work directory, kept on failure or
with `--keep`) lists every step with its duration and checks. The exit code is 0 only
when every step passed.

**Fast mode** needs no old binary. It builds a store at the previous release's schema
(migration 59) from the immutable migration files with sqlx's own bookkeeping, fills it
with the storage upgrade fixture and real bare repositories, and then: takes a backup with
the candidate and checks nothing was migrated; migrates to the newest migration with
integrity, foreign-key and row-count checks; serves it (anonymous and private access,
clone with `git fsck --strict`, archive download, a new account, token, push, and wiki
pages that are kept, renamed and deleted); backs it up (format 6, with the wiki objects)
and restores it into a new directory and serves that, wiki included; refuses a database
with a migration from a newer build without changing it; and kills `rbit migrate` every
5 ms or so across a whole run, requiring each restart to finish with every row intact.
`node --test scripts/tests/release-rehearsal.test.mjs` runs it when
`RBIT_REHEARSAL_CANDIDATE` names a binary, and its unit tests otherwise.

**Full mode** starts the old release on an empty directory, creates accounts with `rbit
user create`, and seeds through the web forms, REST API, smart HTTP and the Git LFS batch
API: an organisation with a member, a public and a private repository with branches and
tags, an LFS object, open and closed issues with comments, a pull request, a release with
a file, a webhook with a secret, a star, an SSH key, a TOTP-enrolled account, and (with
`--baseline-runner`) an Actions run on a real host runner with a masked secret, a log
and an artifact. It then runs the runbook: backup with the old binary, `rbit migrate`,
verification of everything on the new binary (including the old session cookie), new
writes (issue, comment, push, token, and kept, renamed and deleted wiki pages), a format 6
backup and restore of the upgraded instance, the refusals above, the rollback with `rbit
restore` and the old binary, the list of lost writes, and interrupted migrations on the
seeded data.

## Results

Recorded 2026-11-09 on Linux (WSL2 Ubuntu 26.04, x86_64, Git 2.53), debug builds of the
pinned baseline (main `39f932a0`, `rbit 0.1.5`, migrations through 59, backup format 4)
and of this branch (`rbit 0.2.0`, migrations through 73, backup format 6). Both modes
exit 0.

| Mode | Steps | Checks | Time |
| --- | ---: | ---: | ---: |
| Full | 14 passed | 114 passed, 4 expected losses reported | 136 s |
| Fast | 7 passed | 15 passed | 4 s |

Measured in the full run:

- **Upgrade.** `rbit migrate` 59 → 73 takes 0.1–0.3 s on the seeded instance; quick
  check, foreign-key check and every table's row count are unchanged or grown. The
  candidate serves all seeded data: password and TOTP sign-in (the sealed secret opens),
  sessions from before the upgrade, tokens, refs, clones passing `git fsck --strict`,
  archives, the organisation, issues, the pull request, the release file and LFS object
  (by SHA-256), the webhook, the SSH key, the Actions run, its masked log, its artifact and
  its secret.
- **New writes and format 6.** After the upgrade an issue, a comment, a push to `main` and
  a new branch, a token and three wiki pages (one renamed, one deleted) are written; a
  format 6 backup of the upgraded instance (13 wiki objects) restores into a new directory
  that serves all of it, the rename and the deletion included.
- **Refusals.** The baseline refuses the migrated database, and the candidate refuses a
  database with an unknown migration; neither changes a file.
- **Rollback.** `rbit restore` of the baseline's format 4 backup succeeds (warning that
  the Actions secrets key must be put back); after copying `actions/artifacts`,
  `actions/logs` and `actions/secrets.key` from the moved-aside directory, the baseline
  serves every seeded item again. Lost, as documented: sessions (everyone signs in again)
  and every post-upgrade write listed above, each checked absent.
- **Interrupted upgrade.** Of the kills, those landing during the migrations left the
  database at the last completed migration (61, 63, 66, 68, 70, 71 and 72 were seen)
  with a clean quick check; every restart completed to 73 with all rows intact.

Defects the rehearsal found, each fixed in its own commit with a regression test:

- Taking a backup with the new binary migrated the live database first, so the
  "pre-upgrade" backup could no longer roll back (fixed: `rbit backup` never migrates).
- Every Actions artifact download panicked once the response ended ("Unfold must not be
  polled after it returned `Poll::Ready(None)`"); with the release profile's
  `panic = "abort"` that stops the server. The previous release has the same defect, which
  the full run reports as a known baseline failure.
- A database from a newer build was refused with sqlx's "previously applied but is
  missing" message; it now says that a newer rbit migrated it and nothing was changed.

Not covered: Windows (deferred to the 0.2.0 release build), the container image, SSH
transport, an old runner against the new server, production-scale data (migration time
and free space at scale), and power loss as opposed to killed processes.
