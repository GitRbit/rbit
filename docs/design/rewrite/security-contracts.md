# Rewrite security contracts

The replacement must preserve the behavior of GitHub main at `39f932a00b7aa35b324e06a9ff2343a57d4bed76`. These requirements supplement the feature ledger and the historical audit regression tests. They are acceptance contracts, not a claim that the replacement already passes them.

## Trust and data boundaries

| Boundary | Adversary and required behavior |
| --- | --- |
| Browser and API | Anonymous or compromised accounts cannot enumerate or mutate private resources; current authorization precedes cached responses and conditional 304s |
| Identity | Competing bootstrap/recovery/MFA requests cannot bypass one-use or administrator constraints; revocation is synchronous at authoritative boundaries |
| Repository | Hostile refs, object data, filenames and repository configuration cannot execute arbitrary host commands or escape the repository root |
| Outbound network | Hostile URLs/DNS/redirects cannot reach disallowed addresses through hooks, mirrors, imports or identity discovery |
| Runner | Untrusted fork code cannot elevate via approval, actor substitution, cache poisoning, host mounts, socket access or stale credentials |
| Stored data | Secrets remain sealed, key loss is actionable, historical migrations remain immutable and backup/restore retains required key material |
| Build and release | Untrusted PR code/artifacts/cache entries do not cross into privileged signing/publication; release provenance identifies the exact source |

## Resource and lifecycle invariants

Admission precedes expensive work. A capacity permit belongs to the actual worker until it finishes, including after its request is cancelled or expires. Rejected work never enters an unbounded queue. Hashing strength is not reduced to improve latency.

Enforce separate limits for CPU, memory, queued work, subprocesses, body/decompressed bytes, log storage, artifacts/caches and open files. Logs and metrics redact secrets and use bounded labels. Tests distinguish a response timeout from cancellation of child processes or blocking work.

Required background effects use durable state with idempotent delivery. Transient broadcast is allowed for hints; it does not enforce revocation or guarantee Actions triggers. Git/filesystem/SQLite/redb operations require explicit recovery and reconciliation because there is no shared atomic transaction.

## Mandatory regression groups

- Permission predicates match the pure policy for public/private/organization/team/collaborator/admin and archived resources across HTTP, SSH, API, browser edits and runner endpoints.
- Session renewal cannot resurrect revoked sessions; consumed recovery tokens cannot win a second time; TOTP challenges and password-change races retain their protections.
- DNS pinning, redirect/proxy policy, case aliases, path traversal, reparse points, archive links, LFS object scope and upload cancellation are tested with hostile inputs.
- Fork runs receive no secrets/variables and only read-only task credentials even after approval. Job completion requires the current lease/attempt and immutable authorized inputs.
- Parser expansion/depth, logs, diff/blame/highlighting and expensive authentication remain bounded under concurrent hostile requests while ordinary reads remain serviceable.
- Localization/catalogs and fragments preserve Askama escaping, CSP and safe Markdown/Mermaid boundaries. A localized string never becomes an implicit trusted-HTML wrapper.
- Consistent backups restore Git/LFS/releases/artifacts, relational records and sealed credentials. Recovery time, recovery point and power-loss durability policy are explicit.

### Identity/session group status

The second group holds against the stores production currently selects: the redb `SessionStore`, the `SQLite` recovery, profile and two-factor stores, and the `rbit-identity` `SessionApplication` that `AccountService` composes over them. `crates/rbit-storage/tests/identity_races/` releases contenders together through a barrier on a multi-threaded runtime: renewals racing single, account-wide and keep-current revocation; one reset link redeemed concurrently; a verification link consumed concurrently; one verified password replaced concurrently; a self-service change or a stale-proof email change racing a reset link; one TOTP step, the newest of several steps and one recovery code claimed concurrently; and confirmation racing cancellation or secret replacement. `crates/rbit-services/tests/identity_races/` parks a real resume after its read or immediately before its renewal write while the production service revokes one session, signs out elsewhere, changes the password or suspends the account, and presents one authenticator or recovery code to two pending sign-ins at once. Each revocation is visible to the first request after it returns. A request parked after its account lookup can still complete across a suspension, which keeps the row; the next request is refused.

Earlier sequential and two-way checks remain in `crates/rbit-storage/src/kv/sessions/tests.rs`, `crates/rbit-storage/tests/{recovery,two_factor,accounts,account_creation}.rs` and `crates/rbit-services/tests/{sessions,two_factor,recovery,admin/registration}.rs`. The inactive epoch session store and original-proof paths have their own race tests; they are not activated, and this group's status says nothing about their production behavior. SSH key revocation on persistent connections is outside this group.

The initial kernel tests cover worker lifetime and admission only. Full protocol, storage, security and migration parity remain pending in the corresponding ledger entries.
