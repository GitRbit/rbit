# Actions task logs: first slice

Evidence for the first slice of the `actions.logs` ledger row (bounded indexed logs,
masking, summaries, offsets and restart reconciliation). The owner is `rbit-actions`. The
row stays `baseline-retained`: this slice moves the rules a log line passes through, and
the file format, reads and recovery remain in the services adapter. It follows the
[identity retirement](identity-retirement.md) pattern.

## What moved

| Decision | Legacy place | Now |
| --- | --- | --- |
| The server masking pass: every secret value and every line of a multi-line one, of at least four characters, longest match first, replaced by `***` | `rbit-services/src/actions/logs/masking.rs` (`Masker`, `MASK`, `MIN_MASKED_CHARS`) | `rbit_actions::logs::{Masker, MASK, MIN_MASKED_CHARS}`, re-exported at the services paths |
| Append body splitting (a final newline ends the last line) and the per-call limit with its message | `append_log`, `logs::types::MAX_LINES_PER_CALL` | `logs::split_body`, `logs::MAX_LINES_PER_CALL` (re-exported) |
| Offset acceptance: a gap names the line to resend from, a fully known resend changes nothing, otherwise the known prefix is skipped | `append_log` | `logs::accept_append` |
| A stored line is masked, loses a trailing `\r` and is cut on a character boundary with ` [line truncated]` | `append::clean_line` | `logs::clean_line` (services passes the protocol's `MAX_LOG_LINE_BYTES`) |
| Byte budget: every line counts; past `max_log_bytes` one marker line is stored and the rest dropped; one little-endian end offset per stored line | `append::append_lines` | `logs::layout` and `LogProgress` (services' `Progress` is now this type) |

`aho-corasick` moves from `rbit-services` to `rbit-actions` with the masker. It is not in
the model boundary's forbidden list, so `rbit-core`'s graph stays within policy.

## What stays

The services adapter still owns: log paths and files, `fsync` ordering against the
database progress, crash recovery that cuts files back to the recorded length, gzip on
finalisation, deletion and retention, window planning over the line index, the batched
job-page reads with their work counters, the per-task masker cache and its rebuild after a
restart (from `task_secrets` and environment values), and the append lock. These are
file-format and I/O concerns tied to `tokio`, `flate2` and the data directory. The
runner's own first masking pass (`rbit-runner/src/mask.rs`, with base64 forms) is a
separate defence and is unchanged.

## Boundary that fails if the moved rules return

`scripts/rewrite/actions-logs-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
rejects in the services, web and server production sources (test modules excluded,
comments stripped) an `AhoCorasick` automaton or `Masker` type, the line-truncation or
budget markers, a redefined `MAX_LINES_PER_CALL` or its message, and newline splitting
followed by a `pop` (the old body splitting). The runner is not checked. Against the legacy
files at the branch's start it reports 5 violations; against this tree, none.
`scripts/tests/actions-logs-retirement.test.mjs` pins the rules and the current tree.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | The moved code is the same code: same splitting, response for gaps and overlaps, masking automaton and match kind, truncation marker and boundary walk, budget marker text and index bytes. The unchanged services log tests (`logs::tests`, `logs::batch_tests`) still exercise `Masker`, `clean_line` and `append_lines` through the re-exports and wrappers. |
| Security | Masking is unchanged and still applied to every stored line and job output; secrets reach the masker only for trusted runs (`trust::Grants`). |
| Migration and rollback | No schema, file format or route change; logs written before and after are byte-identical. Rollback is the previous binary. |
| Performance | No extra allocation per line; the layout builds the same two buffers it built inline. No release measurement claimed. |

## Results

A quick run of the repository's own CI jobs at `76999984` (base `42aca5a2`): rewrite
contracts (including the logical-type check), workspace-wide formatting and all-target
Clippy with `-D warnings`, licenses and advisories and the front-end build passed. Nextest
ran the changed packages (`rbit-actions`, `rbit-core`, `rbit-services`, `rbit-storage`):
1,410 tests, 1,410 passed, none skipped. Among them, unchanged and passing: every
`rbit-services` `actions::logs::tests` and `actions::logs::batch_tests` case (append, index
and range reads, the budget and crash recovery, masking, longest-match overlap, trimming
and capping, batched windows) and the Actions integration suites; the five new
`rbit-actions` `logs::tests` passed too. `rbit-web` log pages and `scripts/tests/actions-log.test.mjs`
were not part of the quick run. Windows legs were not run. Locally, a `check` of the touched
crates was clean.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus the boundary commit and this record) |
| `rbit-actions` production | 4 files, +164 / −1 |
| `rbit-services` production | 4 files, +19 / −104 |
| Tests | 1 new file, +58 (the masking test duplicated from services, which keeps its own) |
| Checker and policy scripts | one new 29-line Node guard, a 36-line test |

Estimate: the remaining parts of this row (recovery, window planning, finalisation and
retention) are I/O-bound; moving them means file-system ports in `rbit-actions`, about two
sessions.
