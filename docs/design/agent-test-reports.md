# Agent loop: test reports, fingerprints and flaky tests

Status: built (first part). Work packages 3.4, 3.5 and 3.6. "Which test failed" was a
search through a log, every red run looked new, and a flaky test was chased like a bug.

## As built

- Test reports (`rbit_actions::reports`): an artifact named `rbit-test-report`, or that
  name and a `-suffix` (one per matrix leg: a run's artifact names are unique), is read when
  it is stored. Its `.xml` files (at most 16 MiB of XML) are scanned for `<testcase>` and
  the `<failure>` or `<error>` in it by a bounded scanner with no dependency: comments,
  declarations and `CDATA` are handled, only the five XML entities and numeric references
  are decoded. The first 50 failures are kept with an id (`<classname> <name>`), the file and
  line from the report or from a Rust `panicked at path:line:col` line, and a message of at
  most 1 KiB. A report that cannot be read is logged; the upload never fails because of it.
- Migration `0079_actions_test_reports.sql`: counts per task and the failed tests in order.
  rbit's own CI uploads nextest's JUnit report (`--profile ci`).
- Fingerprints: a fault is a failed test, or, without a report, the step that ended the job
  with its first error annotation (else its termination reason). Its fingerprint hashes the
  job name, the test or step and the message with digits, hex ids of seven or more
  characters and directories removed. The digest compares a run's faults with the previous
  finished run of the same workflow and ref (`seen`: `new` or `recurring`; `resolved` lists
  what is gone, at most 10) and with the newest finished run on the default branch before it
  (`also_failing_on_base`). Nothing is stored: fingerprints come from the records.
- Flaky: a failed test of a job at a commit is `flaky` when a job of the same name passed
  at that commit, in another attempt or another run. Earlier attempts are in the API
  (`…/jobs/{id}/attempts`, `…/annotations?attempt=`, `…/steps/{n}/log?attempt=`).
- `rbit run rerun [RUN | --sha COMMIT] [--failed] [--json]`: without a run id, every failed,
  cancelled or timed-out run of the commit.

## Tests

Unit: the scanner on a nextest report, a report with attributes, `CDATA` and entities,
and hostile input (unclosed tags, huge counts, bad references); reading the artifact
archive; normalisation and fingerprint stability across paths, numbers and ids; faults per
test or per step and the `new`/`recurring` comparison.

## Left out

An end-to-end test through a real runner upload, storage tests of the flaky query, a
history of flaky tests across commits, and quarantining a flaky test.
