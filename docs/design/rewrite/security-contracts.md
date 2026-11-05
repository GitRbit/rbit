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

The initial kernel tests cover worker lifetime and admission only. Full protocol, storage, security and migration parity remain pending in the corresponding ledger entries.
