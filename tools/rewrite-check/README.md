# Logical-type size check

Run `cargo run -p rbit-rewrite-check -- crates/rbit-kernel crates/rbit-localization`
through the shared build-lane helper. The command emits JSON and fails if a logical
type exceeds 400 authored lines or its local implementation ownership is unresolved.
The repository's physical-file and dependency checks are separate.

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
