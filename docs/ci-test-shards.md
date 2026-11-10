# CI test shards and re-runs

The workspace tests in `.github/workflows/ci.yml` run as one set of jobs per platform:

| Job | Runs |
| :--- | :--- |
| `Test build (<os>)` | Compiles every test binary once (`cargo nextest archive --workspace --locked`) and uploads the archive with the generated stylesheets as `nextest-archive-<os>`. |
| `Tests (<os>, K/N)` | Downloads that archive and runs partition `hash:K/N` of the nextest suite. Compiles nothing. Linux uses two shards; Windows, where the suite takes about fifteen minutes, uses four. |
| `Doc tests and rehearsal (ubuntu-latest)` | Doc tests, the `rbit-server` debug build and the upgrade rehearsal. |
| `Doc tests (windows-latest)` | Doc tests. |
| `Tests (<os>)` | The required check. It runs after all of the above and fails unless every one of them succeeded. |

The doc-test jobs run alongside the build, so a failing doc test and a failing nextest
test can be re-run independently. `Tests (ubuntu-latest)` and `Tests (windows-latest)`
keep the names the `main` ruleset requires, and `CI required` lists every job above.

## Re-running failed tests

When a test fails, open the run and choose **Re-run jobs → Re-run failed jobs**. GitHub
re-runs only the failed shard (plus the `Tests (<os>)` and `CI required` gates), and
that shard runs only the tests that failed in its previous attempt:

1. After every attempt, each shard parses nextest's JUnit report
   (`target/nextest/ci/junit.xml`, enabled for the `ci` profile in
   `.config/nextest.toml`) and uploads the failed tests (binary id and test name) as
   `nextest-failed-<os>-shard-K-of-N-attempt-<attempt>`.
2. In a later attempt the shard downloads those lists and takes the newest one from an
   earlier attempt. If it names failed tests, the shard runs exactly those tests with a
   nextest filter built by `scripts/ci/nextest-failures.mjs`. Before running, it lists
   the tests the filter selects and checks that they are exactly the recorded ones.
3. Otherwise the shard runs in full: on the first attempt, when the previous attempt
   passed or left no list, when that attempt did not finish (cancelled, timed out,
   interrupted, or nextest failed before running tests), when more than 300 tests
   failed, or when the filter does not select exactly the recorded tests.

Re-running a shard does not compile anything: the build job already succeeded, so it is
not re-run, and the shard reuses its archive. The archive and the lists are kept for
three days. After that, re-running a failed shard fails to download the archive; use
**Re-run all jobs** instead, which builds a new archive. **Re-run all jobs** also
resumes failed tests in shards that failed before, since the commit is unchanged; push a
new commit to run every test again.

Test names never pass through a shell. `scripts/ci/nextest-shard.mjs` receives its inputs
through environment variables, passes the filter to nextest as a single argument, and
escapes every character of a test name outside `[A-Za-z0-9_.:-]` as `\u{…}` so a name
cannot end the matcher or add to the expression.

## Running tests from an archive

Archived test binaries are extracted into the checkout's `target/` directory, the same
absolute paths they were built for, because tests use compile-time paths
(`CARGO_BIN_EXE_*`, `CARGO_MANIFEST_DIR`, and the generated stylesheets that debug builds
read from disk). Hosted runners always check out to the same path, so the paths match.
The archive keeps test binaries in `target/` rather than in the configured
`.cargo-build` directory; the shards set `CARGO_BUILD_BUILD_DIR` to `target` so the e2e
tests that ask Cargo for the build layout find them.

To reproduce a shard locally:

```sh
cargo nextest archive --workspace --locked --archive-file target/nextest-archive.tar.zst
cargo nextest run --archive-file target/nextest-archive.tar.zst --workspace-remap . \
  --extract-to . --extract-overwrite --profile ci --partition hash:2/4
```

## Caches

Only the build and doc-test jobs compile, and only they restore the Rust dependency
cache (see `docs/development-ci-cache.md`). The build job no longer runs tests, so it
saves the cache for a pull request even when a test fails. Later pushes to the same pull
request restore it, and re-runs of failed shards need no cache at all.

The policy and the shard logic are covered by `node --test scripts/tests/rewrite-ci-shards.test.mjs`.
