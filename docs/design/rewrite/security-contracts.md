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

### Permission group status

The first group holds for the legacy ingresses production serves today. One world, `crates/rbit-services/tests/support/permission_matrix.rs`, gives eleven actors (anonymous, unrelated, collaborator read/write/admin, organisation member under a `read` base permission, team member with `write`, organisation owner, personal owner, site administrator, and a suspended write collaborator) a declared relationship to eight repositories: personal and organisation, public and private, each also archived. Its expected-access table is asserted equal to the pure policy (`rbit_auth::repo_access` over `rbit_access`) for every declared relationship, and the stored grants are asserted equal to the declared ones. Every ingress then compares its actual decision with that table for all 88 actor/repository pairs:

- services (`crates/rbit-services/tests/permission_matrix.rs`): `open`, `open_page`, `open_following_renames`, `can_write_content`, `EditService::can_edit`, owner listings, explore/search, issue creation (read) and label creation (write), with archived repositories refusing both after the access check;
- browser (`crates/rbit-web/tests/permission_matrix/browser.rs`): repository pages, settings (administrators, archived included), issue and label forms, browser edit forms and commits (404 to anyone who may not change content, branch moved only for writers), owner pages and explore;
- REST API (`api.rs`): repository reads, issue, label and status creation, repository updates and owner/organisation listings with personal access tokens;
- Git smart HTTP (`git_http.rs`): fetch and push advertisements, and a real `receive-pack` branch deletion that succeeds for writers and is declined with the archive reason;
- SSH (`crates/rbit-ssh/tests/permission_matrix.rs`): `git-upload-pack` and a `git-receive-pack` branch deletion over a real in-process SSH connection;
- runner credentials (`crates/rbit-web/tests/permission_matrix/tasks.rs`): a running task's token reaches its own repository at its grant and every other shape only as an anonymous visitor would, over Git and the API, and its status and push are refused once its repository is archived;
- caching (`caching.rs`): validators from authorized viewers of private pages, `*`, and archive commit validators earn every non-reader a 404 rather than a 304; revoking a collaborator invalidates the validator they hold; private pages are `private`.

Suspension is an authentication gate, not a policy input: credentials are issued before the account is suspended, after which the session falls back to anonymous browsing, the API and Git refuse the token with 401, a fresh SSH key sign-in fails and an SSH connection opened before the suspension is refused on its next command. A temporary removal of the archive check in `ProtectionService::check_push` made the Git HTTP, SSH and task push assertions fail. No defect was found.

Earlier coverage remains in `crates/rbit-auth/src/policy.rs` and `crates/rbit-access/src/tests.rs` (exhaustive policy), `crates/rbit-services/tests/access.rs` (SQL listing agreement), `crates/rbit-ssh/tests/git_over_ssh.rs` (key revocation and administrator demotion on persistent connections), `crates/rbit-services/tests/protection.rs` and `crates/rbit-core/src/protection.rs` (case aliases), `crates/rbit-web/src/routes/lfs/tests.rs` and `crates/rbit-services/tests/lfs.rs` (LFS transfer-token scope), and `crates/rbit-web/tests/security.rs` plus `crates/rbit-web/src/middleware.rs` (private revalidation, partial-page validators, preload exclusions). Not covered by this group: deploy keys, fine-grained token scopes below `admin`, LFS and release-file endpoints in the matrix, pull requests, webhooks, organisation/team management pages, and the replacement crates' prepared ports, which production does not activate.

The initial kernel tests cover worker lifetime and admission only. Full protocol, storage, security and migration parity remain pending in the corresponding ledger entries.
