# Actions task logs retirement acceptance

Acceptance evidence for the `actions.logs` ledger row (bounded indexed logs, masking,
summaries, offsets and restart reconciliation). The owner is `rbit-actions`. Two slices
retire it: the first moved the rules a log line passes through, the second the log files
themselves behind a file-system port. It follows the
[identity retirement](identity-retirement.md) pattern.

## What moved

| Decision | Legacy place | Now |
| --- | --- | --- |
| The server masking pass (secret values and their lines of at least four characters, longest match first) | `rbit-services/src/actions/logs/masking.rs` | `rbit_actions::logs::{Masker, MASK, MIN_MASKED_CHARS}` (first slice) |
| Append body splitting, the per-call limit, offset acceptance (gaps, resends, overlap) | `append_log` | `logs::{split_body, accept_append, MAX_LINES_PER_CALL}` (first slice) |
| Line cleaning and the byte budget, with one index entry per stored line | `append::{clean_line, append_lines}` | `logs::{clean_line, layout, LogProgress}` (first slice) |
| Write and sync ordering: lines, then index entries, then both synced, so the caller records progress only after the files are durable | `append::append_lines` | `logs::append_lines` |
| Crash recovery: before a process first appends to a task, cut the log to the recorded length and the index to entries within it | `append::append_lines` | `logs::append_lines` (`recover`) |
| Finalisation: gzip (level 6) into a partial file, sync, rename into place, then remove the plain log; the index stays | `archive::compress` | `logs::compress`, `GZIP_LEVEL` |
| Retention and deletion: remove every file, missing ones allowed | `ActionsService::delete_log` | `logs::remove` |
| Raw log archive entries (decompressed size first, streamed) and archive file names | `archive::{add_raw_log, log_file_name}` | `logs::{add_raw_log, log_file_name}` |
| Window planning: committed, complete index extent; strictly growing index; the ingestion cap; line and byte budgets for ranges and tails | `plan.rs` | `logs::{durable_extent, plan, WindowRequest, WindowPlan}` |
| Reads: a single window; every window of a job page from one index snapshot and one data descriptor, inflating gzip once; read counters; the one-retry rule after a representation change | `reader.rs`, `batch_reader.rs`, `work.rs`, `batch::retryable` | `logs::{read_window, read_windows, prepare, read_plans, read_data, ReadWork, Representation, Snapshot, retryable}` |
| Window, range and limit types and the step view's bounds | `types.rs` | `logs::{LogWindow, LogRange, LogLimits, STEP_VIEW_LINES, STEP_VIEW_BYTES}` |

**The port.** `TaskLogFiles` names one task's files (`LogPart`: log, index, gzip, partial
gzip) and how they are opened, created, renamed and removed; `LogFile` is an open file
with length, truncation and sync. `rbit-actions` implements `LogFile` for `std::fs::File`.
The services disk adapter (`rbit-services/src/actions/logs/disk.rs`) implements
`TaskLogFiles` for `LogFiles`, the paths `<data_dir>/actions/logs/<repo>/<task>.{log,idx,log.gz}`.

## What stays in the services composition, by design

- The per-task append lock and whether this process has already recovered a task, the
  `spawn_blocking` hand-off, and recording progress in the database after the files are
  synced (`write_lines`), plus marking a log final after compression.
- Authorization of reads (readers of the repository, the job's current task) and the
  job-page orchestration that retries once when `retryable` says the representation moved.
- The per-task masker cache and its rebuild after a restart from the task's secrets and
  environment values: that reads the secrets store, which belongs to `actions.secrets`.
- Step summaries are masked with the same `Masker` and stored through the run store; their
  grouping for pages is presentation.
- Metrics: services publishes `ReadWork` and log byte counters.
- The services modules keep thin wrappers at their old paths, so the unchanged
  `logs::tests` and `logs::batch_tests` suites drive the moved code.

## Boundary that fails if the moved rules return

`scripts/rewrite/actions-logs-retirement.mjs`, run by `check.mjs`, rejects in the services,
web and server production sources a second masking automaton, the line-truncation or
budget markers, a redefined append limit and the old body splitting (first slice). It now
also rejects, in the services' log module except the disk adapter and tests, direct file
I/O: `OpenOptions`, `set_len`, `sync_data`/`sync_all`, gzip encoders or decoders,
`SeekFrom`, index decoding with `from_le_bytes`, `remove_file` and `create_dir_all`.
Against the legacy log files it reports violations in `append.rs` and `archive.rs`; on this
tree, none. `scripts/tests/actions-logs-retirement.test.mjs` covers each rule and the
whole tree.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | The moved code is the same code over the port: same open modes, write and sync order, recovery walk, gzip level and rename order, plan arithmetic, error kinds and messages. The unchanged services `logs::tests` (append, index, range reads, budget, crash recovery, masking, compression) and `logs::batch_tests` (batched windows equal single reads, one-pass inflation and handle counts at 512 steps, durable extents, retention mid-read, retry rules, expiry, corrupt index) exercise it through the adapter. |
| Security | Masking is unchanged and applied to every stored line, job output and step summary. |
| Migration and rollback | No schema, file format or route change; files written before and after are byte-identical. Rollback is the previous binary. |
| Performance | Same syscalls in the same order; the port adds no allocation on the append or read paths. Deletion now runs its three removals on a blocking thread instead of three async removals. No release measurement claimed. |

## Results

A quick run of the repository's own jobs at `a9d97774` (base `88d16e99`; this record's
text was filled in afterwards): rewrite contracts (including `check-types.mjs`),
workspace-wide formatting and all-target Clippy with `-D warnings`, licenses and
advisories and the front-end build passed. Nextest ran the changed packages
(`rbit-actions`, `rbit-services`): 802 tests, 802 passed, none skipped. Among them, all
passing: every unchanged services `actions::logs::tests` and `actions::logs::batch_tests`
case (15), the Actions integration suites, the five first-slice `rbit-actions`
`logs::tests` and the two new `logs::file_tests` (recovery and finalisation over a real
directory; committed-snapshot page reads, expiry without file access, the retry rule). The
first slice's results (1,410 tests at `76999984`) are in its own run. `rbit-web` log pages
and `scripts/tests/actions-log.test.mjs` were outside the changed-package set; no web file
changed. Windows legs were not run.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | first slice 2 plus its boundary; this slice 3 (port and rules, adapter, boundary) plus this record |
| This slice: files changed by code commits | 20 |
| This slice: lines added / removed | about 900 / 480 |
| `rbit-actions` production | 6 files, about +640 (the moved file code, now generic over the port) |
| `rbit-services` production | 11 files, +138 / −479 (wrappers and the 56-line disk adapter remain) |
| Tests | new 93-line `rbit-actions` file test module; the services log suites unchanged |
| Checker and policy scripts | the existing guard gains one file-I/O rule and one test case |

Tricky parts:

- The unchanged services test suites call private module functions by their old paths and
  signatures, so every moved function keeps a thin wrapper there.
- `LogState` lives in `rbit-core`, which `rbit-actions` may not depend on; the read rules
  use their own `Representation`, and the services adapter maps the stored state onto it.
- Functions used only by the services tests became `#[cfg(test)]` wrappers so
  production builds keep no dead code.

Estimate: as the first slice predicted, about two sessions for the I/O half; most of the
cost was keeping the test-visible paths stable rather than the move itself.
