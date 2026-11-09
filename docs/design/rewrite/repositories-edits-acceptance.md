# Repository edits retirement acceptance

Acceptance evidence for the `repositories.edits` ledger row ("Atomic browser file
edit/upload/rename/delete and protected branches"), following the
[identity retirement](identity-retirement.md) pattern.

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Write access, archived and mirror read-only refusals, in that order (archives without a query) | `rbit_repositories::edits::RepositoryEdits::{writable, read_only_reason, can_edit}` |
| Direct versus new-branch-only access and its reason | `RepositoryEdits::access` |
| Pre-commit decision sequence: editor branch exists, new-branch name present/valid/unused, moved-branch refusal, one push-gate update, protection versus validation classification | `RepositoryEdits::check` over the `EditPort` (`is_mirror`, `branch_tip`, `valid_branch_name`, `push_refusal`, `branch_update_refusal`) |
| Upload admission (duplicate name, file-count limit) and the non-empty upload rule | `edits::{admit_upload, require_upload}` |
| `CommitTarget`, `CommitInput`, `EditAccess`, `Edited`, `MIRROR_READ_ONLY` | `rbit_repositories::edits` (re-exported unchanged by `rbit_services::edits`) |

`rbit-services::edits` keeps the Git workspace: path parsing (`FilePath`), text size
and line-ending preparation, editable-text detection (binary, oversized, LFS pointer),
blob writing under the repository lease, commit identity, the compare-and-swap commit
and its push reporting. Its `EditView` port borrows the loaded `RepoContext` and the
existing protection service; the mirror flag is the same single query. HTTP routes,
templates and error text are unchanged.

## Legacy code removed

- The service's own `writable`, `read_only_reason`, `can_edit`, `access` and `check`
  decision bodies, including the gate-refusal classification by target.
- The upload duplicate and count checks and the empty-upload refusal.
- The service-local model definitions and the mirror message constant.

## Boundaries that fail if legacy paths return

`scripts/rewrite/repository-edits-retirement.mjs`, run by `scripts/rewrite/check.mjs`:

- Edit refusal texts (duplicate upload, upload count, empty upload, new-branch name,
  invalid branch name, existing branch, mirror read-only) in production code outside
  `rbit-repositories/src`. Git's own commit-race error keeps its separate text.
- Archive checks or target-based gate classification in `rbit-services/src/edits.rs`
  outside its test module.

`scripts/tests/repository-edits-retirement.test.mjs` covers the rules, the test
exemption and the current tree. `EditView` is registered in `component-types.json`.

## Parity argument

- The decision bodies moved verbatim; refusal order and texts are identical. Each
  port call is the same call the service made: the mirror query, `refs()` for branch
  tips, `is_valid_branch_name`, `check_push` with one `RefUpdate` whose `after` is the
  tip, and `branch_update_refusal`. Refusals map back to the same `EditFailure`
  variants (`Forbidden` and `NotFound` through `ServiceError`).
- Upload admission runs after the same path validation and before the blob is
  streamed, as before.

## Results

One Linux run of the repository's own CI jobs on the final code, with tests limited to
the packages these commits change (`rbit-repositories`, `rbit-storage`,
`rbit-services`):

| Job | Result |
| --- | --- |
| Workspace formatting and all-target Clippy with `-D warnings` | pass |
| nextest for the changed packages | 1,294 run, 1,294 passed, none skipped |
| Licenses and advisories, front-end build | pass |
| Rewrite contracts | `check.mjs` 0 violations and the Node suites pass; `check-types.mjs` stops on the `identity_access_tokens.rs` resolution error already present in the base stack, fixed separately |

The passing set includes the new `rbit-repositories` fixtures (contributions 5,
stars 3, edits 5), the existing `rbit-services` integration suites unchanged
(`edits`, `repo_settings`, `access` and `transfers`, which seed and count stars)
and the contribution unit tests in `rbit-services` (default-branch-only
calendars, cancelled-scan cache reuse). A full workspace run and the Windows legs were
not run for this slice.

Locally: `node scripts/rewrite/check.mjs` reports zero violations,
`node --test scripts/tests/repository-*.test.mjs` passes, `cargo fmt --all --check`
passes and `workspace.mjs check` compiled the three packages with all targets.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed. Rollback is the previous binary
against the same data. No request path gains a query or Git call. No release
measurement was taken and none is claimed.

## Not covered

- Editable-text detection and text preparation depend on `rbit-core` file-edit types
  that `rbit-repositories` cannot depend on; they stay in the service.
- Lifecycle limits of the Git workspace (lease scope, cancellation between commit and
  push reporting) are unchanged.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus this record) |
| Files changed by code commits | 8 |
| Lines added / removed | 474 / 124 |
| Production Rust (`src`, non-test) | 3 files, +270 / −124 (new application 208 lines; service 470 to 407 lines) |
| Tests and fixtures | 1 file, +133 |
| Checker and policy scripts | one Node guard, its test and two checker lines, one component entry |

Tricky parts:

- `rbit-repositories` cannot depend on `rbit-core`, so the decision sequence speaks
  in strings and a `BranchUpdate` the service turns back into a `RefUpdate`; path
  parsing, text preparation and LFS detection had to stay with the Git workspace.
- The push gate needs the actor while the read-only query does not, so the actor is
  an argument of the gate methods rather than part of the borrowed view.
- The service file was already over 400 lines; the extraction brought it down.

Estimate: rows whose decisions are entangled with legacy core types cost about one
session; the split line is set by what the target crate may depend on.
