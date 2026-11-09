# Repository import retirement progress

Evidence for the first slice of the `repositories.imports` ledger row ("GitHub/Gitea
imports with history, issue numbers, LFS and releases"). The row is too large for one
slice, so it stays `baseline-retained`: this slice retires import **admission and
lifecycle**; the copy itself remains in `rbit-services`. It follows the
[identity retirement](identity-retirement.md) pattern.

## Scope of this slice

| Decision | Before | After |
| --- | --- | --- |
| Admission order: refuse while disabled, parse the source, check the token, check the API host, build the client, read the metadata, then create the repository, record the import and start it; nothing is created for a problem with the source | `ImportService::start` | `rbit_repositories::imports::admit` over `ImportAdmission` |
| Token rule: trimmed, at most 1,024 bytes, no whitespace or control characters, empty means none; the token is never stored | `start` | `imports::checked_token` |
| Local name (the source's when none is given) and description (trimmed, at most 350 characters) | `start` | `imports::{local_name, local_description}` |
| Who sees an import's progress: its importer and the repository's administrators | `ImportService::status` | `imports::may_view_status` |
| One run: mark it running, re-open the repository for the importer (who must still be able to write), run, log and count, clean the message | `ImportService::run` | `imports::run` over `ImportRun` |
| Interrupted, stopped, unavailable and disabled texts | `imports.rs` | `imports` constants, re-exported by services |

## What stays in `rbit-services`, unchanged

- **The outbound address guard** (`check_source_host` over `check_public_host`) for
  the API host and every later request, and the API client with its rate-limit wait.
- **Credential handling.** The token lives only in the job's memory, reaches Git only
  through its environment, and travels only to the source's own hosts
  (`Job::token_for`); the header is encoded by the shared `basic_authorization`.
- **Creating repository rows.** Imports create the local repository through the same
  `RepositoryService::create_in` call; the import record is still admitted behind the
  restart gate (`create_record`, `fail_interrupted`), whose tests are unchanged.
- **Progress, cancellation and size limits.** The job's phases, progress saves every 25
  items, the warning cap, the shutdown `select!`, permits, `STOPPED` on shutdown and
  every size and text limit of the copy are unchanged.
- **Source parsing** (`ImportSource::parse`) and the import models stay in `rbit-core`.

## What remains for later slices

The copy in `imports/job.rs` (Git and default branch, LFS objects, labels, milestones,
issues and pull requests with their numbers and authors, releases with their files),
`imports/api.rs` (source API shapes) and `imports/client.rs` (the HTTP client and its
guard). Natural next slices: the phase plan and progress bookkeeping, then author
mapping and attribution text, then label, milestone and release item rules. The
client stays with the shared outbound guard.

## Boundary that fails if legacy paths return

`scripts/rewrite/imports-retirement.mjs`, run by `check.mjs` on its own lines, refuses
the moved constants, admission texts and description truncation in `imports.rs` and the
new `imports/ports.rs` and `imports/startup.rs` adapters (comments and test modules
ignored); the pre-slice `imports.rs` fails it. `scripts/tests/imports-retirement.test.mjs`
covers it and the whole tree.

## Parity argument

- Routes, forms, error texts and log lines are unchanged; `ImportService` keeps its
  public methods and `INTERRUPTED`/`STOPPED` keep their paths.
- Admission makes the same calls in the same order, with the same messages, including
  `owner/name: …` for unreadable metadata; the record failure is still logged before it
  is returned, and "import queued" is still logged before the job is spawned.
- A run marks the import running (a failure is only logged), re-opens the repository,
  logs "import started", runs the job under the same shutdown `select!`, logs and counts
  the outcome, and cleans the message with the same `clean_message`; a repository the
  importer can no longer write to still ends the run before anything is counted.
- No schema, migration or stored format changed.

## Results

One Linux run of the repository's own CI jobs at this slice's last code commit
(changed packages only, base `c6818781`): the rewrite contracts (the policy checker, the
structural type gate and the Node policy tests), workspace-wide formatting and
all-target Clippy with `-D warnings`, licenses and advisories, and the front-end build
passed. Nextest for `rbit-repositories` and `rbit-services` ran 833 tests, all passed.
Two earlier runs failed only on the new fake-port test file (a missing lifetime, then
a Clippy lint), fixed before the tip.

Import-relevant groups, all passing:

| Group | Tests |
| --- | --- |
| `rbit-repositories` unit `imports` (token, name, description, visibility) | 1 |
| `rbit-repositories` integration `imports` (fake ports, new): admission order, nothing created for source problems, run lifecycle | 3 |
| `rbit-services` unit: `imports` 10 (admission gate and restart sweep included), `outbound` 26 | 36 |
| `rbit-services` integration `imports` | 5 |

Not run for this slice: Windows legs and the `rbit-web` and `rbit-e2e` suites, which
quick mode skips because no file in those crates changed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| Files changed by code commits | 8 |
| Lines added / removed (all code commits) | 507 / 78 |
| Production Rust (`src`) | 4 files, +325 / −78 |
| Tests | 1 new fake-port test file (+129), unit tests in the new module, a 2-case Node guard test |

Tricky parts:

- The admission order is observable (nothing may be created for a problem with the
  source, the token is checked before any request), so the port is shaped around the
  legacy call order; the source, client and metadata stay opaque associated types.
- The run takes ownership of the client and metadata; the adapter hands them to the
  job once through a small take-once cell so the port can stay borrowed.
- The copy (`job.rs`, 868 lines) is the bulk of the row and touches Git, LFS, issues,
  pulls and releases stores directly; it is left for later slices rather than wrapped.

Estimate: this slice took about half a focused day. The rest of the row is two to three
slices of one to two days each, mostly the copy's item rules and progress bookkeeping.
