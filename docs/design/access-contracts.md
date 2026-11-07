# Repository access contract

`rbit-access` owns the ordered repository permission lattice and the pure rule that
combines visibility, account ownership, site administration, organisation membership,
base/team grants and collaborator grants. It has no dependencies, performs no I/O,
allocates no policy objects on the heap, and needs no component startup or shutdown.
This is the repository authorization slice of the access extraction, not the complete
access feature or microkernel component lifecycle.

## Trusted input boundary

`RepositoryAccessFacts` describes the repository being checked. The optional
`ViewerAccessFacts` comes from an already authenticated actor and current grants for
that actor and that same repository. Account ownership is an ID comparison. Actor
names, client-supplied role fields, cached grants from another repository and
unverified token claims are not policy facts.

`OrganizationAccess::Member` carries the base and best team grant together. Adapters
must discard stale team/base rows when membership is absent. Organisation fields
have no effect on personal repositories. Anonymous visitors receive only the public
read baseline; outside collaborators may receive access without organisation
membership. Site admins and matching owner accounts retain administrative access.
All applicable rules combine by maximum; grants cannot lower public read access.

The module deliberately cannot verify these assertions: it has no account records,
repository identifiers, identity provider or storage connection. Resolving and
invalidating grants remains the service/storage caller's responsibility. Public
struct construction is not proof of authentication or an issued capability.

## Compatibility and remaining gates

`rbit-core::RepoAccess` and `rbit-core::access::RepoAccess` reexport the exact type.
`rbit-auth::repo_access` and `rbit-auth::policy::repo_access` preserve their existing
entity-based signature and `Grants` shape. Their thin adapter converts `User`,
`OwnedRepository`, `OrgRole` and `Permission` into resolved facts, then delegates.
Existing exhaustive adapter tests remain in place, including stale team membership,
organisation/personal separation, anonymous grants and the public baseline.

Repository access alone does not authorize every operation. Token-scope ceilings,
suspension, archived content, branch protection, issue/comment rules and operation
requirements remain separate existing gates at their current call sites. No grant
lookup, cache, SQL schema, transport denial status or credential behavior changes in
this slice. Token/PAT policy, issue policy and broader role/domain extraction remain
future work; no persisted access identifiers or serde representations move here.

## Validation and integration

The access crate tests cover all lattice thresholds and 3,072 combinations of
visibility, owner kind, grant levels, membership and authenticated actor privilege,
plus focused outsider/personal and monotonic grant cases. The existing auth adapter
matrix independently exercises real domain entity conversion.

Workspace registration and the lockfile include the access crate without external
dependency changes. The core/auth adapters may depend on it; the architecture policy
permits no dependencies for access itself.

The integrated slice passes all 196 access/auth/core tests (4 access, 42 auth and
150 core), strict all-target Clippy for those crates and the workspace source/dependency
policy. Use `node scripts/workspace.mjs test --package rbit-access --package rbit-auth
--package rbit-core --locked` through the shared build lane. Complete workspace,
cross-ingress HTTP/SSH/API acceptance and measured architecture costs remain broader
phase gates; these focused results do not establish the full access port.
