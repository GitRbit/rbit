# Repository import retirement acceptance

Acceptance evidence for the `repositories.imports` ledger row ("GitHub/Gitea imports
with history, issue numbers, LFS and releases"), retired in four slices: admission and
lifecycle first, then the copy's phase plan and progress bookkeeping, author mapping and
attribution, and the per-item rules. It follows the
[identity retirement](identity-retirement.md) pattern.

## Slice 1: admission and lifecycle

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

## Slices 2 to 4: the copy

| Decision | Before (`imports/job.rs`) | After (`rbit_repositories::imports`) |
| --- | --- | --- |
| Import options, status, phases, counts, the warning and message caps and the text helpers (`clean_message`, `truncate_*`, `label_color`) | `rbit-core` | `model`, re-exported unchanged by `rbit_core::import` |
| The phase plan (LFS only while the instance serves it) and progress bookkeeping: saves on every phase and every 25 items, warnings cleaned and capped at 50, the rest counted | `Job::{run, enter, save, tick, warn}` | `progress::{phase_plan, ImportProgress}` |
| Author mapping: none, by verified email (from the listing or, failing that, the source profile), optionally by user name; each login resolved once | `Job::author` | `authors::AuthorMap` over `AuthorDirectory` |
| Attribution: "Originally by [@login](url) on Platform." with a sanitized login and link, `ghost` for deleted authors | `Job::attributed` | `authors::attributed` |
| Labels (name, colour, description, skip warning), milestones (title, description, due date), lookup keys | `import_labels`, `import_milestones`, `attach_metadata` | `items::{label, milestone, due_date, lookup_key}` |
| Threads: title and `(untitled)`, how they end up closed, the note for open pull requests without commits, branch data fallbacks, merge commits, the number reservation, comment text, item warnings | `import_issues`, `import_thread`, `pull_row`, `comments`, `closing_events` | `items::{thread_title, closing, closed_pull_note, non_empty, history_head, merge_commit, merged_event_commit, same_repository, highest_number, comment_text, …}` |
| Releases (tag, name, notes) and their files (size and name refusal, which URL and `Accept`, incomplete files), the LFS batch endpoint | `import_release`, `import_asset`, `copy_lfs` | `items::{release, asset_refusal, asset_download, incomplete_asset, lfs_batch_endpoint}` |

Thread and release text limits stay owned by their features and are handed in as
`items::Limits`. The job keeps only orchestration: the stores, the source client, Git,
LFS storage, credential routing and which client failures stop an import.

## What stays in `rbit-services` by design

- `imports/client.rs`, the source API client with the shared outbound guard, and
  `imports/api.rs`, the source's wire shapes.
- Credential routing (`Job::token_for`: the token goes only to the source's own hosts)
  and the shared `basic_authorization` encoding.
- Which client failures stop an import (cancellation, rate limits, a refused token)
  and which become warnings: these are classes of the client's own errors.

## Boundary that fails if legacy paths return

The copy slices add two rules to the same script: `imports/job.rs` must not redefine the
moved constants, helpers, warning cap, progress counter or any of the moved texts, and
`rbit-core/src/import.rs` must not define the models and helpers it now re-exports. The
pre-branch `job.rs` and `import.rs` fail them.

Slice 1:

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
- The copy makes the same store, client and Git calls in the same order with the same
  values: the phase plan dispatches the same phases in the same order, progress is
  saved at the same moments, warnings keep their exact texts, and authors are looked up
  with the same calls and cached by lower-cased login. The moved models keep their
  serde shapes and stored strings, and every existing `rbit-core` test of them runs
  against the re-exports unchanged.
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

Slices 2 to 4, one Linux run at their last code commit (changed packages only, base
`4f7303b3`): every job passed, including the rewrite contracts and all-target Clippy.
Nextest for `rbit-core`, `rbit-repositories` and `rbit-services` ran 988 tests, all
passed: `rbit-core` import 5 (now against the re-exported models), `rbit-repositories`
unit `imports` 8 and integration `import_authors` 3 (new) and `imports` 3,
`rbit-services` unit `imports` 9 (the job's commit id test moved, unchanged in
substance, to `items_tests.rs`), `outbound` 26 and integration `imports` 5. One earlier
run of the work in progress failed only on a Clippy lint in the new `closing` rule,
fixed before the tip. Windows legs and the `rbit-web` and `rbit-e2e` suites were not
run.


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

Slices 2 to 4:

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record and the ledger row) |
| Files changed by code commits | 11 |
| Lines added / removed (all code commits) | 877 / 376 |
| Production Rust (`src`, non-test files) | 7 files, +672 / −373, of which `job.rs` +132 / −213 and `rbit-core` +7 / −154 |
| Tests | 2 new or extended test files (+133), unit tests in the new modules, one more Node guard case |

Tricky parts: the import models were already shared through `rbit-core`, which may
depend on `rbit-repositories` but not the reverse, so they moved and are re-exported
rather than duplicated. Thread and release limits stay with their features and are
passed in. `job.rs` kept its orchestration, so its diff is mostly calls replacing
inline rules. Estimate: the three copy slices took about one focused day together.
