# Workflow planner modules

This extraction preserves the existing planning algorithm and public module API.
It adds no evaluator, runtime framework, request, allocation policy or dependency.
The planner still processes dependencies in its existing order and emits jobs in
file order, with matrix combinations in the same Forgejo-compatible order.

| Module | Existing responsibility |
| --- | --- |
| `loader` | Loader contract, loaded records and reusable-workflow limits |
| `contexts` | Owned run inputs and borrowed per-job matrix/strategy lookup |
| `planning` | Recursive expansion, permission ceilings, limits/deadlines and deferred completion |
| `dependencies` | Needs detection, conditions and deferred stand-in construction |
| `calls` | Typed call inputs, secrets and permission narrowing |
| `templates` | Existing job, container/service and step wire templates |
| `evaluation` | Located field evaluation, coercions and validation |
| `matrix` | Axes, expression resolution, cartesian expansion and display ordering |
| `names` | Run names and run-scoped concurrency evaluation |
| `group_context` | GitHub-compatible grouping/event context construction |

`plan.rs` keeps its documented public paths through explicit `self::` reexports.
Sibling dependencies use explicit canonical module imports. Borrowed `JobContexts`
and `Evaluator` fields/methods are visible only to the parent planning module.
Every moved struct, trait and alias, including the reusable-workflow test loader,
is registered in the cumulative workflow component policy alongside parser Scope.
All authored modules and cumulative logical types remain below 400 lines.

## Source preservation

Preparation read `bb5d73f63bcfe0a280ae9e907dfaca81af17fe24` as raw Git bytes decoded
with UTF-8. All 49 production items retain their semantic tokens and literal bytes;
all 26 test/helper/fixture items retain their behavior and data. The comparison
normalizes comments, inter-token whitespace, visibility and rustfmt trailing commas.
Rustfmt also removes one redundant closure block in the original `matrix_limits`
test; its expression and assertions remain unchanged. Root documentation uses
qualified links for types/constants that now live in other modules; its Unicode
arrow, section signs and all other prose bytes remain unchanged.

The seven golden cases retain all fourteen exact input/output fixture files.
Moving their include site from `src/plan.rs` to `src/plan/tests/golden.rs` changes
only `../tests/golden/` to `../../../tests/golden/`; each resolved absolute path and
its file-byte hash was checked equal. The optional existing `RBIT_BLESS` output path
still uses the same crate manifest directory. No fixture output was regenerated.

## Pending runtime gates

All sixteen original unit tests remain, grouped into matrix, jobs, names, golden
and reusable-call modules with explicit shared fixture imports. They exercise
expression-derived matrices, combination/run limits, expired deadlines, names,
runner labels, environment timing, every template field, coercions, concurrency,
golden serialization, permission ceilings, deferred inputs, reusable nesting and
maximum call depth. Existing parser/property, deferred-planning, conformance and
Actions/runner suites remain required downstream acceptance.

Preparation used rustfmt, pure Node policy fixtures and source-preservation/bounds
checks only. Rust tests, strict all-target Clippy and the formal cumulative AST gate
remain pending through the owned shared build helper. No measured latency or memory
improvement is claimed for this source-only extraction.
