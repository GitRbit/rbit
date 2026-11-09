# Access policy: collaborator management slice

The `access.policy` row (permission lattice, collaborators, SQL listing parity) is too
broad for one slice. This slice retires one coherent part, collaborator management, and
leaves the row `baseline-retained`.

## Already in `rbit-access` before this slice

The permission lattice (`RepoAccess`) and the combination of visibility, ownership,
organisation role, base permission, team and collaborator grants (`repo_access` over
`ViewerAccessFacts`/`RepositoryAccessFacts`), with its exhaustive grant-table test.

## Retired here

`rbit_access::collaborators::Collaborators` over the `CollaboratorRepository` port owns:
administrators only (`Forbidden` otherwise), self-removal, refusing organisations,
suspended users and the repository's personal owner, the "already a collaborator" and
"is not a collaborator" refusals, and the idempotent `set` (added, changed, unchanged).
`rbit_storage::CollaboratorAccessStore` adapts the existing SQL, which keeps one entry per
user and repository atomic. `CollaboratorService` keeps its API and becomes the
composition adapter: team grants for the listing, audit entries, `CollaboratorChanged`
events (published on a change only when the permission differs, as before) and
`access_of`. `scripts/rewrite/collaborators-retirement.mjs` fails if the adapter regains
the decisions, the conflict mapping or raw collaborator/suspension store calls.

## Still legacy, for later slices

1. `rbit-auth/src/policy.rs`: the entity adapter mapping `User`, `OwnedRepository` and
   `Grants` onto the `rbit-access` facts, and `rbit-services::repositories::viewer_grants`
   that loads them. Moving it needs the record projections behind ports (the facts are
   already pure); every request path uses it, so it deserves its own parity run.
2. SQL listing parity: `readable_by!` and the grant joins in `rbit-storage`
   (`repositories`, `teams`, `stars`, `admin`, `wiki_mutations/authorization`) encode the
   same policy in SQL for listings. That cannot move into `rbit-access`; retiring it means
   a parity test that compares every listing filter against `repo_access` over the same
   fixture matrix, and a boundary that keeps new listings on the shared macro.
3. Other services that call `repo_access` directly (`actions`, `issues`, `lfs`,
   `notifications`, `pulls`, `references`, `admin`) are consumers, not owners; they need no
   change for the row to retire once 1 and 2 land.

## Results

See [collaboration-protection-acceptance.md](collaboration-protection-acceptance.md#results):
the same runs cover this slice.
