# Release-check retirement acceptance

Acceptance evidence for the `operations.updates` ledger row ("Version privacy, release
notices and bounded background checking"), following the
[identity retirement](identity-retirement.md) pattern. The user-facing contract is in
[release checks and version privacy](../../history/architecture-pre-0.2.2.md#91-release-checks-and-version-privacy).

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Release eligibility: drafts and prereleases excluded, tag validation, `SemVer` precedence against the running version, link built under the fixed upstream repository | `rbit-server` `updates::release::newer_release` |
| Scheduling: 24-hour success cache, 15-minute doubling backoff capped at 24 hours, longer upstream hint wins | `updates::release::{CACHE_TIME, retry_delay}` |
| Check loop, published state per result, last notice retained during an outage, one task, prompt cancellation | `updates::UpdateChecker` on `rbit_kernel::TaskLifecycle` |
| Outbound request: fixed feed, constant versionless User-Agent, no credentials, cookies, identity or query, HTTPS only, no redirects, 5 s connect, 10 s total, 64 KiB body, `Retry-After` and reset hints | `ReleaseSource` port, `updates::github::GithubReleases` adapter |
| Disabled checks build no HTTP client and start no task | `updates::start` (server composition) |
| Stored snapshot, revision ordering, browser notice metadata, administrator label | `rbit_services::updates::UpdateNotices` and `UpdateStatus` (unchanged text) |
| Footer header, administrator dashboard status, running-version disclosure (`report_version`) | `rbit-web`, `rbit-ssh`, outbound integration clients (unchanged) |

There is no storage: the snapshot lives in memory and a restart performs one new
check, as before. The "storage adapter" for this row is the outbound feed adapter; the
`UpdateNotices` cell is the publication port that pages read.

`serve::run` starts the checker after composing services and stops it together with
the other background owners, waiting at most the same ten seconds the service waited.
Administrator-only access to the check status is unchanged: only the administrator
dashboard renders the label, and the footer receives only the versionless notice.

## Legacy code removed

1. `rbit_services::updates::UpdateService`: its HTTP client, request and body reader,
   `Retry-After` parsing, release decision, backoff, run loop, `CancellationToken` and
   `TaskTracker`.
2. Its start and stop in `Services::spawn_background` and `Services::shutdown`. Test
   harnesses that compose services directly no longer start a live release check.
3. The `semver` dependency of `rbit-services` (moved to `rbit-server`).

## Boundaries that fail if legacy paths return

`scripts/rewrite/operations-updates-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
over every authored Rust file below `crates/` (tests included), comments ignored:

- The release feed address or the `rbit-update-check` User-Agent outside
  `crates/rbit-server/src/updates`.
- The `UpdateService` identifier anywhere.
- `fn newer_release`, `fn failure_delay` or `cmp_precedence(` in the services, web, SSH
  or end-to-end crates.
- `reqwest`, `semver`, `tokio`, spawning, `CancellationToken` or `TaskTracker` in the
  services snapshot module.
- `.updates.spawn_background(` or `.updates.shutdown(` anywhere.

`scripts/tests/operations-updates-retirement.test.mjs` covers each rule, the allowed
server/link/helper/comment cases and the whole current tree.

## Results

Linux runs at the code tip (identical Rust to the recorded commit), plus local Node runs:

| Gate | Result |
| --- | --- |
| `cargo fmt --all --check`, workspace clippy `-D warnings` | pass |
| Full Linux workspace test suite, including the moved `rbit-server` `updates` tests (worker request privacy and caching, redirect/oversize/timeout refusal, disabled start, outage retention and backoff, precedence and hints) and the new snapshot test | pass |
| Front-end build, licenses and advisories | pass |
| `node scripts/rewrite/check.mjs` | 0 violations |
| `node scripts/rewrite/check-types.mjs` | fails only on `rbit-storage/src/identity_access_tokens.rs` (`AccessTokenStore` self type), a base-stack issue outside this row with a pending fix; no violation in the changed crates |
| `node --test scripts/tests/*.test.mjs` | 657 of 666 passed, 8 skipped; the one failure (`htmx-vendor`) needs `node_modules` and is unrelated |

The existing suites ran unchanged, including `rbit-web` `security` (initial
`Pending`/`Disabled` state through `services.updates.status()`), the footer and
dashboard view tests and `scripts/tests/update-notice.test.mjs`.

## Parity argument

- The adapter's client builder, request headers, size checks, chunked bounded read,
  JSON decoding and hint parsing are the legacy code moved verbatim; the same
  `install_crypto_provider` call precedes client construction.
- Initial state: `Disabled` without checks, `Pending` with checks. A client that fails
  to build at start-up now publishes `Unavailable` before the router is created, so
  every rendered page still observes `Unavailable`, and the check still retries building
  the client with network-failure backoff.
- Each observation publishes a strictly newer UTC-nanosecond revision exactly as
  before; reading never advances it. An outage keeps the previous release and moves
  to `Unavailable`.
- Labels, notice metadata, release links and the `UpdateStatus` fields are unchanged,
  so the footer header, browser ordering and dashboard output are byte-identical.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; migration 0073 remains the latest
and no historical migration was edited. Rollback is the previous binary. No request
path changed: rendering still reads one mutex-protected snapshot. No release
measurement was taken and none is claimed.

## Not covered

- Running-version disclosure is enforced where each surface reads `report_version`
  (footer, `/api/v1/version`, SSH banner, outbound User-Agents). Those reads are owned
  by their surfaces and were not centralized by this slice.
- Windows test legs are left to the release build.

## Next row: `operations.admin`

Not retired. The row covers administrator moderation (rights, suspension, deletion
with disk removal and events), the restart-required configuration editor with sealed
secrets, audit recording, the monitoring dashboard and graceful restart. Unlike the
release checker, these are request-path decisions that `rbit-web` reaches through
`rbit-services`; `rbit-server` sits above both, so moving them there needs every
administration call inverted through ports the server injects into the web state. The
decisions themselves (last-administrator and self-action guards, deletion
confirmation, listing normalization, settings change descriptions) fit `rbit-identity`
and `rbit-access` better, with the configuration editor and restart orchestration as
the genuinely server-owned part. Re-targeting the row, or splitting it into
moderation, configuration and monitoring rows, should precede the port. Estimate:
two to three focused sessions, about 30 files and 1,000–1,500 changed lines, mostly
fixtures in `crates/rbit-services/tests/admin.rs` (441 lines) and the web
administration routes.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus this record) |
| Files changed by code commits | 16 |
| Lines added / removed | 566 / 316 (including the architecture note and scripts) |
| Production Rust (`src`, non-test) | 8 files; the services module shrinks from 269 to 97 lines, the server gains 247 (application 95, decisions 48, feed adapter 104) |
| Tests | moved and split: 123 legacy lines become 215 (server application and adapter fixtures 141, decisions 37, hints 15, snapshot 22) |
| Checker and policy scripts | a 37-line Node guard, a 42-line test, two checker lines |
| Manifests | `semver` moved from `rbit-services` to `rbit-server`; no new internal crate edge (`rbit-server` already depends on `rbit-services`, `rbit-core` and `rbit-kernel`) |

Tricky parts:

- Direction of dependencies. Pages read the snapshot through `rbit-services`, while the
  target crate sits above web and services. The split that keeps every existing caller
  and suite unchanged is a publication cell in services and the checker in the server,
  so `services.updates.status()` and the `rbit_services::updates` model paths stay.
- Start-up state. The legacy constructor chose `Unavailable` when the client failed to
  build; services no longer build a client, so the server publishes that state before
  composing the router.
- The `//` comment stripping shared by the other retirement checkers also strips
  `https://` addresses; this guard keeps `scheme://` text so the feed rule can match.

Estimate for similar rows: a background-only operations row whose output is a cached
snapshot is one short session: two code commits, about 15 files and 500–900 changed
lines, most of it moved code and tests. Request-path rows targeted at `rbit-server`
(such as `operations.admin`) cost several times more because of the dependency
inversion described above.
