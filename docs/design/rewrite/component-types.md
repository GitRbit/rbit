# Replacement component types inside legacy crates

`component-types.json` maps exact crate-relative paths to canonical qualified local
definitions. The extra `rbit-rewrite-check --components <repository-root>` mode
continuously enforces 400 cumulative authored lines for replacement types that still
live inside legacy crates. ThreadServices, ThreadReader, ThreadWriter and ReadView
are the initial required definitions; aliases do not substitute for their ownership.
Reference service adapters and the new dormant SQL/redb credential components are
also mapped explicitly, including internal bindings and row projections. Their
location in legacy storage does not exempt new types from the cumulative limit.

The mode shares the ordinary full-crate scanner, alias/import resolver and unique
line-span aggregation. Every discovered inherent and trait impl is resolved before
size selection. All parse, module-path, missing-file and resolution errors remain
fatal, even outside a selected owner. Missing required definitions, alias-only
substitutes and malformed/duplicate/wildcard policies fail. Unrelated legacy type
sizes remain outside this additional policy; the ordinary strict checker still
reports them when asked to scan that crate. There is no global legacy error waiver.
The authored macro-body expansion limitation remains the same as the existing tool.

The scanner honors Cargo's explicit `autotests = false` before adding explicit
`[[test]]` targets. Disabled conventional files are not independent Rust targets;
their actual modules are still scanned recursively from the configured test entry.
This prevents false module roots while keeping broken nested test paths fatal.
`#[path]` behavior is unchanged, including its sibling module directory semantics.
See the [Cargo target discovery reference](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#target-auto-discovery)
and [Rust module reference](https://doc.rust-lang.org/reference/items/modules.html).

The CI structural helper runs strict new crates, then the component mode, then the
password boundary using the same exact helper-built checker. Any failure stops the
gate. No source substring matching or name grep decides component success.

Source-prepared AST fixtures cover distributed qualified/aliased inherent and trait
impls exceeding 400 while individual files remain small; same-name unrelated legacy
oversize; missing/alias-only types; unresolved self types; absent/malformed modules;
configured integration targets with nested paths; and invalid/duplicate policies.
Node policy tests, source bounds and formatting pass. Rust tests and the new CLI mode
have not executed during source preparation; required shared-lane acceptance is the
checker unit suite, strict all-target Clippy, actual component mode on this checkout,
and the complete scripts/rewrite/check-types.mjs structural gate. No class-size
continuity claim should rely only on the previous manual checker invocation.
