# Git transport retirement acceptance

Acceptance evidence for two ledger rows, following the
[identity retirement](identity-retirement.md) pattern:

- `repositories.git-http` ("Smart HTTP advertisement, protocol v2, clone/push and packet
  limits"), target `rbit-transport`: **ported**.
- `repositories.git-ssh` ("SSH/deploy keys, Git-only exec and per-command
  authorization"), target `rbit-ssh`: **partly retired, still `baseline-retained`**;
  see [What remains](#what-remains).

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

## What remains

`repositories.git-ssh` stays `baseline-retained`. Two decisions of the row still live in
`rbit-services/src/ssh_keys.rs`:

- `SshKeyService::find_by_fingerprint`: suspended users and accounts that must change
  their password resolve to no identity;
- `SshKeyService::open_for_deploy_key`: a deploy key reaches only its own repository
  (old names included), read-only unless allowed to write.

They cannot move into `rbit-ssh` in this slice: `rbit-ssh` depends on `rbit-services`, so
the service cannot call back into it, and the existing suites call both methods directly
(`rbit-services` `ssh_keys` unit tests, `rbit-web` `admin_configuration`). Moving them
needs the key registry behind an identity/repository port that `rbit-ssh` consumes
directly (or composition in `rbit-server`), with those suites migrated. Key registration
(`parse_public_key`, titles, per-user and per-repository limits) is settings behaviour
and also stays in services for now.

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

Code commits on top of `ee9f958c`:

1. Decide smart HTTP and refused-push answers in rbit-transport.
2. Serve Git over HTTP through the transport decisions.
3. Authorize SSH Git commands in rbit_ssh::access.
4. Fail the rewrite check if retired Git transport decisions return.

## Results

RESULTS

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
- The deploy-key and sign-in decisions in `rbit-services` could not move without a crate
  cycle or editing existing suites, which bounds the SSH row to a partial slice.
- The shared build lease was stale and the CI queue was shared with parallel slices;
  waiting dominated wall-clock time.

Estimate: the HTTP row was one session of four code commits because the protocol core
was already in the target crate. Finishing `repositories.git-ssh` needs the SSH key
registry behind an identity/repository port (sign-in identity, deploy-key reach, key
registration), its storage adapter and the two suites migrated: about one more session,
3–4 commits and 500–700 changed lines.
