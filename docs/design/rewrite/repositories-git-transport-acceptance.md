# Git transport retirement acceptance

Acceptance evidence for two ledger rows, following the
[identity retirement](identity-retirement.md) pattern:

- `repositories.git-http` ("Smart HTTP advertisement, protocol v2, clone/push and packet
  limits"), target `rbit-transport`: **ported**.
- `repositories.git-ssh` ("SSH/deploy keys, Git-only exec and per-command
  authorization"), target `rbit-ssh`: **ported** in a second slice; see
  [SSH key registry](#ssh-key-registry).

## Smart HTTP: what moved

Process transport, protocol v2 passthrough (`is_safe_protocol_header`), stateless
advertisement and service runs, push command-list interception and its packet limits
(`receive::CommandLimits`) were already in `rbit-transport`. What still lived in
`rbit-web/src/routes/git_http*` was the decision layer around them:

| Decision | Legacy place | Now |
| --- | --- | --- |
| `Authorization: Basic` decoding (either scheme case, colons kept in the password) | `git_http::basic_credentials` | `rbit_transport::smart_http::credentials::basic_credentials` |
| Task token as `Bearer`/`token` (personal tokens never) | `git_http::bearer_task_token` | `credentials::bearer_task_token` |
| `401` challenge value and body | `Denied::into_response` literals | `credentials::{BASIC_CHALLENGE, AUTHENTICATION_REQUIRED}` |
| Access order: open, then (pushes) write access, then pull mirror; anonymous callers are challenged rather than told "not found"/"forbidden"; lookup failures are `500` | `git_http::authorize` | `smart_http::access::authorize` over the `RepositoryGate` port |
| Refusal texts `repository not found`, `you do not have permission to push to this repository` | `Denied` literals | `access::{REPOSITORY_NOT_FOUND, PUSH_FORBIDDEN}` |
| `ERR` packet for remote errors | `git_http::remote_error` (hand-framed) and `rbit-ssh` `channel_reply` | `rbit_transport::pkt_line::error` |
| Refused push: report when asked for, else text (`403` / SSH stderr) | `git_http::refused`, `git_exchange::push` | `rbit_transport::refused_push::push_refusal` |
| `Git-Protocol` header name, `no-cache`, gzip request bodies, dumb-protocol refusal | literals in `git_http.rs`, `advertisement.rs` | `smart_http::{GIT_PROTOCOL_HEADER, NO_CACHE, is_gzip, DUMB_PROTOCOL_REFUSED}` |

`rbit-transport` stays framework-free: no axum, no services; `base64` is its only new
(external) dependency. No internal crate edge was added. The routes keep what is HTTP
glue or another row's concern:

- extraction and rendering (`Denied` → status, headers, body; the web keeps
  `IntoResponse` because the transport has no HTTP types);
- the `RepositoryGate` adapter `git_http/repositories.rs` (`CallerRepositories`): task
  grants, `open_following_renames` and the token-scope cap, mapping `NotFound` to
  `None`;
- which credential is tried and how (task token, personal access token, password with
  two-factor and throttling): identity and Actions rows;
- the Git process admission slot, push recording, the protection verdict and the Actions
  task-push spool: limits, `collaboration.protection` and Actions rows;
- LFS (`routes/lfs*`, `repositories.lfs`), which reuses `authenticate` and is untouched.

## SSH: what moved

`rbit-ssh` already owned command parsing (`command.rs`, Git-only exec), the russh
handler, channel limits and the exchanges. Per-command authorization was written inside
the russh `Handler` (`session.rs`), and its client text was repeated in `git_exchange.rs`.
It is now `rbit_ssh::access`, independent of russh:

| Decision | Legacy place | Now |
| --- | --- | --- |
| Order: suspended user, open (user keys as HTTP; deploy keys via the key's repository), write access (optional, so receive-pack can read commands first), pull mirror | `Connection::authorize` | `access::authorize` over the `SshRepositories` port (adapter: `impl SshRepositories for Services`) |
| Key still held by the same user/repository | `session::same_key_holder` | `access::same_key_holder` |
| Client text: not found, read-only deploy key, no permission, key no longer authorized, internal error | `session.rs`, `git_exchange.rs` | `access` |
| Refused push answer, `ERR` packet | `git_exchange.rs`, `channel_reply.rs` | shared transport functions above |

## SSH key registry

The first slice left two decisions of `repositories.git-ssh` in
`rbit-services/src/ssh_keys.rs`. `rbit-ssh` depends on `rbit-services`, so they could
not move into `rbit-ssh` while the service suites still call them. They now live in
`rbit_repositories::ssh_keys`, a feature crate both depend on. `rbit-ssh` calls them
directly; the service keeps thin compatibility wrappers.

| Decision | Legacy place | Now |
| --- | --- | --- |
| Sign-in: unknown key → no one; suspended user → no one, without the password-change read; user who must change their password → no one; deploy key → its repository | `SshKeyService::find_by_fingerprint` | `rbit_repositories::ssh_keys::sign_in` over the `SshKeyDirectory` port, returning `KeySignIn` |
| Deploy-key reach: user keys reach nothing without a lookup; current then former `owner/name` (`.git` accepted); only the key's own repository; write only when added with write permission | `SshKeyService::open_for_deploy_key` | `ssh_keys::deploy_key_reach` over the existing `RepositoryAddresses` port |
| `SshIdentity`, `SshKeyId`, `DeployKeyId` | `rbit-services`, `rbit-storage` | `rbit_repositories::ssh_keys`; both crates re-export them, so every path still compiles |
| Push source of an SSH identity | `SshIdentity::push_source` (services) | `rbit_ssh::access::push_source` (its only caller) |

Adapters:

- `rbit_storage::SshKeyDirectoryStore` implements `SshKeyDirectory`. It reuses the key
  service's open key store for the unchanged fingerprint query and reads the account's
  password-change flag with the unchanged accounts query. It is registered in
  `component-types.json`.
- `SshKeyService::{directory, addresses, deploy_key_context}` hand out the storage
  adapters and turn a reached repository into a `RepoContext` (same paths, reader and
  access as before).
- `rbit_ssh::access::key_holder` (sign-in and the per-command key recheck) and the
  deploy-key `SshRepositories` adapter call `sign_in` and `deploy_key_reach` directly.
- `SshKeyService::find_by_fingerprint` and `open_for_deploy_key` remain as wrappers over
  the same functions for their other callers. The suspended-key log line is emitted
  by the shared `ssh_keys::log_refusal`.

New internal edge: `rbit-ssh` → `rbit-repositories`, added to `ADDED_EDGES`. The plan
lets SSH depend on feature APIs. This is the feature crate that owns repository
access models (`OwnedRepository`, `RepositoryAddresses`), and the only place both the
SSH server and the service wrappers can reach. Putting the decisions in `rbit-ssh` would
create a `rbit-services` → `rbit-ssh` cycle.

Test edits: none. No existing test file changed. The `rbit-services` `ssh_keys` unit
tests and `rbit-web` `admin_configuration` call the wrappers unchanged. New tests:
`crates/rbit-repositories/tests/ssh_keys.rs`, which checks each sign-in outcome and
deploy-key reach with recording fakes. The checks include no password read for
suspended users or deploy keys, and no address lookup for user keys.

Key registration (`parse_public_key`, titles, per-user and per-repository limits, and
add/list/delete) is account and repository settings behaviour. It stays in the service
as before; it decides nothing at sign-in.

## Boundary that fails if legacy paths return

`scripts/rewrite/git-transport-retirement.mjs`, run by `scripts/rewrite/check.mjs`, over
production Rust (`crates/*/src`, comments and the trailing inline test module ignored;
suites assert the bytes):

- outside `rbit-transport/src`: hand-framed `ERR` packets, `receive::refusal(` or
  `refusal_message(`, the `git-protocol` literal, the dumb-protocol text;
- in `rbit-web/src/routes/git_http*`: base64 Basic decoding, `strip_prefix("Basic `,
  `Basic realm=`, the bearer scheme test, the gzip test, `no-cache`, the not-found and
  push-forbidden texts; outside the `repositories.rs` adapter also
  `open_following_renames`, `open_for_task`, `is_mirror` and the scope cap;
- outside `rbit-storage/src`: matching on the stored `KeyHolder`; outside
  `rbit-repositories/src/ssh_keys`: defining `SshIdentity`, `KeySignIn`, `SshKeyId` or
  `DeployKeyId`; in `rbit-services/src/ssh_keys.rs` and `rbit-ssh/src`:
  `RepositoryAliases::resolve` or a password-change read; in `rbit-ssh/src`: the service
  wrappers `find_by_fingerprint` and `open_for_deploy_key`;
- in `rbit-ssh/src` outside `access`: `open_following_renames`, `open_for_deploy_key`,
  `is_mirror`, `is_suspended`, a `same_key_holder` definition and the authorization
  texts.

Run against the baseline files of this slice it reports 15 violations; against this tree,
none. `scripts/tests/git-transport-retirement.test.mjs` covers each rule, the allowed
places (adapter, `access`, LFS, tests, comments) and both trees.

## Gates

| Gate | Evidence |
| --- | --- |
| Protocol bytes | Moved verbatim. `ERR` packets keep `{len:04x}ERR <message>\n` (`pkt_line::error` uses the same `encode` SSH already used; messages are far below the pkt-line limit). Refusal reports are still `receive::refusal`. Challenge, content types, `no-cache`, `Retry-After`, LFS hand-off and status codes are unchanged; `remote_errors_are_single_err_packets`, `refused_slots_carry_retry_after`, `throttled_credentials_carry_retry_after`, the advertisement and `Denied` unit tests are unchanged and pass. |
| Access order | Same calls in the same order with the same short-circuits; new unit tests pin the order with recording fakes (`smart_http::tests`, `access::tests`): no mirror lookup before write access, anonymous challenges, suspension before open. |
| Suites | Existing transport, hostile-input and e2e protocol suites unchanged: `rbit-transport` (properties, receive), `rbit-ssh` (`git_over_ssh`, `permission_matrix`, `protection`, `shutdown`), `rbit-web` Git HTTP and LFS tests, `rbit-e2e` compatibility and protection. |
| Security | Unchanged: no shell, strict command parsing, key rechecked per command, deploy-key scope, suspension, mirror refusal after access, token-scope cap, throttling. |
| Migration and rollback | No schema, migration, stored data or route change; 0073 remains the latest. Rollback is the previous binary. |
| Performance | No added lookups or allocations on the request path beyond a borrowed adapter; one `Vec<u8>` instead of a `String` for the `ERR` body. Log lines keep their messages; the `git authorization failed`/`git mirror check failed` events now come from the `rbit_transport::smart_http::access` target. No release measurement claimed. |

## Source under test

Second slice (key registry), on top of `8c0ba633`, with the base fixes `4f0b4602` and
`83cba8c0` cherry-picked:

1. Decide SSH sign-in and deploy-key reach in rbit-repositories.
2. Use the key registry decisions directly from rbit-ssh.
3. Fail the rewrite check if the SSH key registry decisions return.

First slice:

Code commits on top of `ee9f958c`:

1. Decide smart HTTP and refused-push answers in rbit-transport.
2. Serve Git over HTTP through the transport decisions.
3. Authorize SSH Git commands in rbit_ssh::access.
4. Fail the rewrite check if retired Git transport decisions return.

## Results

First slice, on Linux, at its final commits. Full workspace run: 3295 tests, all
passed, 2 skipped. The suites listed under Gates ran unchanged. The quick run at the
final commit passed workspace clippy (`-D warnings`) and formatting, and 773 tests for
the changed packages, 1 skipped. In both runs the rewrite contracts failed only on a
base type-check issue outside this slice (`AccessTokenStore` re-export, fixed in
`4f0b4602`); the policy check reported 0 violations.

Second slice, on Linux: workspace clippy (`-D warnings`) and formatting; tests for the
changed packages (`rbit-repositories`, `rbit-storage`, `rbit-services`, `rbit-ssh` and
their dependants): 1322 passed, 0 skipped. Unchanged suites in that run: `rbit-ssh`
(`git_over_ssh` with deploy keys and renamed repositories, `permission_matrix`,
`protection`, `shutdown`, `access`), `rbit-services` `ssh_keys` unit tests,
`rbit-storage` `ssh_keys` and `upgrade`. The rewrite contracts passed, type check
included (`4f0b4602` cherry-picked). Formatting of the new test file failed once and was
fixed before the final run.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 4 (plus this record and the ledger rows) |
| Files changed by code commits | 22 |
| Lines added / removed (all) | 855 / 147 |
| `rbit-transport` production | 6 files, +217 (four new modules, lockfile and manifest lines) |
| `rbit-web` and `rbit-ssh` production | 8 files, +129 / −147 (two new adapters/modules, 49 and 148 lines) |
| Tests | 3 new Rust test modules (+265 incl. the transport `pkt_line`/`refused_push` cases), one 75-line Node test |
| Checker and policy scripts | one 59-line Node guard, 2 lines in `check.mjs` |

Tricky parts:

- `rbit-web/src/routes/git_http.rs` is a grandfathered 568-line file that may not grow;
  the gate adapter went into its own file so the route file shrank to 531 lines.
- Protocol bytes as an interface: the `ERR` packet had two framings (hand-built in HTTP,
  `pkt_line::encode` in SSH) that produce the same bytes; both now share one function.
- SSH defers the write check so receive-pack can read the command list (clients hang if
  the server closes before advertising refs), but still refuses mirrors up front; the
  port keeps that asymmetry explicit.
- The deploy-key and sign-in decisions in `rbit-services` could not move into `rbit-ssh`
  without a crate cycle; the second slice placed them in `rbit-repositories` instead.
- The shared build lease was stale and the CI queue was shared with parallel slices;
  waiting dominated wall-clock time.

Second slice (key registry): 3 code commits, 15 files, +456 / −77. Production Rust
+269 / −72 in 11 files (new: the `ssh_keys` decisions, 130 lines; the storage
directory, 48). New tests: one file, about 160 lines. Checker: 14 lines of rules, 23 of
tests, one edge entry. Tricky parts: the crate graph (the decisions had to sit below
both `rbit-services` and `rbit-ssh`); keeping the open key store and the
no-password-read path for suspended users and deploy keys; moving `SshIdentity` and the
key ids without breaking any `rbit_services`/`rbit_storage` path (re-exports); and
the suspended-key log line, now shared through `log_refusal`.

Estimate: each slice was one session of three to four code commits, because the protocol
core and the per-command checks were already in the target crates. Rows whose
decisions are similarly thin, but sit behind a crate-graph constraint, need a feature
crate below both callers rather than the ledger's target, plus thin compatibility
wrappers: about 400–500 changed lines.
