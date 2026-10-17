# Source audit — 2026-10-17

The audit covered the eleven workspace crates, frontend scripts, server configuration,
deployment and CI files. Three agents reviewed Git/SSH/transport, services/storage/core,
and web/security boundaries; the primary agent integrated changes, reviewed their
interactions and ran the shared quality gate. Existing interface and repository-view
changes in the working tree were retained and excluded from the audit commit.

## Findings fixed

| Area | Failure and resulting behavior |
| --- | --- |
| Registration | Concurrent password or external registrations could all become the first administrator, including on closed instances. Account creation now selects the bootstrap administrator inside the SQLite write transaction. |
| Sessions | A stale renewal could restore a revoked session. Renewal now updates only an existing, unexpired entry; replacement also maintains the account index. |
| Password changes | Revocation depended on broadcast delivery. Credential finalization now awaits enforcement, continues after client cancellation, and distinguishes self-service changes from recovery/admin resets. |
| Password recovery | Separate token consumption, password replacement and token invalidation admitted races. They now share one transaction that checks the current email and suspension state. Stale password proofs cannot overwrite a newer reset or change the email. |
| Two-factor authentication | Enrollment cancellation, confirmation and replay claims could act on changed state. Conditional writes bind operations to the verified secret, and pending sign-ins are consumed only after a valid proof. |
| Resource limits | Concurrent requests could exceed token and release-file counts. Storage enforces both limits atomically. Mail now bounds retained work and relay concurrency, with backpressure for account mail. |
| Outbound requests | Git re-resolved hostnames after SSRF validation. Mirrors, imports and push mirrors now pin validated addresses and disable proxies/redirects. Webhooks validate the same canonical URL they send; additional internal IPv6 forms are rejected. |
| SSH authorization | Persistent connections retained revoked keys or administrator status. Every exec request now reloads the key identity and current holder permissions. |
| Branch protection | Filesystem case aliases could bypass protection or Git's non-fast-forward enforcement. Windows/macOS protection matches case aliases and rejects noncanonical protected namespaces. |
| Git process lifecycle | Abandoned HTTP streams left child work running; output errors could leave exchanges waiting on another stream. Response ownership and coordinated cancellation now stop that work. |
| Git resource use | Remote diagnostics were unbounded; LFS inventory was buffered and deduplicated quadratically. Output retention is bounded, inventory streams with backpressure, and object deduplication uses a hash set. |
| Git parsing | SHA-256 deletion commands were mistaken for updates, and malformed protocol headers produced inconsistent framing. Both cases now have explicit validation. |
| Language attributes | Repeated `**` path patterns caused exponential recursive matching. An iterative matcher handles adversarial and deep paths without recursive partition enumeration. |
| LFS authorization | Object-scoped transfer credentials could reach repository-wide batch or lock operations. Those endpoints now require repository-scoped credentials; download replies report the stored size. |
| Uploads and release files | Cancellation could leave partial files or interrupt placement. Temporary files have cleanup ownership; completed placement and deletion run in tracked tasks. Release metadata/file mutations are exclusive while downloads use shared read access; delayed repository cleanup preserves current assets when IDs are reused. |
| HTTP and browser privacy | Private downloads lacked a consistent shared-cache barrier; offline caching could capture authenticated content. Responses default to private revalidation, the offline page is anonymous, and the service worker caches only correctly versioned immutable assets. |
| Navigation and validation | Unsafe redirect controls could panic or redirect incorrectly; hover preloads could mark notifications read. Redirect validation and preload exclusions now prevent these cases. API dates and pagination overflow are validated; HTML rate-limit responses retain `Retry-After`. |
| Dependencies | Updated ammonia to 4.2.4, inout to 0.2.4, the build watcher to 2.6.0 and selector parser to 7.1.6. Added JavaScript regressions to the CI asset job. |

Regression coverage includes real SQLite/redb concurrency, persistent SSH connections,
actual Git HTTP requests with pinned DNS, request cancellation, reused repository/asset
IDs, adversarial patterns, web responses and service-worker behavior.

## Verification

- Clean frontend dependency installation: passed, zero reported vulnerabilities.
- Frontend build: passed; vendored scripts unchanged.
- JavaScript tests: 5 passed.
- Cargo dependency advisories and licenses: passed.
- Workspace formatting and whitespace checks: passed.
- Strict Clippy across all workspace targets (`-D warnings`): passed.
- Full Rust workspace tests: 1,127 passed, zero failures or ignored tests, including
  all 80 end-to-end tests. Tests ran with four build jobs and four harness threads.

Commands used:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --target-dir target/audit-quality -j 4 -- -D warnings
cargo test --workspace --locked --target-dir target/audit-quality -j 4 -- --test-threads=4
cargo deny --locked check licenses advisories
npm ci --prefix target/audit-frontend --no-fund
npm run build
npm test
git diff --check
```

The clean npm install used copied manifests in an isolated directory because the live
development watcher held a native module open in the existing installation. The build
and JavaScript tests initially ran against the workspace. Before committing, the exact
staged sources were exported to `target/audit-commit-source/` and formatting, strict
Clippy, the full Rust suite, the frontend build and JavaScript tests were rerun there.
The counts above are from that isolated audit-only version. Its frontend build left all
staged source and vendored asset files unchanged. Rust checks used a separate target
directory to avoid replacing the running development server.

Detailed local gate logs are under `target/audit-review/` (ignored build output).
Runtime verification is performed on Windows; Linux and macOS were not executed in this
local session. The audit does not establish the absence of all defects or claim
benchmarked throughput gains.
