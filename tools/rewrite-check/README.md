# Structural and password-boundary checks

Run `node scripts/rewrite/check-types.mjs` to build the development checker through the
shared lane, check strict replacement crates, then run the password-boundary mode using
the same exact executable. Each run completes verified process-tree cleanup before the
next starts; disposable runtime generations are pruned afterward. The command emits JSON
and fails if a logical type exceeds 400 authored lines or its local implementation
ownership is unresolved. The repository's physical-file and dependency checks are separate.

The checker parses conventional library/binary/integration-test/example/bench targets
and explicit manifest target paths, follows external and inline modules, and keys types
by compilation target and module scope. Definitions, inherent implementations, trait
implementations and local aliases share the same underlying local type. Source line
spans are unioned per file so a line is not counted twice for the same owner. Foreign
or blanket self types have no local class definition; their impls get individual limits.
Cargo dependency names identify valid external types rather than silently treating an
unresolved local self type as foreign. Ambiguous/cyclic local aliases fail closed.

This is a source-structure check, not Rust's name resolver or a replacement for compiling
the crate. Cfg alternatives are included conservatively. Module cfg_attr attributes
are rejected pending explicit path support so alternate files cannot evade checking.
`#[path]` on an inline module is likewise rejected; ordinary external `#[path]`
lookup follows the [Rust module path rules](https://doc.rust-lang.org/reference/items/modules.html#the-path-attribute).
Use explicit imports for foreign self types where glob resolution cannot establish
ownership. Custom compiler-provided entrypoints must be declared as Cargo targets.

Macros are not expanded: their authored invocation tokens remain covered by physical
file limits, and ordinary attributes such as tokio::test are accepted. This does not
certify macro-generated bodies. Generated business logic cannot serve as a way around
the size policy; introducing such generation needs an explicit architecture decision
and review of its source/output. Generated data-only exemptions belong to the outer
repository policy. Comments and blanks inside the definition/impl spans count; comments
between separate items do not belong to either logical type but still count per file.

This development tool has no place in the production server dependency graph.

## Legacy password scheduling boundary

The unadmitted `Passwords` facade is retired (see the
[identity retirement](../../docs/design/rewrite/identity-retirement.md)).
`rbit-rewrite-check --password-boundary [repository-root]` recursively scans every Rust
file below `crates`: sources, integration tests, benches, examples and build scripts.
`crates/rbit-services/src` and `crates/rbit-server/src` remain required anchors.
Missing, malformed or linked source entries fail closed. Every component below the
explicitly supplied repository anchor is checked, including `crates`, the crate
directory and `src`; an alias of the anchor itself does not authorize linked
descendants. Directory traversal does not follow symlinks or Windows reparse points.
The link-creation regression fixtures run on Unix, where they do not require Windows
symlink privileges; the Windows implementation checks the reparse-point attribute as
well as symbolic-link metadata. No crate, compatibility path or test may name or
redefine the facade.

The mode parses Rust and examines authored tokens, including opaque macro groups and
cfg-gated source. It rejects the `Passwords` identifier in bare, qualified, renamed or
raw-identifier forms. Comments, documentation text and string literals are allowed.
Auth/identity namespace wildcards, including imported namespace aliases, are rejected
because they can hide that facade name; use explicit imports. Alias origins are tracked
conservatively across authored files rather than relying on compiler name resolution.
Diagnostics include the source path and line. This policy does not ban unrelated
`spawn_blocking` work, such as Git operations.

Macros are not expanded: generated imports or concatenated identifiers are outside this
source-token proof and require review. This is a narrow guard against accidentally
restoring the retired direct password scheduling, not a complete call-graph
or whole-program security analysis.
