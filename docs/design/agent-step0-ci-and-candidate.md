# Agent loop, step 0: rbit's CI on rbit, the candidate check and the baseline

Status: in progress. This is the contract for the first step of making rbit a good place
for coding agents to work: the loop **push → CI result → fix → push** should be short,
cheap to read, and never give a wrong answer. Step 0 builds nothing new into the product.
It moves rbit's own development onto an rbit instance, adds a check that every build can
start and run CI, and measures how the loop behaves today so later steps can show what
they improve.

## Scope

1. `.rbit/workflows/ci.yml`: the Linux half of `.github/workflows/ci.yml` for a
   self-hosted runner, plus a candidate job.
2. `.rbit/workflows/focused.yml`: a dispatch-only workflow that tests one package.
3. `scripts/agent-loop/`: the candidate harness and the agent-loop benchmark, with unit
   tests in `scripts/tests/agent-loop.test.mjs`.
4. `scripts/ci/install-cargo-deny.sh`: a version- and checksum-pinned `cargo-deny`.

Out of scope: any change to the server, the runner, the CLI or the API; GitHub's CI;
Windows (GitHub keeps testing it).

## The workflows

rbit reads `.rbit/workflows` and ignores `.github/workflows` when the first exists; GitHub
ignores `.rbit/`. The split is deliberate: GitHub keeps its own CI unchanged, and its
contract test (`scripts/ci/required-results.mjs`) does not cover `.rbit/`.

`ci.yml` runs on every push and on dispatch. All jobs ask for the label `linux`, the only
machine fact a workflow may carry; the runner provides the tools (Rust stable with
rustfmt and clippy, cargo-nextest, Node.js 24, git, curl, a C toolchain). Jobs:

| Job | What |
|---|---|
| Rewrite contracts | the script tests, `check.mjs`, the feature-contract builds of the GitHub Linux leg, `check-types.mjs` |
| Format and clippy | `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` |
| Licenses and advisories | `cargo deny` from a pinned, checksum-verified release |
| Front-end build | `npm ci`, `npm run build`, `npm test`, no change to committed assets |
| Tests | `cargo nextest run --workspace`, doc tests, the upgrade rehearsal |
| Release build | release binaries, the rehearsal on them, `SHA256SUMS`, uploaded as `rbit-linux-x86_64` |
| Candidate | the candidate check below, run on the uploaded artifact after verifying its checksums |
| CI required | the one status branch protection requires; fails unless every job succeeded |

`concurrency` cancels a branch's older run when a newer push arrives; runs on `main` are
never cancelled. Tests run unsharded: shards only pay off with several runners.

Pull request events are not a trigger. A pull request's head commit gets its statuses
from the branch push, so one push costs one run, and the required check is
`CI / CI required (push)`.

## The candidate check

`node scripts/agent-loop/bench.mjs --rbit <rbit> --runner <rbit-runner> --quick`

1. Starts `rbit serve` on a free loopback port with an empty data directory, Actions on,
   update checks and rate limits off, and a random setup token.
2. Signs up the first account and creates a `write` token (the agent's) and an `admin`
   token (only to get a runner registration token), through the same forms a browser
   uses. No API creates tokens yet.
3. Registers `rbit-runner` on the host backend with the label `agent-loop` and starts it.
4. Creates a private repository and pushes a generated fixture project to it.
5. Runs the scenarios, grades every answer, writes a JSON report, and stops everything.

A server that does not start, a runner that does not register, or a "correct" scenario
answered wrong exits 1, which fails the job. Logs of a failed start are kept and named.

Secrets never reach a command line or a log: git receives the token through
`GIT_CONFIG_*` environment variables; the runner receives its registration token through
`RBIT_RUNNER_TOKEN`.

## The fixture project

Generated at run time (nothing is committed), in `fixtures.mjs`: a shell "calculator"
with a linter that prints `path:line:col: error:` and a `::error file=…` annotation, a
test runner that prints 3,000 passing lines before its results, and a workflow with two
jobs whose `paths-ignore` skips docs-only pushes. Each scenario changes it in a known way,
so the true answer is known exactly and grading is a comparison, not a judgement.

## Scenarios

`correct` scenarios must be answered right today; a wrong one fails CI. `known_wrong`
scenarios are traps today's rbit falls into. They are recorded each run; when a later
step fixes one, the report lists it under `fixed` and that step moves it to `correct`.

| Scenario | Gate | True answer |
|---|---|---|
| `green` | correct | success for this commit |
| `compile_error` | correct | failure at the seeded `path:line` |
| `test_failure` | correct | failure at the seeded test's `path:line`, after 3,000 lines of noise |
| `status_after_push` | known_wrong | "expected": no check has reported yet, which is not success |
| `flaky` | known_wrong | labelled flaky (fails once, passes on a re-run of the same commit) |
| `timeout` | known_wrong | the job reads `timed_out`, not a plain failure |
| `docs_only` | known_wrong | "none expected" at once, not a wait that ends in "no run" |
| `newest_run_trap` | known_wrong | the run for my commit, not the repository's newest |
| `head_moves` | known_wrong | "head changed" when a newer commit lands during a watch |
| `burst` | known_wrong | ten pushes: one finished run, nine superseded |

`--quick` runs the first four; the full set takes several minutes and runs on demand.
Not yet covered: a job killed mid-log and a lost runner (the server takes 600 seconds to
notice today, too slow for every commit), a required check that reports late, a lost
response to creating a pull request, a merge with an old head, two changes that only fail
together, and a revoked token. They come with the steps that add the matching features.

## The baseline recipe

`recipes.mjs` is what an agent can do with today's API, done carefully: list the branch's
newest runs and pick the one whose `head_sha` is the pushed commit (never "the newest
run"), poll it every 3 seconds until it completes, list its jobs, fetch the whole log of
the first failed job, and take the first annotation, compiler error or failing-test line.
Every call goes through a meter that records calls and response bytes per scenario.

Measured per scenario: calls, bytes, wall time, and for failures the time from push to the
first failure line. Byte and call counts and correctness are deterministic and can become
hard gates; times depend on the machine and are reported, not gated.

## Baseline numbers

Filled in from the first runs on the stable instance's CI; see "As built".

## Tests

`scripts/tests/agent-loop.test.mjs` covers the parts that need no server: failure-line
extraction (annotations, compiler lines, test lines, colour codes and timestamps), the
meter, token scraping, the fixture's workflow and line lookups, the candidate
configuration, scenario selection and the pass/fail summary. The harness itself is tested
by the candidate job on every commit.

## As built

To be completed when the step lands.
