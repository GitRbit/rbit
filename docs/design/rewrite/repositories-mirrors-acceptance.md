# Pull and push mirror retirement acceptance

Acceptance evidence for the `repositories.mirrors` ("Scheduled pull mirrors, failures
and archive suspension") and `repositories.push-mirrors` ("Push mirror credentials,
retry and outbound protection") ledger rows. It follows the
[identity retirement](identity-retirement.md) pattern.

## Scope

| Decision | Before | After |
| --- | --- | --- |
| Pull mirror interval: the default, the minimum, a year at most, and the error text | `MirrorService::checked_interval` | `rbit_repositories::mirrors::checked_interval` |
| What one fetch does: skip mirrors and repositories that are gone, skip archived ones, show progress and clear it however the fetch ends, record time and error | `MirrorService::sync` | `mirrors::sync` over `MirrorSync` |
| Which default branch a fetched mirror adopts, and when the upstream is asked | `adopt_default_branch` | `mirrors::{needs_default_branch, adopted_branch}` |
| Scheduler bounds and texts: tick, concurrent fetches, batch, push refusal, phases | `mirrors.rs` constants | `mirrors` constants, re-exported by services |
| Push mirror limit and its text | `PushMirrorService::create` | `push_mirrors::limit_error` |
| Where a stored secret may follow an edit (same scheme, host and port; local remotes are their own origin) and the error | `same_origin`, `update` | `push_mirrors::{same_origin, secret_must_be_entered_again}` over a feature-owned `RemoteOrigin` |
| The user name sent with a secret (`git` when empty) | `attempt` | `push_mirrors::credentials` |
| What one push does: claim an interval ahead first, skip gone or archived repositories, record time and error | `PushMirrorService::push` | `push_mirrors::push` over `MirrorPush` |
| Push and debounce times (saturating) | inline arithmetic | `push_mirrors::{next_regular_push, after}` |

## What stays in `rbit-services`, unchanged

- **The outbound address guard.** `check_public_host` runs before every fetch and push
  and when remotes are saved, exactly as before; the fetch and push still pin Git's
  connection to the approved addresses and refuse redirects (`rbit_git`).
- **Credentials.** Secrets are stored and read exactly as before and never logged;
  the `Authorization: Basic` value is still encoded in services
  (`basic_authorization`, also used by imports) and reaches Git only through its
  environment. The feature only decides which user name goes with the secret.
- **Remote URL validation.** `MirrorUrl`, `PushMirrorUrl::parse`, `validate_username`
  and `validate_secret` stay in `rbit-core`, untouched; services map a parsed URL to the
  feature's `RemoteOrigin`.
- **Scheduling runtime.** The scheduler loops, the per-mirror running sets, the
  semaphores, debounce wake-ups and shutdown are unchanged; they use the feature's
  constants. Store queries for due mirrors, claims and backoff keep their SQL.
- **Composition.** `mirrors/port.rs` and `push_mirrors/port.rs` implement the ports
  over the stores, the system repository loader, progress, Git, logging and the
  `rbit_mirror_fetches_total` and `rbit_push_mirror_pushes_total` counters.

## Legacy code removed

Pull mirrors: the interval check, the sync sequence, the progress guard, the branch
adoption choice and the duplicated constants and phase enum. Push mirrors: the limit
check, the origin comparison and its error, the user name default, the push sequence and
the saturating time arithmetic.

## Boundary that fails if legacy paths return

`scripts/rewrite/mirrors-retirement.mjs`, run by `check.mjs` on its own lines, scans
production sources outside `rbit-storage` and `rbit-repositories`, ignoring comments
and `#[cfg(test)]` modules: mirror outcomes and push claims (`record_sync`,
`record_attempt`, `claim` on a mirror store) are written only by the two port adapters,
and the services mirror modules must not redefine the moved constants, the phase enum,
the progress guard or the policy error texts. The pre-branch `mirrors.rs` and
`push_mirrors.rs` fail both rules; `scripts/tests/mirrors-retirement.test.mjs` covers
them and the whole tree.

## Parity argument

- Routes, forms, error texts, log lines and metric names are unchanged; the services'
  public methods and constants keep their names and signatures.
- Calls happen in the same order: a fetch reads the mirror, then the repository, shows
  `Connecting`, fetches (address check, push report, lease, `Fetching`, `Finalizing`,
  branch adoption), logs and counts, records with a fresh clock reading, and clears its
  progress last; the upstream's default branch is still asked for only when the
  mirror's default is missing. A push reads its target, claims it an interval ahead
  from the current time, loads the repository, skips archived ones, pushes, logs and
  counts, and records with a fresh clock reading.
- The same-origin rule compares the same three parts, case-insensitively for the host,
  and an unreadable stored URL still never matches.
- Address checks, credential storage and encoding, URL validation, scheduling and
  backoff are unchanged. No schema, migration or stored format changed.

## Results

One Linux run of the repository's own CI jobs at the last code commit (changed
packages only, base `0ec76cae`): the rewrite contracts (the policy checker, the
structural type gate and the Node policy tests), workspace-wide formatting and
all-target Clippy with `-D warnings`, licenses and advisories, and the front-end build
passed. Nextest for `rbit-repositories` and `rbit-services` ran 829 tests, all passed.

Mirror-relevant groups, all passing:

| Group | Tests |
| --- | --- |
| `rbit-repositories` unit: intervals, branch adoption, limits, origins, user name, times | 5 |
| `rbit-repositories` integration `mirrors` (fake ports, new): fetch sequence and skips, push claim and skips | 3 |
| `rbit-services` integration: `mirrors` 1, `push_mirrors` 4 | 5 |
| `rbit-services` unit: `push_mirrors` 1 (the Basic header), `imports` 10 (shares the header), `outbound` 26 (the address guard) | 37 |
| `rbit-services` integration `imports` | 5 |

Not run for this branch: Windows legs and the `rbit-web` and `rbit-e2e` suites, which
quick mode skips because no file in those crates changed. The record commit only adds
two component-type registrations and the ledger rows after that run.

## Migration, rollback and performance

No migration, schema or stored format changed; rollback is the previous binary. Fetches
and pushes issue the same statements and Git commands. No release measurement was taken,
and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record and the ledger rows) |
| Files changed by code commits | 13 |
| Lines added / removed (all code commits) | 620 / 105 |
| Production Rust (`src`) | 7 files, +405 / −105 |
| Tests | 1 new fake-port test file (+141), unit tests in the new modules, a 3-case Node guard test |

Tricky parts:

- `rbit-repositories` must stay a model-only graph: a first version encoded the Basic
  header there and the policy checker refused the `base64` dependency. The feature now
  decides only which user name goes with the secret; encoding stays with the shared
  services helper that imports also use.
- The upstream's default branch is a network call, so the adoption rule is split into
  "does it need one" and "which one" to keep it as lazy as before.
- Both services files were near or over the 400-line limit; the port adapters went
  into new child modules so neither grew.

Estimate for similar rows: these two rows, mostly orchestration around an unchanged
guard, took about half a focused day together once the port pattern was settled.
