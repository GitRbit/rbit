# Batched Actions log read candidate

The immediately used `ActionsService::job_log_detail` resolves enabled/readable
repository, expected run, repository-scoped job, current task, steps and actor names
once per normal request. The job route consumes its ordered windows. Templates,
three-second polling, per-step budgets, dedicated step routes and raw downloads
are unchanged. It introduces no cache, migration, runtime dependency or new crate.

## Read boundary

The reader captures a complete index extent, additionally clamped to the selected
task's committed line and byte progress. Uncommitted append entries cannot advance
a page view. Selected offsets are checked for monotonicity, ingestion line size
and committed byte extent before allocating. Index entries are read in bulk per
window. Missing-index behavior remains an empty window, not invented expiry.

One data descriptor serves all nonempty windows. Plaintext seeks selected ranges;
finalized gzip sorts windows and decodes forward once, buffering only overlapping
look-ahead and returning windows in original step order. Duplicate/overlapping
steps keep their existing output. The existing at-least-one-line byte overshoot
and per-step limits remain; this is not a new aggregate job byte/DOM bound.

Only a missing data representation plus a forward persisted Live-to-Final/Expired
or Final-to-Expired state change allows one authorized retry. An unchanged state
keeps the real I/O error. No read transaction or append lock is held while decoding.
The task ID is pinned from job metadata; a later rerun is observed on a later
request rather than combining a new task's steps with old task bytes. This is not
a serializable database/filesystem snapshot. Dropping the blocking awaiter does
not claim the underlying read has stopped; owned paths/buffers live in the closure.

Batch I/O removes the old per-step await. The route yields between groups of 16
view conversions to retain cooperative scheduling. Askama rendering, closed-step
DOM size, focus/scroll and cursor/delta design remain separate work.

## Actual work counters and written acceptance fixtures

Window-reader metrics count index/data open attempts, constructed gzip decoders
and bytes returned by the inflated reader. They contain no IDs, contents, paths or
secrets. The legacy reader uses the same wrapper, so parity fixtures compare real
repeated prefix inflation rather than estimates based on step counts. Compressed
input readahead, process RSS and wall/CPU time are not inferred from these counters.

`crates/rbit-services/src/actions/logs/batch_tests.rs` contains:

- Legacy/batch exact window parity over Tail/From, byte/line budgets, empty ranges,
  multibyte text, ANSI/command text, duplicate/overlapping/out-of-order windows,
  plaintext and finalized gzip.
- A 512-step fixture measuring legacy repeated prefix reads against one batch
  index/data open and decoder, with output equality and inflated-byte bounds.
- Complete committed extent clamping with uncommitted/partial index entries;
  captured plans remain fixed across append.
- Real temporary-file handle retention through deletion, a planned plaintext read
  racing gzip finalization, recorded expiry with no file I/O, truncation with an
  indexed cursor distinct from acknowledged dropped lines, capped oversize-line
  parity and bounded malformed-index errors.

`crates/rbit-services/tests/actions_log_reads.rs` uses existing task-local
`rbit_storage::count_work` for actual statements/acquisitions. Fixtures cover
1/8/32/128/512 steps, legacy/batch output parity, private/revoked/disabled access,
wrong run/repository, expiry versus not-started and new-attempt isolation while old
files remain. Existing web job/raw tests continue exercising the production route
and escaping/download contract; no prototype markup is treated as production proof.

## Verification status

Rust formatting and source policy have been checked. Rust tests, strict Clippy,
wall/CPU/RSS measurements and browser acceptance await the shared validation lane.
Written assertions are candidate evidence until executed; no measured speedup or
visual acceptance is claimed by source inspection. Run the services log unit and
`actions_log_reads` integration tests, existing web `actions_job_pages` tests, then
strict Clippy through the shared workspace helper. Use serial paired timing/memory
runs for any later performance claim; do not globally clean compiler or OS caches.
