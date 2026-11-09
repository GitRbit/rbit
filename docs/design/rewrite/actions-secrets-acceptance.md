# Actions secrets and environments acceptance

Acceptance evidence for the `actions.secrets` ledger row (sealed scoped secrets,
variables, environment approvals and timers). The owner is `rbit-actions`. It follows the
[identity retirement](identity-retirement.md) pattern.

The rules moved first, while `rbit-core` still depended on `rbit-actions` for the
fork-policy re-exports. The model boundary forbids cryptography in `rbit-core`'s graph, so
sealing, the key file and the glob engine had to wait. Batch 3 removed that edge
(`token_permissions` takes `trusted: bool`), and this branch, rebased onto it, moves the
rest. `rbit-core` keeps the stored records (`SecretScope`, `SecretId`, `SealedSecret`,
`BranchPolicy`), and `rbit-actions` names none of them, so no new edge or model crate is
needed.

## What moved

| Decision | Legacy place | Now |
| --- | --- | --- |
| The secrets key, AES-256-GCM sealing and opening, associated data binding scope kind, scope id and secret id, key ids, key file format | `rbit-services/src/actions/secrets.rs` (`SecretsKey`) | `rbit_actions::secrets::{SecretsKey, SecretAddress, KEY_PREFIX}` |
| When the key file is read, refused or created (only while no secret is sealed), and every message | `secrets::load_key` | `secrets::load_key` over the `KeyFile` port; `KeyUnavailable` (re-exported in services) |
| Value limits and checks, repository-only visibility, `MAX_VALUE_BYTES`, `MAX_VARIABLE_BYTES`, `MAX_PER_SCOPE`, `UNREADABLE` | services | `rbit_actions::secrets` (re-exported in services) |
| Opening a task's secrets (first of each name, never a shadowed one, fail closed) and an environment's (in order, fail closed) | services | `secrets::{open_task_secrets, open_environment_secrets}` |
| Environment names, selected patterns (1 to 50, valid, not all negative) and their messages | `put_environment` | `environments::check_name_and_policy` |
| Full-ref rewriting and glob matching of selected patterns | services (`ref_patterns`, `selected`) | `environments::{full_ref_patterns, selected}` with `rbit_workflow::pattern` |
| The deployment ref gate: branch or tag, any / protected / selected, protection read only for a protected policy and a branch, the refusal message | services | `environments::DeploymentGate` over `ProtectedBranches`, reading the stored policy as `RefPolicy` |
| Review rights; reviewer and wait-timer bounds | services | `environments::{may_review, check_reviewer_count, check_wait_timer, MAX_REVIEWERS, MAX_WAIT_TIMER_MINUTES}` |

## What the adapters keep

- `rbit-services/src/actions/secrets.rs`:
  - the disk `KeyFile` (`tokio::fs`, create-new with mode `0600`, the Unix group/other permission check, the Windows ACL warning) and the creation log line;
  - a `SecretsKey` that maps stored scopes, ids and `SealedSecret`s onto the sealing key's addresses and parts;
  - storage CRUD with audit, and the in-process key cache.
- `rbit-services/src/actions/environments.rs`: storage, audit, reviewer lookup, release of
  waiting jobs, branch protection as `ProtectedBranches`, and `ref_policy` from the stored
  `BranchPolicy`.
- `rbit-storage`: secret and environment tables, and a defensive cap of six reviewers on
  insert.
- The review gate and the wait timer are atomic SQL in `actions_runs` (settle and claim).
  They belong to the `actions.scheduling` row and are not claimed here.

## Boundary that fails if legacy paths return

`scripts/rewrite/actions-secrets-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
rejects in production sources outside `rbit-actions`:

- redefined secret or environment limits;
- the environment protection messages and full-ref rewriting;
- in the services and server adapters only:
  - the secret value and scope messages;
  - AES-GCM (`Aes256Gcm`), a `KEY_PREFIX` or `AAD_PREFIX` constant, a `KeyUnavailable` type, or the missing-key rule;
  - `valid_environment_name`, `ref_patterns`, or the negative-pattern message.

Three things are allowed: `rbit-storage`'s unrelated instance sealing, the REST API's
request-shape refusal of `visibility`, and the services `SecretsKey` record mapper.
`scripts/tests/actions-secrets-retirement.test.mjs` pins the rules and the current tree.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Sealing, key file and pattern code moved unchanged. A new test pins the stored associated-data bytes and the key file format. Messages, read order and laziness are unchanged. The services unit tests are unchanged (sealing round trips, key ids, fresh nonces, key file creation, refusal of broken and shared key files, `check_value`, selected patterns) and run through the services `SecretsKey`, `load_key` and the re-exports. |
| Security | Ciphertexts stay bound to their row, a replaced key is detected, a missing key while secrets exist stays missing, untrusted runs get nothing (`trust::Grants`), and secrets fail closed. Values never reach `Debug`. |
| Migration and rollback | No schema, stored ciphertext, key file or route change: secrets sealed before the change open after it, and the other way round. Rollback is the previous binary. |
| Performance | Same queries and the same cipher work per secret; one name vector per task or environment. No release measurement claimed. |

## Results

A quick run of the repository's own CI jobs at `da705cd4` (base `88d16e99`, the batch 3
tip): rewrite contracts (including the logical-type check), workspace-wide formatting and
all-target Clippy with `-D warnings` (which compiles every crate, `rbit-web` included),
licenses and advisories and the front-end build passed. Nextest ran the changed packages,
`rbit-actions` and `rbit-services`: 817 tests, 817 passed, none skipped. Among them,
unchanged and passing: `rbit-services` `actions_secrets`,
`actions_settings`, the secrets and environment unit tests (sealing round trips, key files,
`check_value`, selected patterns) and the Actions integration suites. The new
`rbit-actions` `secrets::key_tests`, `secrets::tests` and `environments::tests` passed too.
`rbit-web` suites (`actions_environments`, `actions_settings`, `actions_fork_runs`) were not
part of the quick run. Windows legs were not run. Locally, a `check` of `rbit-actions`,
`rbit-core` and `rbit-services` (all targets) was clean.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 5 (rules, adapters, boundary; sealing and key file; boundary) |
| `rbit-actions` production | 7 files, +466 / −1 (`secrets`, `secrets/key`, `secrets/key_file`, `environments`, `environments/gate`, wiring) |
| `rbit-services` production | 2 files, +134 / −213 |
| Tests | 3 new `rbit-actions` files, +292 |
| Checker and policy scripts | one 45-line Node guard with a 46-line test |

Tricky parts:

- My first attempt moved sealing while `rbit-core` still linked `rbit-actions`, and it
  failed the model boundary. A crypto-free model crate would also have worked; batch 3
  removed the edge instead, and this slice was rebuilt on that.
- `rbit-actions` may not name `rbit-core`'s records, so sealing works on row addresses and
  raw parts, and the gate reads the stored policy as a borrowed `RefPolicy`. The services
  `SecretsKey` keeps the record-level signatures, so the services tests did not change.
- The key file mixes policy (when to create, what to refuse, every message) with platform
  I/O (mode bits). The policy moved behind a port; the permission check stays with the disk.

Estimate: with no edge in the way, the code itself was about one session.
